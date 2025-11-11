use crate::{
    decode,
    message::{ConnectMessage, ConnectMessageAck},
    util::Align4,
    version, EndpointID, Error, Label, LabelOp, MemoryRegion, Message, MessageBox, Selector,
    Version,
};
pub(crate) use memory_region::page_mask;
use std::{
    ffi::CString,
    hash::Hash,
    io, mem,
    os::unix::prelude::{AsRawFd, FromRawFd, OwnedFd},
    ptr, slice,
    sync::{
        mpsc,
        mpsc::{Receiver, Sender, TryRecvError},
        Arc, Once,
    },
    thread,
    time::{Duration, Instant},
};
use type_uuid::TypeUuid;

pub mod mach_sys;
mod memory_region;

// #define MACH_PORT_RIGHT_DEAD_NAME       ((mach_port_right_t) 4)
// #define MACH_PORT_TYPE(right)
// 	        ((mach_port_type_t)(((mach_port_type_t) 1)
// 	        << ((right) + ((mach_port_right_t) 16))))
// #define MACH_PORT_TYPE_DEAD_NAME    MACH_PORT_TYPE(MACH_PORT_RIGHT_DEAD_NAME)
const MACH_PORT_TYPE_DEAD_NAME: mach_sys::mach_port_type_t = 1 << (4 + 16);

#[derive(Debug, PartialEq)]
pub struct Remote {
    port: MachPort,
}

impl Remote {
    pub fn is_dead(&self) -> bool {
        let mut ty = 0;
        unsafe {
            let r =
                mach_sys::mach_port_type(mach_sys::mach_task_self(), self.port.as_raw(), &mut ty);
            assert_eq!(r, mach_sys::KERN_SUCCESS);
        }

        ty & MACH_PORT_TYPE_DEAD_NAME != 0
    }
}

#[inline]
fn mach_msgh_bits_set(remote: u8, local: u8, voucher: u8, other: u32) -> u32 {
    remote as u32 | ((local as u32) << 8) | ((voucher as u32) << 16) | other
}

pub(crate) fn look_up(
    identifier: &str,
    label: Label,
    token: String,
    im: Arc<IoMultiplexing>,
) -> Result<(IoHub, Remote, EndpointID), Error> {
    let identifier = CString::new(identifier).unwrap();
    let mut remote = 0;

    unsafe {
        let r = mach_sys::bootstrap_look_up(
            get_bootstrap_port_root(),
            identifier.as_ptr(),
            &mut remote,
        );

        match r {
            mach_sys::BOOTSTRAP_SUCCESS if remote > 0 => {
                let remote = Remote {
                    port: MachPort::from_raw(remote),
                };
                let local = MachPort::with_receive_right();

                let mut msg = Message::new(
                    Selector::unicast(LabelOp::True),
                    ConnectMessage {
                        version: version(),
                        token,
                        label,
                    },
                );
                msg.objects.push(local.clone()?);
                let mut encoded_msg = msg.into_encoded();
                encoded_msg.send(&remote)?;

                let mut io_hub: IoHub = IoHub::for_endpoint(local, im);
                // Wait ack
                let encoded_msg = io_hub.recv(Some(Duration::from_secs(2)), Some(&remote))?;
                let ack =
                    ConnectMessageAck::decode(encoded_msg.selector.uuid, encoded_msg.payload_data)?;

                match ack {
                    ConnectMessageAck::Ok(endpoint_id) => Ok((io_hub, remote, endpoint_id)),
                    ConnectMessageAck::ErrVersion(v) => Err(Error::VersionMismatch(v, None)),
                    ConnectMessageAck::ErrToken => Err(Error::TokenMismatch),
                }
            }
            mach_sys::BOOTSTRAP_UNKNOWN_SERVICE => Err(Error::IdentifierNotInUse),
            mach_sys::BOOTSTRAP_NOT_PRIVILEGED => Err(Error::PermissonDenied),
            _ => Err(Error::Unknown), // TODO
        }
    }
}

