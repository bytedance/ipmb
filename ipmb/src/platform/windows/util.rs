use super::security::SecurityAttr;
use super::{pipe, Handle, Remote};
use crate::message::FetchProcessHandleMessage;
use crate::{decode, EncodedMessage, Error, LabelOp, Message, Selector};
use std::os::windows::prelude::{FromRawHandle, OwnedHandle};
use windows::Win32::Foundation;
use windows::Win32::Storage::FileSystem;
use windows::Win32::System::{Pipes, Threading, IO};

pub fn fetch_remote_process_handle(
    remote: &Remote,
    sa: &SecurityAttr,
) -> Result<OwnedHandle, Error> {
    unsafe {
        let (read_pipe, name) = pipe::anno_pipe_half(sa)?;
        // Don't append object/region
        Message::new(
            Selector::unicast(LabelOp::True),
            FetchProcessHandleMessage {
                pid: Threading::GetCurrentProcessId(),
                reply_pipe: name,
            },
        )
        .into_encoded()
        .send(remote)?;

        // Wait connect
        let event = Threading::CreateEventW(None, true, false, None)?;
        let _event = OwnedHandle::from_raw_handle(event.0 as _);
        let mut overlapped = IO::OVERLAPPED::default();
        overlapped.hEvent = event;

        macro_rules! wait {
            () => {
                match Threading::WaitForSingleObject(event, 2000) {
                    Foundation::WAIT_OBJECT_0 => {}
                    Foundation::WAIT_TIMEOUT => {
                        let _ = IO::CancelIoEx(read_pipe.as_raw_windows(), Some(&overlapped));
                        return Err(Error::Timeout);
                    }
                    _ => {
                        let _ = IO::CancelIoEx(read_pipe.as_raw_windows(), Some(&overlapped));
                        return Err(Error::WinError(windows::core::Error::from_win32()));
                    }
                }
            };
        }

        if !Pipes::ConnectNamedPipe(read_pipe.as_raw_windows(), Some(&mut overlapped)).as_bool() {
            match Foundation::GetLastError() {
                Foundation::ERROR_IO_PENDING | Foundation::ERROR_PIPE_LISTENING => {}
                _ => {
                    return Err(Error::WinError(windows::core::Error::from_win32()));
                }
            }
        }

        wait!();
        if !Threading::ResetEvent(event).as_bool() {
            return Err(Error::WinError(windows::core::Error::from_win32()));
        }

        // Read response
        let mut buf = 0i64;

        if !FileSystem::ReadFile(
            read_pipe.as_raw_windows(),
            Some((&mut buf) as *mut i64 as _),
            8,
            None,
            Some(&mut overlapped),
        )
        .as_bool()
        {
            match Foundation::GetLastError() {
                Foundation::ERROR_IO_PENDING | Foundation::ERROR_IO_INCOMPLETE => {}
                _ => {
                    return Err(Error::WinError(windows::core::Error::from_win32()));
                }
            }
        }

        wait!();

        Ok(OwnedHandle::from_raw_handle(buf as isize as _))
    }
}

pub(crate) fn reply_current_process_handle(encoded_msg: EncodedMessage) -> Result<(), Error> {
    let msg = decode::<FetchProcessHandleMessage>(encoded_msg.payload_data)?;
    let reply_pipe = windows::core::HSTRING::from(msg.reply_pipe);

    unsafe {
        // Open client process
        let client_process_h =
            Threading::OpenProcess(Threading::PROCESS_DUP_HANDLE, false, msg.pid)?;
        let client_process_h = Handle(OwnedHandle::from_raw_handle(client_process_h.0 as _));

        // Open staging pipe
        let pipe_handle = FileSystem::CreateFileW(
            &reply_pipe,
            FileSystem::FILE_GENERIC_WRITE.0,
            FileSystem::FILE_SHARE_NONE,
            None,
            FileSystem::OPEN_EXISTING,
            // We write synchronously, otherwise may incorrectly report that the write operation is complete
            FileSystem::FILE_FLAGS_AND_ATTRIBUTES::default(),
            None,
        )?;
        let pipe_handle = Handle(OwnedHandle::from_raw_handle(pipe_handle.0 as _));

        // Open current process
        let current_process_h = Handle(OwnedHandle::from_raw_handle(
            Threading::GetCurrentProcess().0 as _,
        ));

        // Duplicate to remote
        let mut remote_h = current_process_h.to_remote(&client_process_h)?;

        // Reply to remote
        let pseudo_h = remote_h.pseudo_handle.unwrap() as isize as i64;
        let buf = std::slice::from_raw_parts(&pseudo_h as *const i64 as *const u8, 8);
        if !FileSystem::WriteFile(pipe_handle.as_raw_windows(), Some(buf), None, None).as_bool() {
            return Err(Error::WinError(windows::core::Error::from_win32()));
        }

        remote_h.drain();
    }

    Ok(())
}
