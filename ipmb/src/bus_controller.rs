use crate::message::{ConnectMessage, ConnectMessageAck};
use crate::platform::IoHub;
use crate::{
    decode, version, EncodedMessage, EndpointID, Error, Label, LabelOp, Message, Remote, Selector,
    SelectorMode,
};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};
use std::{mem, thread};
use type_uuid::TypeUuid;

pub struct BusController {
    label: Label,
    token: String,
    #[allow(dead_code)]
    endpoint_id: EndpointID,
    sender: Sender<EncodedMessage>,
    endpoints: Vec<Endpoint>,
    message_buffer: Vec<(Instant, EncodedMessage)>,
    message_buffer_swap: Vec<(Instant, EncodedMessage)>,
    io_hub: IoHub,
    last_detect_reachable: Instant,
}

impl BusController {
    pub(crate) fn new(
        endpoint_id: EndpointID,
        label: Label,
        token: String,
        sender: Sender<EncodedMessage>,
        io_hub: IoHub,
    ) -> Self {
        Self {
            endpoint_id,
            label,
            token,
            sender,
            endpoints: Default::default(),
            message_buffer: Default::default(),
            message_buffer_swap: Default::default(),
            io_hub,
            last_detect_reachable: Instant::now(),
        }
    }

    pub fn run(mut self) {
        thread::Builder::new()
            .name(String::from("ipmb bus controller"))
            .spawn(move || loop {
                let msg = match self.io_hub.recv(None, None) {
                    Ok(msg) => msg,
                    Err(Error::VersionMismatch(_, Some(remote))) => {
                        let _ = Message::new(
                            Selector::unicast(LabelOp::True),
                            ConnectMessageAck::ErrVersion(version()),
                        )
                        .into_encoded()
                        .send(&remote);
                        continue;
                    }
                    _ => continue,
                };

                let now = Instant::now();

                let (remain, endpoint_connected) = self.handle_message(msg);

                if let Some(remain) = remain {
                    if !remain.selector.ttl.is_zero() {
                        self.message_buffer
                            .push((now + remain.selector.ttl, remain));
                    }
                } else if endpoint_connected && !self.message_buffer.is_empty() {
                    let mut message_buffer = mem::take(&mut self.message_buffer);

                    for (expire, msg) in message_buffer.drain(..) {
                        let (remain, _) = self.handle_message(msg);
                        if let Some(remain) = remain {
                            if expire > now {
                                self.message_buffer_swap.push((expire, remain));
                            }
                        }
                    }

                    self.message_buffer = message_buffer;
                    mem::swap(&mut self.message_buffer, &mut self.message_buffer_swap);
                }

                self.detect_reachable(now);
            })
            .expect("failed to spawn ipmb bus controller");
    }

    // Don't read or write self.message_buffer
    fn handle_message(
        &mut self,
        mut encoded_msg: EncodedMessage,
    ) -> (Option<EncodedMessage>, bool) {
        let mut routed = false;
        let mut remain = None;
        let mut endpoint_connected = false;

        match encoded_msg.selector.uuid {
            <ConnectMessage as TypeUuid>::UUID => {
                endpoint_connected = self.endpoint_connect(encoded_msg);
            }
            #[cfg(windows)]
            <crate::message::FetchProcessHandleMessage as TypeUuid>::UUID => {
                if let Err(err) =
                    crate::platform::windows::util::reply_current_process_handle(encoded_msg)
                {
                    log::error!("{}", err);
                }
            }
            _ => {
                self.endpoints.retain(|Endpoint { label, remote, .. }| {
                    let mut online = true;

                    if routed && encoded_msg.selector.mode == SelectorMode::Unicast {
                        return online;
                    }

                    if encoded_msg.selector.label_op.validate(label) {
                        match encoded_msg.send(remote) {
                            Ok(_) => routed = true,
                            Err(Error::Disconnect) => online = false,
                            _ => {}
                        }
                    }

                    online
                });

                if (!routed || encoded_msg.selector.mode == SelectorMode::Multicast)
                    && encoded_msg.selector.label_op.validate(&self.label)
                {
                    match self.sender.send(encoded_msg) {
                        Ok(_) => {}
                        Err(err) => {
                            if !routed {
                                remain = Some(err.0);
                            }
                        }
                    }
                } else {
                    if !routed {
                        remain = Some(encoded_msg);
                    }
                }
            }
        }

        (remain, endpoint_connected)
    }

    fn endpoint_connect(&mut self, mut encoded_msg: EncodedMessage) -> bool {
        let remote = encoded_msg.extract_remote();
        let remote = if let Some(remote) = remote {
            remote
        } else {
            return false;
        };

        // TODO: Check size
        let payload = if let Ok(payload) = decode::<ConnectMessage>(encoded_msg.payload_data) {
            payload
        } else {
            let _ = Message::new(
                encoded_msg.selector.clone(),
                ConnectMessageAck::ErrVersion(version()),
            )
            .into_encoded()
            .send(&remote);
            return false;
        };

        // Ack
        if !payload.version.compatible(version()) {
            let _ = Message::new(
                encoded_msg.selector.clone(),
                ConnectMessageAck::ErrVersion(version()),
            )
            .into_encoded()
            .send(&remote);
            return false;
        }

        if payload.token != self.token {
            let _ = Message::new(encoded_msg.selector.clone(), ConnectMessageAck::ErrToken)
                .into_encoded()
                .send(&remote);
            return false;
        }

        let endpoint_id = EndpointID::new(); // TODO: Check conflict

        if let Err(err) = Message::new(
            encoded_msg.selector.clone(),
            ConnectMessageAck::Ok(endpoint_id),
        )
        .into_encoded()
        .send(&remote)
        {
            log::error!("connect ack: {:?}", err);
            return false;
        }

        let pair = Endpoint {
            id: endpoint_id,
            label: payload.label,
            remote,
        };

        if self
            .endpoints
            .iter()
            .any(|ep| ep.label == pair.label && ep.remote == pair.remote)
        {
            return false;
        }

        let _ = self.endpoints.push(pair);
        true
    }

    fn detect_reachable(&mut self, now: Instant) {
        if now - self.last_detect_reachable > Duration::from_secs(30) {
            self.endpoints.retain(|ep| !ep.remote.is_dead());

            self.last_detect_reachable = now;
        }
    }
}

#[derive(PartialEq)]
struct Endpoint {
    id: EndpointID,
    label: Label,
    remote: Remote,
}