pub(crate) fn register(
    identifier: &str,
    im: Arc<IoMultiplexing>,
) -> Result<(IoHub, Sender<EncodedMessage>, EndpointID), Error> {
    let identifier = CString::new(identifier).unwrap();
    let local = MachPort::with_receive_right();
    unsafe {
        let r = mach_sys::bootstrap_register(
            get_bootstrap_port_root(),
            identifier.as_ptr(),
            local.as_raw(),
        );

        match r {
            mach_sys::BOOTSTRAP_SUCCESS => {
                let (bus_sender, bus_receiver) = mpsc::channel();
                Ok((
                    IoHub::for_bus_controller(local, bus_receiver, im),
                    bus_sender,
                    EndpointID::new(),
                ))
            }
            mach_sys::BOOTSTRAP_NAME_IN_USE => Err(Error::IdentifierInUse),
            mach_sys::BOOTSTRAP_NOT_PRIVILEGED => Err(Error::PermissonDenied),
            _ => Err(Error::Unknown), // TODO
        }
    }
}

pub(crate) struct IoHub {
    local: Option<Pipe>,
    bus_receiver: Option<Receiver<EncodedMessage>>,
    #[allow(dead_code)]
    waker: usize,
    im: Arc<IoMultiplexing>,
    in_buffer: Vec<KEvent>,
}

impl IoHub {
    fn for_bus_controller(
        local: MachPort,
        bus_receiver: Receiver<EncodedMessage>,
        im: Arc<IoMultiplexing>,
    ) -> Self {
        im.register_mach_port(&local);

        Self {
            local: Some(Pipe::new(local)),
            bus_receiver: Some(bus_receiver),
            waker: im.waker,
            im,
            in_buffer: Vec::with_capacity(2),
        }
    }

    fn for_endpoint(local: MachPort, im: Arc<IoMultiplexing>) -> Self {
        im.register_mach_port(&local);

        Self {
            local: Some(Pipe::new(local)),
            bus_receiver: None,
            waker: im.waker,
            im,
            in_buffer: Vec::with_capacity(2),
        }
    }

    pub fn recv(
        &mut self,
        timeout: Option<Duration>,
        remote: Option<&Remote>,
    ) -> Result<EncodedMessage, Error> {
        let end = timeout.map(|timeout| Instant::now() + timeout);

        loop {
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

            if let Some(local) = &mut self.local {
                match local.read() {
                    Some(mach_msg) => break EncodedMessage::new(mach_msg),
                    None => match local.status {
                        PipeStatus::Pending => {
                            if let Some(remote) = remote {
                                if remote.is_dead() {
                                    self.local = None;
                                }
                            }
                        }
                        PipeStatus::Offline => self.local = None,
                        PipeStatus::Readable => unreachable!(),
                    },
                }
            }

            if self.bus_receiver.is_none() && self.local.is_none() {
                break Err(Error::Disconnect);
            }

            self.im
                .wait(&mut self.in_buffer, Some(Duration::from_millis(200)));

            if self.in_buffer.is_empty() {
                if let Some(end) = end {
                    if Instant::now() > end {
                        break Err(Error::Timeout);
                    }
                }
                continue;
            }

            if let Some(local) = &mut self.local {
                if self
                    .in_buffer
                    .drain(..)
                    .any(|i| i.0.ident == local.port.as_raw() as _)
                {
                    local.status = PipeStatus::Readable;
                }
            }
        }
    }

    pub fn io_multiplexing(&self) -> Arc<IoMultiplexing> {
        self.im.clone()
    }
}

#[repr(C)]
#[derive(Default)]
struct BaseMessage {
    header: mach_sys::mach_msg_header_t,
    body: mach_sys::mach_msg_body_t,
}

