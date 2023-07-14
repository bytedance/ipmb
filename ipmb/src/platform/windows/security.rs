use crate::Error;
use std::{mem, ptr};
use windows::Win32::Foundation;
use windows::Win32::Security::{self, Authorization};
use windows::Win32::Storage::FileSystem;
use windows::Win32::System::{Memory, SystemServices};

const SECURITY_DESCRIPTOR_MIN_LENGTH: usize = 64;

pub struct SecurityAttr {
    raw: Security::SECURITY_ATTRIBUTES,
    _sd: SecurityDescriptor,
    _acl: Acl,
    _sid: Sid,
}

unsafe impl Send for SecurityAttr {}

unsafe impl Sync for SecurityAttr {}

impl SecurityAttr {
    pub fn allow_everyone() -> Result<Self, Error> {
        unsafe {
            let sid = Sid::everyone()?;

            let mut ea = Authorization::EXPLICIT_ACCESS_W::default();
            ea.grfAccessPermissions = FileSystem::FILE_ALL_ACCESS.0;
            ea.grfAccessMode = Authorization::SET_ACCESS;
            ea.grfInheritance = Security::NO_INHERITANCE;
            ea.Trustee.TrusteeForm = Authorization::TRUSTEE_IS_SID;
            ea.Trustee.TrusteeType = Authorization::TRUSTEE_IS_WELL_KNOWN_GROUP;
            ea.Trustee.ptstrName = windows::core::PWSTR::from_raw(sid.raw.0 as _);

            let acl = Acl::with_ea(ea)?;

            let sd = SecurityDescriptor::new()?;

            if !Security::SetSecurityDescriptorDacl(sd.raw, true, Some(acl.raw), false).as_bool() {
                return Err(Error::WinError(windows::core::Error::from_win32()));
            }

            Ok(Self {
                raw: Security::SECURITY_ATTRIBUTES {
                    nLength: mem::size_of::<Security::SECURITY_ATTRIBUTES>() as _,
                    lpSecurityDescriptor: sd.raw.0,
                    bInheritHandle: false.into(),
                },
                _sd: sd,
                _acl: acl,
                _sid: sid,
            })
        }
    }

    pub fn attr(&self) -> &Security::SECURITY_ATTRIBUTES {
        &self.raw
    }
}

struct Sid {
    raw: Foundation::PSID,
}

impl Sid {
    fn everyone() -> Result<Self, Error> {
        unsafe {
            let mut every_sid = Foundation::PSID::default();
            if !Security::AllocateAndInitializeSid(
                &Security::SECURITY_WORLD_SID_AUTHORITY,
                1,
                SystemServices::SECURITY_WORLD_RID as _,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                &mut every_sid,
            )
            .as_bool()
            {
                Err(Error::WinError(windows::core::Error::from_win32()))
            } else {
                Ok(Self { raw: every_sid })
            }
        }
    }
}

impl Drop for Sid {
    fn drop(&mut self) {
        unsafe {
            Security::FreeSid(self.raw);
        }
    }
}

struct Acl {
    raw: *mut Security::ACL,
}

impl Acl {
    fn with_ea(ea: Authorization::EXPLICIT_ACCESS_W) -> Result<Self, Error> {
        unsafe {
            let mut acl: *mut Security::ACL = ptr::null_mut();
            if Authorization::SetEntriesInAclW(Some(&[ea]), None, &mut acl) != Foundation::NO_ERROR
            {
                Err(Error::WinError(windows::core::Error::from_win32()))
            } else {
                Ok(Self { raw: acl })
            }
        }
    }
}

impl Drop for Acl {
    fn drop(&mut self) {
        unsafe {
            // TODO:
            let _ = Memory::LocalFree(Foundation::HLOCAL(self.raw as isize));
        }
    }
}

struct SecurityDescriptor {
    raw: Security::PSECURITY_DESCRIPTOR,
}

impl SecurityDescriptor {
    fn new() -> Result<Self, Error> {
        unsafe {
            let local = Memory::LocalAlloc(Memory::LPTR, SECURITY_DESCRIPTOR_MIN_LENGTH)?;
            let desc = Self {
                raw: Security::PSECURITY_DESCRIPTOR(local.0 as _),
            };

            if !Security::InitializeSecurityDescriptor(
                desc.raw,
                SystemServices::SECURITY_DESCRIPTOR_REVISION,
            )
            .as_bool()
            {
                return Err(Error::WinError(windows::core::Error::from_win32()));
            }

            Ok(desc)
        }
    }
}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        unsafe {
            // TODO:
            let _ = Memory::LocalFree(Foundation::HLOCAL(self.raw.0 as isize));
        }
    }
}
