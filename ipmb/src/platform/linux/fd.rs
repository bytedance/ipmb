use std::{
    fmt::Debug,
    mem,
    os::fd::{self, AsRawFd, FromRawFd, IntoRawFd},
    sync::{Mutex, MutexGuard},
};

pub struct Fd(fd::OwnedFd);

impl Clone for Fd {
    fn clone(&self) -> Self {
        Self(self.0.try_clone().expect("try clone fd"))
    }
}

impl Fd {
    pub unsafe fn from_raw(raw: fd::RawFd) -> Self {
        Self(fd::OwnedFd::from_raw_fd(raw))
    }

    pub fn into_raw(self) -> fd::RawFd {
        self.0.into_raw_fd()
    }

    pub fn as_raw(&self) -> fd::RawFd {
        self.0.as_raw_fd()
    }
}

pub struct Remote {
    v: i32,
    fd: Mutex<Fd>,
}

impl Remote {
    pub fn new(fd: Fd) -> Self {
        Self {
            v: fd.as_raw(),
            fd: Mutex::new(fd),
        }
    }

    pub fn lock(&self) -> MutexGuard<Fd> {
        self.fd.lock().unwrap()
    }

    pub fn is_dead(&self) -> bool {
        let mut err: i32 = 0;
        let mut len: u32 = mem::size_of_val(&err) as _;
        let r = unsafe {
            libc::getsockopt(
                self.v,
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                &mut err as *mut _ as *mut _,
                &mut len,
            )
        };

        if r == -1 || err != 0 {
            true
        } else {
            false
        }
    }
}

impl Debug for Remote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Remote").field("v", &self.v).finish()
    }
}

impl PartialEq for Remote {
    fn eq(&self, other: &Self) -> bool {
        self.v == other.v
    }
}

pub struct Local(pub(crate) Fd);
