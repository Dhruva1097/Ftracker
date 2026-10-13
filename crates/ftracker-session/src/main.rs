#![windows_subsystem = "windows"]

mod activity;
mod admin_cmd;
mod admin_fs;
mod app;
mod cli;
mod enroll;
mod keepalive;
mod location;
mod metrics;
mod mutex;
mod native_host;
mod permissions;
mod screenshot;
mod shutdown;
mod ui;
mod uninstall;
mod usb;

#[cfg(windows)]
mod wincons {
    #[link(name = "kernel32")]
    extern "system" {
        pub fn FreeConsole() -> i32;
    }
}

fn detach_console() {
    #[cfg(windows)]
    unsafe {
        let _ = wincons::FreeConsole();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let want_console = args.iter().any(|a| a == "--console");
    if !want_console {
        detach_console();
    }
    std::panic::set_hook(Box::new(|info| {
        eprintln!("Ftracker panic: {info}");
        if let Some(loc) = info.location() {
            eprintln!("  at {}:{}:{}", loc.file(), loc.line(), loc.column());
        }
        let _ = std::io::Write::flush(&mut std::io::stderr());
    }));
    let cmd = cli::parse(args);
    if matches!(cmd, cli::Cmd::UpgradeTotp) {
        ui::dpi::enable();
        match enroll::upgrade_totp() {
            Ok(()) => return,
            Err(_) => std::process::exit(1),
        }
    }
    ui::log("starting");
    ui::dpi::enable();
    ftracker_core::config::load_dotenv();
    ui::log(format!("command: {cmd:?}"));
    let result = match cmd {
        cli::Cmd::Run { restarted } => app::run(restarted, true),
        cli::Cmd::Enroll { preassigned } => enroll::run(preassigned),
        cli::Cmd::Uninstall { otp } => uninstall::run(otp),
        cli::Cmd::UpgradeTotp => unreachable!(),
        cli::Cmd::NativeHost => native_host::run(),
        cli::Cmd::EnsureRunning => app::run(true, false),
        cli::Cmd::Service { console } => {
            if console {
                ftracker_service::service::run_worker();
                Ok(())
            } else {
                ftracker_service::start_dispatcher()
            }
        }
    };
    if let Err(e) = result {
        let msg = format!("{e:#}");
        eprintln!("Ftracker error: {msg}");
        let _ = std::io::Write::flush(&mut std::io::stderr());
        let handled = msg.contains("not registered")
            || msg.contains("enrol cancelled")
            || msg.contains("cancelled")
            || msg.contains("os error 5")
            || msg.contains("Access is denied")
            || msg.contains("0x80070005")
            || msg.contains("db busy")
            || msg.contains("db lock");
        #[cfg(not(debug_assertions))]
        {
            if !handled {
                let _ = tinyfiledialogs::message_box_ok(
                    "Ftracker",
                    &msg,
                    tinyfiledialogs::MessageBoxIcon::Error,
                );
            }
        }
        if !handled {
            std::process::exit(1);
        }
    } else {
        ui::log("exit ok");
    }
}
