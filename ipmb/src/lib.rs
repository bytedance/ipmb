use bus_controller::BusController;
pub use errors::{Error, JoinError, RecvError, SendError};
pub use ipmb_derive::MessageBox;
pub use label::{Label, LabelOp};
pub use memory_registry::MemoryRegistry;
pub use message::{BytesMessage, Message, MessageBox};
use once_cell::sync::Lazy;
pub use options::Options;
use platform::{look_up, register, EncodedMessage, IoHub, IoMultiplexing, Remote};
pub use platform::{MemoryRegion, Object};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::marker::PhantomData;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{mpsc, Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use type_uuid::Bytes;
use util::EndpointID;

mod bus_controller;
mod errors;
mod label;
mod memory_registry;
mod message;
mod options;
pub mod platform;
mod util;

/// Describe how a messages is routed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Selector {
    pub label_op: LabelOp,
    pub mode: SelectorMode,
    uuid: Bytes,
    memory_region_count: u16,
    /// The time to live when a message cannot be routed to any endpoint.
    pub ttl: Duration,
}

impl Selector {
    pub fn unicast(label_op: impl Into<LabelOp>) -> Self {
        Self {
            label_op: label_op.into(),
            mode: SelectorMode::Unicast,
            uuid: [0; 16],
            memory_region_count: 0,
            ttl: Duration::ZERO,
        }
    }

