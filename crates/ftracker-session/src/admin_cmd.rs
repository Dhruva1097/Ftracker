use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use ftracker_core::config::{windows_user, AppConfig};
use ftracker_core::paths;
use ftracker_core::report::day_totals::{self, required_seconds};
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
const LIVE_JPEG_WIDTH: u32 = 800;
const LIVE_JPEG_QUALITY: u8 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminCmd {
    Online,
    Snapshot,
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
                            if tx.send(AdminCmd::Online).is_err() {
                                return;
                            }
                        }
                        Some(AdminCommand::Who(name)) => {
                            if commands::names_match(&name, &cfg.employee_name, &windows_user()) {
                                if tx.send(AdminCmd::Snapshot).is_err() {
                                    return;
                                }
                            }
                        }
                        None => {}
                    }
                    state.processed = state.processed.max(u.update_id);
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
