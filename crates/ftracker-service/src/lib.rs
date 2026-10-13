//! Background worker: named-pipe IPC, USB eject backup, integrity.
//! The session exe (`ftracker.exe --service`) hosts this so there is only one binary.

pub mod ipc_server;
pub mod service;
pub mod usb;

use std::ffi::OsString;

use windows_service::define_windows_service;

define_windows_service!(ffi_service_main, service_main);

fn service_main(_args: Vec<OsString>) {
    if let Err(e) = service::run_as_service() {
        eprintln!("ftracker service: {e:#}");
    }
}

pub fn start_dispatcher() -> anyhow::Result<()> {
    windows_service::service_dispatcher::start("FtrackerSvc", ffi_service_main)?;
    Ok(())
}
