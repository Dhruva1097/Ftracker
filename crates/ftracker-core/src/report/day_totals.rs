use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone};

use crate::domain::time::{format_hm, is_weekday};
use crate::Store;

#[derive(Debug, Clone)]
pub struct DayTotal {
    pub day: String,
    pub counted_s: i64,
    pub idle_s: i64,
    pub required_s: i64,
    pub short: bool,
}

/// One calendar day in the last-month hours table.
#[derive(Debug, Clone)]
pub struct MonthDayRow {
    pub day: String,
    pub weekday: String,
    pub counted_s: i64,
    pub idle_s: i64,
    pub required_s: i64,
    pub weekend: bool,
    pub short: bool,
    pub browser_s: i64,
    pub ide_s: i64,
    pub office_s: i64,
    pub other_s: i64,
    pub loc_first: String,
    pub loc_last: String,
    pub loc_fixes: i64,
    pub shots: u32,
}

pub fn last_month_bounds() -> (NaiveDate, NaiveDate) {
    let to = Local::now().date_naive();
    let from = to - Duration::days(29);
    (from, to)
}

fn cat_named(cats: &[(String, i64)], name: &str) -> i64 {
    cats.iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| *v)
        .unwrap_or(0)
}

pub fn collect_month_rows(store: &Store, daily_hours: f64) -> Vec<MonthDayRow> {
    let req = required_seconds(daily_hours);
    let (from, to) = last_month_bounds();
    let mut rows = Vec::new();
    let mut d = from;
    while d <= to {
        let day = d.format("%Y-%m-%d").to_string();
        let counted = store.day_counted(&day).unwrap_or(0);
        let idle = store.day_idle(&day).unwrap_or(0);
        let cats = store.category_seconds(&day).unwrap_or_default();
        let (a, b, n) = store.first_last_loc_day(&day).unwrap_or((None, None, 0));
        let shots = store.screenshot_count_day(&day).unwrap_or(0);
        let weekend = !is_weekday(d.weekday());
        rows.push(MonthDayRow {
            day,
            weekday: d.format("%a").to_string(),
            counted_s: counted,
            idle_s: idle,
            required_s: if weekend { 0 } else { req },
            weekend,
            short: !weekend && counted < req,
            browser_s: cat_named(&cats, "Browser"),
            ide_s: cat_named(&cats, "IDE"),
            office_s: cat_named(&cats, "Office"),
            other_s: cat_named(&cats, "Other"),
            loc_first: fmt_loc_opt(a),
            loc_last: fmt_loc_opt(b),
            loc_fixes: n,
            shots,
        });
        d += Duration::days(1);
    }
    rows
}

pub fn required_seconds(daily_hours: f64) -> i64 {
    (daily_hours * 3600.0).round() as i64
}

pub fn today_iso() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

