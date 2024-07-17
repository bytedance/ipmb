use std::{
    ffi::{c_void, CStr},
    mem,
    ops::{Deref, DerefMut},
    os::raw::c_char,
    ptr, slice,
    time::Duration,
};

type ErrorCode = i32;

pub const ERROR_CODE_SUCCESS: ErrorCode = 0;
pub const ERROR_CODE_UNKNOWN: ErrorCode = -1;
pub const ERROR_CODE_TIMEOUT: ErrorCode = -2;
pub const ERROR_CODE_DECODE: ErrorCode = -3;
pub const ERROR_CODE_VERSION_MISMATCH: ErrorCode = -4;
pub const ERROR_CODE_TOKEN_MISMATCH: ErrorCode = -5;
pub const ERROR_CODE_PERMISSION_DENIED: ErrorCode = -6;

pub const TIMEOUT_INFINITE: u32 = !0u32;

macro_rules! opaque_type {
    ($ffi: ident => $rust: ty) => {
        impl Drop for $ffi {
            fn drop(&mut self) {
                unsafe {
                    let _ = Box::from_raw(self.0 as *mut $rust);
                }
            }
        }

        impl Deref for $ffi {
            type Target = $rust;

            fn deref(&self) -> &Self::Target {
                unsafe { &*(self.0 as *mut $rust) }
            }
        }

        impl DerefMut for $ffi {
            fn deref_mut(&mut self) -> &mut Self::Target {
                unsafe { &mut *(self.0 as *mut $rust) }
            }
        }

        impl From<$rust> for $ffi {
            fn from(v: $rust) -> Self {
                $ffi(Box::into_raw(Box::new(v)) as _)
            }
        }

        impl From<$ffi> for $rust {
            fn from(v: $ffi) -> Self {
                unsafe {
                    let r = *Box::from_raw(v.0 as *mut $rust);
                    mem::forget(v);
                    r
                }
            }
        }
    };
}

#[repr(transparent)]
pub struct RString(*mut c_void);
opaque_type!(RString => String);

#[no_mangle]
pub unsafe extern "C" fn ipmb_rstring_data(
    rstring: &RString,
    ptr: &mut *const c_char,
    size: &mut usize,
) {
    *ptr = rstring.as_ptr() as _;
    *size = rstring.len();
}

#[no_mangle]
pub unsafe extern "C" fn ipmb_rstring_drop(rstring: RString) {
    let _ = rstring;
}

/// Get version
#[no_mangle]
pub unsafe extern "C" fn ipmb_version(major: *mut u8, minor: *mut u8, patch: *mut u8) {
    let v = ipmb::version();

    if !major.is_null() {
        *major = v.major();
    }
    if !minor.is_null() {
        *minor = v.minor();
    }
    if !patch.is_null() {
        *patch = v.patch();
    }
}

#[no_mangle]
pub unsafe extern "C" fn ipmb_version_pre() -> RString {
    ipmb::version_pre().into()
}

/// Join bus
#[no_mangle]
pub unsafe extern "C" fn ipmb_join(
    options: Options,
    timeout: u32,
    p_sender: *mut Sender,
    p_receiver: *mut Receiver,
) -> ErrorCode {
    let identifier = match CStr::from_ptr(options.identifier).to_str() {
        Ok(identifier) => identifier.to_owned(),
        Err(_) => return ERROR_CODE_UNKNOWN,
    };
    let token = match CStr::from_ptr(options.token).to_str() {
        Ok(token) => token.to_owned(),
        Err(_) => return ERROR_CODE_UNKNOWN,
    };

    match ipmb::join::<ipmb::BytesMessage, ipmb::BytesMessage>(
        ipmb::Options {
            identifier,
            label: (*options.label).clone(),
            token,
            controller_affinity: options.controller_affinity,
        },
        if timeout == TIMEOUT_INFINITE {
            None
        } else {
            Some(Duration::from_millis(timeout as _))
        },
    ) {
        Ok((sender, receiver)) => {
            if !p_sender.is_null() {
                ptr::write(p_sender, sender.into());
            }
            if !p_receiver.is_null() {
                ptr::write(p_receiver, receiver.into());
            }
            ERROR_CODE_SUCCESS
        }
        Err(ipmb::JoinError::VersionMismatch(_)) => ERROR_CODE_VERSION_MISMATCH,
        Err(ipmb::JoinError::TokenMismatch) => ERROR_CODE_TOKEN_MISMATCH,
        Err(ipmb::JoinError::PermissonDenied) => ERROR_CODE_PERMISSION_DENIED,
        Err(ipmb::JoinError::Timeout) => ERROR_CODE_TIMEOUT,
    }
}

