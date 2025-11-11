use crate::{
    message, platform, util::Align4, version, EndpointID, Error, Label, LabelOp, MemoryRegion,
    Message, MessageBox, Object, Selector,
};
pub(crate) use encoded_message::EncodedMessage;
use fd::Local;
pub use fd::{Fd, Remote};
pub(crate) use io_mul::IoMultiplexing;
use std::{
    ffi, io, mem,
    os::fd::RawFd,
    ptr, slice,
    sync::{mpsc, Arc, Once},
    time::Duration,
};

mod encoded_message;
mod fd;
mod io_mul;

static MAXIMUM_BUF_SIZE: i32 = 64 << 10;

impl MemoryRegion {
    pub(crate) fn obj_new(size: usize) -> Option<Object> {
        unsafe {
            let fd = libc::memfd_create(c"ipmb".as_ptr(), libc::MFD_CLOEXEC);
            if fd == -1 {
                return None;
            }
            let fd = Object::from_raw(fd);

            let r = libc::ftruncate(fd.as_raw(), size as _);
            if r == -1 {
                return None;
            }
            Some(fd)
        }
    }
}

impl platform::MappedRegion {
    pub(crate) fn map(
        obj: &Object,
        aligned_offset: usize,
        aligend_size: usize,
    ) -> Result<*mut u8, Error> {
        let addr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                aligend_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                obj.as_raw(),
                aligned_offset as _,
            )
        };
        if addr == libc::MAP_FAILED {
            Err(Error::MemoryRegionMapping)
        } else {
            Ok(addr as _)
        }
    }

    pub(crate) fn unmap(addr: *mut u8, len: usize) {
        let r = unsafe { libc::munmap(addr as _, len) };
        assert_ne!(r, -1);
    }
}

static mut PAGE_MASK: usize = 0;
static PAGE_MASK_ONCE: Once = Once::new();
pub(crate) fn page_mask() -> usize {
    unsafe {
        PAGE_MASK_ONCE.call_once(|| {
            PAGE_MASK = libc::sysconf(libc::_SC_PAGESIZE) as usize - 1;
        });
        PAGE_MASK
    }
}

// we use abstract socket address
fn identifier_to_socket_addr(identifier: &str) -> libc::sockaddr_un {
    let identifier = ffi::CString::new(identifier).unwrap();
    let mut addr = libc::sockaddr_un {
        sun_family: libc::AF_UNIX as _,
        sun_path: [0; 108],
    };
    unsafe {
        libc::strncpy(
            addr.sun_path[1..].as_mut_ptr(),
            identifier.as_ptr() as _,
            addr.sun_path.len() - 2,
        );
    }
    addr
}

pub(crate) fn look_up(
    identifier: &str,
    label: Label,
    token: String,
    im: Arc<IoMultiplexing>,
) -> Result<(IoHub, Remote, EndpointID), Error> {
    unsafe {
        let fd = libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0);
        if fd == -1 {
            return Err(Error::IoError(io::Error::last_os_error()));
        }
        let fd = Fd::from_raw(fd);

        let addr = identifier_to_socket_addr(identifier);

        let mut r = libc::connect(
            fd.as_raw(),
            &addr as *const _ as _,
            mem::size_of_val(&addr) as _,
        );
        if r == -1 {
            let err = io::Error::last_os_error();

            return Err(match err.kind() {
                io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound => {
                    Error::IdentifierNotInUse
                }
                io::ErrorKind::PermissionDenied => Error::PermissonDenied,
                _ => Error::IoError(err),
            });
        }
        let _ = libc::setsockopt(
            fd.as_raw(),
            libc::SOL_SOCKET,
            libc::SO_SNDBUF,
            &MAXIMUM_BUF_SIZE as *const _ as _,
            mem::size_of_val(&MAXIMUM_BUF_SIZE) as _,
        );
        let remote = Remote::new(fd);

        let mut pair = [0, 0];
        r = libc::socketpair(
            libc::AF_UNIX,
            libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
            0,
            pair.as_mut_ptr(),
        );
        if r == -1 {
            return Err(Error::IoError(io::Error::last_os_error()));
        }

        let read_fd = Fd::from_raw(pair[0]);
        let write_fd = Fd::from_raw(pair[1]);

        let _ = libc::shutdown(read_fd.as_raw(), libc::SHUT_WR);
        let _ = libc::shutdown(write_fd.as_raw(), libc::SHUT_RD);

        let _ = libc::setsockopt(
            read_fd.as_raw(),
            libc::SOL_SOCKET,
            libc::SO_RCVBUF,
            &MAXIMUM_BUF_SIZE as *const _ as _,
            mem::size_of_val(&MAXIMUM_BUF_SIZE) as _,
        );
        let _ = libc::setsockopt(
            write_fd.as_raw(),
            libc::SOL_SOCKET,
            libc::SO_SNDBUF,
            &MAXIMUM_BUF_SIZE as *const _ as _,
            mem::size_of_val(&MAXIMUM_BUF_SIZE) as _,
        );

        /*
        let mut read_flags = libc::fcntl(read_pipe.as_raw(), libc::F_GETFL, 0);
        if read_flags == -1 {
            return Err(Error::IoError(io::Error::last_os_error()));
        }

        read_flags |= libc::O_NONBLOCK;
        if libc::fcntl(read_pipe.as_raw(), libc::F_SETFL, read_flags) == -1 {
            return Err(Error::IoError(io::Error::last_os_error()));
        }
        */

        let mut msg = Message::new(
            Selector::unicast(LabelOp::True),
            message::ConnectMessage {
                version: version(),
                token,
                label,
            },
        );
        msg.objects.push(write_fd);

        let mut encoded_msg = msg.into_encoded();
        encoded_msg.send(&remote)?;

        let mut io_hub: IoHub = IoHub::for_endpoint(Local(read_fd), im);
        // Wait ack
        let encoded_msg = io_hub.recv(Some(Duration::from_secs(2)), Some(&remote))?;
        let ack = message::ConnectMessageAck::decode(
            encoded_msg.selector.uuid,
            encoded_msg.payload_data,
        )?;

        match ack {
            message::ConnectMessageAck::Ok(endpoint_id) => Ok((io_hub, remote, endpoint_id)),
            message::ConnectMessageAck::ErrVersion(v) => Err(Error::VersionMismatch(v, None)),
            message::ConnectMessageAck::ErrToken => Err(Error::TokenMismatch),
        }
    }
}