/// Message layout
/// | mach_msg_header_t
/// | mach_msg_body_t
/// | mach_msg_port_descriptor_t * N
/// | version (magic__major__minor__patch)
/// | selector_size (u32)
/// | selector
/// | selector_padding
/// | payload_size (u32)
/// | payload
/// | payload_padding
///
impl<T: MessageBox> Message<T> {
    fn encode_inner(&self) -> (&'static [u8], Vec<u8>) {
        unsafe {
            let mut size = mem::size_of::<mach_sys::mach_msg_header_t>();

            size += mem::size_of::<mach_sys::mach_msg_body_t>()
                + (self.objects.len() + self.memory_regions.len())
                    * mem::size_of::<mach_sys::mach_msg_port_descriptor_t>();

            size += 4; // version

            size += 4; // selector size

            // TODO: How can we hint size
            let selector_data =
                bincode::serde::encode_to_vec(&self.selector, bincode::config::standard()).unwrap();
            size += selector_data.len().align4();

            size += 4; // payload size

            // TODO: How can we hint size
            let payload_bytes = self.payload.encode().unwrap();
            size += payload_bytes.len().align4();

            let mut mach_msg: Vec<u8> = Vec::with_capacity(size);

            let header_ptr = mach_msg.as_mut_ptr() as *mut mach_sys::mach_msg_header_t;

            (*header_ptr).msgh_bits = mach_msgh_bits_set(
                mach_sys::MACH_MSG_TYPE_COPY_SEND,
                0,
                0,
                mach_sys::MACH_MSGH_BITS_COMPLEX,
            );
            (*header_ptr).msgh_size = size as _;
            (*header_ptr).msgh_remote_port = mach_sys::MACH_PORT_NULL;
            (*header_ptr).msgh_local_port = mach_sys::MACH_PORT_NULL;
            (*header_ptr).msgh_reserved = 0;
            (*header_ptr).msgh_id = 0;
            let body_ptr = header_ptr.offset(1) as *mut mach_sys::mach_msg_body_t;

            (*body_ptr).msgh_descriptor_count =
                (self.objects.len() + self.memory_regions.len()) as _;
            let mut descriptor_ptr =
                body_ptr.offset(1) as *mut mach_sys::mach_msg_port_descriptor_t;

            for object in self
                .objects
                .iter()
                .chain(self.memory_regions.iter().map(|r| r.object()))
            {
                (*descriptor_ptr).name = object.as_raw();
                (*descriptor_ptr).pad1 = 0;
                (*descriptor_ptr).pad2 = 0;
                (*descriptor_ptr).disposition = mach_sys::MACH_MSG_TYPE_COPY_SEND;
                (*descriptor_ptr).type_ = mach_sys::MACH_MSG_PORT_DESCRIPTOR;

                descriptor_ptr = descriptor_ptr.offset(1);
            }

            // Version
            let version_ptr = descriptor_ptr as *mut u32;
            {
                let v = version();
                *version_ptr = u32::from_ne_bytes([0xFF, v.major(), v.minor(), v.patch()]);
            }

            let selector_size_ptr = version_ptr.offset(1);

            *selector_size_ptr = selector_data.len() as _;
            let selector_ptr = selector_size_ptr.offset(1) as *mut u8;

            ptr::copy_nonoverlapping(selector_data.as_ptr(), selector_ptr, selector_data.len());
            let payload_len_ptr =
                selector_ptr.offset(selector_data.len().align4() as _) as *mut u32;

            *payload_len_ptr = payload_bytes.len() as _;
            let payload_ptr = payload_len_ptr.offset(1) as *mut u8;

            ptr::copy_nonoverlapping(payload_bytes.as_ptr(), payload_ptr, payload_bytes.len());

            (
                slice::from_raw_parts(payload_ptr, payload_bytes.len()),
                mach_msg,
            )
        }
    }

    pub(crate) fn into_encoded(self) -> EncodedMessage {
        let (payload_data, mach_msg) = self.encode_inner();

        EncodedMessage {
            selector: self.selector,
            payload_data,
            mach_msg,
            objects: self.objects,
            memory_regions: self.memory_regions,
        }
    }
}

pub(crate) struct EncodedMessage {
    pub selector: Selector,
    pub payload_data: &'static [u8],
    mach_msg: Vec<u8>,
    pub objects: Vec<MachPort>,
    pub memory_regions: Vec<MemoryRegion>,
}

impl EncodedMessage {
    pub fn extract_remote(&mut self) -> Option<Remote> {
        debug_assert_eq!(self.selector.uuid, <ConnectMessage as TypeUuid>::UUID);

        let reply = self.objects.pop()?;
        Some(Remote { port: reply })
    }

    pub fn send(&mut self, remote: &Remote) -> Result<(), Error> {
        unsafe {
            let header_ptr = self.mach_msg.as_mut_ptr() as *mut BaseMessage;

            (*header_ptr).header.msgh_remote_port = remote.port.as_raw();

            // Add memory region's ref count
            for r in self.memory_regions.iter() {
                r.ref_count_inner(1);
            }

            loop {
                let r = mach_sys::mach_msg_send(header_ptr as *mut _);
                // TODO: handle too large
                if r == mach_sys::MACH_MSG_SUCCESS {
                    break Ok(());
                } else if r == mach_sys::MACH_SEND_NO_BUFFER {
                    // Retry
                    thread::sleep(Duration::from_millis(200));
                } else {
                    for r in self.memory_regions.iter() {
                        r.ref_count_inner(-1);
                    }

                    break if remote.is_dead() {
                        Err(Error::Disconnect)
                    } else {
                        log::error!("mach_msg failed: {r}");
                        Ok(())
                    };
                }
            }
        }
    }

