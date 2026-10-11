#[derive(Debug, Clone, Default)]
pub struct Foreground {
    pub hwnd: isize,
    pub title: String,
    pub process: String,
}

#[cfg(windows)]
pub fn read() -> Foreground {
    use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0 == 0 {
        return Foreground::default();
    }
    let mut title_buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut title_buf) };
    let title = String::from_utf16_lossy(&title_buf[..n as usize]);
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    let mut process = String::new();
    if pid != 0 {
        if let Ok(h) = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
            let mut buf = [0u16; MAX_PATH as usize];
            let mut len = buf.len() as u32;
            let ok = unsafe { QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len) };
            if ok.is_ok() {
                process = String::from_utf16_lossy(&buf[..len as usize]);
            }
            unsafe {
                let _ = CloseHandle(h);
            }
        }
    }
    Foreground {
        hwnd: hwnd.0,
        title,
        process,
    }
}

#[cfg(not(windows))]
pub fn read() -> Foreground {
    Foreground::default()
}

pub fn process_file_name(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).to_string()
}
