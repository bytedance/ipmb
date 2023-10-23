use napi::bindgen_prelude::*;
use napi::{NapiRaw, NapiValue};
use napi_derive::napi;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use std::{ffi, mem, ptr, thread};

#[napi]
pub enum SelectorMode {
    Unicast,
    Multicast,
}

#[napi]
pub struct LabelOp(ipmb::LabelOp);

#[napi]
impl LabelOp {
    #[napi(constructor, ts_args_type = "v: boolean | string")]
    pub fn new(v: napi::JsUnknown) -> Result<Self> {
        Ok(Self(match v.get_type()? {
            ValueType::Boolean => {
                if v.coerce_to_bool()?.get_value()? {
                    ipmb::LabelOp::True
                } else {
                    ipmb::LabelOp::False
                }
            }
            _ => {
                let s = v.coerce_to_string()?.into_utf8()?;
                ipmb::LabelOp::from(s.as_str()?)
            }
        }))
    }

    #[napi]
    pub fn not(&mut self) {
        self.0 = !self.0.clone();
    }

    #[napi]
    pub fn and(&mut self, right: &LabelOp) {
        self.0 = self.0.clone().and(right.0.clone());
    }

    #[napi]
    pub fn or(&mut self, right: &LabelOp) {
        self.0 = self.0.clone().or(right.0.clone());
    }

    #[napi]
    pub fn to_string(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[napi(object)]
pub struct Selector {
    pub label_op: ClassInstance<LabelOp>,
    pub mode: SelectorMode,
    pub ttl: u32,
}

impl From<Selector> for ipmb::Selector {
    fn from(selector: Selector) -> Self {
        let mut ipmb_selector = match selector.mode {
            SelectorMode::Unicast => ipmb::Selector::unicast(selector.label_op.0.clone()),
            SelectorMode::Multicast => ipmb::Selector::multicast(selector.label_op.0.clone()),
        };
        ipmb_selector.ttl = Duration::from_millis(selector.ttl as _);
        ipmb_selector
    }
}

#[napi(object)]
pub struct Options {
    pub identifier: String,
    pub label: Vec<String>,
    pub token: String,
    pub controller_affinity: bool,
}

#[napi(object)]
pub struct BytesMessage {
    pub format: u16,
    pub data: Buffer,
}

#[napi]
pub struct Object(ipmb::Object);

#[napi]
impl Object {
    #[napi]
    pub fn value(&self) -> i64 {
        self.0.as_raw() as _
    }
}

#[napi]
pub struct MemoryRegion(ipmb::MemoryRegion);

#[napi]
impl MemoryRegion {
    #[napi]
    pub fn map(&mut self, offset: u32, size: i32) -> Result<Buffer> {
        let v = if size < 0 {
            self.0.map(offset as usize..)
        } else {
            self.0.map(offset as usize..offset as usize + size as usize)
        }
        .map_err(|err| Error::new(Status::GenericFailure, format!("{:?}", err)))?;
        Ok(v.to_owned().into())
    }
}

#[napi]
pub struct Sender {
    sender: ipmb::EndpointSender<ipmb::BytesMessage>,
    memory_registry: ipmb::MemoryRegistry,
}

#[napi]
impl Sender {
    #[napi]
    pub fn send(
        &mut self,
        selector: Selector,
        bytes_message: BytesMessage,
        buffers: Vec<Buffer>,
    ) -> Result<()> {
        let mut ipmb_message = ipmb::Message::new(
            selector.into(),
            ipmb::BytesMessage {
                format: bytes_message.format,
                data: bytes_message.data.into(),
            },
        );

        for buf in buffers {
            let mut region = self.memory_registry.alloc(buf.len(), None);
            unsafe {
                ptr::copy(
                    buf.as_ptr(),
                    region
                        .map(..)
                        .map_err(|err| Error::new(Status::GenericFailure, format!("{:?}", err)))?
                        .as_mut_ptr(),
                    buf.len(),
                );
            }
            ipmb_message.memory_regions.push(region);
        }

        self.sender
            .send(ipmb_message)
            .map_err(|err| Error::new(Status::GenericFailure, format!("{:?}", err)))?;

        Ok(())
    }
}

#[napi]
#[derive(Clone)]
pub struct Receiver {
    guard_sender: mpsc::SyncSender<()>,
    local_buffer: Arc<Mutex<LocalBuffer>>,
}

impl Drop for Receiver {
    fn drop(&mut self) {
        self.close();
    }
}

#[napi]
impl Receiver {
    #[napi(
        ts_return_type = "Promise<{ bytesMessage: BytesMessage, objects: Array<Object>, memoryRegions: Array<MemoryRegion> }>"
    )]
    pub fn recv(&mut self, timeout: Option<u32>, env: Env) -> Result<napi::JsObject> {
        let mut local = self.local_buffer.lock().unwrap();
        debug_assert!(local.deferred_list.is_empty() || local.messages.is_empty());

        if local.closed {
            return Err(Error::new(Status::GenericFailure, "closed"));
        }

        unsafe {
            let mut deferred = ptr::null_mut();
            let mut promise = ptr::null_mut();
            let r = sys::napi_create_promise(env.raw(), &mut deferred, &mut promise);
            assert_eq!(r, sys::Status::napi_ok);

            if !local.messages.is_empty() {
                let r = local.messages.remove(0);
                let _ = consume_deferred(env, deferred, r);
            } else {
                local.deferred_list.push((
                    Deferred(deferred),
                    timeout.map(|t| Instant::now() + Duration::from_millis(t as _)),
                ));
            }

            Ok(napi::JsObject::from_raw_unchecked(env.raw(), promise))
        }
    }

