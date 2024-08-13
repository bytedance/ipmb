use super::Fd;
use std::{ffi, mem, time::Duration};

pub struct IoMultiplexing {
    fd: Fd,
    pub(crate) waker_fd: Fd,
}

impl IoMultiplexing {
    pub fn new() -> Self {
        let fd = unsafe {
            let fd = libc::epoll_create1(0);
            assert_ne!(fd, -1);
            Fd::from_raw(fd)
        };

        let waker_fd = unsafe {
            let efd = libc::eventfd(0, 0);
            assert_ne!(efd, -1);
            Fd::from_raw(efd)
        };

        let im = Self { fd, waker_fd };

        im.register(&im.waker_fd);

        im
    }

    pub(crate) fn register(&self, fd: &Fd) {
        unsafe {
            let mut ev = libc::epoll_event {
                events: libc::EPOLLIN as _,
                u64: fd.as_raw() as _,
            };
            let r = libc::epoll_ctl(self.fd.as_raw(), libc::EPOLL_CTL_ADD, fd.as_raw(), &mut ev);
            assert_ne!(r, -1);
        }
    }

    pub(crate) fn wait(&self, events: &mut Vec<libc::epoll_event>, timeout: Option<Duration>) {
        unsafe {
            let n = libc::epoll_wait(
                self.fd.as_raw(),
                events.as_mut_ptr(),
                events.capacity() as _,
                timeout
                    .map(|tot| tot.as_millis() as ffi::c_int)
                    .unwrap_or(-1),
            );
            if n < 1 {
                events.clear();
            } else {
                events.set_len(n as _);
            }
        }
    }

    pub fn wake(&self) {
        unsafe {
            let u: u64 = 1;
            let r = libc::write(
                self.waker_fd.as_raw(),
                &u as *const _ as _,
                mem::size_of_val(&u),
            );
            assert_ne!(r, -1);
        }
    }

    pub(crate) fn clear_waker(&self) {
        unsafe {
            let mut u: u64 = 0;
            libc::read(
                self.waker_fd.as_raw(),
                &mut u as *mut _ as _,
                mem::size_of_val(&u),
            );
        }
    }
}
