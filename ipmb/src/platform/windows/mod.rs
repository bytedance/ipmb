use crate::message::{ConnectMessage, ConnectMessageAck};
use crate::util::Align4;
use crate::{
    decode, encode, version, EndpointID, Error, Label, LabelOp, Message, MessageBox, Selector,
    Version,
};
pub use memory_region::MemoryRegion;
use security::SecurityAttr;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::os::windows::prelude::{
    AsRawHandle, FromRawHandle, HandleOrInvalid, IntoRawHandle, OwnedHandle, RawHandle,
};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{mpsc, Arc};
use std::time::Duration;
use std::{mem, ptr, slice};
use type_uuid::*;
use windows::core::HSTRING;
use windows::Win32::Foundation::{self, WIN32_ERROR};
use windows::Win32::Storage::FileSystem;
use windows::Win32::System::{Pipes, Threading, IO};

mod memory_region;
mod pipe;
mod security;
pub(crate) mod util;

#[derive(Debug, PartialEq)]
pub struct Remote {
    pipe: Handle,
    // Empty only during the connecting phase
    process: Option<Handle>,
}

impl Remote {
    pub fn is_dead(&self) -> bool {
        unsafe { !FileSystem::WriteFile(self.pipe.as_raw_windows(), None, None, None).as_bool() }
    }
}

pub(crate) fn look_up(
    identifier: &str,
    label: Label,
    token: String,
    im: Arc<IoMultiplexing>,
) -> Result<(IoHub, Remote, EndpointID), Error> {
    unsafe {
        let identifier: HSTRING = format!("\\\\.\\pipe\\{}", identifier).into();

        let pipe_handle: Foundation::HANDLE = FileSystem::CreateFileW(
            &identifier,
            FileSystem::FILE_GENERIC_WRITE.0,
            FileSystem::FILE_SHARE_NONE,
            None,
            FileSystem::OPEN_EXISTING,
            // We write synchronously, otherwise may incorrectly report that the write operation is complete
            FileSystem::FILE_FLAGS_AND_ATTRIBUTES::default(),
            None,
        )
        .map_err(|err| match WIN32_ERROR::from_error(&err) {
            Some(Foundation::ERROR_PIPE_BUSY) => Error::WinError(err), // TODO
            Some(Foundation::ERROR_FILE_NOT_FOUND) => Error::IdentifierNotInUse,
            _ => Error::WinError(err),
        })?;

        let remote_pipe = Handle(OwnedHandle::from_raw_handle(pipe_handle.0 as _));

        let mut server_pid = 0;
        let r = Pipes::GetNamedPipeServerProcessId(remote_pipe.as_raw_windows(), &mut server_pid);
        if !r.as_bool() {
            return Err(Error::WinError(windows::core::Error::from_win32()));
        }

        let mut remote = Remote {
            pipe: remote_pipe,
            process: None,
        };

        let server_process =
            match Threading::OpenProcess(Threading::PROCESS_DUP_HANDLE, false, server_pid) {
                Ok(server_process) => OwnedHandle::from_raw_handle(server_process.0 as _),
                Err(err) => {
                    if Foundation::GetLastError() == Foundation::ERROR_ACCESS_DENIED {
                        util::fetch_remote_process_handle(&remote, &im.sa)?
                    } else {
                        return Err(Error::WinError(err));
                    }
                }
            };
        remote.process = Some(Handle(server_process));

        let (read_pipe, write_pipe) = pipe::anon_pipe(&im.sa)?;
        let read_pipe = NamedPipe::new(read_pipe, NamedPipeStatus::Readable);

        let msg = Message::new(
            Selector::unicast(LabelOp::True),
            ConnectMessage {
                version: version(),
                token,
                label,
            },
        );

        let mut encoded_msg = msg.into_encoded();
        encoded_msg.add_local(
            Handle(OwnedHandle::from_raw_handle(
                Threading::GetCurrentProcess().0 as _,
            )),
            write_pipe,
        );

        encoded_msg.send(&remote)?;

        let mut io_hub: IoHub = IoHub::for_endpoint(im, identifier, read_pipe);
        // Wait ack
        let encoded_msg = io_hub.recv(Some(Duration::from_secs(2)), None)?;
        let ack = ConnectMessageAck::decode(encoded_msg.selector.uuid, encoded_msg.payload_data)?;

        match ack {
            ConnectMessageAck::Ok(endpoint_id) => Ok((io_hub, remote, endpoint_id)),
            ConnectMessageAck::ErrVersion(v) => Err(Error::VersionMismatch(v, None)),
            ConnectMessageAck::ErrToken => Err(Error::TokenMismatch),
        }
    }
}