    #[napi]
    pub fn close(&mut self) {
        {
            let mut local = self.local_buffer.lock().unwrap();
            if local.closed {
                return;
            }
            // As close is an asynchronous operation, to avoid repeated close, set the flag here
            local.closed = true;
        }

        // Signal close
        let _ = self.guard_sender.send(());
        // Wait close
        let _ = self.guard_sender.send(());
    }
}

#[napi(ts_return_type = "{ sender: Sender, receiver: Receiver }")]
pub fn join(options: Options, timeout: Option<u32>, mut env: Env) -> Result<napi::JsObject> {
    let (sender, mut receiver) = ipmb::join::<ipmb::BytesMessage, ipmb::BytesMessage>(
        ipmb::Options {
            identifier: options.identifier,
            label: options.label.into(),
            token: options.token,
            controller_affinity: options.controller_affinity,
        },
        timeout.map(|v| Duration::from_millis(v as _)),
    )
    .map_err(|err| Error::new(Status::GenericFailure, format!("{:?}", err)))?;

    let (guard_sender, guard_receiver) = mpsc::sync_channel(0);
    let local_buffer = Arc::new(Mutex::new(LocalBuffer::default()));
    let local_buffer_ptr = Arc::into_raw(local_buffer.clone());

    let tsfn = unsafe {
        let name = "delegate_receiver";
        let mut async_resource_name = ptr::null_mut();
        let mut r = sys::napi_create_string_utf8(
            env.raw(),
            name.as_ptr() as _,
            name.len(),
            &mut async_resource_name,
        );
        assert_eq!(r, sys::Status::napi_ok);

        let mut tsfn = ptr::null_mut();
        r = sys::napi_create_threadsafe_function(
            env.raw(),
            ptr::null_mut(),
            ptr::null_mut(),
            async_resource_name,
            0,
            1,
            local_buffer_ptr.cast_mut() as _,
            Some(threadsafe_function_finalize),
            local_buffer_ptr.cast_mut() as _,
            Some(delegate_receiver),
            &mut tsfn,
        );
        assert_eq!(r, sys::Status::napi_ok);

        r = sys::napi_ref_threadsafe_function(env.raw(), tsfn);
        assert_eq!(r, sys::Status::napi_ok);

        ThreadsafeFunction(tsfn)
    };

    thread::spawn(move || loop {
        let r = receiver.recv(Some(Duration::from_millis(200)));

        match r {
            Err(ipmb::RecvError::Timeout) => match guard_receiver.try_recv() {
                Ok(_) => {
                    tsfn.destroy();
                    break;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    tsfn.call(DelegateAction::CleanTimeout);
                }
            },
            Err(ipmb::RecvError::VersionMismatch(_) | ipmb::RecvError::TokenMismatch) => {
                tsfn.call(DelegateAction::Recv(r));
                tsfn.destroy();
                break;
            }
            _ => {
                tsfn.call(DelegateAction::Recv(r));
            }
        }
    });

    let receiver = Receiver {
        local_buffer,
        guard_sender,
    };

    let _ = env.add_env_cleanup_hook(receiver.clone(), |_| {})?;

    let mut js_obj = env.create_object()?;
    js_obj.set(
        "sender",
        Sender {
            sender,
            memory_registry: ipmb::MemoryRegistry::default(),
        },
    )?;
    js_obj.set("receiver", receiver)?;

    Ok(js_obj)
}

