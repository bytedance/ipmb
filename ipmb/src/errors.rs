use crate::{platform::Remote, Version};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("encode error")]
    Encode(#[from] bincode::error::EncodeError),
    #[error("decode error")]
    Decode(#[from] bincode::error::DecodeError),
    #[error("type uuid not found")]
    TypeUuidNotFound,
    #[error("timeout")]
    Timeout,
    #[error("disconnected")]
    Disconnect,
    #[error("version mismatch: {0}")]
    VersionMismatch(Version, Option<Remote>),
    #[error("token mismatch")]
    TokenMismatch,
    #[error("identifier in use")]
    IdentifierInUse,
    #[error("identifier not in use")]
    IdentifierNotInUse,
    #[cfg(target_os = "windows")]
    #[error("win error: {0}")]
    WinError(#[from] windows::core::Error),
    #[error("memory region mapping error")]
    MemoryRegionMapping,
    #[error("unknown error")]
    Unknown,
}

#[derive(Debug, Error)]
pub enum JoinError {
    #[error("version mismatch: {0}")]
    VersionMismatch(Version),
    #[error("token mismatch")]
    TokenMismatch,
    #[error("timeout")]
    Timeout,
}

#[derive(Debug, Error)]
pub enum SendError {
    #[error("timeout")]
    Timeout,
    #[error("version mismatch: {0}")]
    VersionMismatch(Version),
    #[error("token mismatch")]
    TokenMismatch,
}

impl From<JoinError> for SendError {
    fn from(value: JoinError) -> Self {
        match value {
            JoinError::VersionMismatch(v) => Self::VersionMismatch(v),
            JoinError::TokenMismatch => Self::TokenMismatch,
            JoinError::Timeout => Self::Timeout,
        }
    }
}

#[derive(Debug, Error)]
pub enum RecvError {
    #[error("decode error")]
    Decode(#[from] bincode::error::DecodeError),
    #[error("timeout")]
    Timeout,
    #[error("version mismatch: {0}")]
    VersionMismatch(Version),
    #[error("token mismatch")]
    TokenMismatch,
}

impl From<JoinError> for RecvError {
    fn from(value: JoinError) -> Self {
        match value {
            JoinError::VersionMismatch(v) => Self::VersionMismatch(v),
            JoinError::TokenMismatch => Self::TokenMismatch,
            JoinError::Timeout => Self::Timeout,
        }
    }
}
