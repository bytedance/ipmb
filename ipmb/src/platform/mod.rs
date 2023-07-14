#[cfg(target_os = "macos")]
pub type Object = self::macos::MachPort;
#[cfg(target_os = "windows")]
pub type Object = self::windows::Handle;

#[cfg(target_os = "macos")]
pub use self::macos::MemoryRegion;
#[cfg(target_os = "windows")]
pub use self::windows::MemoryRegion;

#[cfg(target_os = "macos")]
pub(crate) use self::macos::{look_up, register, EncodedMessage, IoHub, IoMultiplexing, Remote};
#[cfg(target_os = "windows")]
pub(crate) use self::windows::{look_up, register, EncodedMessage, IoHub, IoMultiplexing, Remote};

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

impl Clone for MemoryRegion {
    fn clone(&self) -> Self {
        Self::from_object(self.object().clone())
    }
}