pub(crate) fn register(
    identifier: &str,
    im: Arc<IoMultiplexing>,
) -> Result<(IoHub, Sender<EncodedMessage>, EndpointID), Error> {
    unsafe {
        let identifier: HSTRING = format!("\\\\.\\pipe\\{}", identifier).into();

        let pipe_handle = Pipes::CreateNamedPipeW(
            &identifier,
            FileSystem::PIPE_ACCESS_INBOUND
                | FileSystem::FILE_FLAG_OVERLAPPED
                | FileSystem::FILE_FLAG_FIRST_PIPE_INSTANCE,
            Pipes::PIPE_TYPE_MESSAGE | Pipes::PIPE_READMODE_MESSAGE | Pipes::PIPE_WAIT,
            Pipes::PIPE_UNLIMITED_INSTANCES,
            pipe::DEFAULT_BUFFER_SIZE,
            pipe::DEFAULT_BUFFER_SIZE,
            0,
            Some(im.sa.attr()),
        );
        let pipe_handle: OwnedHandle = HandleOrInvalid::from_raw_handle(pipe_handle.0 as _)
            .try_into()
            .map_err(|_| match Foundation::GetLastError() {
                Foundation::ERROR_ALREADY_EXISTS => Error::IdentifierInUse,
                err => Error::WinError(err.into()),
            })?;

        let local = NamedPipe::new(Handle(pipe_handle), NamedPipeStatus::Free);
        let (bus_sender, bus_receiver) = mpsc::channel();

        Ok((
            IoHub::for_bus_controller(im, identifier, bus_receiver, local),
            bus_sender,
            EndpointID::new(),
        ))
    }
}

pub(crate) struct IoHub {
    identifier: HSTRING,
    bus_receiver: Option<Receiver<EncodedMessage>>,
    waker: usize,
    im: Arc<IoMultiplexing>,
    pipes: Vec<NamedPipe>,
    listening_pipe: Option<NamedPipe>,
}

impl IoHub {
    fn for_bus_controller(
        im: Arc<IoMultiplexing>,
        identifier: HSTRING,
        bus_receiver: Receiver<EncodedMessage>,
        listening_pipe: NamedPipe,
    ) -> Self {
        im.register_handle(&listening_pipe);

        let io_hub = Self {
            identifier,
            bus_receiver: Some(bus_receiver),
            waker: im.waker,
            im,
            pipes: vec![],
            listening_pipe: Some(listening_pipe),
        };

        io_hub
    }

    fn for_endpoint(im: Arc<IoMultiplexing>, identifier: HSTRING, pipe: NamedPipe) -> Self {
        im.register_handle(&pipe);

        Self {
            identifier,
            bus_receiver: None,
            waker: im.waker,
            im,
            pipes: vec![pipe],
            listening_pipe: None,
        }
    }