pub(crate) fn register(
    identifier: &str,
    im: Arc<IoMultiplexing>,
) -> Result<(IoHub, mpsc::Sender<EncodedMessage>, EndpointID), Error> {
    unsafe {
        let fd = libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0);
        if fd == -1 {
            return Err(Error::IoError(io::Error::last_os_error()));
        }
        let fd = Fd::from_raw(fd);

        let addr = identifier_to_socket_addr(identifier);

        let mut r = libc::bind(
            fd.as_raw(),
            &addr as *const _ as _,
            mem::size_of_val(&addr) as _,
        );
        if r == -1 {
            let err = io::Error::last_os_error();

            return Err(match err.kind() {
                io::ErrorKind::AddrInUse => Error::IdentifierInUse,
                io::ErrorKind::PermissionDenied => Error::PermissonDenied,
                _ => Error::IoError(err),
            });
        }

        r = libc::listen(fd.as_raw(), 32);
        if r == -1 {
            return Err(Error::IoError(io::Error::last_os_error()));
        }

        let (bus_tx, bus_rx) = mpsc::channel();
        Ok((
            IoHub::for_bus_controller(fd, bus_rx, im),
            bus_tx,
            EndpointID::new(),
        ))
    }
}

pub(crate) struct IoHub {
    bus_rx: Option<mpsc::Receiver<EncodedMessage>>,
    local_list: Vec<Local>,
    listener: Option<Fd>,
    in_buffer: Vec<libc::epoll_event>,
    im: Arc<IoMultiplexing>,
}

impl IoHub {
    fn for_bus_controller(
        listener: Fd,
        bus_rx: mpsc::Receiver<EncodedMessage>,
        im: Arc<IoMultiplexing>,
    ) -> Self {
        im.register(&listener);

        Self {
            bus_rx: Some(bus_rx),
            local_list: vec![],
            listener: Some(listener),
            in_buffer: Vec::with_capacity(2),
            im,
        }
    }

    fn for_endpoint(local: Local, im: Arc<IoMultiplexing>) -> Self {
        im.register(&local.0);

        Self {
            bus_rx: None,
            local_list: vec![local],
            listener: None,
            in_buffer: Vec::with_capacity(2),
            im,
        }
    }

