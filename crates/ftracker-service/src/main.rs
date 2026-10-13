#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> anyhow::Result<()> {
    ftracker_core::config::load_dotenv();
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--console") {
        ftracker_service::service::run_worker();
        return Ok(());
    }
    ftracker_service::start_dispatcher()
}