    fn listen(&mut self) {
        if let Some(listening_pipe) = &mut self.listening_pipe {
            loop {
                listening_pipe.listen();

                match listening_pipe.status {
                    NamedPipeStatus::Readable => {
                        self.pipes.push(mem::replace(
                            listening_pipe,
                            self.im.create_instance(&self.identifier),
                        ));
                    }
                    NamedPipeStatus::Pending => break,
                    NamedPipeStatus::Offline => {
                        *listening_pipe = self.im.create_instance(&self.identifier);
                    }
                    NamedPipeStatus::Free | NamedPipeStatus::Ready => unreachable!(),
                }
            }
        }
    }

    pub fn recv(
        &mut self,
        timeout: Option<Duration>,
        _remote: Option<&Remote>,
    ) -> Result<EncodedMessage, Error> {
        'ret: loop {
            // Listen new instance
            self.listen();

            // Receive message from bus_receiver
            if let Some(bus_receiver) = &self.bus_receiver {
                match bus_receiver.try_recv() {
                    Ok(message) => {
                        break Ok(message);
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        self.bus_receiver = None;
                    }
                }
            }

            // Read message from pipes
            let mut i = 0;
            while i < self.pipes.len() {
                let pipe = &mut self.pipes[i];
                match pipe.read() {
                    Some((pipe_msg, msg_size)) => {
                        break 'ret EncodedMessage::new(pipe_msg, msg_size)
                    }
                    None => match pipe.status {
                        NamedPipeStatus::Pending => i += 1,
                        NamedPipeStatus::Offline => {
                            let _ = self.pipes.swap_remove(i);
                        }
                        NamedPipeStatus::Free
                        | NamedPipeStatus::Readable
                        | NamedPipeStatus::Ready => unreachable!(),
                    },
                }
            }

            if self.bus_receiver.is_none() && self.listening_pipe.is_none() && self.pipes.is_empty()
            {
                break Err(Error::Disconnect);
            }

            // Wait
            let mut key = 0;
            let mut n = 0;
            let r = self.im.wait(&mut key, &mut n, timeout);
            if r {
                if key == self.waker {
                    continue;
                }

                if let Some(listening_pipe) = &mut self.listening_pipe {
                    if listening_pipe.as_raw() as usize == key {
                        listening_pipe.status = NamedPipeStatus::Readable;
                        self.pipes.push(mem::replace(
                            listening_pipe,
                            self.im.create_instance(&self.identifier),
                        ));
                        continue;
                    }
                }

                if let Some(pipe) = self
                    .pipes
                    .iter_mut()
                    .find(|pipe| pipe.as_raw() as usize == key)
                {
                    pipe.message_size += n as usize;
                    pipe.status = NamedPipeStatus::Ready;

                    continue;
                }
            } else {
                let err = unsafe { Foundation::GetLastError() };

                match err {
                    Foundation::ERROR_ABANDONED_WAIT_0 => continue, // Handle is closed
                    Foundation::ERROR_TIMEOUT | Foundation::WAIT_TIMEOUT => {
                        break Err(Error::Timeout)
                    }
                    _ => {}
                }

                if key == self.waker {
                    continue;
                }

                if let Some(listening_pipe) = &mut self.listening_pipe {
                    if listening_pipe.as_raw() as usize == key {
                        match err {
                            Foundation::ERROR_PIPE_CONNECTED => {
                                listening_pipe.status = NamedPipeStatus::Readable;
                                self.pipes.push(mem::replace(
                                    listening_pipe,
                                    self.im.create_instance(&self.identifier),
                                ));
                            }
                            Foundation::ERROR_PIPE_LISTENING | Foundation::ERROR_IO_PENDING => {}
                            _ => {
                                log::error!("Listening: {:?}", err);
                                *listening_pipe = self.im.create_instance(&self.identifier);
                            }
                        }
                        continue;
                    }
                }

                if let Some(pipe) = self
                    .pipes
                    .iter_mut()
                    .find(|pipe| pipe.as_raw() as usize == key)
                {
                    match err {
                        Foundation::ERROR_MORE_DATA => {
                            pipe.message_size += n as usize;
                            pipe.buffer.reserve(pipe.buffer.capacity() * 2);
                            pipe.status = NamedPipeStatus::Readable;
                        }
                        Foundation::ERROR_IO_PENDING | Foundation::ERROR_IO_INCOMPLETE => continue,
                        _ => {
                            pipe.status = NamedPipeStatus::Offline;
                            continue;
                        }
                    }
                }
            }
        }
    }

    pub fn io_multiplexing(&self) -> Arc<IoMultiplexing> {
        self.im.clone()
    }
}

