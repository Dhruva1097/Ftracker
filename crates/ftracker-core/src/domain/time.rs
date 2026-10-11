pub fn format_hms(seconds: i64) -> String {
    let s = seconds.max(0);
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    format!("{h:02}:{m:02}:{sec:02}")
}

pub fn format_hm(seconds: i64) -> String {
    let s = seconds.max(0);
    let h = s / 3600;
    let m = (s % 3600) / 60;
    format!("{h}h{m:02}m")
}

pub fn is_weekday(weekday: chrono::Weekday) -> bool {
    !matches!(weekday, chrono::Weekday::Sat | chrono::Weekday::Sun)
}
