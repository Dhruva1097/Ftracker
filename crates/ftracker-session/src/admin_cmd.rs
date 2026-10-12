use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use ftracker_core::config::{windows_user, AppConfig};
use ftracker_core::paths;
use ftracker_core::report::day_totals::{self, required_seconds, MonthDayRow};
use ftracker_core::report::pdf;
use ftracker_core::telegram::client::TelegramClient;
use ftracker_core::telegram::commands::{self, AdminCommand};
use ftracker_core::telegram::messages::{self, LiveSnapshot};
use ftracker_core::GeoFix;
use serde::{Deserialize, Serialize};

use crate::activity;
use crate::app::SessionState;
use crate::metrics;
use crate::screenshot;

/// Slow poll: reply delay stays well under 1 minute, few HTTP calls.
const POLL_SLEEP: Duration = Duration::from_secs(20);
const ERR_SLEEP: Duration = Duration::from_secs(20);
const SNAPSHOT_DEBOUNCE: Duration = Duration::from_secs(15);
const MONTH_PDF_DEBOUNCE: Duration = Duration::from_secs(45);
const LIST_DEBOUNCE: Duration = Duration::from_secs(20);
const LIVE_JPEG_WIDTH: u32 = 1920;
const LIVE_JPEG_QUALITY: u8 = 90;
const SHOT_DEBOUNCE: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminCmd {
    Online,
    Snapshot,
    Shot,
    MonthData,
    List,
}

enum Job {
    Online {
        chat: String,
        employee: String,
        windows_user: String,
        pc: String,
        idle: bool,
        loc: Option<GeoFix>,
    },
    Snapshot {
        chat: String,
        employee: String,
        windows_user: String,
        pc: String,
        idle: bool,
        fg: activity::foreground::Foreground,
        loc: Option<GeoFix>,
        tracked: i64,
        required: i64,
        cats: Vec<(String, i64)>,
    },
    MonthPdf {
        chat: String,
        employee: String,
        windows_user: String,
        pc: String,
        from: String,
        to: String,
        rows: Vec<MonthDayRow>,
        cfg: AppConfig,
    },
    CommandList {
        chat: String,
    },
    ShotOnly {
        chat: String,
        employee: String,
        pc: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Default, PartialEq)]
struct OffsetState {
    processed: i64,
    confirmed: i64,
}

pub fn start(cfg: AppConfig, client: TelegramClient) -> (Receiver<AdminCmd>, JobHandle) {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (job_tx, job_rx) = mpsc::channel::<Job>();
    let poll_client = client.clone();
    let worker_client = client;
    let _ = thread::Builder::new()
        .name("ft-tg-poll".into())
        .spawn(move || {
            lower_priority();
            poll_loop(cfg, poll_client, cmd_tx);
        });
    let _ = thread::Builder::new()
        .name("ft-tg-send".into())
        .spawn(move || {
            lower_priority();
            worker_loop(worker_client, job_rx);
        });
    (cmd_rx, JobHandle(job_tx))
}

/// Cheap cloneable queue into the silent send worker.
#[derive(Clone)]
pub struct JobHandle(Sender<Job>);

fn lower_priority() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
        };
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

fn poll_loop(cfg: AppConfig, client: TelegramClient, tx: Sender<AdminCmd>) {
    let _ = client.delete_webhook();
    let mut state = load_offset();
    loop {
        match client.get_updates(state.confirmed + 1, 0) {
            Ok(updates) => {
                let now = chrono::Utc::now().timestamp();
                let before = state.processed;
                let conf_before = state.confirmed;
                let mut fleet_cmd = false;
                let mut list_id: Option<i64> = None;
                for u in &updates {
                    if u.update_id <= state.processed {
                        continue;
                    }
                    if commands::is_stale(u.date, now)
                        || !commands::chat_matches(&u.chat_id, &cfg.telegram_admin_chat_id)
                    {
                        state.processed = state.processed.max(u.update_id);
                        continue;
                    }
                    match commands::parse_command(&u.text) {
                        Some(AdminCommand::Online) => {
                            fleet_cmd = true;
                            if tx.send(AdminCmd::Online).is_err() {
                                return;
                            }
                        }
                        Some(AdminCommand::MonthData) => {
                            fleet_cmd = true;
                            if tx.send(AdminCmd::MonthData).is_err() {
                                return;
                            }
                        }
                        Some(AdminCommand::List) => {
                            list_id = Some(u.update_id);
                            if tx.send(AdminCmd::List).is_err() {
                                return;
                            }
                        }
                        Some(AdminCommand::Who(name)) => {
                            fleet_cmd = true;
                            if commands::names_match(&name, &cfg.employee_name, &windows_user()) {
                                if tx.send(AdminCmd::Snapshot).is_err() {
                                    return;
                                }
                            }
                        }
                        Some(AdminCommand::WhoShot(name)) => {
                            fleet_cmd = true;
                            if commands::names_match(&name, &cfg.employee_name, &windows_user()) {
                                if tx.send(AdminCmd::Shot).is_err() {
                                    return;
                                }
                            }
                        }
                        None => {}
                    }
                    state.processed = state.processed.max(u.update_id);
                }
                // /list is the same on every PC — ack it now so the rest of the
                // fleet does not send duplicate help, but never skip a live fleet command.
                if !fleet_cmd {
                    if let Some(id) = list_id {
                        state.confirmed = state.confirmed.max(id);
                    }
                }
                state.confirmed = commands::confirm_upto(
                    updates.iter().map(|u| (u.update_id, u.date)),
                    state.confirmed,
                    now,
                );
                if state.processed != before || state.confirmed != conf_before {
                    save_offset(&state);
                }
            }
            Err(_) => thread::sleep(ERR_SLEEP),
        }
        thread::sleep(POLL_SLEEP);
    }
}