fn send_helper(
    remote: &Remote,
    pipe_msg: &mut [u8],
    msg_size: usize,
    objects: &[Handle],
    memory_regions: &[MemoryRegion],
    reply: Option<&[Handle; 2]>,
) -> Result<(), Error> {
    unsafe {
        let mut remote_objects = Vec::with_capacity(objects.len() + memory_regions.len());

        let reply_process_ptr = (pipe_msg.as_mut_ptr() as *mut u32).offset(1) as *mut u64;
        let reply_pipe_ptr = reply_process_ptr.offset(1);

        // Write reply
        if let Some(replay) = reply {
            for (object, object_ptr) in replay.into_iter().zip([reply_process_ptr, reply_pipe_ptr])
            {
                let remote_object = object
                    .to_remote(remote.process.as_ref().unwrap())
                    .map_err(|_| Error::Disconnect)?;
                ptr::write_unaligned(
                    object_ptr,
                    remote_object.pseudo_handle.unwrap() as isize as u64,
                );
                remote_objects.push(remote_object);
            }
        }

        let object_count_ptr = reply_pipe_ptr.offset(1) as *mut u32;
        let mut object_ptr = object_count_ptr.add(1) as *mut u64;

        for object in objects
            .iter()
            .chain(memory_regions.iter().map(|r| r.object()))
        {
            let remote_object = object
                .to_remote(remote.process.as_ref().unwrap())
                .map_err(|_| Error::Disconnect)?;
            ptr::write_unaligned(
                object_ptr,
                remote_object.pseudo_handle.unwrap() as isize as u64,
            );
            remote_objects.push(remote_object);
            object_ptr = object_ptr.add(1);
        }

        let mut n = 0;

        // Add memory region's ref count
        for r in memory_regions {
            r.ref_count_inner(1);
        }

        let r = FileSystem::WriteFile(
            remote.pipe.as_raw_windows(),
            Some(&pipe_msg[..msg_size]),
            Some(&mut n),
            None,
        );
        if !r.as_bool() {
            for r in memory_regions {
                r.ref_count_inner(-1);
            }
            return Err(Error::Disconnect);
        }

        assert_eq!(n, msg_size as _);

        // r = FlushFileBuffers(remote.pipe.as_raw());
        // if !r.as_bool() {
        //     return Err(Error::Disconnect);
        // }

        for mut remote_object in remote_objects {
            remote_object.drain();
        }
    }
    Ok(())
}

/// Message layout
/// | version (magic__major__minor__patch)
/// | reply process (u64)
/// | reply pipe (u64)
/// | object_count (u32)
/// | object (u64) * N
/// | selector_size (u32)
/// | selector
/// | selector_padding
/// | payload_size (u32)
/// | payload
/// | payload_padding
///
pub(crate) struct EncodedMessage {
    pub selector: Selector,
    pub payload_data: &'static [u8],
    pipe_msg: Vec<u8>,
    msg_size: usize,
    pub objects: Vec<Handle>,
    pub memory_regions: Vec<MemoryRegion>,
    reply: Option<[Handle; 2]>,
}

impl EncodedMessage {
    pub fn add_local(&mut self, process: Handle, pipe: Handle) {
        self.reply = Some([process, pipe]);
    }

    pub fn extract_remote(&mut self) -> Option<Remote> {
        debug_assert_eq!(self.selector.uuid, <ConnectMessage as TypeUuid>::UUID);
        self.reply.take().map(|[process, pipe]| Remote {
            pipe,
            process: Some(process),
        })
    }

