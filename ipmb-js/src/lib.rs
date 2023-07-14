use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::sync::mpsc::TryRecvError;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use std::{ptr, thread};

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
        self.0 = self.0.clone().not();
    }

    #[napi]
    pub fn and(&mut self, right: &LabelOp) {
        self.0 = self.0.clone().and(right.0.clone());
    }

    #[napi]
    pub fn or(&mut self, right: &LabelOp) {
        self.0 = self.0.clone().or(right.0.clone());
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
pub struct Receiver {
    alive: bool,
    receiver: Arc<
        Mutex<
            mpsc::Receiver<std::result::Result<ipmb::Message<ipmb::BytesMessage>, ipmb::RecvError>>,
        >,
    >,
    guard_sender: mpsc::SyncSender<()>,
}

#[napi]
impl Receiver {
    #[napi(
        ts_return_type = "{ bytesMessage: BytesMessage, memoryRegions: Array<MemoryRegion> } | undefined"
    )]
    pub fn try_recv(&mut self, env: Env) -> Result<Option<napi::JsObject>> {
        let Ok(rx) = self.receiver.try_lock() else {
            return Ok(None);
        };

        match rx.try_recv() {
            Ok(Ok(message)) => {
                let bytes_message = BytesMessage {
                    format: message.payload.format,
                    data: message.payload.data.into(),
                };

                let mut js_obj = env.create_object()?;
                js_obj.set("bytesMessage", bytes_message)?;
                let js_regions: Vec<MemoryRegion> = message
                    .memory_regions
                    .into_iter()
                    .map(MemoryRegion)
                    .collect();
                js_obj.set("memoryRegions", js_regions)?;

                Ok(Some(js_obj))
            }
            Err(TryRecvError::Empty) => Ok(None),
            Ok(Err(err)) => Err(Error::new(Status::GenericFailure, format!("{}", err))),
            Err(err) => Err(Error::new(Status::GenericFailure, format!("{}", err))),
        }
    }

    #[napi(
        ts_return_type = "Promise<{ bytesMessage: BytesMessage, memoryRegions: Array<MemoryRegion> }>"
    )]
    pub fn recv(&mut self, timeout: Option<u32>) -> AsyncTask<AsyncReceiver> {
        AsyncTask::new(AsyncReceiver {
            receiver: self.receiver.clone(),
            timeout: timeout.map(|v| Duration::from_millis(v as _)),
        })
    }

    #[napi]
    pub fn close(&mut self) {
        if !self.alive {
            return;
        }
        self.alive = false;

        // Signal destruction
        let _ = self.guard_sender.send(());
        // Wait destruction
        let _ = self.guard_sender.send(());
    }
}

pub struct AsyncReceiver {
    receiver: Arc<
        Mutex<
            mpsc::Receiver<std::result::Result<ipmb::Message<ipmb::BytesMessage>, ipmb::RecvError>>,
        >,
    >,
    timeout: Option<Duration>,
}

impl Task for AsyncReceiver {
    type Output = std::result::Result<ipmb::Message<ipmb::BytesMessage>, ipmb::RecvError>;
    type JsValue = napi::JsObject;

    fn compute(&mut self) -> Result<Self::Output> {
        let rx = self.receiver.lock().unwrap();
        Ok(if let Some(timeout) = self.timeout {
            rx.recv_timeout(timeout)
                .map_err(|err| Error::new(Status::GenericFailure, format!("{}", err)))?
        } else {
            rx.recv()
                .map_err(|err| Error::new(Status::GenericFailure, format!("{}", err)))?
        })
    }

    fn resolve(&mut self, env: Env, output: Self::Output) -> Result<Self::JsValue> {
        let message =
            output.map_err(|err| Error::new(Status::GenericFailure, format!("{:?}", err)))?;

        let bytes_message = BytesMessage {
            format: message.payload.format,
            data: message.payload.data.into(),
        };

        let mut js_obj = env.create_object()?;
        js_obj.set("bytesMessage", bytes_message)?;
        let js_regions: Vec<MemoryRegion> = message
            .memory_regions
            .into_iter()
            .map(MemoryRegion)
            .collect();
        js_obj.set("memoryRegions", js_regions)?;

        Ok(js_obj)
    }
}

#[napi(ts_return_type = "{ sender: Sender, receiver: Receiver }")]
pub fn join(options: Options, timeout: Option<u32>, env: Env) -> Result<napi::JsObject> {
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

    let (chan_sender, chan_receiver) =
        mpsc::channel::<std::result::Result<ipmb::Message<ipmb::BytesMessage>, ipmb::RecvError>>();
    let (guard_sender, guard_receiver) = mpsc::sync_channel(0);

    thread::spawn(move || loop {
        let r = receiver.recv(Some(Duration::from_millis(200)));

        let should_break = match &r {
            Err(ipmb::RecvError::Timeout) => match guard_receiver.try_recv() {
                Ok(_) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    continue;
                }
            },
            Err(ipmb::RecvError::VersionMismatch(_) | ipmb::RecvError::TokenMismatch) => true,
            _ => false,
        };

        if chan_sender.send(r).is_err() || should_break {
            break;
        }
    });

    let mut js_obj = env.create_object()?;
    js_obj.set(
        "sender",
        Sender {
            sender,
            memory_registry: ipmb::MemoryRegistry::default(),
        },
    )?;
    js_obj.set(
        "receiver",
        Receiver {
            alive: true,
            receiver: Arc::new(Mutex::new(chan_receiver)),
            guard_sender,
        },
    )?;

    Ok(js_obj)
}
