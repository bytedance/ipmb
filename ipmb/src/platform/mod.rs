use crate::{util, Error};
use std::{
    mem,
    ops::RangeBounds,
    slice,
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
};

#[cfg(target_os = "macos")]
pub type Object = self::macos::MachPort;
#[cfg(target_os = "windows")]
pub type Object = self::windows::Handle;
#[cfg(target_os = "linux")]
pub type Object = self::linux::Fd;

#[cfg(target_os = "linux")]
pub(crate) use self::linux::{
    look_up, page_mask, register, EncodedMessage, IoHub, IoMultiplexing, Remote,
};
#[cfg(target_os = "macos")]
pub(crate) use self::macos::{
    look_up, page_mask, register, EncodedMessage, IoHub, IoMultiplexing, Remote,
};
#[cfg(target_os = "windows")]
pub(crate) use self::windows::{
    look_up, page_mask, register, EncodedMessage, IoHub, IoMultiplexing, Remote,
};

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

pub struct MemoryRegion {
    header: MappedRegion,
    buffer_size: u64,
    buffer: Option<MappedRegion>,
    obj: Object,
}

impl Drop for MemoryRegion {
    fn drop(&mut self) {
        self.ref_count_inner(-1);
    }
}

impl Clone for MemoryRegion {
    fn clone(&self) -> Self {
        Self::from_object(self.object().clone())
    }
}

impl MemoryRegion {
    const HEADER_REFERENCE_COUNT: usize = 4;
    const HEADER_BUFFER_SIZE: usize = 8;

    const fn header_length() -> usize {
        Self::HEADER_REFERENCE_COUNT + Self::HEADER_BUFFER_SIZE
    }

    pub fn new(size: usize) -> Option<Self> {
        let real_size = Self::header_length() + size;
        let obj = Self::obj_new(real_size)?;

        unsafe {
            let header = MappedRegion::from_object(&obj, 0, Self::header_length())
                .expect("MappedRegion::from_object");

            let rc: &AtomicU32 = mem::transmute(header.as_slice().as_ptr());
            rc.store(1, Ordering::SeqCst);

            let buffer_size: &AtomicU64 =
                mem::transmute(header.as_slice()[Self::HEADER_REFERENCE_COUNT..].as_ptr());
            buffer_size.store(size as _, Ordering::SeqCst);

            Some(Self {
                header,
                buffer_size: size as _,
                buffer: None,
                obj,
            })
        }
    }

    pub fn from_object(obj: Object) -> Self {
        let mr = unsafe {
            let header = MappedRegion::from_object(&obj, 0, Self::header_length())
                .expect("MappedRegion::from_object");

            let buffer_size: &AtomicU64 =
                mem::transmute(header.as_slice()[Self::HEADER_REFERENCE_COUNT..].as_ptr());
            let buffer_size = buffer_size.load(Ordering::SeqCst);

            Self {
                header,
                buffer_size,
                buffer: None,
                obj,
            }
        };
        mr.ref_count_inner(1);

        mr
    }

    pub fn object(&self) -> &Object {
        &self.obj
    }

    pub fn map(&mut self, range: impl RangeBounds<usize>) -> Result<&mut [u8], Error> {
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
                MappedRegion::from_object(&self.obj, Self::header_length() + offset, size)?
            };
            self.buffer = Some(buffer);
        }

        Ok(self.buffer.as_mut().unwrap().as_mut())
    }

    /// Safely attempts to get a read-only reference to currently mapped data
    /// Returns `Some` only if no remapping is needed for requested range
    pub fn try_map_read(&self, range: impl RangeBounds<usize>) -> Option<&[u8]> {
        let (offset, size) = util::range_to_offset_size(range);
        let size = size.unwrap_or_else(|| {
            usize::try_from(self.buffer_size() - offset as u64).unwrap_or(usize::MAX)
        });

        if let Some(buffer) = &self.buffer {
            if offset == buffer.offset && size == buffer.as_slice().len() {
                Some(buffer.as_slice())
            } else {
                None
            }
        } else {
            None
        }
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

struct MappedRegion {
    offset: usize,
    buffer: &'static mut [u8],
}

impl Drop for MappedRegion {
    fn drop(&mut self) {
        unsafe {
            let aligned_offset = trunc_page(self.offset);
            let adjustment_for_alignment = self.offset - aligned_offset;
            Self::unmap(
                self.buffer.as_mut_ptr().sub(adjustment_for_alignment) as _,
                adjustment_for_alignment + self.buffer.len(),
            );
        }
    }
}

impl MappedRegion {
    pub(crate) unsafe fn from_object(
        obj: &Object,
        offset: usize,
        size: usize,
    ) -> Result<Self, Error> {
        let aligned_offset = trunc_page(offset);
        let adjustment_for_alignment = offset - aligned_offset;
        let addr = Self::map(obj, aligned_offset, adjustment_for_alignment + size)?;

        Ok(Self {
            offset,
            buffer: slice::from_raw_parts_mut(
                (addr as *mut u8).add(adjustment_for_alignment),
                size,
            ),
        })
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        self.buffer
    }

    fn as_mut(&mut self) -> &mut [u8] {
        self.buffer
    }
}

#[inline]
fn trunc_page(size: usize) -> usize {
    size & !(page_mask())
}
