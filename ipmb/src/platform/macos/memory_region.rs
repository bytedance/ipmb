use super::{mach_sys, MachPort};
use crate::{util, Error};
use std::{
    mem,
    ops::RangeBounds,
    slice,
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
};

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

const MEMORY_REGION_HEADER_REFERENCE_COUNT: usize = 4;
const MEMORY_REGION_HEADER_BUFFER_SIZE: usize = 8;
const MEMORY_REGION_HEADER_LENGTH: usize =
    MEMORY_REGION_HEADER_REFERENCE_COUNT + MEMORY_REGION_HEADER_BUFFER_SIZE;

pub struct MemoryRegion {
    port: MachPort,
    header: MappedRegion,
    buffer_size: u64,
    buffer: Option<MappedRegion>,
}

impl MemoryRegion {
    pub fn new(size: usize) -> Self {
        let real_size = MEMORY_REGION_HEADER_LENGTH + size;

        unsafe {
            let mut port = 0;
            let mut alloc_size = real_size as _;
            let r = mach_sys::mach_make_memory_entry_64(
                mach_sys::mach_task_self(),
                &mut alloc_size,
                0,
                MAP_MEM_NAMED_CREATE | VM_PROT_DEFAULT,
                &mut port,
                mach_sys::MACH_PORT_NULL,
            );
            assert_eq!(r, mach_sys::KERN_SUCCESS);
            let port = MachPort::from_raw(port);

            assert!(alloc_size >= real_size as u64);

            let header = MappedRegion::from_object(&port, 0, MEMORY_REGION_HEADER_LENGTH)
                .expect("MappedRegion::from_object");

            let rc: &AtomicU32 = mem::transmute(header.as_slice().as_ptr());
            rc.store(1, Ordering::SeqCst);

            let buffer_size: &AtomicU64 = mem::transmute(header.as_slice()[4..].as_ptr());
            buffer_size.store(size as _, Ordering::SeqCst);

            Self {
                port,
                header,
                buffer_size: size as _,
                buffer: None,
            }
        }
    }

    pub fn from_object(port: MachPort) -> Self {
        let mr = unsafe {
            let header = MappedRegion::from_object(&port, 0, MEMORY_REGION_HEADER_LENGTH)
                .expect("MappedRegion::from_object");

            let buffer_size: &AtomicU64 = mem::transmute(header.as_slice()[4..].as_ptr());
            let buffer_size = buffer_size.load(Ordering::SeqCst);

            Self {
                port,
                header,
                buffer_size,
                buffer: None,
            }
        };

        mr.ref_count_inner(1);

        mr
    }

    pub fn object(&self) -> &MachPort {
        &self.port
    }

    pub fn map<S: RangeBounds<usize>>(&mut self, range: S) -> Result<&mut [u8], Error> {
        let (offset, size) = util::range_to_offset_size(range);
        let size = size.unwrap_or_else(|| {
            usize::try_from(self.buffer_size() - offset as u64).unwrap_or(usize::MAX)
        });

        let need_remap = if let Some(buffer) = &self.buffer {
            if offset != buffer.offset || size != buffer.as_slice().len() {
                true
            } else {
                false
            }
        } else {
            true
        };

        if need_remap {
            let buffer = unsafe {
                MappedRegion::from_object(&self.port, MEMORY_REGION_HEADER_LENGTH + offset, size)?
            };
            self.buffer = Some(buffer);
        }

        Ok(self.buffer.as_mut().unwrap().as_mut())
    }

    pub(crate) fn ref_count_inner(&self, val: i32) -> u32 {
        let rc: &AtomicU32 = unsafe { mem::transmute(self.header.as_slice().as_ptr()) };

        if val == 0 {
            rc.load(Ordering::SeqCst)
        } else if val > 0 {
            rc.fetch_add(val as _, Ordering::SeqCst)
        } else {
            rc.fetch_sub(-val as _, Ordering::SeqCst)
        }
    }

    pub fn ref_count(&self) -> u32 {
        self.ref_count_inner(0)
    }

    pub fn buffer_size(&self) -> u64 {
        self.buffer_size
    }
}

impl Drop for MemoryRegion {
    fn drop(&mut self) {
        self.ref_count_inner(-1);
    }
}

struct MappedRegion {
    offset: usize,
    buffer: &'static mut [u8],
}

impl MappedRegion {
    unsafe fn from_object(obj: &MachPort, offset: usize, size: usize) -> Result<Self, Error> {
        // https://opensource.apple.com/source/xnu/xnu-792/osfmk/vm/vm_user.c.auto.html
        let aligned_offset = trunc_page(offset);
        let adjustment_for_alignment = offset - aligned_offset;

        let mut address = 0;
        let r = mach_sys::vm_map(
            mach_sys::mach_task_self(),
            &mut address,
            adjustment_for_alignment + size,
            0,
            VM_FLAGS_ANYWHERE,
            obj.as_raw(),
            aligned_offset,
            0,
            VM_PROT_DEFAULT,
            VM_PROT_DEFAULT,
            VM_INHERIT_NONE,
        );
        if r != mach_sys::KERN_SUCCESS {
            return Err(Error::MemoryRegionMapping);
        }

        Ok(Self {
            offset,
            buffer: slice::from_raw_parts_mut(
                (address as *mut u8).add(adjustment_for_alignment),
                size,
            ),
        })
    }

    fn as_slice(&self) -> &[u8] {
        self.buffer
    }

    fn as_mut(&mut self) -> &mut [u8] {
        self.buffer
    }
}

impl Drop for MappedRegion {
    fn drop(&mut self) {
        unsafe {
            let aligned_offset = trunc_page(self.offset);
            let adjustment_for_alignment = self.offset - aligned_offset;

            let r = mach_sys::vm_deallocate(
                mach_sys::mach_task_self(),
                self.buffer.as_mut_ptr().sub(adjustment_for_alignment) as _,
                adjustment_for_alignment + self.buffer.len(),
            );
            assert_eq!(r, mach_sys::KERN_SUCCESS);
        }
    }
}

#[inline]
fn trunc_page(size: usize) -> usize {
    unsafe { size & !mach_sys::vm_page_mask }
}