fn load_offset() -> OffsetState {
    let path = match paths::telegram_offset_path() {
        Ok(p) => p,
        Err(_) => return OffsetState::default(),
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => return OffsetState::default(),
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn save_offset(state: &OffsetState) {
    if let Ok(path) = paths::telegram_offset_path() {
        if let Ok(bytes) = serde_json::to_vec(state) {
            let _ = std::fs::write(path, bytes);
        }
    }
}

fn worker_loop(client: TelegramClient, rx: Receiver<Job>) {
    while let Ok(job) = rx.recv() {
        match job {
            Job::Online {
                chat,
                employee,
                windows_user,
                pc,
                idle,
                loc,
            } => {
                let body = messages::online_here(&employee, &windows_user, &pc, idle, loc.as_ref());
                let _ = client.send_admin(&chat, &body, loc.as_ref());
            }
            Job::Snapshot {
                chat,
                employee,
                windows_user,
                pc,
                idle,
                fg,
                loc,
                tracked,
                required,
                cats,
            } => {
                send_snapshot(
                    &client,
                    &chat,
                    employee,
                    windows_user,
                    pc,
                    idle,
                    fg,
                    loc,
                    tracked,
                    required,
                    cats,
                );
            }
            Job::MonthPdf {
                chat,
                employee,
                windows_user,
                pc,
                from,
                to,
                rows,
                cfg,
            } => {
                send_month_pdf(&client, &chat, employee, windows_user, pc, from, to, rows, &cfg);
            }
            Job::CommandList { chat } => {
                let _ = client.send_message(&chat, &messages::command_list());
            }
            Job::ShotOnly {
                chat,
                employee,
                pc,
            } => {
                send_shot_only(&client, &chat, employee, pc);
            }
        }
    }
}

pub fn handle(session: &mut SessionState, jobs: &JobHandle, cmd: AdminCmd) {
    match cmd {
        AdminCmd::Online => {
            let _ = jobs.0.send(Job::Online {
                chat: session.cfg.telegram_admin_chat_id.clone(),
                employee: session.cfg.employee_name.clone(),
                windows_user: windows_user(),
                pc: session.cfg.machine_id.clone(),
                idle: session.idle,
                loc: session.last_loc.clone(),
            });
        }
        AdminCmd::Snapshot => {
            if let Some(prev) = session.last_snapshot {
                if prev.elapsed() < SNAPSHOT_DEBOUNCE {
                    return;
                }
            }
            session.last_snapshot = Some(Instant::now());
            let day = day_totals::today_iso();
            let tracked = session.store.day_counted(&day).unwrap_or(0);
            let cats = session.store.category_seconds(&day).unwrap_or_default();
            let required = required_seconds(session.cfg.daily_hours_required);
            let _ = jobs.0.send(Job::Snapshot {
                chat: session.cfg.telegram_admin_chat_id.clone(),
                employee: session.cfg.employee_name.clone(),
                windows_user: windows_user(),
                pc: session.cfg.machine_id.clone(),
                idle: session.idle,
                fg: session.last_fg.clone(),
                loc: session.last_loc.clone(),
                tracked,
                required,
                cats,
            });
        }
        AdminCmd::Shot => {
            if let Some(prev) = session.last_snapshot {
                if prev.elapsed() < SHOT_DEBOUNCE {
                    return;
                }
            }
            session.last_snapshot = Some(Instant::now());
            let _ = jobs.0.send(Job::ShotOnly {
                chat: session.cfg.telegram_admin_chat_id.clone(),
                employee: session.cfg.employee_name.clone(),
                pc: session.cfg.machine_id.clone(),
            });
        }
        AdminCmd::List => {
            if let Some(prev) = session.last_list {
                if prev.elapsed() < LIST_DEBOUNCE {
                    return;
                }
            }
            session.last_list = Some(Instant::now());
            let _ = jobs.0.send(Job::CommandList {
                chat: session.cfg.telegram_admin_chat_id.clone(),
            });
        }
        AdminCmd::MonthData => {
            if let Some(prev) = session.last_month_pdf {
                if prev.elapsed() < MONTH_PDF_DEBOUNCE {
                    return;
                }
            }
            session.last_month_pdf = Some(Instant::now());
            let (from_d, to_d) = day_totals::last_month_bounds();
            let from = from_d.format("%Y-%m-%d").to_string();
            let to = to_d.format("%Y-%m-%d").to_string();
            let rows = day_totals::collect_month_rows(&session.store, session.cfg.daily_hours_required);
            let _ = jobs.0.send(Job::MonthPdf {
                chat: session.cfg.telegram_admin_chat_id.clone(),
                employee: session.cfg.employee_name.clone(),
                windows_user: windows_user(),
                pc: session.cfg.machine_id.clone(),
                from,
                to,
                rows,
                cfg: session.cfg.clone(),
            });
        }
    }
}

fn send_month_pdf(
    client: &TelegramClient,
    chat: &str,
    employee: String,
    user: String,
    pc: String,
    from: String,
    to: String,
    rows: Vec<MonthDayRow>,
    cfg: &AppConfig,
) {
    let tracked: i64 = rows.iter().map(|r| r.counted_s).sum();
    let required: i64 = rows.iter().map(|r| r.required_s).sum();
    let short_days = rows.iter().filter(|r| r.short).count();
    let caption = messages::month_data_caption(
        &employee, &user, &pc, &from, &to, tracked, required, short_days,
    );
    match pdf::build_month_pdf(cfg, &user, &from, &to, &rows) {
        Ok(bytes) => {
            let safe: String = employee
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '_' })
                .collect();
            let name = format!("ftracker-month-{safe}-{from}-to-{to}.pdf");
            let _ = client.send_document(chat, &name, &bytes, &caption);
        }
        Err(_) => {
            let _ = client.send_message(chat, &caption);
        }
    }
}