    pub fn recv(
        &mut self,
        timeout: Option<Duration>,
        remote: Option<&Remote>,
    ) -> Result<EncodedMessage, Error> {
        let _ = remote;

        'ret: loop {
            if let Some(ref rx) = self.bus_rx {
                match rx.try_recv() {
                    Ok(message) => {
                        break Ok(message);
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.bus_rx = None;
                    }
                }
            }

            self.im.wait(&mut self.in_buffer, timeout);

            if self.in_buffer.is_empty() {
                break Err(Error::Timeout);
            }

            for ev in self.in_buffer.drain(..) {
                if ev.u64 == self.im.waker_fd.as_raw() as u64 {
                    self.im.clear_waker();
                    continue;
                }
                if let Some(ref listener) = self.listener {
                    if ev.u64 == listener.as_raw() as u64 {
                        unsafe {
                            let fd =
                                libc::accept(listener.as_raw(), ptr::null_mut(), ptr::null_mut());
                            if fd != -1 {
                                let local = Local(Fd::from_raw(fd));
                                let _ = libc::setsockopt(
                                    local.0.as_raw(),
                                    libc::SOL_SOCKET,
                                    libc::SO_RCVBUF,
                                    &MAXIMUM_BUF_SIZE as *const _ as _,
                                    mem::size_of_val(&MAXIMUM_BUF_SIZE) as _,
                                );
                                self.im.register(&local.0);
                                self.local_list.push(local);
                            }
                        }
                        continue;
                    }
                }
                if let Some((i, local)) = self
                    .local_list
                    .iter_mut()
                    .enumerate()
                    .find(|(_, p)| ev.u64 == p.0.as_raw() as u64)
                {
                    let r = EncodedMessage::from_local(local);
                    if r.is_err() {
                        self.local_list.swap_remove(i);
                    }
                    break 'ret r;
                }
            }
        }
    }

    pub fn io_multiplexing(&self) -> Arc<IoMultiplexing> {
        self.im.clone()
    }
}

impl<T: MessageBox> message::Message<T> {
    fn encode_inner(&self) -> (&'static [u8], Vec<u8>, Vec<u8>) {
        // iov data
        let mut size = 4 // version 
            + 4 // selector size
            + 4 // payload size
            ;
        let selector_data =
            bincode::serde::encode_to_vec(&self.selector, bincode::config::standard()).unwrap();
        size += selector_data.len().align4();

        let payload_bytes = self.payload.encode().unwrap();
        size += payload_bytes.len().align4();

        let mut iov_data: Vec<u8> = alloc_buffer::<u32>(size);
        let payload_data = unsafe {
            let version_ptr = iov_data.as_mut_ptr() as *mut u32;
            let v = version();
            ptr::write(
                version_ptr,
                u32::from_ne_bytes([0xFF, v.major(), v.minor(), v.patch()]),
            );

            let selector_size_ptr = version_ptr.offset(1);
            ptr::write(selector_size_ptr, selector_data.len() as _);

            let selector_ptr = selector_size_ptr.offset(1) as *mut u8;
            ptr::copy_nonoverlapping(selector_data.as_ptr(), selector_ptr, selector_data.len());

            let payload_len_ptr =
                selector_ptr.offset(selector_data.len().align4() as _) as *mut u32;
            ptr::write(payload_len_ptr, payload_bytes.len() as _);

            let payload_ptr = payload_len_ptr.offset(1) as *mut u8;
            ptr::copy_nonoverlapping(payload_bytes.as_ptr(), payload_ptr, payload_bytes.len());

            slice::from_raw_parts(payload_ptr, payload_bytes.len())
        };

        // control data
        let control_len: u32 =
            ((self.objects.len() + self.memory_regions.len()) * mem::size_of::<RawFd>()) as _;
        size = unsafe { libc::CMSG_SPACE(control_len) } as _;
        let mut control_data: Vec<u8> = alloc_buffer::<usize>(size);
        unsafe {
            let control_ptr = control_data.as_mut_ptr() as *mut libc::cmsghdr;
            let control_ref = &mut *control_ptr;
            control_ref.cmsg_len = libc::CMSG_LEN(control_len) as _;
            control_ref.cmsg_level = libc::SOL_SOCKET;
            control_ref.cmsg_type = libc::SCM_RIGHTS;

            let mut control_data_ptr = libc::CMSG_DATA(control_ptr) as *mut RawFd;
            for object in self
                .objects
                .iter()
                .chain(self.memory_regions.iter().map(|region| region.object()))
            {
                ptr::write(control_data_ptr, object.as_raw());
                control_data_ptr = control_data_ptr.offset(1);
            }
        }

        (payload_data, iov_data, control_data)
    }

    pub(crate) fn into_encoded(self) -> EncodedMessage {
        let (payload_data, iov_data, control_data) = self.encode_inner();

        EncodedMessage {
            selector: self.selector,
            payload_data,
            iov_data,
            control_data,
            objects: self.objects,
            memory_regions: self.memory_regions,
        }
    }
}

fn alloc_buffer<T>(size: usize) -> Vec<u8> {
    let mut buffer_t_size = size / mem::size_of::<T>();
    if size % mem::size_of::<T>() != 0 {
        buffer_t_size += 1;
    }
    let mut buf_t = Vec::<T>::with_capacity(buffer_t_size);

    let buffer_size = buffer_t_size * mem::size_of::<T>();
    let buf =
        unsafe { Vec::from_raw_parts(buf_t.as_mut_ptr().cast::<u8>(), buffer_size, buffer_size) };

    mem::forget(buf_t);

    buf
}
