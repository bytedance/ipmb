use super::security::SecurityAttr;
use super::Handle;
use crate::{util, Error};
use std::os::windows::prelude::{FromRawHandle, HandleOrInvalid, OwnedHandle};
use windows::core::HSTRING;
use windows::Win32::Foundation;
use windows::Win32::Storage::FileSystem;
use windows::Win32::System::Pipes;

pub(crate) const DEFAULT_BUFFER_SIZE: u32 = 4 << 20;

pub unsafe fn anon_pipe(sa: &SecurityAttr) -> Result<(Handle, Handle), Error> {
    let name: HSTRING = format!("\\\\.\\pipe\\{}", util::rand_string(8)).into();

    // Create read pipe
    let pipe_handle = Pipes::CreateNamedPipeW(
        &name,
        FileSystem::PIPE_ACCESS_INBOUND
            | FileSystem::FILE_FLAG_OVERLAPPED
            | FileSystem::FILE_FLAG_FIRST_PIPE_INSTANCE,
        Pipes::PIPE_TYPE_MESSAGE | Pipes::PIPE_READMODE_MESSAGE | Pipes::PIPE_WAIT,
        1,
        DEFAULT_BUFFER_SIZE,
        DEFAULT_BUFFER_SIZE,
        0,
        Some(sa.attr()),
    );

    let pipe_handle: OwnedHandle = HandleOrInvalid::from_raw_handle(pipe_handle.0 as _)
        .try_into()
        .map_err(|_| windows::core::Error::from_win32())?;
    let read_pipe = Handle(pipe_handle);

    // Open write pipe
    let pipe_handle: Foundation::HANDLE = FileSystem::CreateFileW(
        &name,
        FileSystem::FILE_GENERIC_WRITE.0,
        FileSystem::FILE_SHARE_NONE,
        None,
        FileSystem::OPEN_EXISTING,
        // We write synchronously, otherwise may incorrectly report that the write operation is complete
        FileSystem::FILE_FLAGS_AND_ATTRIBUTES::default(),
        None,
    )?;

    let pipe_handle = OwnedHandle::from_raw_handle(pipe_handle.0 as _);
    let write_pipe = Handle(pipe_handle);

    Ok((read_pipe, write_pipe))
}

pub unsafe fn anno_pipe_half(sa: &SecurityAttr) -> Result<(Handle, String), Error> {
    let name = format!("\\\\.\\pipe\\{}", util::rand_string(8));
    let h_name: HSTRING = (&name).into();

    // Create read pipe
    let pipe_handle = Pipes::CreateNamedPipeW(
        &h_name,
        FileSystem::PIPE_ACCESS_INBOUND
            | FileSystem::FILE_FLAG_OVERLAPPED
            | FileSystem::FILE_FLAG_FIRST_PIPE_INSTANCE,
        Pipes::PIPE_TYPE_MESSAGE | Pipes::PIPE_READMODE_MESSAGE | Pipes::PIPE_WAIT,
        1,
        DEFAULT_BUFFER_SIZE,
        DEFAULT_BUFFER_SIZE,
        0,
        Some(sa.attr()),
    );

    let pipe_handle: OwnedHandle = HandleOrInvalid::from_raw_handle(pipe_handle.0 as _)
        .try_into()
        .map_err(|_| windows::core::Error::from_win32())?;
    let read_pipe = Handle(pipe_handle);

    Ok((read_pipe, name))
}