    pub fn multicast(label_op: impl Into<LabelOp>) -> Self {
        Self {
            label_op: label_op.into(),
            mode: SelectorMode::Multicast,
            uuid: [0; 16],
            memory_region_count: 0,
            ttl: Duration::ZERO,
        }
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub enum SelectorMode {
    /// The message can only be consumed by one endpoint.
    Unicast,
    /// The message can be consumed by multiple endpoints.
    Multicast,
}

pub fn decode<'de, T: Deserialize<'de>>(data: &'de [u8]) -> Result<T, Error> {
    bincode::serde::decode_borrowed_from_slice(data, bincode::config::standard())
        .map_err(Error::Decode)
}

pub fn encode<T: Serialize>(t: T) -> Result<Vec<u8>, Error> {
    let data = bincode::serde::encode_to_vec(t, bincode::config::standard())?;
    Ok(data)
}

pub fn join<'de, T: MessageBox, R: MessageBox>(
    options: Options,
    timeout: Option<Duration>,
) -> Result<(EndpointSender<T>, EndpointReceiver<R>), JoinError> {
    let rule = Arc::new(RwLock::new(Rule::join(
        options,
        0,
        Arc::new(IoMultiplexing::new()),
        timeout,
    )?));

    Ok((
        EndpointSender {
            rule: rule.clone(),
            _marker: PhantomData,
        },
        EndpointReceiver {
            rule,
            _maker: PhantomData,
        },
    ))
}

/// The sending half of endpoint, messages can be sent with [`send`](EndpointSender::send).
pub struct EndpointSender<T> {
    rule: Arc<RwLock<Rule>>,
    _marker: PhantomData<T>,
}

impl<T> Clone for EndpointSender<T> {
    fn clone(&self) -> Self {
        Self {
            rule: self.rule.clone(),
            _marker: PhantomData,
        }
    }
}

impl<T: MessageBox> EndpointSender<T> {
    pub fn send(&self, mut msg: Message<T>) -> Result<(), SendError> {
        msg.selector.memory_region_count = msg.memory_regions.len() as _;
        let mut msg = msg.into_encoded();

        loop {
            let rule = self.rule.read().unwrap();
            match &*rule {
                Rule::Client {
                    endpoint_id: _,
                    options: _,
                    remote,
                    io_hub: _,
                    reader_closed: _,
                    im: _,
                    epoch,
                } => match msg.send(remote) {
                    Err(Error::Disconnect) => {
                        let epoch = *epoch;
                        drop(rule);

                        let mut rule = self.rule.write().unwrap();
                        match &mut *rule {
                            Rule::Client {
                                endpoint_id: _,
                                options,
                                remote: _,
                                io_hub,
                                reader_closed,
                                im,
                                epoch: epoch1,
                            } => {
                                if epoch == *epoch1 {
                                    let reader_closed = *reader_closed;

                                    // Close reader
                                    drop(io_hub.take());

                                    *rule = Rule::join(
                                        options.clone(),
                                        epoch.overflowing_add(1).0,
                                        im.clone(),
                                        None,
                                    )?;

                                    if reader_closed {
                                        rule.reader_close();
                                    }
                                }
                            }
                            Rule::Server { .. } => {}
                        }
                    }
                    Err(_) => unreachable!(),
                    Ok(_) => break Ok(()),
                },
                Rule::Server {
                    endpoint_id: _,
                    bus_sender,
                    receiver: _,
                    im,
                } => {
                    bus_sender.lock().unwrap().send(msg).unwrap();
                    im.wake();
                    break Ok(());
                }
            }
        }
    }
}

/// The receiving half of endpoint, messages sent to the endpoint can be retrieved using [`recv`](EndpointReceiver::recv), dropping receiver will close underly receving kernel buffer.
// Don't impl Clone
pub struct EndpointReceiver<R> {
    rule: Arc<RwLock<Rule>>,
    _maker: PhantomData<R>,
}

impl<'de, R: MessageBox> EndpointReceiver<R> {
    pub fn recv(&mut self, timeout: Option<Duration>) -> Result<Message<R>, RecvError> {
        loop {
            let rule = self.rule.read().unwrap();
            match &*rule {
                Rule::Client {
                    endpoint_id: _,
                    options,
                    remote,
                    io_hub,
                    reader_closed,
                    im: _,
                    epoch,
                } => {
                    if !*reader_closed && io_hub.is_none() {
                        let epoch = *epoch;
                        drop(rule);

                        let mut rule = self.rule.write().unwrap();
                        match &mut *rule {
                            Rule::Client {
                                endpoint_id: _,
                                options,
                                remote: _,
                                io_hub,
                                reader_closed,
                                im,
                                epoch: epoch1,
                            } => {
                                if epoch == *epoch1 {
                                    let reader_closed = *reader_closed;

                                    // Close reader
                                    drop(io_hub.take());

                                    *rule = Rule::join(
                                        options.clone(),
                                        epoch.overflowing_add(1).0,
                                        im.clone(),
                                        timeout,
                                    )?;

                                    if reader_closed {
                                        rule.reader_close();
                                    }
                                }

                                continue;
                            }
                            Rule::Server { .. } => continue,
                        }
                    }

                    let mut io_hub_guard = io_hub.as_ref().expect("reader closed").lock().unwrap();

                    match io_hub_guard.recv(timeout, Some(remote)) {
                        Ok(encoded_msg) => {
                            if encoded_msg.selector.label_op.validate(&options.label) {
                                match R::decode(encoded_msg.selector.uuid, encoded_msg.payload_data)
                                {
                                    Ok(payload) => {
                                        let mut msg = Message::new(encoded_msg.selector, payload);
                                        msg.objects = encoded_msg.objects;
                                        msg.memory_regions = encoded_msg.memory_regions;
                                        break Ok(msg);
                                    }
                                    Err(Error::TypeUuidNotFound) => {
                                        continue;
                                    }
                                    Err(Error::Decode(err)) => {
                                        break Err(RecvError::Decode(err));
                                    }
                                    Err(_) => unreachable!(),
                                }
                            } else {
                                log::warn!(
                                    "Unexpected message label_op: {:?}",
                                    encoded_msg.selector.label_op
                                );
                                continue;
                            }
                        }
                        Err(Error::Disconnect) => {
                            let epoch = *epoch;
                            drop(io_hub_guard);
                            drop(rule);

                            let mut rule = self.rule.write().unwrap();
                            match &mut *rule {
                                Rule::Client {
                                    endpoint_id: _,
                                    options,
                                    remote: _,
                                    io_hub,
                                    reader_closed,
                                    im,
                                    epoch: epoch1,
                                } => {
                                    if epoch == *epoch1 {
                                        let reader_closed = *reader_closed;

                                        // Close reader
                                        drop(io_hub.take());

                                        *rule = Rule::join(
                                            options.clone(),
                                            epoch.overflowing_add(1).0,
                                            im.clone(),
                                            timeout,
                                        )?;

                                        if reader_closed {
                                            rule.reader_close();
                                        }
                                    }

                                    continue;
                                }
                                Rule::Server { .. } => continue,
                            }
                        }
                        Err(Error::Timeout) => {
                            break Err(RecvError::Timeout);
                        }
                        Err(_) => unreachable!(),
                    }
                }
                Rule::Server {
                    endpoint_id: _,
                    bus_sender: _,
                    receiver,
                    im: _,
                } => {
                    let receiver = receiver.as_ref().expect("reader closed").lock().unwrap();
                    break match timeout {
                        Some(timeout) => match receiver.recv_timeout(timeout) {
                            Ok(encoded_msg) => {
                                match R::decode(encoded_msg.selector.uuid, encoded_msg.payload_data)
                                {
                                    Ok(payload) => {
                                        let mut msg = Message::new(encoded_msg.selector, payload);
                                        msg.objects = encoded_msg.objects;
                                        msg.memory_regions = encoded_msg.memory_regions;
                                        Ok(msg)
                                    }
                                    Err(Error::TypeUuidNotFound) => {
                                        continue;
                                    }
                                    Err(Error::Decode(err)) => Err(RecvError::Decode(err)),
                                    Err(_) => unreachable!(),
                                }
                            }
                            Err(RecvTimeoutError::Timeout) => Err(RecvError::Timeout),
                            Err(_) => unreachable!(),
                        },
                        None => {
                            let encoded_msg = receiver.recv().unwrap();
                            match R::decode(encoded_msg.selector.uuid, encoded_msg.payload_data) {
                                Ok(payload) => {
                                    let mut msg = Message::new(encoded_msg.selector, payload);
                                    msg.objects = encoded_msg.objects;
                                    msg.memory_regions = encoded_msg.memory_regions;
                                    Ok(msg)
                                }
                                Err(Error::TypeUuidNotFound) => {
                                    continue;
                                }
                                Err(Error::Decode(err)) => Err(RecvError::Decode(err)),
                                Err(_) => unreachable!(),
                            }
                        }
                    };
                }
            }
        }
    }
}

