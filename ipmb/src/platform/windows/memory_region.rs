use super::Handle;
use crate::{util, Error};
use std::ops::RangeBounds;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Once;
use std::{mem, slice};
use windows::Win32::Foundation;
use windows::Win32::System::{Memory, SystemInformation};

const MEMORY_REGION_HEADER_REFERENCE_COUNT: usize = 4;
const MEMORY_REGION_HEADER_BUFFER_SIZE: usize = 8;
const MEMORY_REGION_HEADER_LENGTH: usize =
    MEMORY_REGION_HEADER_REFERENCE_COUNT + MEMORY_REGION_HEADER_BUFFER_SIZE;

pub struct MemoryRegion {
    handle: Handle,
    header: MappedRegion,
    buffer_size: u64,
    buffer: Option<MappedRegion>,
}

impl MemoryRegion {
    pub fn new(size: usize) -> Self {
        let real_size = MEMORY_REGION_HEADER_LENGTH + size;

        unsafe {
            let handle: Foundation::HANDLE = Memory::CreateFileMappingW(
                Foundation::INVALID_HANDLE_VALUE,
                None,
                Memory::PAGE_READWRITE,
                real_size.checked_shr(32).unwrap_or(0) as _,
                real_size as _,
                None,
            )
            .expect("CreateFileMappingW failed");
            let handle = Handle::from_raw(handle.0 as _);

            let header = MappedRegion::from_object(&handle, 0, MEMORY_REGION_HEADER_LENGTH)
                .expect("MappedRegion::from_object");

            let rc: &AtomicU32 = mem::transmute(header.as_slice().as_ptr());
            rc.store(1, Ordering::SeqCst);

            let buffer_size: &AtomicU64 = mem::transmute(header.as_slice()[4..].as_ptr());
            buffer_size.store(size as _, Ordering::SeqCst);

            Self {
                handle,
                header,
                buffer_size: size as _,
                buffer: None,
            }
        }
    }

    pub fn from_object(handle: Handle) -> Self {
        let mr = unsafe {
            let header = MappedRegion::from_object(&handle, 0, MEMORY_REGION_HEADER_LENGTH)
                .expect("MappedRegion::from_object");

            let buffer_size: &AtomicU64 = mem::transmute(header.as_slice()[4..].as_ptr());
            let buffer_size = buffer_size.load(Ordering::SeqCst);

            Self {
                handle,
                header,
                buffer_size,
                buffer: None,
            }
        };

        mr.ref_count_inner(1);

        mr
    }

    pub fn object(&self) -> &Handle {
        &self.handle
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
                MappedRegion::from_object(&self.handle, MEMORY_REGION_HEADER_LENGTH + offset, size)?
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
    unsafe fn from_object(obj: &Handle, offset: usize, size: usize) -> Result<Self, Error> {
        // https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-mapviewoffile
        let aligned_offset = trunc_page(offset);
        let adjustment_for_alignment = offset - aligned_offset;

        let view_ptr = Memory::MapViewOfFile(
            obj.as_raw_windows(),
            Memory::FILE_MAP_READ | Memory::FILE_MAP_WRITE,
            aligned_offset.checked_shr(32).unwrap_or(0) as _,
            aligned_offset as _,
            adjustment_for_alignment + size,
        )?;

        Ok(Self {
            offset,
            buffer: slice::from_raw_parts_mut(
                (view_ptr.0 as *mut u8).add(adjustment_for_alignment),
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

            let r = Memory::UnmapViewOfFile(Memory::MEMORYMAPPEDVIEW_HANDLE(
                self.buffer.as_ptr().sub(adjustment_for_alignment) as _,
            ));
            assert!(r.as_bool());
        }
    }
}

#[inline]
fn trunc_page(size: usize) -> usize {
    size & !(allocation_granularity() as usize - 1)
}

static mut ALLOCATION_GRANULARITY: u32 = 0;
static ALLOCATION_GRANULARITY_ONCE: Once = Once::new();

fn allocation_granularity() -> u32 {
    unsafe {
        ALLOCATION_GRANULARITY_ONCE.call_once(|| {
            let mut info = mem::zeroed();
            SystemInformation::GetSystemInfo(&mut info);
            ALLOCATION_GRANULARITY = info.dwAllocationGranularity;
        });
        ALLOCATION_GRANULARITY
    }
}