    pub fn send(&mut self, remote: &Remote) -> Result<(), Error> {
        send_helper(
            remote,
            self.pipe_msg.as_mut_slice(),
            self.msg_size,
            self.objects.as_slice(),
            self.memory_regions.as_slice(),
            self.reply.as_ref(),
        )
    }

    // TODO: Check size
    fn new(mut pipe_msg: Vec<u8>, msg_size: usize) -> Result<Self, Error> {
        unsafe {
            pipe_msg.set_len(msg_size);

            // Version
            let version_ptr = pipe_msg.as_mut_ptr() as *mut u32;
            let [magic, major, minor, patch]: [u8; 4] = mem::transmute(*version_ptr);
            let remote_version = Version((major, minor, patch));

            if magic != 0xFF {
                return Err(Error::VersionMismatch(Version((0, 0, 0)), None));
            }

            // Reply
            let reply_process_ptr = version_ptr.offset(1) as *mut u64;
            let reply_pipe_ptr = reply_process_ptr.offset(1);
            let reply_process = ptr::read_unaligned(reply_process_ptr);
            let reply_pipe = ptr::read_unaligned(reply_pipe_ptr);
            let reply = if reply_process > 0 && reply_pipe > 0 {
                Some([
                    Handle(OwnedHandle::from_raw_handle(reply_process as isize as _)),
                    Handle(OwnedHandle::from_raw_handle(reply_pipe as isize as _)),
                ])
            } else {
                None
            };

            let object_count_ptr = reply_pipe_ptr.offset(1) as *mut u32;
            let object_count = *object_count_ptr;

            let mut object_ptr = object_count_ptr.add(1) as *mut u64;
            let mut objects = Vec::with_capacity(object_count as _);
            for _ in 0..object_count {
                objects.push(Handle(OwnedHandle::from_raw_handle(
                    ptr::read_unaligned(object_ptr) as isize as _,
                )));
                object_ptr = object_ptr.add(1);
            }

            if !version().compatible(remote_version) {
                return Err(Error::VersionMismatch(
                    remote_version,
                    reply.map(|[process, pipe]| Remote {
                        pipe,
                        process: Some(process),
                    }),
                ));
            }

            let selector_size_ptr = object_ptr as *mut u32;
            let selector_size = *selector_size_ptr;

            let selector_ptr = selector_size_ptr.add(1) as *mut u8;
            // TODO: Check size
            let selector: Selector =
                decode(slice::from_raw_parts(selector_ptr, selector_size as _))?;

            // Pop memory regions
            let memory_regions: Vec<_> = objects
                .drain((objects.len() - selector.memory_region_count as usize)..)
                .map(|obj| {
                    let r = MemoryRegion::from_object(obj);
                    r.ref_count_inner(-1);
                    r
                })
                .collect();

            let payload_size_ptr = selector_ptr.add(selector_size.align4() as _) as *mut u32;
            let payload_size = *payload_size_ptr;

            let payload_ptr = payload_size_ptr.add(1) as *mut u8;

            Ok(Self {
                selector,
                payload_data: slice::from_raw_parts(payload_ptr, payload_size as _),
                pipe_msg,
                msg_size,
                objects,
                memory_regions,
                reply,
            })
        }
    }
}

