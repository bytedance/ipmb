use super::mach_sys;
use crate::{platform::MappedRegion, Error, MemoryRegion, Object};

// https://opensource.apple.com/source/xnu/xnu-792.25.20/osfmk/mach/vm_statistics.h
const VM_FLAGS_ANYWHERE: libc::c_int = 0x0001;

// https://opensource.apple.com/source/xnu/xnu-6153.61.1/osfmk/mach/vm_prot.h
const VM_PROT_READ: mach_sys::vm_prot_t = 0x01;
const VM_PROT_WRITE: mach_sys::vm_prot_t = 0x02;
const VM_PROT_DEFAULT: mach_sys::vm_prot_t = VM_PROT_READ | VM_PROT_WRITE;

// https://opensource.apple.com/source/xnu/xnu-4570.41.2/osfmk/mach/memory_object_types.h.auto.html
const MAP_MEM_NAMED_CREATE: mach_sys::vm_prot_t = 0x020000;

// https://opensource.apple.com/source/xnu/xnu-4570.1.46/osfmk/mach/vm_inherit.h.auto.html
const VM_INHERIT_NONE: mach_sys::vm_inherit_t = 2;

impl MemoryRegion {
    pub(crate) fn obj_new(size: usize) -> Object {
        let mut port = 0;
        let mut alloc_size = size as _;
        unsafe {
            let r = mach_sys::mach_make_memory_entry_64(
                mach_sys::mach_task_self(),
                &mut alloc_size,
                0,
                MAP_MEM_NAMED_CREATE | VM_PROT_DEFAULT,
                &mut port,
                mach_sys::MACH_PORT_NULL,
            );
            assert_eq!(r, mach_sys::KERN_SUCCESS);
            let obj = Object::from_raw(port);
            assert!(alloc_size >= size as u64);
            obj
        }
    }
}

impl MappedRegion {
    pub(crate) fn map(
        obj: &Object,
        aligned_offset: usize,
        aligned_size: usize,
    ) -> Result<*mut u8, Error> {
        let mut address = 0;
        let r = unsafe {
            mach_sys::vm_map(
                mach_sys::mach_task_self(),
                &mut address,
                aligned_size,
                0,
                VM_FLAGS_ANYWHERE,
                obj.as_raw(),
                aligned_offset,
                0,
                VM_PROT_DEFAULT,
                VM_PROT_DEFAULT,
                VM_INHERIT_NONE,
            )
        };
        if r != mach_sys::KERN_SUCCESS {
            Err(Error::MemoryRegionMapping)
        } else {
            Ok(address as *mut u8)
        }
    }

    pub(crate) fn unmap(addr: *mut u8, len: usize) {
        let r = unsafe { mach_sys::vm_deallocate(mach_sys::mach_task_self(), addr as _, len) };
        assert_eq!(r, mach_sys::KERN_SUCCESS);
    }
}

pub(crate) fn page_mask() -> usize {
    unsafe { mach_sys::vm_page_mask }
}
