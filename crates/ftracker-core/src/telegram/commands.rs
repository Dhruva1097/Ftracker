/// Seconds before a PC may ack an update so every online session can poll it first.
pub const CONFIRM_AFTER_SECS: i64 = 20;
/// Ignore commands older than this (covers leftover queue after restart).
pub const STALE_AFTER_SECS: i64 = 120;

const FS_VERBS: &[&str] = &[
    "files",
    "get",
    "copy",
    "del",
    "delete",
    "programs",
    "uninstall",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminCommand {
    Online,
    MonthData,
    List,
    Who(String),
    WhoShot(String),
    Files {
        name: String,
        path: Option<String>,
    },
    Get {
        name: String,
        path: String,
    },
    Copy {
        name: String,
        src: String,
        dest: String,
    },
    Del {
        name: String,
        path: String,
    },
    Programs {
        name: String,
    },
    Uninstall {
        name: String,
        display: String,
    },
}

impl AdminCommand {
    /// Target employee / Windows user for PC-specific commands (never fleet-wide).
    pub fn target_name(&self) -> Option<&str> {
        match self {
            AdminCommand::Who(n)
            | AdminCommand::WhoShot(n)
            | AdminCommand::Files { name: n, .. }
            | AdminCommand::Get { name: n, .. }
            | AdminCommand::Copy { name: n, .. }
            | AdminCommand::Del { name: n, .. }
            | AdminCommand::Programs { name: n }
            | AdminCommand::Uninstall { name: n, .. } => Some(n),
            AdminCommand::Online | AdminCommand::MonthData | AdminCommand::List => None,
        }
    }
}

pub fn name_key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

pub fn names_match(query: &str, employee: &str, windows_user: &str) -> bool {
    let q = name_key(query);
    if q.is_empty() {
        return false;
    }
    q == name_key(employee) || q == name_key(windows_user)
}

pub fn chat_matches(msg_chat: &str, admin_chat: &str) -> bool {
    norm_chat(msg_chat) == norm_chat(admin_chat) && !norm_chat(admin_chat).is_empty()
}

fn norm_chat(s: &str) -> String {
    s.trim().trim_start_matches('+').to_string()
}

pub fn parse_command(text: &str) -> Option<AdminCommand> {
    let line = text.trim().lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return None;
    }
    let first = line.split_whitespace().next().unwrap_or("");
    if is_cmd(first, "online") {
        return Some(AdminCommand::Online);
    }
    if is_cmd(first, "data") {
        return Some(AdminCommand::MonthData);
    }
    if is_cmd(first, "list") {
        return Some(AdminCommand::List);
    }
    if let Some(rest) = line.strip_prefix('@') {
        let rest = rest.trim();
        if rest.is_empty() || rest.starts_with('@') {
            return None;
        }
        let (name, shot_only) = split_who(rest);
        if name.is_empty() {
            return None;
        }
        if shot_only {
            return Some(AdminCommand::WhoShot(name));
        }
        if let Some(cmd) = parse_named_op(&name) {
            return Some(cmd);
        }
        if rest.split_whitespace().any(is_fs_verb) {
            return None;
        }
        return Some(AdminCommand::Who(name));
    }
    None
}

fn parse_named_op(rest: &str) -> Option<AdminCommand> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let verb_at = tokens.iter().position(|t| is_fs_verb(t))?;
    if verb_at == 0 {
        return None;
    }
    let name = tokens[..verb_at].join(" ");
    if name.is_empty() {
        return None;
    }
    let verb = tokens[verb_at];
    let args = skip_tokens(rest, verb_at + 1);
    match verb.to_ascii_lowercase().as_str() {
        "files" => Some(AdminCommand::Files {
            name,
            path: if args.is_empty() { None } else { Some(args) },
        }),
        "get" => {
            if args.is_empty() {
                return None;
            }
            Some(AdminCommand::Get { name, path: args })
        }
        "copy" => {
            let (src, dest) = split_copy_args(&args)?;
            Some(AdminCommand::Copy { name, src, dest })
        }
        "del" | "delete" => {
            if args.is_empty() {
                return None;
            }
            Some(AdminCommand::Del { name, path: args })
        }
        "programs" => Some(AdminCommand::Programs { name }),
        "uninstall" => {
            if args.is_empty() {
                return None;
            }
            Some(AdminCommand::Uninstall {
                name,
                display: args,
            })
        }
        _ => None,
    }
}

fn is_fs_verb(tok: &str) -> bool {
    FS_VERBS.iter().any(|v| tok.eq_ignore_ascii_case(v))
}

fn skip_tokens(s: &str, n: usize) -> String {
    let mut rest = s.trim_start();
    for _ in 0..n {
        rest = rest.trim_start();
        match rest.find(char::is_whitespace) {
            Some(i) => rest = &rest[i..],
            None => return String::new(),
        }
    }
    rest.trim().to_string()
}

