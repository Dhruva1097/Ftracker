#[cfg(windows)]
pub fn idle_ms() -> u64 {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    let ok = unsafe { GetLastInputInfo(&mut info) };
    if !ok.as_bool() {
        return 0;
    }
    let now = unsafe { GetTickCount() };
    now.wrapping_sub(info.dwTime) as u64
}

#[cfg(not(windows))]
pub fn idle_ms() -> u64 {
    0
}

pub fn is_idle(threshold_secs: u64) -> bool {
    idle_ms() >= threshold_secs.saturating_mul(1000)
}