fn send_snapshot(
    client: &TelegramClient,
    chat: &str,
    employee: String,
    user: String,
    pc: String,
    idle: bool,
    fg: activity::foreground::Foreground,
    loc: Option<GeoFix>,
    tracked: i64,
    required: i64,
    cats: Vec<(String, i64)>,
) {
    let host = metrics::collect();
    let usb = metrics::removable_volumes();
    let apps = activity::visible::process_names(12);
    let fg_proc = activity::foreground::process_file_name(&fg.process);
    let fg_title = ftracker_core::redact::redact_title(&fg.title);
    let jpeg = screenshot::capture_jpeg(LIVE_JPEG_WIDTH, LIVE_JPEG_QUALITY)
        .ok()
        .flatten()
        .map(|(j, _)| j);
    let snap = LiveSnapshot {
        employee: employee.clone(),
        windows_user: user,
        pc: pc.clone(),
        idle,
        tracked_today_s: tracked,
        required_s: required,
        cats,
        fg_process: fg_proc,
        fg_title,
        apps,
        usb,
        cpu_pct: host.cpu_pct,
        ram_used_mb: host.ram_used_mb,
        ram_total_mb: host.ram_total_mb,
        disk_root: host.disk_root,
        disk_free_gb: host.disk_free_gb,
        disk_total_gb: host.disk_total_gb,
        loc: loc.clone(),
    };
    let body = messages::live_status(&snap);
    let _ = client.send_admin(chat, &body, loc.as_ref());
    if let Some(bytes) = jpeg {
        let caption = format!("Screenshot\nEmployee: {employee}\nPC: {pc}");
        let _ = client.send_photo(chat, "screenshot.jpg", &bytes, &caption);
    }
}

fn send_shot_only(client: &TelegramClient, chat: &str, employee: String, pc: String) {
    let caption = messages::screenshot_only_caption(&employee, &pc);
    match screenshot::capture_jpeg(LIVE_JPEG_WIDTH, LIVE_JPEG_QUALITY) {
        Ok(Some((bytes, _))) => {
            let _ = client.send_photo(chat, "screenshot.jpg", &bytes, &caption);
        }
        _ => {
            let _ = client.send_message(chat, &format!("{caption}\n(no screenshot)"));
        }
    }
}
