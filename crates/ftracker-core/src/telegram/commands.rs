/// Seconds before a PC may ack an update so every online session can poll it first.
pub const CONFIRM_AFTER_SECS: i64 = 50;
/// Ignore commands older than this (covers leftover queue after restart).
pub const STALE_AFTER_SECS: i64 = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminCommand {
    Online,
    MonthData,
    List,
    Who(String),
    WhoShot(String),
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
        return Some(AdminCommand::Who(name));
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
        assert_eq!(parse_command("  @online please"), Some(AdminCommand::Online));
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
        assert_eq!(confirm_upto([(10, old), (11, fresh), (12, fresh)], 0, now), 10);
        assert_eq!(confirm_upto([(10, fresh), (11, fresh)], 0, now), 0);
        assert_eq!(confirm_upto([(10, old), (11, old)], 0, now), 11);
        assert!(is_stale(now - STALE_AFTER_SECS - 1, now));
        assert!(!is_stale(now - 10, now));
    }
}
