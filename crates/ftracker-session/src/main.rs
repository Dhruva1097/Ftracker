#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod activity;
mod app;
mod cli;
mod enroll;
mod keepalive;
mod location;
mod mutex;
mod native_host;
mod screenshot;
mod shutdown;
mod ui;
mod uninstall;

fn main() {
    ftracker_core::config::load_dotenv();
    let cmd = cli::parse(std::env::args().skip(1).collect());
    let result = match cmd {
        cli::Cmd::Run { restarted } => app::run(restarted),
        cli::Cmd::Enroll { preassigned } => enroll::run(preassigned),
        cli::Cmd::Uninstall { otp } => uninstall::run(otp),
        cli::Cmd::NativeHost => native_host::run(),
        cli::Cmd::EnsureRunning => {
            if mutex::already_running() {
                return;
            }
            app::run(true)
        }
    };
    if let Err(e) = result {
        let msg = format!("{e:#}");
        eprintln!("Ftracker: {msg}");
        #[cfg(not(debug_assertions))]
        {
            let _ = tinyfiledialogs::message_box_ok(
                "Ftracker",
                &msg,
                tinyfiledialogs::MessageBoxIcon::Error,
            );
        }
        let handled = msg.contains("not registered")
            || msg.contains("enrol cancelled")
            || msg.contains("cancelled");
        if !handled {
            std::process::exit(1);
        }
    }
}