/// Sender
#[repr(transparent)]
pub struct Sender(*mut c_void);
opaque_type!(Sender => ipmb::EndpointSender<ipmb::BytesMessage>);

#[allow(unused_variables)]
#[no_mangle]
pub unsafe extern "C" fn ipmb_sender_drop(sender: Sender) {}

#[no_mangle]
pub unsafe extern "C" fn ipmb_send(sender: &mut Sender, message: Message) -> ErrorCode {
    match sender.send(message.into()) {
        Ok(_) => ERROR_CODE_SUCCESS,
        Err(ipmb::SendError::Timeout) => ERROR_CODE_TIMEOUT,
        Err(ipmb::SendError::VersionMismatch(_)) => ERROR_CODE_VERSION_MISMATCH,
        Err(ipmb::SendError::TokenMismatch) => ERROR_CODE_TOKEN_MISMATCH,
        Err(ipmb::SendError::PermissonDenied) => ERROR_CODE_PERMISSION_DENIED,
    }
}

/// Receiver
#[repr(transparent)]
pub struct Receiver(*mut c_void);
opaque_type!(Receiver => ipmb::EndpointReceiver<ipmb::BytesMessage>);

#[allow(unused_variables)]
#[no_mangle]
pub unsafe extern "C" fn ipmb_receiver_drop(receiver: Receiver) {}

#[no_mangle]
pub unsafe extern "C" fn ipmb_recv(
    receiver: &mut Receiver,
    p_message: *mut Message,
    timeout: u32,
) -> ErrorCode {
    match receiver.recv(if timeout == TIMEOUT_INFINITE {
        None
    } else {
        Some(Duration::from_millis(timeout as _))
    }) {
        Ok(message) => {
            ptr::write(p_message, message.into());
            ERROR_CODE_SUCCESS
        }
        Err(ipmb::RecvError::Timeout) => ERROR_CODE_TIMEOUT,
        Err(ipmb::RecvError::Decode(_)) => ERROR_CODE_DECODE,
        Err(ipmb::RecvError::VersionMismatch(_)) => ERROR_CODE_VERSION_MISMATCH,
        Err(ipmb::RecvError::TokenMismatch) => ERROR_CODE_TOKEN_MISMATCH,
        Err(ipmb::RecvError::PermissonDenied) => ERROR_CODE_PERMISSION_DENIED,
    }
}

/// MemoryRegistry
#[repr(transparent)]
pub struct MemoryRegistry(*mut c_void);
opaque_type!(MemoryRegistry => ipmb::MemoryRegistry);

#[no_mangle]
pub unsafe extern "C" fn ipmb_memory_registry() -> MemoryRegistry {
    ipmb::MemoryRegistry::default().into()
}

#[allow(unused_variables)]
#[no_mangle]
pub unsafe extern "C" fn ipmb_memory_registry_drop(registry: MemoryRegistry) {}

#[no_mangle]
pub unsafe extern "C" fn ipmb_memory_registry_alloc(
    registry: &mut MemoryRegistry,
    min_size: usize,
    tag: *const c_char,
) -> MemoryRegion {
    let tag = if tag.is_null() {
        None
    } else {
        Some(CStr::from_ptr(tag).to_string_lossy())
    };

    registry.alloc(min_size, tag.as_deref()).into()
}