impl<R> Drop for EndpointReceiver<R> {
    fn drop(&mut self) {
        let mut rule = self.rule.write().unwrap();
        rule.reader_close();
    }
}

enum Rule {
    Client {
        #[allow(dead_code)]
        endpoint_id: EndpointID,
        options: Options,
        remote: Remote,
        io_hub: Option<Mutex<IoHub>>,
        reader_closed: bool,
        im: Arc<IoMultiplexing>,
        epoch: u32,
    },
    Server {
        #[allow(dead_code)]
        endpoint_id: EndpointID,
        bus_sender: Mutex<Sender<EncodedMessage>>,
        receiver: Option<Mutex<Receiver<EncodedMessage>>>,
        im: Arc<IoMultiplexing>,
    },
}

impl Rule {
    fn join(
        options: Options,
        epoch: u32,
        im: Arc<IoMultiplexing>,
        timeout: Option<Duration>,
    ) -> Result<Self, JoinError> {
        let end = timeout.map(|timeout| Instant::now() + timeout);

        macro_rules! wait {
            () => {
                let mut wait = Duration::from_secs(2);
                if let Some(end) = end {
                    let remain = end.saturating_duration_since(Instant::now());
                    if remain.is_zero() {
                        return Err(JoinError::Timeout);
                    }
                    wait = wait.min(remain);
                }
                thread::sleep(wait);
            };
        }

        let mut timeout_count = 0;

        let rule = loop {
            let r = look_up(
                &options.identifier,
                options.label.clone(),
                options.token.clone(),
                im.clone(),
            );

            match r {
                Ok((io_hub, remote, endpoint_id)) => {
                    let rule = Rule::Client {
                        endpoint_id,
                        options,
                        remote,
                        io_hub: Some(Mutex::new(io_hub)),
                        reader_closed: false,
                        im,
                        epoch,
                    };
                    break rule;
                }
                Err(Error::IdentifierNotInUse) => {
                    if !options.controller_affinity {
                        log::error!("lookup: controller not found");
                        wait!();
                        continue;
                    }

                    let r = register(&options.identifier, im.clone());

                    match r {
                        Ok((io_hub, bus_sender, endpoint_id)) => {
                            let (sender, receiver) = mpsc::channel::<EncodedMessage>();

                            let im = io_hub.io_multiplexing();

                            let bus_controller = BusController::new(
                                endpoint_id,
                                options.label,
                                options.token,
                                sender,
                                io_hub,
                            );
                            bus_controller.run();

                            let rule = Rule::Server {
                                endpoint_id,
                                bus_sender: Mutex::new(bus_sender),
                                receiver: Some(Mutex::new(receiver)),
                                im,
                            };
                            break rule;
                        }
                        Err(Error::IdentifierInUse) => {}
                        Err(err) => {
                            log::error!("register: {:?}", err);
                            wait!();
                        }
                    }
                }
                Err(Error::VersionMismatch(v, _)) => {
                    return Err(JoinError::VersionMismatch(v));
                }
                Err(Error::TokenMismatch) => {
                    return Err(JoinError::TokenMismatch);
                }
                Err(Error::Timeout) => {
                    timeout_count += 1;

                    if timeout_count > 5 {
                        return Err(JoinError::VersionMismatch(Version((0, 0, 0))));
                    }

                    wait!();
                }
                Err(err) => {
                    log::error!("look_up: {:?}", err);
                    wait!();
                }
            }
        };

        Ok(rule)
    }
}