struct Deferred(sys::napi_deferred);

unsafe impl Send for Deferred {}

unsafe impl Sync for Deferred {}

struct ThreadsafeFunction(sys::napi_threadsafe_function);

impl ThreadsafeFunction {
    fn call(&self, action: DelegateAction) {
        unsafe {
            sys::napi_call_threadsafe_function(
                self.0,
                Box::into_raw(Box::new(action)) as _,
                sys::ThreadsafeFunctionCallMode::blocking,
            );
        }
    }

    fn destroy(self) {
        unsafe {
            sys::napi_call_threadsafe_function(
                self.0,
                Box::into_raw(Box::new(DelegateAction::Close(self))) as _,
                sys::ThreadsafeFunctionCallMode::blocking,
            );
        }
    }
}

impl Drop for ThreadsafeFunction {
    fn drop(&mut self) {
        unsafe {
            sys::napi_release_threadsafe_function(
                self.0,
                sys::ThreadsafeFunctionReleaseMode::release,
            );
        }
    }
}

unsafe impl Send for ThreadsafeFunction {}

unsafe impl Sync for ThreadsafeFunction {}

#[derive(Default)]
struct LocalBuffer {
    closed: bool,
    messages: Vec<std::result::Result<ipmb::Message<ipmb::BytesMessage>, ipmb::RecvError>>,
    deferred_list: Vec<(Deferred, Option<Instant>)>,
}

impl LocalBuffer {
    fn clean_timeout(&mut self, env: Env) {
        if self.deferred_list.is_empty() {
            return;
        }

        let now = Instant::now();
        let mut i = 0;
        while i < self.deferred_list.len() {
            let Some(deadline) = self.deferred_list[i].1 else {
                i += 1;
                continue;
            };

            if deadline < now {
                let (deferred, _) = self.deferred_list.remove(i);

                if let Ok(err) = env.create_error(Error::new(Status::GenericFailure, "timeout")) {
                    unsafe {
                        sys::napi_reject_deferred(env.raw(), deferred.0, err.raw());
                    }
                }
            } else {
                i += 1;
            }
        }
    }

    fn close(&mut self, env: Env, tsfn: ThreadsafeFunction) {
        self.closed = true;
        let deferred_list = mem::take(&mut self.deferred_list);

        if let Ok(err) = env.create_error(Error::new(Status::GenericFailure, "closed")) {
            for (deferred, _) in deferred_list {
                unsafe {
                    sys::napi_reject_deferred(env.raw(), deferred.0, err.raw());
                }
            }
        }

        unsafe {
            sys::napi_unref_threadsafe_function(env.raw(), tsfn.0);
        }
    }
}

enum DelegateAction {
    CleanTimeout,
    Close(ThreadsafeFunction),
    Recv(std::result::Result<ipmb::Message<ipmb::BytesMessage>, ipmb::RecvError>),
}

