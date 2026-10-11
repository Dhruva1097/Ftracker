use crate::domain::location::{loc_text, GeoFix};
use crate::domain::time::format_hm;
use crate::VERSION;

pub fn registered(
    employee: &str,
    pc: &str,
    windows_user: &str,
    loc: Option<&GeoFix>,
) -> String {
    format!(
        "REGISTERED time={} pc={pc} employee={employee} windows_user={windows_user} {} version={VERSION}",
        now_stamp(),
        loc_text(loc)
    )
}

pub fn install_failed(pc: &str, reason: &str, loc: Option<&GeoFix>) -> String {
    format!("INSTALL_FAILED pc={pc} reason={reason} {}", loc_text(loc))
}

pub fn uninstalled(pc: &str, windows_user: &str, loc: Option<&GeoFix>) -> String {
    format!(
        "UNINSTALLED time={} pc={pc} windows_user={windows_user} {} by=admin_totp",
        now_stamp(),
        loc_text(loc)
    )
}

pub fn uninstall_denied(pc: &str, windows_user: &str, loc: Option<&GeoFix>) -> String {
    format!(
        "UNINSTALL_DENIED time={} pc={pc} windows_user={windows_user} {}",
        now_stamp(),
        loc_text(loc)
    )
}

pub fn process_restarted(employee: &str, pc: &str, loc: Option<&GeoFix>) -> String {
    format!(
        "PROCESS_RESTARTED employee={employee} pc={pc} after=kill|crash delay=60s {}",
        loc_text(loc)
    )
}

pub fn integrity_fail(
    employee: &str,
    pc: &str,
    file: &str,
    reason: &str,
    expected: &str,
    got: &str,
    loc: Option<&GeoFix>,
) -> String {
    format!(
        "INTEGRITY_FAIL employee={employee} pc={pc} file={file} reason={reason} expected={expected} got={got}\n{}",
        loc_text(loc)
    )
}

pub fn db_corrupt(employee: &str, pc: &str, loc: Option<&GeoFix>) -> String {
    format!("DB_CORRUPT employee={employee} pc={pc} {}", loc_text(loc))
}

pub fn secrets_corrupt(pc: &str, loc: Option<&GeoFix>) -> String {
    format!("SECRETS_CORRUPT pc={pc} {}", loc_text(loc))
}

pub fn summary(
    employee: &str,
    pc: &str,
    range: &str,
    active_s: i64,
    idle_s: i64,
    cats: &[(String, i64)],
    loc: Option<&GeoFix>,
) -> String {
    let mut cat_line = String::new();
    for name in ["Browser", "IDE", "Office", "Other"] {
        let s = cats
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| *v)
            .unwrap_or(0);
        if !cat_line.is_empty() {
            cat_line.push(' ');
        }
        cat_line.push_str(&format!("{name}={}", format_hm(s)));
    }
    format!(
        "SUMMARY employee={employee} pc={pc} {range} version={VERSION}\nactive={} idle_excl={}\n{cat_line}\n{}",
        format_hm(active_s),
        format_hm(idle_s),
        loc_text(loc)
    )
}

pub fn hours_short_admin(
    employee: &str,
    day: &str,
    required_s: i64,
    got_s: i64,
    loc: Option<&GeoFix>,
) -> String {
    let missing = (required_s - got_s).max(0);
    format!(
        "HOURS_SHORT employee={employee} day={day} required={} got={} missing={}\n{}",
        format_hm(required_s),
        format_hm(got_s),
        format_hm(missing),
        loc_text(loc)
    )
}

pub fn hours_short_employee(got_s: i64, required_s: i64) -> String {
    format!(
        "Yesterday {} of {} required",
        format_hm(got_s),
        format_hm(required_s)
    )
}

pub fn daily_caption(
    employee: &str,
    pc: &str,
    day: &str,
    active_s: i64,
    loc_first: &str,
    loc_last: &str,
    fixes: i64,
    retry: bool,
) -> String {
    let kind = if retry {
        "DAILY_REPORT_RETRY"
    } else {
        "DAILY_REPORT"
    };
    format!(
        "{kind} employee={employee} pc={pc} day={day} active={} loc_first={loc_first} loc_last={loc_last} fixes={fixes} version={VERSION}",
        format_hm(active_s)
    )
}

pub fn employee_daily(day_pretty: &str, got_s: i64, required_s: i64) -> String {
    format!(
        "{day_pretty} — tracked time {} of {} required",
        format_hm(got_s),
        format_hm(required_s)
    )
}

pub fn employee_subscribed() -> String {
    "subscribed — you will get daily tracked time only".into()
}

pub fn employee_subscribed_admin(pc: &str, windows_user: &str, loc: Option<&GeoFix>) -> String {
    format!(
        "EMPLOYEE_SUBSCRIBED pc={pc} windows_user={windows_user} {}",
        loc_text(loc)
    )
}

pub fn employee_changed_admin(pc: &str, windows_user: &str, loc: Option<&GeoFix>) -> String {
    format!(
        "EMPLOYEE_CHANGED pc={pc} windows_user={windows_user} {}",
        loc_text(loc)
    )
}

pub fn employee_unsubscribed_admin(pc: &str, windows_user: &str, loc: Option<&GeoFix>) -> String {
    format!(
        "EMPLOYEE_UNSUBSCRIBED pc={pc} windows_user={windows_user} {}",
        loc_text(loc)
    )
}

pub fn reregistered(old: &str, new: &str, pc: &str, loc: Option<&GeoFix>) -> String {
    format!(
        "EMPLOYEE_REREGISTERED old={old} new={new} pc={pc} {}",
        loc_text(loc)
    )
}

pub fn usb(
    kind: &str,
    user: &str,
    pc: &str,
    letter: &str,
    name: &str,
    serial: &str,
    vidpid: &str,
    extra: &str,
    loc: Option<&GeoFix>,
) -> String {
    format!(
        "{kind} user={user} pc={pc} letter={letter} name={name} serial={serial} vidpid={vidpid} {} {extra}",
        loc_text(loc)
    )
}

pub fn upload(
    kind: &str,
    user: &str,
    pc: &str,
    browser: &str,
    site: &str,
    name: &str,
    size_mb: u64,
    extra: &str,
    loc: Option<&GeoFix>,
) -> String {
    format!(
        "{kind} user={user} pc={pc} browser={browser} site={site} name={name} size_mb={size_mb} {} {extra}",
        loc_text(loc)
    )
}

fn now_stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}