impl Rule {
    fn reader_close(&mut self) {
        match self {
            Rule::Client {
                io_hub,
                reader_closed,
                ..
            } => {
                let _ = io_hub.take();
                *reader_closed = true;
            }
            Rule::Server { receiver, .. } => {
                let _ = receiver.take();
            }
        }
    }
}

// Serialize bug with multiple field
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Version((u8, u8, u8));

impl Version {
    fn compatible(&self, rhs: Self) -> bool {
        if self.major() == 0 && rhs.major() == 0 {
            self.minor() == rhs.minor()
        } else {
            self.major() == rhs.major()
        }
    }

    pub fn major(&self) -> u8 {
        self.0 .0
    }

    pub fn minor(&self) -> u8 {
        self.0 .1
    }

    pub fn patch(&self) -> u8 {
        self.0 .2
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0 .0, self.0 .1, self.0 .2)
    }
}

static VERSION: Lazy<Version> = Lazy::new(|| {
    let v_major = env!("CARGO_PKG_VERSION_MAJOR");
    let v_minor = env!("CARGO_PKG_VERSION_MINOR");
    let v_patch = env!("CARGO_PKG_VERSION_PATCH");
    Version((
        v_major.parse().unwrap(),
        v_minor.parse().unwrap(),
        v_patch.parse().unwrap(),
    ))
});
static VERSION_PRE: Lazy<&'static str> = Lazy::new(|| env!("CARGO_PKG_VERSION_PRE"));

pub fn version() -> Version {
    *VERSION
}

pub fn version_pre() -> String {
    VERSION_PRE.to_owned()
}