/// `SRC => DST`, `SRC -> DST`, quoted pair, or `SRC D:\dst`.
pub fn split_copy_args(s: &str) -> Option<(String, String)> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some((a, b)) = s.split_once("=>") {
        let a = a.trim();
        let b = b.trim().trim_start_matches('>').trim();
        if !a.is_empty() && !b.is_empty() {
            return Some((a.to_string(), b.to_string()));
        }
    }
    if let Some((a, b)) = s.split_once(" -> ") {
        let a = a.trim();
        let b = b.trim();
        if !a.is_empty() && !b.is_empty() {
            return Some((a.to_string(), b.to_string()));
        }
    }
    if let Some(inner) = s.strip_prefix('"') {
        if let Some(end) = inner.find('"') {
            let a = inner[..end].trim();
            let rest = inner[end + 1..].trim().trim_matches('"').trim();
            if !a.is_empty() && !rest.is_empty() {
                return Some((a.to_string(), rest.to_string()));
            }
        }
    }
    let bytes = s.as_bytes();
    for i in (2..bytes.len()).rev() {
        if bytes[i] != b':' {
            continue;
        }
        let drive = bytes[i - 1];
        if !drive.is_ascii_alphabetic() {
            continue;
        }
        if !bytes[i - 2].is_ascii_whitespace() {
            continue;
        }
        let src = s[..i - 1].trim();
        let dest = s[i - 1..].trim();
        if !src.is_empty() && dest.len() >= 2 {
            return Some((src.to_string(), dest.to_string()));
        }
    }
    None
}

/// `@{name}_ss` → screenshot only. Suffix is case-insensitive.
fn split_who(rest: &str) -> (String, bool) {
    let t = rest.trim();
    if t.len() > 3 {
        let (head, tail) = t.split_at(t.len() - 3);
        if tail.eq_ignore_ascii_case("_ss") {
            return (head.trim().to_string(), true);
        }
    }
    (t.to_string(), false)
}

fn is_cmd(tok: &str, name: &str) -> bool {
    if let Some(rest) = tok.strip_prefix('/') {
        return rest
            .split('@')
            .next()
            .unwrap_or(rest)
            .eq_ignore_ascii_case(name);
    }
    if let Some(rest) = tok.strip_prefix('@') {
        return rest.eq_ignore_ascii_case(name);
    }
    false
}

pub fn is_stale(message_date: i64, now: i64) -> bool {
    now.saturating_sub(message_date) > STALE_AFTER_SECS
}

pub fn should_confirm(message_date: i64, now: i64) -> bool {
    now.saturating_sub(message_date) >= CONFIRM_AFTER_SECS
}