impl<T: MessageBox> Message<T> {
    fn encode_inner(&self) -> (&'static [u8], Vec<u8>, usize) {
        unsafe {
            // TODO: How can we hint size
            let selector_bytes = encode(&self.selector).unwrap();
            let payload_bytes = self.payload.encode().unwrap();

            let msg_size = 4
                + 16
                + 4
                + (self.objects.len() + self.memory_regions.len()) * 8
                + 4
                + selector_bytes.len().align4()
                + 4
                + payload_bytes.len().align4();

            let mut pipe_msg: Vec<u8> = Vec::with_capacity(msg_size);
            pipe_msg.set_len(msg_size);

            // Version
            let version_ptr = pipe_msg.as_mut_ptr() as *mut u32;
            {
                let v = version();
                *version_ptr = mem::transmute([0xFF, v.major(), v.minor(), v.patch()]);
            }

            // Reply
            let reply_process_ptr = version_ptr.offset(1) as *mut u64;
            ptr::write_unaligned(reply_process_ptr, 0);
            let reply_pipe_ptr = reply_process_ptr.offset(1);
            ptr::write_unaligned(reply_pipe_ptr, 0);

            let object_count_ptr = reply_pipe_ptr.offset(1) as *mut u32;
            *object_count_ptr = (self.objects.len() + self.memory_regions.len()) as _;

            let object_ptr = object_count_ptr.add(1) as *mut u64;
            // Skip remote objects
            let selector_size_ptr =
                object_ptr.add(self.objects.len() + self.memory_regions.len()) as *mut u32;
            *selector_size_ptr = selector_bytes.len() as _;

            let selector_ptr = selector_size_ptr.add(1) as *mut u8;
            ptr::copy_nonoverlapping(selector_bytes.as_ptr(), selector_ptr, selector_bytes.len());

            let payload_size_ptr = selector_ptr.add(selector_bytes.len().align4()) as *mut u32;
            *payload_size_ptr = payload_bytes.len() as _;

            let payload_ptr = payload_size_ptr.add(1) as *mut u8;
            ptr::copy_nonoverlapping(payload_bytes.as_ptr(), payload_ptr, payload_bytes.len());

            (
                slice::from_raw_parts(payload_ptr, payload_bytes.len()),
                pipe_msg,
                msg_size,
            )
        }
    }

    pub(crate) fn into_encoded(self) -> EncodedMessage {
        let (payload_data, pipe_msg, msg_size) = self.encode_inner();

        EncodedMessage {
            selector: self.selector,
            payload_data,
            pipe_msg,
            msg_size,
            objects: self.objects,
            memory_regions: self.memory_regions,
            reply: None,
        }
    }
}

#[repr(transparent)]
#[derive(Debug)]
pub struct Handle(OwnedHandle);

impl Handle {
    pub unsafe fn from_raw(raw: RawHandle) -> Self {
        Self(OwnedHandle::from_raw_handle(raw))
    }

    pub fn into_raw(self) -> RawHandle {
        self.0.into_raw_handle()
    }

    pub fn as_raw(&self) -> RawHandle {
        self.0.as_raw_handle()
    }

    pub(crate) fn as_raw_windows(&self) -> Foundation::HANDLE {
        Foundation::HANDLE(self.as_raw() as _)
    }

    fn to_remote<'a>(&self, remote: &'a Self) -> Result<RemoteHandle<'a>, Error> {
        let mut pseudo_handle = Foundation::HANDLE::default();

        unsafe {
            let r = Foundation::DuplicateHandle(
                // pseudo handle, no leak here
                // https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getcurrentprocess
                Threading::GetCurrentProcess(),
                self.as_raw_windows(),
                remote.as_raw_windows(),
                &mut pseudo_handle,
                0,
                false,
                Foundation::DUPLICATE_SAME_ACCESS,
            );
            if !r.as_bool() {
                let err = match Foundation::GetLastError() {
                    Foundation::ERROR_ACCESS_DENIED => Error::Disconnect, // TODO
                    err => Error::WinError(err.into()),
                };

                return Err(err);
            }
        }

        Ok(RemoteHandle {
            pseudo_handle: Some(pseudo_handle.0 as _),
            remote,
        })
    }
}

impl PartialEq for Handle {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_raw_handle() == other.0.as_raw_handle()
    }
}

impl Clone for Handle {
    fn clone(&self) -> Self {
        Self(self.0.try_clone().expect("Clone handle failed"))
    }
}

unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

struct RemoteHandle<'a> {
    pseudo_handle: Option<RawHandle>,
    remote: &'a Handle,
}

impl<'a> RemoteHandle<'a> {
    fn drain(&mut self) {
        self.pseudo_handle = None;
    }
}

impl<'a> Drop for RemoteHandle<'a> {
    fn drop(&mut self) {
        if let Some(pseudo_handle) = self.pseudo_handle.take() {
            unsafe {
                let _ = Foundation::DuplicateHandle(
                    self.remote.as_raw_windows(),
                    Foundation::HANDLE(pseudo_handle as _),
                    None,
                    ptr::null_mut(),
                    0,
                    false,
                    Foundation::DUPLICATE_CLOSE_SOURCE,
                );
            }
        }
    }
}

// Free -> Pending -> Readable -> Pending -> Ready -> Readable -> ..
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum NamedPipeStatus {
    Free,
    Readable,
    Pending,
    Ready,
    Offline,
}

pub struct NamedPipe {
    handle: Handle,
    status: NamedPipeStatus,
    buffer: Vec<u8>,
    message_size: usize,
    overlapped: Box<IO::OVERLAPPED>,
}

unsafe impl Send for NamedPipe {}

impl NamedPipe {
    fn new(handle: Handle, status: NamedPipeStatus) -> Self {
        unsafe {
            let overlapped: Box<IO::OVERLAPPED> = Box::new(MaybeUninit::zeroed().assume_init());

            Self {
                handle,
                status,
                buffer: Vec::with_capacity(64),
                message_size: 0,
                overlapped,
            }
        }
    }

    fn listen(&mut self) {
        unsafe {
            match self.status {
                NamedPipeStatus::Free => {}
                NamedPipeStatus::Pending | NamedPipeStatus::Offline => return,
                NamedPipeStatus::Readable | NamedPipeStatus::Ready => unreachable!(),
            }

            let r = Pipes::ConnectNamedPipe(self.as_raw_windows(), Some(self.overlapped.as_mut()));

            if r.as_bool() {
                // self.status = NamedPipeStatus::Readable;
                self.status = NamedPipeStatus::Pending;
            } else {
                match Foundation::GetLastError() {
                    Foundation::ERROR_PIPE_CONNECTED => {
                        self.status = NamedPipeStatus::Readable;
                    }
                    Foundation::ERROR_PIPE_LISTENING | Foundation::ERROR_IO_PENDING => {
                        self.status = NamedPipeStatus::Pending;
                    }
                    err @ _ => {
                        log::error!("Listening: {:?}", err);
                        self.status = NamedPipeStatus::Offline;
                    }
                }
            }
        }
    }

    fn read(&mut self) -> Option<(Vec<u8>, usize)> {
        match self.status {
            NamedPipeStatus::Readable => {}
            NamedPipeStatus::Pending => return None,
            NamedPipeStatus::Ready => {
                self.status = NamedPipeStatus::Readable;
                if self.message_size > 0 {
                    let pipe_msg = mem::replace(&mut self.buffer, Vec::with_capacity(64));
                    let msg_size = mem::replace(&mut self.message_size, 0);
                    return Some((pipe_msg, msg_size));
                }
            }
            NamedPipeStatus::Offline => return None,
            NamedPipeStatus::Free => unreachable!(),
        }

        let mut n = 0;
        // loop {
        unsafe {
            let r = FileSystem::ReadFile(
                self.as_raw_windows(),
                Some(self.buffer.as_mut_ptr().add(self.message_size) as _),
                (self.buffer.capacity() - self.message_size) as _,
                Some(&mut n),
                Some(self.overlapped.as_mut()),
            );

            if r.as_bool() {
                // self.message_size += n as usize;
                //
                // let pipe_msg = mem::replace(&mut self.buffer, Vec::with_capacity(64));
                // let msg_size = mem::replace(&mut self.message_size, 0);
                //
                // break Some((pipe_msg, msg_size));

                self.status = NamedPipeStatus::Pending;
                return None;
            }

            match Foundation::GetLastError() {
                Foundation::ERROR_MORE_DATA => {
                    // self.message_size += n as usize;
                    // self.buffer.reserve(self.buffer.capacity() * 2);

                    self.status = NamedPipeStatus::Pending;
                    None
                }
                Foundation::ERROR_IO_PENDING | Foundation::ERROR_IO_INCOMPLETE => {
                    self.status = NamedPipeStatus::Pending;
                    None
                }
                err @ _ => {
                    log::error!("ReadFile: {:?}", err);
                    self.status = NamedPipeStatus::Offline;
                    None
                }
            }
        }
        // }
    }
}

