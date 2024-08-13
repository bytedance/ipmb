use super::fd::{Local, Remote};
use crate::{
    decode, message, util::Align4, version, Error, MemoryRegion, Object, Selector, Version,
};
use std::{io, mem, os::fd::RawFd, ptr, slice};
use type_uuid::TypeUuid;

/// Message layout
/// | version (magic__major__minor__patch)
/// | selector_size (u32)
/// | selector
/// | selector_padding
/// | payload_size (u32)
/// | payload
/// | payload_padding
pub(crate) struct EncodedMessage {
    pub selector: crate::Selector,
    pub payload_data: &'static [u8],
    pub iov_data: Vec<u8>,
    pub control_data: Vec<u8>,
    pub objects: Vec<crate::Object>,
    pub memory_regions: Vec<crate::MemoryRegion>,
}

impl EncodedMessage {
    pub fn extract_remote(&mut self) -> Option<Remote> {
        debug_assert_eq!(
            self.selector.uuid,
            <message::ConnectMessage as TypeUuid>::UUID
        );

        let reply = self.objects.pop()?;
        Some(Remote::new(reply))
    }

    // mutable reference ensure no other references
    pub fn from_local(local: &mut Local) -> Result<Self, Error> {
        unsafe {
            // recv meta
            let mut meta = Meta::default();

            let mut iov = libc::iovec {
                iov_base: &mut meta as *mut _ as _,
                iov_len: mem::size_of_val(&meta),
            };

            let mut hdr = libc::msghdr {
                msg_name: ptr::null_mut(),
                msg_namelen: 0,
                msg_iov: &mut iov,
                msg_iovlen: 1,
                msg_control: ptr::null_mut(),
                msg_controllen: 0,
                msg_flags: 0,
            };

            let mut r = libc::recvmsg(local.0.as_raw(), &mut hdr, 0);
            if r < 1 || (hdr.msg_flags & libc::MSG_TRUNC == libc::MSG_TRUNC) {
                return Err(Error::Disconnect);
            }

            // recv payload
            let mut iov_data: Vec<u8> = super::alloc_buffer(meta.iov_len as _);
            iov.iov_base = iov_data.as_mut_ptr() as _;
            iov.iov_len = iov_data.len();

            let mut control_data: Vec<u8> = super::alloc_buffer(meta.control_len as _);
            hdr.msg_control = control_data.as_mut_ptr() as _;
            hdr.msg_controllen = control_data.len();

            hdr.msg_flags = 0;

            r = libc::recvmsg(local.0.as_raw(), &mut hdr, 0);
            if r < 1 || (hdr.msg_flags & libc::MSG_TRUNC == libc::MSG_TRUNC) {
                return Err(Error::Disconnect);
            }

            // parse
            let control_ptr = control_data.as_ptr() as *const libc::cmsghdr;
            let mut control_data_ptr = libc::CMSG_DATA(control_ptr) as *const RawFd;

            let control_count = ((*control_ptr).cmsg_len
                - (control_data_ptr as usize - control_ptr as usize))
                / mem::size_of::<RawFd>();

            let mut objects: Vec<_> = (0..control_count)
                .map(|_| {
                    let obj = Object::from_raw(ptr::read(control_data_ptr));
                    control_data_ptr = control_data_ptr.offset(1);
                    obj
                })
                .collect();

            let version_ptr = iov_data.as_ptr() as *const u32;
            let [magic, major, minor, patch]: [u8; 4] = mem::transmute(*version_ptr);
            if magic != 0xFF {
                return Err(Error::VersionMismatch(Version((0, 0, 0)), None));
            }
            let remote_version = Version((major, minor, patch));

            if !version().compatible(remote_version) {
                return Err(Error::VersionMismatch(remote_version, None));
            }

            let selector_size_ptr = version_ptr.offset(1);
            let selector_size = *selector_size_ptr;
            let selector_ptr = selector_size_ptr.offset(1) as *const u8;

            // TODO: Check size
            let selector: Selector =
                decode(slice::from_raw_parts(selector_ptr, selector_size as _))?;

            let memory_regions: Vec<_> = objects
                .drain((objects.len() - selector.memory_region_count as usize)..)
                .map(|obj| {
                    let r = MemoryRegion::from_object(obj);
                    r.ref_count_inner(-1);
                    r
                })
                .collect();

            let payload_size_ptr = selector_ptr.offset(selector_size.align4() as _) as *const u32;

            Ok(Self {
                selector,
                payload_data: slice::from_raw_parts(
                    payload_size_ptr.offset(1) as *const u8,
                    *payload_size_ptr as _,
                ),
                iov_data,
                control_data,
                objects,
                memory_regions,
            })
        }
    }

    pub fn send(&mut self, remote: &Remote) -> Result<(), Error> {
        // send meta
        let mut meta = Meta {
            iov_len: self.iov_data.len() as u32,
            control_len: self.control_data.len() as u32,
        };

        let mut iov = libc::iovec {
            iov_base: &mut meta as *mut _ as _,
            iov_len: mem::size_of_val(&meta),
        };
        let mut hdr = libc::msghdr {
            msg_name: ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov,
            msg_iovlen: 1,
            msg_control: ptr::null_mut(),
            msg_controllen: 0,
            msg_flags: 0,
        };

        let remote_guard = remote.lock();

        let mut r = unsafe { libc::sendmsg(remote_guard.as_raw(), &hdr, 0) };
        if r == -1 {
            return Err(Error::Disconnect);
        }

        // send payload
        iov.iov_base = self.iov_data.as_mut_ptr() as _;
        iov.iov_len = self.iov_data.len();

        hdr.msg_control = self.control_data.as_mut_ptr() as _;
        hdr.msg_controllen = self.control_data.len();

        // Add memory region's ref count
        for r in self.memory_regions.iter() {
            r.ref_count_inner(1);
        }

        r = unsafe { libc::sendmsg(remote_guard.as_raw(), &hdr, 0) };
        if r == -1 {
            for r in self.memory_regions.iter() {
                r.ref_count_inner(-1);
            }
            log::error!("sendmsg: {}", io::Error::last_os_error());
            Err(Error::Disconnect)
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
#[repr(C)]
struct Meta {
    iov_len: u32,
    control_len: u32,
}
