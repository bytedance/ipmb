use rand::Rng;
use serde::{Deserialize, Serialize};
use std::ops::{Bound, RangeBounds};
use type_uuid::Bytes;

#[allow(dead_code)]
pub fn vec_take<T, F: FnMut(&mut T) -> bool>(source: &mut Vec<T>, receiver: &mut Vec<T>, mut f: F) {
    let mut i = 0;
    while i < source.len() {
        if f(&mut source[i]) {
            let val = source.swap_remove(i);
            receiver.push(val);
        } else {
            i += 1;
        }
    }
}

pub trait Align4 {
    fn align4(self) -> Self;
}

impl Align4 for usize {
    #[inline]
    fn align4(mut self) -> Self {
        if (self & 0x3) != 0 {
            self = (self & !0x3) + 4;
        }
        self
    }
}

impl Align4 for u32 {
    #[inline]
    fn align4(mut self) -> Self {
        if (self & 0x3) != 0 {
            self = (self & !0x3) + 4;
        }
        self
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct EndpointID(Bytes);

impl EndpointID {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().into_bytes())
    }
}

#[allow(dead_code)]
pub fn rand_string(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

pub fn range_to_offset_size<S: RangeBounds<usize>>(bounds: S) -> (usize, Option<usize>) {
    let offset = match bounds.start_bound() {
        Bound::Included(&bound) => bound,
        Bound::Excluded(&bound) => bound + 1,
        Bound::Unbounded => 0,
    };
    let size = match bounds.end_bound() {
        Bound::Included(&bound) => Some(bound + 1 - offset),
        Bound::Excluded(&bound) => Some(bound - offset),
        Bound::Unbounded => None,
    };

    (offset, size)
}