#[no_mangle]
pub unsafe extern "C" fn ipmb_memory_registry_alloc_with_free(
    registry: &mut MemoryRegistry,
    min_size: usize,
    tag: *const c_char,
    free_context: *mut c_void,
    free: Option<extern "C" fn(*mut c_void)>,
) -> MemoryRegion {
    let tag = if tag.is_null() {
        None
    } else {
        Some(CStr::from_ptr(tag).to_string_lossy())
    };

    if let Some(free) = free {
        registry
            .alloc_with_free(min_size, tag.as_deref(), move || {
                free(free_context);
            })
            .into()
    } else {
        registry.alloc(min_size, tag.as_deref()).into()
    }
}

#[no_mangle]
pub unsafe extern "C" fn ipmb_memory_registry_maintain(registry: &mut MemoryRegistry) {
    registry.maintain();
}

/// Message
#[repr(transparent)]
pub struct Message(*mut c_void);
opaque_type!(Message => ipmb::Message<ipmb::BytesMessage>);

#[no_mangle]
pub unsafe extern "C" fn ipmb_message(
    selector: Selector,
    format: u16,
    ptr: *const u8,
    size: u32,
) -> Message {
    let data = slice::from_raw_parts(ptr, size as _).to_owned();

    ipmb::Message::<ipmb::BytesMessage>::new(selector.into(), ipmb::BytesMessage { format, data })
        .into()
}

#[allow(unused_variables)]
#[no_mangle]
pub unsafe extern "C" fn ipmb_message_drop(message: Message) {}

#[no_mangle]
pub unsafe extern "C" fn ipmb_message_bytes_data(
    message: &Message,
    format: &mut u16,
    ptr: &mut *const u8,
    size: &mut u32,
) {
    *format = message.payload.format;
    *ptr = message.payload.data.as_ptr();
    *size = message.payload.data.len() as _;
}

#[no_mangle]
pub unsafe extern "C" fn ipmb_message_object_append(message: &mut Message, obj: Object) {
    let obj = ipmb::Object::from_raw(obj as usize as _);
    message.objects.push(obj);
}

/// Retrieve object from message with ownership
#[no_mangle]
pub unsafe extern "C" fn ipmb_message_object_retrieve(
    message: &mut Message,
    index: usize,
) -> Object {
    if message.objects.len() > index {
        message.objects.remove(index).into_raw() as _
    } else {
        0
    }
}

/// Get object from message without ownership
#[no_mangle]
pub unsafe extern "C" fn ipmb_message_object_get(message: &Message, index: usize) -> Object {
    if let Some(obj) = message.objects.get(index) {
        obj.as_raw() as _
    } else {
        0
    }
}

/// Drop object with ownership
#[no_mangle]
pub unsafe extern "C" fn ipmb_object_drop(obj: Object) {
    let _ = ipmb::Object::from_raw(obj as usize as _);
}

#[no_mangle]
pub unsafe extern "C" fn ipmb_message_memory_region_append(
    message: &mut Message,
    region: MemoryRegion,
) {
    message.memory_regions.push(region.into());
}

/// Retrieve memory region from message with ownership
#[no_mangle]
pub unsafe extern "C" fn ipmb_message_memory_region_retrieve(
    message: &mut Message,
    index: usize,
) -> MemoryRegion {
    if message.memory_regions.len() > index {
        message.memory_regions.remove(index).into()
    } else {
        MemoryRegion(ptr::null_mut())
    }
}

/// Get memory region from message without ownership
#[no_mangle]
pub unsafe extern "C" fn ipmb_message_memory_region_get(
    message: &mut Message,
    index: usize,
) -> MemoryRegion {
    if message.memory_regions.len() > index {
        let region = &mut message.memory_regions[index];
        MemoryRegion(region as *mut ipmb::MemoryRegion as _)
    } else {
        MemoryRegion(ptr::null_mut())
    }
}

/// MemoryRegion
#[repr(transparent)]
pub struct MemoryRegion(*mut c_void);
opaque_type!(MemoryRegion => ipmb::MemoryRegion);

#[no_mangle]
pub unsafe extern "C" fn ipmb_memory_region(size: usize) -> MemoryRegion {
    ipmb::MemoryRegion::new(size).into()
}

