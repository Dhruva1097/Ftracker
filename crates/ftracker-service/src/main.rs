#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ipc_server;
mod service;
mod usb;

use std::ffi::OsString;
use windows_service::define_windows_service;

define_windows_service!(ffi_service_main, service_main);

fn main() -> anyhow::Result<()> {
    ftracker_core::config::load_dotenv();
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--console") {
        service::run_worker();
        return Ok(());
    }
    windows_service::service_dispatcher::start("FtrackerSvc", ffi_service_main)?;
    Ok(())
}

fn service_main(_args: Vec<OsString>) {
    if let Err(e) = service::run_as_service() {
        eprintln!("ftracker-svc: {e:#}");
    }
}