extern "C" fn delegate_receiver(
    env: sys::napi_env,
    _js_callback: sys::napi_value,
    context: *mut ffi::c_void,
    data: *mut ffi::c_void,
) {
    unsafe {
        let env = Env::from_raw(env);
        let action = *Box::from_raw(data as *mut DelegateAction);
        let local_buffer = &*(context as *const Mutex<LocalBuffer>);
        let mut local = local_buffer.lock().unwrap();
        debug_assert!(local.deferred_list.is_empty() || local.messages.is_empty());

        match action {
            DelegateAction::Close(tsfn) => local.close(env, tsfn),
            DelegateAction::CleanTimeout => local.clean_timeout(env),
            DelegateAction::Recv(r) => {
                if local.deferred_list.is_empty() {
                    local.messages.push(r);
                } else {
                    let (deferred, _) = local.deferred_list.remove(0);
                    let _ = consume_deferred(env, deferred.0, r);
                }
            }
        }
    }
}

extern "C" fn threadsafe_function_finalize(
    _env: sys::napi_env,
    finalize_data: *mut ffi::c_void,
    _finalize_hint: *mut ffi::c_void,
) {
    unsafe {
        Arc::from_raw(finalize_data as *const Mutex<LocalBuffer>);
    }
}

fn consume_deferred(
    env: Env,
    deferred: sys::napi_deferred,
    r: std::result::Result<ipmb::Message<ipmb::BytesMessage>, ipmb::RecvError>,
) -> Result<()> {
    match r {
        Ok(message) => {
            let bytes_message = BytesMessage {
                format: message.payload.format,
                data: message.payload.data.into(),
            };
            let js_objects: Vec<Object> = message.objects.into_iter().map(Object).collect();
            let js_regions: Vec<MemoryRegion> = message
                .memory_regions
                .into_iter()
                .map(MemoryRegion)
                .collect();

            let mut js_obj = env.create_object()?;
            js_obj.set("bytesMessage", bytes_message)?;
            js_obj.set("objects", js_objects)?;
            js_obj.set("memoryRegions", js_regions)?;

            let r = unsafe { sys::napi_resolve_deferred(env.raw(), deferred, js_obj.raw()) };
            if r == sys::Status::napi_ok {
                Ok(())
            } else {
                Err(Error::new(Status::GenericFailure, "napi_resolve_deferred"))
            }
        }
        Err(err) => {
            let err = env.create_error(Error::new(Status::GenericFailure, err))?;
            let r = unsafe { sys::napi_reject_deferred(env.raw(), deferred, err.raw()) };
            if r == sys::Status::napi_ok {
                Ok(())
            } else {
                Err(Error::new(Status::GenericFailure, "napi_reject_deferred"))
            }
        }
    }
}

#[allow(dead_code)]
fn execute_with_env(env: Env, f: impl FnOnce(Env)) {
    let f: Box<dyn FnOnce(Env)> = Box::new(f);

    unsafe {
        let name = "execute_with_env";
        let mut async_resource_name = ptr::null_mut();
        let mut r = sys::napi_create_string_utf8(
            env.raw(),
            name.as_ptr() as _,
            name.len(),
            &mut async_resource_name,
        );
        assert_eq!(r, sys::Status::napi_ok);

        let mut async_work = ptr::null_mut();
        r = sys::napi_create_async_work(
            env.raw(),
            ptr::null_mut(),
            async_resource_name,
            Some(execute),
            Some(complete),
            Box::into_raw(Box::new(Some(f))) as _,
            &mut async_work,
        );
        assert_eq!(r, sys::Status::napi_ok);

        r = sys::napi_queue_async_work(env.raw(), async_work);
        assert_eq!(r, sys::Status::napi_ok);
    }
}

extern "C" fn execute(env: sys::napi_env, data: *mut ffi::c_void) {
    let f = data as *mut Option<Box<dyn FnOnce(Env)>>;
    unsafe {
        let f = &mut *f;
        (f.take().unwrap())(Env::from_raw(env));
    }
}

extern "C" fn complete(_env: sys::napi_env, _status: sys::napi_status, data: *mut ffi::c_void) {
    let f = data as *mut Option<Box<dyn FnOnce(Env)>>;
    unsafe {
        let _ = *Box::from_raw(f);
    }
}
