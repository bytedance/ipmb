use crate::{platform::MappedRegion, Error, MemoryRegion, Object};
use std::{mem, sync::Once};
use windows::Win32::{
    Foundation,
    System::{Memory, SystemInformation},
};

impl MemoryRegion {
    pub(crate) fn obj_new(size: usize) -> Object {
        unsafe {
            let handle: Foundation::HANDLE = Memory::CreateFileMappingW(
                Foundation::INVALID_HANDLE_VALUE,
                None,
                Memory::PAGE_READWRITE,
                size.checked_shr(32).unwrap_or(0) as _,
                size as _,
                None,
            )
            .expect("CreateFileMappingW failed");
            let handle = Object::from_raw(handle.0 as _);
            handle
        }
    }
}

impl MappedRegion {
    pub(crate) fn map(
        obj: &Object,
        aligned_offset: usize,
        aligned_size: usize,
    ) -> Result<*mut u8, Error> {
        unsafe {
            let view_ptr = Memory::MapViewOfFile(
                obj.as_raw_windows(),
                Memory::FILE_MAP_READ | Memory::FILE_MAP_WRITE,
                aligned_offset.checked_shr(32).unwrap_or(0) as _,
                aligned_offset as _,
                aligned_size,
            )?;
            Ok(view_ptr.0 as *mut u8)
        }
    }

    pub(crate) fn unmap(addr: *mut u8, len: usize) {
        let _ = len;
        unsafe {
            let r = Memory::UnmapViewOfFile(Memory::MEMORYMAPPEDVIEW_HANDLE(addr as _));
            assert!(r.as_bool());
        }
    }
}

static mut PAGE_MASK: usize = 0;
static PAGE_MASK_ONCE: Once = Once::new();
pub(crate) fn page_mask() -> usize {
    unsafe {
        PAGE_MASK_ONCE.call_once(|| {
            let mut info = mem::zeroed();
            SystemInformation::GetSystemInfo(&mut info);
            PAGE_MASK = info.dwAllocationGranularity as usize - 1;
        });
        PAGE_MASK
    }
}