#[allow(unused_variables)]
#[no_mangle]
pub unsafe extern "C" fn ipmb_memory_region_drop(region: MemoryRegion) {}

#[no_mangle]
pub unsafe extern "C" fn ipmb_memory_region_map(
    region: &mut MemoryRegion,
    offset: usize,
    size: isize,
    real_size: *mut isize,
) -> *mut u8 {
    if size < 0 {
        region.map(offset..)
    } else {
        region.map(offset..offset + size as usize)
    }
    .map(|v| {
        if !real_size.is_null() {
            *real_size = v.len() as _;
        }
        v.as_mut_ptr()
    })
    .unwrap_or(ptr::null_mut())
}

/// Get reference count of memory region
#[no_mangle]
pub unsafe extern "C" fn ipmb_memory_region_ref_count(region: &MemoryRegion) -> u32 {
    region.ref_count()
}

/// Clone a new MemoryRegion and share the underlying kernel object.
/// # Safety
/// - region must be a valid MemoryRegion.
#[no_mangle]
pub unsafe extern "C" fn ipmb_memory_region_clone(region: &MemoryRegion) -> MemoryRegion {
    (*region).clone().into()
}

/// Label
#[repr(transparent)]
pub struct Label(*mut c_void);
opaque_type!(Label => ipmb::Label);

#[no_mangle]
pub unsafe extern "C" fn ipmb_label() -> Label {
    ipmb::Label::default().into()
}

#[allow(unused_variables)]
#[no_mangle]
pub unsafe extern "C" fn ipmb_label_drop(label: Label) {}

#[no_mangle]
pub unsafe extern "C" fn ipmb_label_insert(label: &mut Label, s: *const c_char) {
    label.insert(CStr::from_ptr(s).to_string_lossy());
}

/// LabelOp
#[repr(transparent)]
pub struct LabelOp(*mut c_void);
opaque_type!(LabelOp => ipmb::LabelOp);

#[no_mangle]
pub unsafe extern "C" fn ipmb_label_op_bool(v: bool) -> LabelOp {
    (if v {
        ipmb::LabelOp::True
    } else {
        ipmb::LabelOp::False
    })
    .into()
}

#[no_mangle]
pub unsafe extern "C" fn ipmb_label_op_leaf(s: *const c_char) -> LabelOp {
    let s = CStr::from_ptr(s).to_string_lossy();
    ipmb::LabelOp::from(s).into()
}

#[allow(unused_variables)]
#[no_mangle]
pub unsafe extern "C" fn ipmb_label_op_drop(left: LabelOp) {}

#[no_mangle]
pub unsafe extern "C" fn ipmb_label_op_not(left: LabelOp) -> LabelOp {
    let left: ipmb::LabelOp = left.into();
    (!left).into()
}

#[no_mangle]
pub unsafe extern "C" fn ipmb_label_op_and(left: LabelOp, right: LabelOp) -> LabelOp {
    let left: ipmb::LabelOp = left.into();
    left.and(right).into()
}

#[no_mangle]
pub unsafe extern "C" fn ipmb_label_op_or(left: LabelOp, right: LabelOp) -> LabelOp {
    let left: ipmb::LabelOp = left.into();
    left.or(right).into()
}

/// Selector
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Selector {
    label_op: &'static LabelOp,
    mode: SelectorMode,
    ttl: u32,
}

impl From<Selector> for ipmb::Selector {
    fn from(v: Selector) -> Self {
        let mut s = match v.mode {
            SelectorMode::kUnicast => ipmb::Selector::unicast((*v.label_op).clone()),
            SelectorMode::kMulticast => ipmb::Selector::multicast((*v.label_op).clone()),
        };
        s.ttl = Duration::from_millis(v.ttl as _);
        s
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub enum SelectorMode {
    kUnicast,
    kMulticast,
}

/// Options
#[repr(C)]
pub struct Options {
    identifier: *const c_char,
    label: &'static Label,
    token: *const c_char,
    controller_affinity: bool,
}

/// Kernel Object
type Object = u64;
