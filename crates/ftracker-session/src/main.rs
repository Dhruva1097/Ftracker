#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod activity;
mod admin_cmd;
mod app;
mod cli;
mod metrics;
mod enroll;
mod keepalive;
mod location;
mod mutex;
mod native_host;
mod permissions;
mod screenshot;
mod shutdown;
mod ui;
mod uninstall;

fn main() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("Ftracker panic: {info}");
        if let Some(loc) = info.location() {
            eprintln!("  at {}:{}:{}", loc.file(), loc.line(), loc.column());
        }
        let _ = std::io::Write::flush(&mut std::io::stderr());
    }));
    ui::log("starting");
    ui::dpi::enable();
    ftracker_core::config::load_dotenv();
    let args: Vec<String> = std::env::args().skip(1).collect();
    ui::log(format!("args: {args:?}"));
    let cmd = cli::parse(args);
    ui::log(format!("command: {cmd:?}"));
    let result = match cmd {
        cli::Cmd::Run { restarted } => app::run(restarted, true),
        cli::Cmd::Enroll { preassigned } => enroll::run(preassigned),
        cli::Cmd::Uninstall { otp } => uninstall::run(otp),
        cli::Cmd::NativeHost => native_host::run(),
        cli::Cmd::EnsureRunning => app::run(true, false),
    };
    if let Err(e) = result {
        let msg = format!("{e:#}");
        eprintln!("Ftracker error: {msg}");
        let _ = std::io::Write::flush(&mut std::io::stderr());
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
            || msg.contains("cancelled")
            || msg.contains("os error 5")
            || msg.contains("Access is denied");
        if !handled {
            std::process::exit(1);
        }
    } else {
        ui::log("exit ok");
    }
}