    // TODO: Check size
    fn new(mut mach_msg: Vec<u8>) -> Result<Self, Error> {
        unsafe {
            // Decode
            let base_ptr = mach_msg.as_mut_ptr() as *mut BaseMessage;

            let mut descriptor_ptr =
                base_ptr.offset(1) as *mut mach_sys::mach_msg_port_descriptor_t;
            let mut objects = Vec::with_capacity((*base_ptr).body.msgh_descriptor_count as _);

            for _ in 0..(*base_ptr).body.msgh_descriptor_count {
                assert_eq!((*descriptor_ptr).type_, mach_sys::MACH_MSG_PORT_DESCRIPTOR);

                objects.push(MachPort::from_raw((*descriptor_ptr).name));
                (*descriptor_ptr).disposition = mach_sys::MACH_MSG_TYPE_COPY_SEND;
                descriptor_ptr = descriptor_ptr.offset(1);
            }

            // Version
            let version_ptr = descriptor_ptr as *mut u32;
            let [magic, major, minor, patch]: [u8; 4] = u32::to_ne_bytes(*version_ptr);
            let remote_version = Version((major, minor, patch));

            if magic != 0xFF {
                return Err(Error::VersionMismatch(Version((0, 0, 0)), None));
            }

            if !version().compatible(remote_version) {
                return Err(Error::VersionMismatch(remote_version, None));
            }

            let selector_size_ptr = version_ptr.offset(1);

            let selector_ptr = selector_size_ptr.offset(1) as *mut u8;
            // TODO: Check size
            let selector: Selector =
                decode(slice::from_raw_parts(selector_ptr, *selector_size_ptr as _))?;

            // Pop memory regions
            let memory_regions: Vec<_> = objects
                .drain((objects.len() - selector.memory_region_count as usize)..)
                .map(|obj| {
                    let r = MemoryRegion::from_object(obj);
                    r.ref_count_inner(-1);
                    r
                })
                .collect();

            let payload_size_ptr =
                selector_ptr.offset((*selector_size_ptr).align4() as _) as *mut u32;

            // Used for route
            (*base_ptr).header.msgh_bits = mach_msgh_bits_set(
                mach_sys::MACH_MSG_TYPE_COPY_SEND,
                0,
                0,
                mach_sys::MACH_MSGH_BITS_COMPLEX,
            );

            (*base_ptr).header.msgh_size = mem::size_of::<BaseMessage>() as u32
                + (*base_ptr).body.msgh_descriptor_count
                    * mem::size_of::<mach_sys::mach_msg_port_descriptor_t>() as u32
                + 4 // Version
                + 4
                + (*selector_size_ptr).align4()
                + 4
                + (*payload_size_ptr).align4();
            (*base_ptr).header.msgh_local_port = mach_sys::MACH_PORT_NULL;

            Ok(Self {
                selector,
                payload_data: slice::from_raw_parts(
                    payload_size_ptr.offset(1) as *const u8,
                    *payload_size_ptr as _,
                ),
                mach_msg,
                objects,
                memory_regions,
            })
        }
    }
}

static mut BOOTSTRAP_PORT: mach_sys::mach_port_t = 0;
static mut BOOTSTRAP_PORT_ROOT: mach_sys::mach_port_t = 0;
static INIT: Once = Once::new();

unsafe fn init() {
    INIT.call_once(|| {
        let mut r = mach_sys::task_get_special_port(
            mach_sys::mach_task_self(),
            mach_sys::TASK_BOOTSTRAP_PORT,
            ptr::addr_of_mut!(BOOTSTRAP_PORT),
        );
        assert_eq!(r, mach_sys::BOOTSTRAP_SUCCESS);
        BOOTSTRAP_PORT_ROOT = BOOTSTRAP_PORT;

        let mut up = 0;
        loop {
            r = mach_sys::bootstrap_parent(BOOTSTRAP_PORT_ROOT, ptr::addr_of_mut!(up));
            if r != mach_sys::BOOTSTRAP_SUCCESS {
                break;
            }
            if BOOTSTRAP_PORT_ROOT == up {
                break;
            }
            BOOTSTRAP_PORT_ROOT = up;
        }
    });
}