pub fn parse_day(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

pub fn pretty_day(s: &str) -> String {
    match parse_day(s) {
        Some(d) => d.format("%d %b %Y").to_string(),
        None => s.to_string(),
    }
}

pub fn hour_bucket_key(ts: i64) -> String {
    chrono::Local
        .timestamp_opt(ts, 0)
        .single()
        .unwrap_or_else(Local::now)
        .format("%Y-%m-%dT%H:00")
        .to_string()
}

pub fn hour_range_label(hour_start: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_str(
        &format!("{hour_start}:00 %z").replace("T", " "),
        "%Y-%m-%d %H:%M:%S %z",
    ) {
        let end = dt + Duration::hours(1);
        return format!("{}-{}", dt.format("%H:%M"), end.format("%H:%M"));
    }
    if hour_start.len() >= 16 {
        let start = &hour_start[11..16];
        if let Ok(h) = start[..2].parse::<u32>() {
            return format!("{start}-{:02}:00", (h + 1) % 24);
        }
    }
    hour_start.to_string()
}

/// Local hour start like 2026-10-11T13:00
pub fn current_hour_start() -> String {
    Local::now().format("%Y-%m-%dT%H:00").to_string()
}

pub fn yesterday_iso() -> String {
    (Local::now().date_naive() - Duration::days(1))
        .format("%Y-%m-%d")
        .to_string()
}

pub fn is_iso_weekday(day: &str) -> bool {
    parse_day(day)
        .map(|d| is_weekday(d.weekday()))
        .unwrap_or(false)
}

pub fn weekday_totals(
    store: &Store,
    from: &str,
    to: &str,
    daily_hours: f64,
) -> Vec<DayTotal> {
    let req = required_seconds(daily_hours);
    store
        .days_in_range(from, to)
        .unwrap_or_default()
        .into_iter()
        .map(|(day, counted, idle)| {
            let wd = is_iso_weekday(&day);
            DayTotal {
                short: wd && counted < req,
                day,
                counted_s: counted,
                idle_s: idle,
                required_s: req,
            }
        })
        .collect()
}

pub fn action_window_lines(store: &Store, daily_hours: f64) -> Vec<String> {
    let req = required_seconds(daily_hours);
    let mut lines = Vec::new();
    let today = today_iso();
    let today_s = store.day_counted(&today).unwrap_or(0);
    lines.push(format!(
        "Today: {} of {} required",
        format_hm(today_s),
        format_hm(req)
    ));
    for (day, counted, _) in store.recent_days(14).unwrap_or_default() {
        if day == today {
            continue;
        }
        let mark = if is_iso_weekday(&day) && counted < req {
            " (short)"
        } else {
            ""
        };
        lines.push(format!(
            "{} — {} / {}{mark}",
            pretty_day(&day),
            format_hm(counted),
            format_hm(req)
        ));
    }
    lines
}

pub fn chip_text(store: &Store, daily_hours: f64) -> String {
    let req = required_seconds(daily_hours);
    let today = today_iso();
    let got = store.day_counted(&today).unwrap_or(0);
    format!("{}/{}", format_hm(got), format_hm(req))
}

pub fn iso_week_key(d: NaiveDate) -> String {
    let iso = d.iso_week();
    format!("{:04}-W{:02}", iso.year(), iso.week())
}

pub fn week_bounds(d: NaiveDate) -> (NaiveDate, NaiveDate) {
    let wd = d.weekday().num_days_from_monday() as i64;
    let mon = d - Duration::days(wd);
    let fri = mon + Duration::days(4);
    (mon, fri)
}

pub fn month_hours(store: &Store, daily_hours: f64, on: NaiveDate) -> (i64, i64, i64, i64, i64) {
    let req_day = required_seconds(daily_hours);
    let start = NaiveDate::from_ymd_opt(on.year(), on.month(), 1).unwrap_or(on);
    let next_month = if on.month() == 12 {
        NaiveDate::from_ymd_opt(on.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(on.year(), on.month() + 1, 1)
    }
    .unwrap_or(on);
    let mut expected_so_far = 0i64;
    let mut weekdays_left = 0i64;
    let mut d = start;
    while d < next_month {
        if is_weekday(d.weekday()) {
            if d <= on {
                expected_so_far += req_day;
            }
            if d > on {
                weekdays_left += 1;
            }
        }
        d += Duration::days(1);
    }
    let from = start.format("%Y-%m-%d").to_string();
    let to = on.format("%Y-%m-%d").to_string();
    let worked: i64 = store
        .days_in_range(&from, &to)
        .unwrap_or_default()
        .into_iter()
        .map(|(_, c, _)| c)
        .sum();
    let remaining = weekdays_left * req_day;
    (worked, expected_so_far, remaining, weekdays_left, req_day)
}

pub fn weekly_body(
    store: &Store,
    employee: &str,
    pc: &str,
    week_key: &str,
    mon: NaiveDate,
    fri: NaiveDate,
    daily_hours: f64,
) -> String {
    use crate::domain::location::loc_text;
    use crate::VERSION;

    let req = required_seconds(daily_hours);
    let from = mon.format("%Y-%m-%d").to_string();
    let to = fri.format("%Y-%m-%d").to_string();
    let days = store.days_in_range(&from, &to).unwrap_or_default();
    let mut week_got = 0i64;
    let mut lines = Vec::new();
    let labels = ["Mon", "Tue", "Wed", "Thu", "Fri"];
    for i in 0..5 {
        let day = (mon + Duration::days(i as i64)).format("%Y-%m-%d").to_string();
        let got = days
            .iter()
            .find(|(d, _, _)| d == &day)
            .map(|(_, c, _)| *c)
            .unwrap_or(0);
        week_got += got;
        let cats = store.category_seconds(&day).unwrap_or_default();
        let mut cat = String::new();
        for name in ["Browser", "IDE", "Office", "Other"] {
            let s = cats
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| *v)
                .unwrap_or(0);
            if !cat.is_empty() {
                cat.push(' ');
            }
            cat.push_str(&format!("{name}={}", format_hm(s)));
        }
        lines.push(format!(
            "{} {} / {}  {cat}",
            labels[i],
            format_hm(got),
            format_hm(req)
        ));
    }
    let loc = store.last_location().ok().flatten();
    let on = fri;
    let (worked, expected, remaining, left, _) = month_hours(store, daily_hours, on);
    format!(
        "WEEKLY employee={employee} pc={pc} week={week_key} required/day={} version={VERSION}\n{}\nweek={} / {}\nmonth_so_far={} / {} expected\nmonth_remaining={} ({} weekdays left × {})\n{}",
        format_hm(req),
        lines.join("\n"),
        format_hm(week_got),
        format_hm(req * 5),
        format_hm(worked),
        format_hm(expected),
        format_hm(remaining),
        left,
        format_hm(req),
        loc_text(loc.as_ref())
    )
}

pub fn fmt_loc_opt(t: Option<(f64, f64, f64)>) -> String {
    match t {
        Some((lat, lon, acc)) => format!("{lat:.4},{lon:.4} ±{acc:.0}m"),
        None => "unavailable".into(),
    }
}
