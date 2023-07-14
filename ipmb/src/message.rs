use crate::{EndpointID, Error, Label, MemoryRegion, Object, Selector, Version};
use serde::{Deserialize, Serialize};
use type_uuid::{Bytes, TypeUuid};

pub struct Message<T> {
    pub(crate) selector: Selector,
    pub payload: T,
    pub objects: Vec<Object>,
    pub memory_regions: Vec<MemoryRegion>,
}

impl<T: MessageBox> Message<T> {
    pub fn new(mut selector: Selector, payload: T) -> Self {
        selector.uuid = payload.uuid();

        Self {
            selector,
            payload,
            objects: vec![],
            memory_regions: vec![],
        }
    }
}

pub trait MessageBox: Send + 'static {
    fn decode(uuid: Bytes, data: &[u8]) -> Result<Self, Error>
    where
        Self: Sized;

    fn encode(&self) -> Result<Vec<u8>, Error>;

    fn uuid(&self) -> Bytes;
}

/// A predefined message type.
#[derive(Debug, Serialize, Deserialize, TypeUuid)]
#[uuid = "dd95ba8e-1279-47cf-925e-83e614e79588"]
pub struct BytesMessage {
    pub format: u16,
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

#[cfg(windows)]
#[derive(Debug, Serialize, Deserialize, TypeUuid)]
#[uuid = "fbf88372-d2cd-425a-a183-133f8f119df2"]
pub struct FetchProcessHandleMessage {
    pub pid: u32,
    pub reply_pipe: String,
}

#[derive(Debug, Serialize, Deserialize, TypeUuid)]
#[uuid = "b2c1deb3-3091-4a74-a99c-c8e8d710d4b2"]
pub struct ConnectMessage {
    pub version: Version,
    pub token: String,
    pub label: Label,
}

#[derive(Debug, Serialize, Deserialize, TypeUuid)]
#[uuid = "c3de9eb4-c310-4c14-9747-093d62c09998"]
pub enum ConnectMessageAck {
    Ok(EndpointID),
    ErrVersion(Version),
    ErrToken,
}

impl<T: TypeUuid + Serialize + for<'de> Deserialize<'de> + Send + 'static> MessageBox for T {
    fn decode(uuid: Bytes, data: &[u8]) -> Result<Self, Error>
    where
        Self: Sized,
    {
        if uuid == T::UUID {
            crate::decode(data)
        } else {
            Err(Error::TypeUuidNotFound)
        }
    }

    fn encode(&self) -> Result<Vec<u8>, Error> {
        crate::encode(self)
    }

    fn uuid(&self) -> Bytes {
        T::UUID
    }
}