impl Drop for NamedPipe {
    fn drop(&mut self) {
        unsafe {
            let _ = Pipes::DisconnectNamedPipe(self.as_raw_windows());
        }
    }
}

impl Deref for NamedPipe {
    type Target = Handle;

    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

pub(crate) struct IoMultiplexing {
    completion_port: Handle,
    waker: usize,
    sa: SecurityAttr,
}

impl IoMultiplexing {
    pub fn new() -> Self {
        unsafe {
            let completion_port: Foundation::HANDLE =
                IO::CreateIoCompletionPort(Foundation::INVALID_HANDLE_VALUE, None, 0, 0)
                    .expect("CreateIoCompletionPort failed");
            let completion_port = OwnedHandle::from_raw_handle(completion_port.0 as _);

            Self {
                completion_port: Handle(completion_port),
                waker: 0, // We use 0 as waker key, because a valid handle will never be 0
                sa: SecurityAttr::allow_everyone().unwrap(),
            }
        }
    }

    fn create_instance(&self, identifier: &HSTRING) -> NamedPipe {
        unsafe {
            let pipe_handle = Pipes::CreateNamedPipeW(
                identifier,
                FileSystem::PIPE_ACCESS_INBOUND | FileSystem::FILE_FLAG_OVERLAPPED,
                Pipes::PIPE_TYPE_MESSAGE | Pipes::PIPE_READMODE_MESSAGE | Pipes::PIPE_WAIT,
                Pipes::PIPE_UNLIMITED_INSTANCES,
                pipe::DEFAULT_BUFFER_SIZE,
                pipe::DEFAULT_BUFFER_SIZE,
                0,
                Some(self.sa.attr()),
            );

            let pipe_handle: OwnedHandle = HandleOrInvalid::from_raw_handle(pipe_handle.0 as _)
                .try_into()
                .expect("CreateNamedPipeW");

            let pipe = NamedPipe::new(Handle(pipe_handle), NamedPipeStatus::Free);

            self.register_handle(&pipe);

            pipe
        }
    }

    fn register_handle(&self, handle: &Handle) {
        unsafe {
            let _ = IO::CreateIoCompletionPort(
                handle.as_raw_windows(),
                self.completion_port.as_raw_windows(),
                handle.as_raw() as _,
                0,
            )
            .expect("CreateIoCompletionPort failed");
        }
    }

    fn wait(&self, key: &mut usize, n: &mut u32, timeout: Option<Duration>) -> bool {
        let mut overlapped = ptr::null_mut();

        unsafe {
            IO::GetQueuedCompletionStatus(
                self.completion_port.as_raw_windows(),
                n,
                key,
                &mut overlapped,
                timeout
                    .map(|timeout| timeout.as_millis() as _)
                    .unwrap_or(Threading::INFINITE),
            )
            .as_bool()
        }
    }

    pub fn wake(&self) {
        unsafe {
            let r = IO::PostQueuedCompletionStatus(
                self.completion_port.as_raw_windows(),
                0,
                self.waker,
                None,
            );
            assert!(r.as_bool());
        }
    }
}
