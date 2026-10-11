use std::sync::atomic::{AtomicBool, Ordering};

static HELD: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
static mut HANDLE: windows::Win32::Foundation::HANDLE = windows::Win32::Foundation::HANDLE(0);

pub fn already_running() -> bool {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::System::Threading::CreateMutexW;
        extern "system" {
            fn GetLastError() -> u32;
        }
        let name: Vec<u16> = ftracker_core::paths::mutex_name()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let h = unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) };
        match h {
            Ok(handle) => {
                let exists = unsafe { GetLastError() } == 183;
                if exists {
                    true
                } else {
                    unsafe { HANDLE = handle };
                    HELD.store(true, Ordering::SeqCst);
                    false
                }
            }
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub fn acquire_or_exit() {
    if already_running() {
        std::process::exit(0);
    }
}

#[allow(dead_code)]
pub fn hold_for_process() {
    let _ = HELD.load(Ordering::SeqCst);
}
