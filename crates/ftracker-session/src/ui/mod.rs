pub mod action_window;
pub mod chrome;
pub mod dpi;
pub mod enroll_window;
pub mod logo;
pub mod theme;
pub mod totp_dialog;
pub mod tray;

pub fn log(msg: impl AsRef<str>) {
    eprintln!("Ftracker: {}", msg.as_ref());
    let _ = std::io::Write::flush(&mut std::io::stderr());
}
