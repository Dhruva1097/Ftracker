use anyhow::{anyhow, Result};

pub struct DbLock {
    #[cfg(windows)]
    handle: windows::Win32::Foundation::HANDLE,
}

// Local, not Global: creating Global\\* needs SeCreateGlobalPrivilege. The
// session process runs as the logged-on user (medium IL after MSI launch) and
// CreateMutexW then returns 0x80070005, which was mis-reported as DB_CORRUPT.
const NAME: &str = "Local\\FtrackerDbMutex";

impl DbLock {
    pub fn acquire(wait_ms: u32) -> Result<Self> {
        #[cfg(windows)]
        {
            use windows::core::PCWSTR;
            use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT};
            use windows::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};
            let wide: Vec<u16> = NAME.encode_utf16().chain(std::iter::once(0)).collect();
            let handle = unsafe { CreateMutexW(None, false, PCWSTR(wide.as_ptr()))? };
            let w = unsafe { WaitForSingleObject(handle, wait_ms) };
            if w == WAIT_TIMEOUT {
                unsafe {
                    let _ = CloseHandle(handle);
                }
                return Err(anyhow!("db busy"));
            }
            if w != WAIT_OBJECT_0 {
                unsafe {
                    let _ = CloseHandle(handle);
                }
                return Err(anyhow!("db lock failed"));
            }
            return Ok(Self { handle });
        }
        #[cfg(not(windows))]
        {
            let _ = wait_ms;
            Ok(Self {})
        }
    }
}

impl Drop for DbLock {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::CloseHandle;
            use windows::Win32::System::Threading::ReleaseMutex;
            unsafe {
                let _ = ReleaseMutex(self.handle);
                let _ = CloseHandle(self.handle);
            }
        }
    }
}
