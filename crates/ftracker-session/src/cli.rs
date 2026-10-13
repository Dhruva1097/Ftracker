#[derive(Debug)]
pub enum Cmd {
    Run { restarted: bool },
    Enroll { preassigned: bool },
    Uninstall { otp: Option<String> },
    NativeHost,
    EnsureRunning,
    Service { console: bool },
}

pub fn parse(args: Vec<String>) -> Cmd {
    let mut enroll = false;
    let mut preassigned = false;
    let mut uninstall = false;
    let mut native = false;
    let mut ensure = false;
    let mut service = false;
    let mut console = false;
    let mut otp = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--enroll" => enroll = true,
            "--preassigned" | "--pre-assigned" => preassigned = true,
            "--uninstall" => uninstall = true,
            "--native-host" => native = true,
            "--ensure-running" => ensure = true,
            "--service" => service = true,
            "--console" => console = true,
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
    } else if service || console {
        Cmd::Service { console: console || !service }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Cmd {
        parse(args.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn parses_flags() {
        assert!(matches!(p(&["--enroll"]), Cmd::Enroll { preassigned: false }));
        assert!(matches!(
            p(&["--enroll", "--preassigned"]),
            Cmd::Enroll { preassigned: true }
        ));
        assert!(matches!(p(&["--ensure-running"]), Cmd::EnsureRunning));
        assert!(matches!(p(&["--native-host"]), Cmd::NativeHost));
        assert!(matches!(p(&["--service"]), Cmd::Service { console: false }));
        assert!(matches!(p(&["--console"]), Cmd::Service { console: true }));
        match p(&["--uninstall", "--otp", "123456"]) {
            Cmd::Uninstall { otp } => assert_eq!(otp.as_deref(), Some("123456")),
            _ => panic!("expected uninstall"),
        }
        assert!(matches!(p(&[]), Cmd::Run { restarted: false }));
    }
}