#[allow(dead_code)]
fn get_bootstrap_port() -> mach_sys::mach_port_t {
    unsafe {
        init();
        BOOTSTRAP_PORT
    }
}

// https://opensource.apple.com/source/launchd/launchd-328/launchd/src/bootstrap.h.auto.html
fn get_bootstrap_port_root() -> mach_sys::mach_port_t {
    unsafe {
        init();
        BOOTSTRAP_PORT_ROOT
    }
}

#[derive(Debug, Eq, PartialEq, Hash)]
pub struct MachPort {
    port: mach_sys::mach_port_t,
    receive_right: bool,
}

impl MachPort {
    fn with_receive_right() -> Self {
        let mut local = 0;

        unsafe {
            let mut r = mach_sys::mach_port_allocate(
                mach_sys::mach_task_self(),
                mach_sys::MACH_PORT_RIGHT_RECEIVE,
                &mut local,
            );
            assert_eq!(r, mach_sys::KERN_SUCCESS);

            r = mach_sys::mach_port_insert_right(
                mach_sys::mach_task_self(),
                local,
                local,
                mach_sys::MACH_MSG_TYPE_MAKE_SEND as _,
            );
            assert_eq!(r, mach_sys::KERN_SUCCESS);

            let mut limits = mach_sys::mach_port_limits_t {
                mpl_qlimit: mach_sys::MACH_PORT_QLIMIT_MAX,
            };
            r = mach_sys::mach_port_set_attributes(
                mach_sys::mach_task_self(),
                local,
                mach_sys::MACH_PORT_LIMITS_INFO,
                &mut limits as *mut _ as _,
                1,
            );
            assert_eq!(r, mach_sys::KERN_SUCCESS);

            Self {
                port: local,
                receive_right: true,
            }
        }
    }

    pub unsafe fn from_raw(port: mach_sys::mach_port_t) -> Self {
        Self {
            port,
            receive_right: false,
        }
    }

    pub unsafe fn into_raw(self) -> mach_sys::mach_port_t {
        let raw = self.as_raw();
        mem::forget(self);
        raw
    }

    pub fn as_raw(&self) -> mach_sys::mach_port_t {
        self.port
    }
}

impl MachPort {
    pub fn clone(&self) -> io::Result<Self> {
        unsafe {
            let r = mach_sys::mach_port_mod_refs(
                mach_sys::mach_task_self(),
                self.as_raw(),
                mach_sys::MACH_PORT_RIGHT_SEND,
                1,
            );
            if r != mach_sys::KERN_SUCCESS {
                return Err(io::Error::other("mach_port_mod_refs"));
            }
            Ok(Self::from_raw(self.as_raw()))
        }
    }
}

