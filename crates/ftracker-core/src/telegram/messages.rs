use crate::domain::location::{loc_text, location_lines, GeoFix};
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
    attendance_in(employee, "", pc, "restart_after_kill", loc)
}

pub fn attendance_in(
    employee: &str,
    windows_user: &str,
    pc: &str,
    reason: &str,
    loc: Option<&GeoFix>,
) -> String {
    let title = if reason == "restart_after_kill" {
        "Attendance — back online"
    } else {
        "Attendance — signed in"
    };
    format!(
        "{title}\n\nEmployee: {employee}\nWindows user: {windows_user}\nPC: {pc}\nTime: {}\nReason: {reason}\nVersion: {VERSION}\n{}",
        now_stamp(),
        location_lines(loc)
    )
}

pub fn attendance_out(
    employee: &str,
    windows_user: &str,
    pc: &str,
    reason: &str,
    tracked_today_s: i64,
    loc: Option<&GeoFix>,
) -> String {
    format!(
        "Attendance — signed out\n\nEmployee: {employee}\nWindows user: {windows_user}\nPC: {pc}\nTime: {}\nReason: {reason}\nTracked today: {}\nVersion: {VERSION}\n{}",
        now_stamp(),
        format_hm(tracked_today_s),
        location_lines(loc)
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

pub fn command_list() -> String {
    format!(
        "Ftracker admin commands\n\n\
@online\n\
  Every online PC replies that it is online (employee, Windows user, PC, status, location).\n\
  Also: /online\n\n\
@data\n\
  Each online PC sends a PDF of the last 30 days of daily hours (date-wise table).\n\
  Also: /data\n\n\
@{{name}}\n\
  Live snapshot of that person: screenshot, location, apps, USB, CPU/RAM/disk, tracked time today.\n\
  {{name}} is the employee name or the Windows login.\n\n\
@{{name}}_ss\n\
  Screenshot only of that person’s current screen.\n\n\
/list\n\
  Lists these commands.\n\
  Also: @list\n\n\
Version: {VERSION}"
    )
}

pub fn month_data_caption(
    employee: &str,
    windows_user: &str,
    pc: &str,
    from: &str,
    to: &str,
    tracked_s: i64,
    required_s: i64,
    short_days: usize,
) -> String {
    format!(
        "Monthly hours report (last 30 days)\n\nEmployee: {employee}\nWindows user: {windows_user}\nPC: {pc}\nPeriod: {from} to {to}\nTracked: {} of {}\nShort weekdays: {short_days}\nVersion: {VERSION}",
        format_hm(tracked_s),
        format_hm(required_s)
    )
}

pub fn online_here(
    employee: &str,
    windows_user: &str,
    pc: &str,
    idle: bool,
    loc: Option<&GeoFix>,
) -> String {
    let status = if idle { "idle" } else { "active" };
    format!(
        "I am online\n\nEmployee: {employee}\nWindows user: {windows_user}\nPC: {pc}\nStatus: {status}\nTime: {}\nVersion: {VERSION}\n{}",
        now_stamp(),
        location_lines(loc)
    )
}

pub struct LiveSnapshot {
    pub employee: String,
    pub windows_user: String,
    pub pc: String,
    pub idle: bool,
    pub tracked_today_s: i64,
    pub required_s: i64,
    pub cats: Vec<(String, i64)>,
    pub fg_process: String,
    pub fg_title: String,
    pub apps: Vec<String>,
    pub usb: Vec<String>,
    pub cpu_pct: u32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub disk_root: String,
    pub disk_free_gb: f64,
    pub disk_total_gb: f64,
    pub loc: Option<GeoFix>,
}

pub fn live_status(s: &LiveSnapshot) -> String {
    let status = if s.idle { "idle" } else { "active" };
    let mut cats = String::new();
    for name in ["Browser", "IDE", "Office", "Other"] {
        let sec = s
            .cats
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| *v)
            .unwrap_or(0);
        cats.push_str(&format!("  {name}: {}\n", format_hm(sec)));
    }
    let apps = if s.apps.is_empty() {
        "  none".into()
    } else {
        s.apps
            .iter()
            .map(|a| format!("  {a}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let usb = if s.usb.is_empty() {
        "none".into()
    } else {
        s.usb.join(", ")
    };
    format!(
        "Live status\n\n\
Employee: {}\n\
Windows user: {}\n\
PC: {}\n\
Time: {}\n\
Status: {status}\n\
Version: {VERSION}\n\n\
Tracked today: {} / {}\n\
{cats}\n\
Foreground: {}\n\
Window: {}\n\n\
Open apps:\n\
{apps}\n\n\
External devices: {usb}\n\
CPU: {}%\n\
RAM: {} / {} GB\n\
{}\n\
{}",
        s.employee,
        s.windows_user,
        s.pc,
        now_stamp(),
        format_hm(s.tracked_today_s),
        format_hm(s.required_s),
        s.fg_process,
        s.fg_title,
        s.cpu_pct,
        fmt_gb(s.ram_used_mb),
        fmt_gb(s.ram_total_mb),
        fmt_drive(s),
        location_lines(s.loc.as_ref())
    )
}

fn fmt_gb(mb: u64) -> String {
    format!("{:.1}", mb as f64 / 1024.0)
}

fn fmt_drive(s: &LiveSnapshot) -> String {
    let letter = s
        .disk_root
        .chars()
        .next()
        .unwrap_or('C')
        .to_ascii_uppercase();
    let used = (s.disk_total_gb - s.disk_free_gb).max(0.0).round() as i64;
    let total = s.disk_total_gb.round() as i64;
    format!("{letter} Drive: {used} Used / {total} GB")
}

pub fn screenshot_only_caption(employee: &str, pc: &str) -> String {
    format!(
        "Screenshot\n\nEmployee: {employee}\nPC: {pc}\nTime: {}",
        now_stamp()
    )
}

fn now_stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::location::GeoFix;

    #[test]
    fn employee_messages_have_no_coords_or_maps() {
        let fix = GeoFix {
            lat: 12.9716,
            lon: 77.5946,
            accuracy_m: 20.0,
        };
        let daily = employee_daily("11 Oct 2026", 7 * 3600 + 12 * 60, 8 * 3600);
        assert!(!daily.contains("12.97"));
        assert!(!daily.contains("maps.google"));
        assert!(daily.contains("tracked time"));
        let admin = hours_short_admin("Alice", "2026-10-10", 8 * 3600, 6 * 3600 + 10 * 60, Some(&fix));
        assert!(admin.contains("loc="));
        let emp = hours_short_employee(6 * 3600 + 10 * 60, 8 * 3600);
        assert!(!emp.contains("loc="));
        assert!(!emp.contains("maps.google"));
        let inn = attendance_in("Alice", "alice", "PC1", "sign_in", Some(&fix));
        assert!(inn.contains("Attendance — signed in"));
        assert!(inn.contains("Employee: Alice"));
        assert!(inn.contains("Location:"));
        let out = attendance_out("Alice", "alice", "PC1", "logoff_or_shutdown", 7 * 3600, Some(&fix));
        assert!(out.contains("Attendance — signed out"));
        assert!(out.contains("Tracked today:"));
        assert!(out.contains("Location:"));
        let inn_on = online_here("Alice", "alice", "PC1", false, Some(&fix));
        assert!(inn_on.contains("I am online"));
        assert!(inn_on.contains("Employee: Alice"));
        assert!(inn_on.contains("Status: active"));
        assert!(inn_on.contains("Location:"));
        let live = live_status(&LiveSnapshot {
            employee: "Alice".into(),
            windows_user: "alice".into(),
            pc: "PC1".into(),
            idle: false,
            tracked_today_s: 7 * 3600,
            required_s: 8 * 3600,
            cats: vec![("IDE".into(), 5 * 3600)],
            fg_process: "Code.exe".into(),
            fg_title: "ftracker".into(),
            apps: vec!["chrome.exe".into()],
            usb: vec![],
            cpu_pct: 12,
            ram_used_mb: 8000,
            ram_total_mb: 16000,
            disk_root: "C:".into(),
            disk_free_gb: 100.0,
            disk_total_gb: 500.0,
            loc: Some(fix.clone()),
        });
        assert!(live.contains("Live status"));
        assert!(live.contains("Employee: Alice"));
        assert!(live.contains("External devices: none"));
        assert!(live.contains("Location:"));
        assert!(live.contains("RAM: 7.8 / 15.6 GB"));
        assert!(!live.contains(" MB"));
        assert!(live.contains("C Drive: 400 Used / 500 GB"));
        let list = command_list();
        assert!(list.contains("@online"));
        assert!(list.contains("@data"));
        assert!(list.contains("@{name}"));
        assert!(list.contains("@{name}_ss"));
        assert!(list.contains("/list"));
    }
}
