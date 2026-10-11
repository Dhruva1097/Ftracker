use anyhow::{anyhow, Result};
use std::fs::File;
use std::io::{Read, Write};

use crate::protocol::IpcMsg;

pub const PIPE_NAME: &str = r"\\.\pipe\ftracker-ipc";

pub fn write_msg<W: Write>(w: &mut W, msg: &IpcMsg) -> Result<()> {
    let json = serde_json::to_vec(msg)?;
    w.write_all(&(json.len() as u32).to_le_bytes())?;
    w.write_all(&json)?;
    w.flush()?;
    Ok(())
}

pub fn read_msg<R: Read>(r: &mut R) -> Result<IpcMsg> {
    let mut lenb = [0u8; 4];
    r.read_exact(&mut lenb)?;
    let n = u32::from_le_bytes(lenb) as usize;
    if n == 0 || n > 1024 * 1024 {
        return Err(anyhow!("ipc message too large"));
    }
    let mut body = vec![0u8; n];
    r.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body)?)
}

#[cfg(windows)]
pub fn create_server_instance() -> Result<File> {
    use std::os::windows::io::FromRawHandle;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
    use windows::Win32::System::Pipes::{
        CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };

    let sddl: Vec<u16> = "D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;AU)\0"
        .encode_utf16()
        .collect();
    let mut psd = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut psd,
            None,
        )?;
    }
    let mut sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: psd.0 as *mut _,
        bInheritHandle: false.into(),
    };
    let name: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe {
        CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            65536,
            65536,
            0,
            Some(&mut sa),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(anyhow!("CreateNamedPipeW failed"));
    }
    Ok(unsafe { File::from_raw_handle(handle.0 as _) })
}

#[cfg(windows)]
pub fn wait_for_client(server: &File) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Pipes::ConnectNamedPipe;
    let h = HANDLE(server.as_raw_handle() as isize);
    let _ = unsafe { ConnectNamedPipe(h, None) };
    Ok(())
}

#[cfg(windows)]
pub fn disconnect_client(server: &File) {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Pipes::DisconnectNamedPipe;
    let h = HANDLE(server.as_raw_handle() as isize);
    unsafe {
        let _ = DisconnectNamedPipe(h);
    }
}

#[cfg(windows)]
pub fn connect_client(timeout_ms: u32) -> Result<File> {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
    use windows::Win32::System::Pipes::WaitNamedPipeW;

    let name: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let _ = WaitNamedPipeW(PCWSTR(name.as_ptr()), timeout_ms);
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
        .open(PIPE_NAME)
        .map_err(|e| anyhow!("pipe connect: {e}"))
}

#[cfg(windows)]
pub fn bytes_available(file: &File) -> u32 {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Pipes::PeekNamedPipe;
    let mut avail = 0u32;
    let h = HANDLE(file.as_raw_handle() as isize);
    let _ = unsafe { PeekNamedPipe(h, None, 0, None, Some(&mut avail), None) };
    avail
}

#[cfg(not(windows))]
pub fn bytes_available(_file: &File) -> u32 {
    0
}

#[cfg(not(windows))]
pub fn connect_client(_timeout_ms: u32) -> Result<File> {
    Err(anyhow!("windows only"))
}