impl Drop for MachPort {
    fn drop(&mut self) {
        unsafe {
            let mut r;
            if self.receive_right {
                r = mach_sys::mach_port_mod_refs(
                    mach_sys::mach_task_self(),
                    self.as_raw(),
                    mach_sys::MACH_PORT_RIGHT_RECEIVE,
                    -1,
                );
                debug_assert_eq!(r, mach_sys::KERN_SUCCESS);
            }
            r = mach_sys::mach_port_deallocate(mach_sys::mach_task_self(), self.as_raw());
            debug_assert_eq!(r, mach_sys::KERN_SUCCESS);
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum PipeStatus {
    Readable,
    Pending,
    Offline,
}

struct Pipe {
    port: MachPort,
    status: PipeStatus,
}

impl Pipe {
    fn new(port: MachPort) -> Self {
        Self {
            port,
            status: PipeStatus::Readable,
        }
    }

    fn read(&mut self) -> Option<Vec<u8>> {
        match self.status {
            PipeStatus::Readable => {}
            PipeStatus::Pending | PipeStatus::Offline => return None,
        }

        let mut mach_msg: Vec<u8> = Vec::with_capacity(
            mem::size_of::<BaseMessage>() + mem::size_of::<mach_sys::mach_msg_trailer_t>() + 64,
        );
        let option = mach_sys::MACH_RCV_MSG | mach_sys::MACH_RCV_LARGE | mach_sys::MACH_RCV_TIMEOUT;
        let mut r;

        unsafe {
            // Receive
            loop {
                let header_ptr = mach_msg.as_mut_ptr() as *mut BaseMessage;
                (*header_ptr).header.msgh_local_port = 0;
                (*header_ptr).header.msgh_size = 0;
                (*header_ptr).header.msgh_bits = 0;
                (*header_ptr).header.msgh_remote_port = 0;
                (*header_ptr).header.msgh_reserved = 0;
                (*header_ptr).header.msgh_id = 0;

                r = mach_sys::mach_msg(
                    header_ptr as *mut _,
                    option,
                    0,
                    mach_msg.capacity() as _,
                    self.port.as_raw(),
                    0,
                    mach_sys::MACH_PORT_NULL,
                );

                match r {
                    mach_sys::MACH_RCV_TOO_LARGE => {
                        mach_msg.reserve(
                            (*header_ptr).header.msgh_size as usize
                                + mem::size_of::<mach_sys::mach_msg_trailer_t>(),
                        );
                        continue;
                    }
                    mach_sys::MACH_MSG_SUCCESS => {
                        break Some(mach_msg);
                    }
                    mach_sys::MACH_RCV_TIMED_OUT => {
                        self.status = PipeStatus::Pending;
                        break None;
                    }
                    mach_sys::MACH_RCV_PORT_DIED => {
                        self.status = PipeStatus::Offline;
                        break None;
                    }
                    _ => {
                        self.status = PipeStatus::Offline;
                        break None;
                    }
                }
            }
        }
    }
}

#[repr(transparent)]
struct KEvent(libc::kevent);

unsafe impl Send for KEvent {}

pub(crate) struct IoMultiplexing {
    fd: OwnedFd,
    waker: usize,
}

impl IoMultiplexing {
    pub fn new() -> Self {
        unsafe {
            let fd = libc::kqueue();
            assert_ne!(fd, -1); // TODO
            let fd = OwnedFd::from_raw_fd(fd);

            let event = libc::kevent {
                ident: 2887,
                filter: libc::EVFILT_USER,
                flags: libc::EV_ADD | libc::EV_CLEAR,
                fflags: 0,
                data: 0,
                udata: ptr::null_mut(),
            };
            let r = libc::kevent(fd.as_raw_fd(), &event, 1, ptr::null_mut(), 0, ptr::null());
            assert_ne!(r, -1); // TODO

            Self {
                fd,
                waker: event.ident,
            }
        }
    }

    fn register_mach_port(&self, mach_port: &MachPort) {
        unsafe {
            let event = libc::kevent {
                ident: mach_port.as_raw() as _,
                filter: libc::EVFILT_MACHPORT,
                flags: libc::EV_ADD | libc::EV_RECEIPT,
                fflags: 0,
                data: 0,
                udata: ptr::null_mut(),
            };
            let r = libc::kevent(
                self.fd.as_raw_fd(),
                &event,
                1,
                ptr::null_mut(),
                0,
                ptr::null(),
            );
            assert_ne!(r, -1); // TODO
        }
    }

    fn wait(&self, receiver: &mut Vec<KEvent>, timeout: Option<Duration>) {
        let mut tmout = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };

        if let Some(timeout) = timeout {
            tmout.tv_sec = timeout.as_secs() as _;
            tmout.tv_nsec = timeout.subsec_nanos() as _;
        }

        unsafe {
            let n = libc::kevent(
                self.fd.as_raw_fd(),
                ptr::null(),
                0,
                receiver.as_mut_ptr() as _,
                receiver.capacity() as _,
                if timeout.is_some() {
                    &tmout
                } else {
                    ptr::null()
                },
            );

            if n < 0 {
                receiver.clear();
            } else {
                receiver.set_len(n as _);
            }
        }
    }

    pub fn wake(&self) {
        unsafe {
            let event = libc::kevent {
                ident: self.waker,
                filter: libc::EVFILT_USER,
                flags: 0,
                fflags: libc::NOTE_TRIGGER,
                data: 0,
                udata: ptr::null_mut(),
            };
            let r = libc::kevent(
                self.fd.as_raw_fd(),
                &event,
                1,
                ptr::null_mut(),
                0,
                ptr::null(),
            );
            assert_ne!(r, -1); // TODO
        }
    }
}
