#[derive(Debug)]
pub enum Cmd {
    Run { restarted: bool },
    Enroll { preassigned: bool },
    Uninstall { otp: Option<String> },
    NativeHost,
    EnsureRunning,
}

pub fn parse(args: Vec<String>) -> Cmd {
    let mut enroll = false;
    let mut preassigned = false;
    let mut uninstall = false;
    let mut native = false;
    let mut ensure = false;
    let mut otp = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--enroll" => enroll = true,
            "--preassigned" | "--pre-assigned" => preassigned = true,
            "--uninstall" => uninstall = true,
            "--native-host" => native = true,
            "--ensure-running" => ensure = true,
            "--otp" => {
                if i + 1 < args.len() {
                    i += 1;
                    otp = Some(args[i].clone());
                }
            }
            s if s.starts_with("--otp=") => otp = Some(s[6..].to_string()),
            _ => {}
        }
        i += 1;
    }
    if native {
        Cmd::NativeHost
    } else if ensure {
        Cmd::EnsureRunning
    } else if uninstall {
        Cmd::Uninstall { otp }
    } else if enroll {
        Cmd::Enroll { preassigned }
    } else {
        Cmd::Run { restarted: false }
    }
}