/// Highest update_id that can be acknowledged without dropping a still-fresh command.
pub fn confirm_upto(
    updates: impl IntoIterator<Item = (i64, i64)>,
    last_confirmed: i64,
    now: i64,
) -> i64 {
    let mut v: Vec<(i64, i64)> = updates
        .into_iter()
        .filter(|(id, _)| *id > last_confirmed)
        .collect();
    v.sort_by_key(|(id, _)| *id);
    let mut confirmed = last_confirmed;
    for (id, date) in v {
        if should_confirm(date, now) {
            confirmed = id;
        } else {
            break;
        }
    }
    confirmed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_online_forms() {
        assert_eq!(parse_command("@online"), Some(AdminCommand::Online));
        assert_eq!(parse_command("@ONLINE"), Some(AdminCommand::Online));
        assert_eq!(
            parse_command("  @online please"),
            Some(AdminCommand::Online)
        );
        assert_eq!(parse_command("/online"), Some(AdminCommand::Online));
        assert_eq!(parse_command("/online@MyBot"), Some(AdminCommand::Online));
        assert_eq!(parse_command("@data"), Some(AdminCommand::MonthData));
        assert_eq!(parse_command("/data"), Some(AdminCommand::MonthData));
        assert_eq!(parse_command("@DATA please"), Some(AdminCommand::MonthData));
        assert_eq!(parse_command("/list"), Some(AdminCommand::List));
        assert_eq!(parse_command("@list"), Some(AdminCommand::List));
        assert_eq!(parse_command("/list@MyBot"), Some(AdminCommand::List));
        assert_eq!(parse_command("hello"), None);
        assert_eq!(parse_command(""), None);
    }

    #[test]
    fn parses_who() {
        assert_eq!(
            parse_command("@Alice"),
            Some(AdminCommand::Who("Alice".into()))
        );
        assert_eq!(
            parse_command("@Alice Smith"),
            Some(AdminCommand::Who("Alice Smith".into()))
        );
        assert_eq!(parse_command("@"), None);
        assert_eq!(
            parse_command("@Alice_ss"),
            Some(AdminCommand::WhoShot("Alice".into()))
        );
        assert_eq!(
            parse_command("@alice_SS"),
            Some(AdminCommand::WhoShot("alice".into()))
        );
        assert_eq!(
            parse_command("@Alice Smith_ss"),
            Some(AdminCommand::WhoShot("Alice Smith".into()))
        );
    }

    #[test]
    fn parses_named_file_ops() {
        assert_eq!(
            parse_command("@Alice files"),
            Some(AdminCommand::Files {
                name: "Alice".into(),
                path: None,
            })
        );
        assert_eq!(
            parse_command("@Alice Smith files C:\\Users"),
            Some(AdminCommand::Files {
                name: "Alice Smith".into(),
                path: Some(r"C:\Users".into()),
            })
        );
        assert_eq!(
            parse_command("@Alice get C:\\path\\file.pdf"),
            Some(AdminCommand::Get {
                name: "Alice".into(),
                path: r"C:\path\file.pdf".into(),
            })
        );
        assert_eq!(
            parse_command("@Alice copy C:\\src => D:\\dst"),
            Some(AdminCommand::Copy {
                name: "Alice".into(),
                src: r"C:\src".into(),
                dest: r"D:\dst".into(),
            })
        );
        assert_eq!(
            parse_command("@Alice copy C:\\Program Files\\Foo D:\\Backup\\Foo"),
            Some(AdminCommand::Copy {
                name: "Alice".into(),
                src: r"C:\Program Files\Foo".into(),
                dest: r"D:\Backup\Foo".into(),
            })
        );
        assert_eq!(
            parse_command("@Alice del C:\\Temp\\old"),
            Some(AdminCommand::Del {
                name: "Alice".into(),
                path: r"C:\Temp\old".into(),
            })
        );
        assert_eq!(
            parse_command("@Alice delete C:\\Temp\\old"),
            Some(AdminCommand::Del {
                name: "Alice".into(),
                path: r"C:\Temp\old".into(),
            })
        );
        assert_eq!(
            parse_command("@Alice programs"),
            Some(AdminCommand::Programs {
                name: "Alice".into(),
            })
        );
        assert_eq!(
            parse_command("@Alice uninstall Google Chrome"),
            Some(AdminCommand::Uninstall {
                name: "Alice".into(),
                display: "Google Chrome".into(),
            })
        );
        assert_eq!(
            parse_command("@Alice"),
            Some(AdminCommand::Who("Alice".into()))
        );
        assert_eq!(parse_command("@files"), None);
        assert_eq!(parse_command("@Alice get"), None);
        assert_eq!(parse_command("@Alice copy C:\\only"), None);
        assert!(matches!(
            parse_command("@Alice files"),
            Some(c) if c.target_name() == Some("Alice")
        ));
    }

    #[test]
    fn split_copy_paths() {
        assert_eq!(
            split_copy_args(r#"C:\src => D:\dst"#),
            Some((r"C:\src".into(), r"D:\dst".into()))
        );
        assert_eq!(
            split_copy_args(r#""C:\Program Files\A" "D:\B""#),
            Some((r"C:\Program Files\A".into(), r"D:\B".into()))
        );
        assert_eq!(
            split_copy_args(r"C:\a D:\b"),
            Some((r"C:\a".into(), r"D:\b".into()))
        );
        assert_eq!(split_copy_args("one"), None);
    }

    #[test]
    fn matches_employee_or_windows_user() {
        assert!(names_match("Alice", "Alice", "bob"));
        assert!(names_match("alice", "Alice Smith", "alice"));
        assert!(names_match("alice_smith", "Alice Smith", "bob"));
        assert!(names_match("AliceSmith", "Alice Smith", "bob"));
        assert!(!names_match("carol", "Alice", "bob"));
        assert!(!names_match("", "Alice", "bob"));
    }

    #[test]
    fn chat_id_compare() {
        assert!(chat_matches("12345", "12345"));
        assert!(chat_matches("12345", " 12345 "));
        assert!(chat_matches("-1001", "-1001"));
        assert!(!chat_matches("1", "2"));
        assert!(!chat_matches("1", ""));
    }

    #[test]
    fn delayed_confirm_keeps_fresh_prefix() {
        let now = 1_000_000;
        let old = now - CONFIRM_AFTER_SECS - 1;
        let fresh = now - 5;
        assert_eq!(
            confirm_upto([(10, old), (11, fresh), (12, fresh)], 0, now),
            10
        );
        assert_eq!(confirm_upto([(10, fresh), (11, fresh)], 0, now), 0);
        assert_eq!(confirm_upto([(10, old), (11, old)], 0, now), 11);
        assert!(is_stale(now - STALE_AFTER_SECS - 1, now));
        assert!(!is_stale(now - 10, now));
    }
}
