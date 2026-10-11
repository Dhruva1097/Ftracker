pub mod config;
pub mod crypto;
pub mod db;
pub mod db_lock;
pub mod domain;
pub mod integrity;
pub mod paths;
pub mod redact;
pub mod report;
pub mod sqlcipher;
pub mod telegram;
pub mod totp;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PIPE_NAME: &str = r"\\.\pipe\ftracker-ipc";

pub use config::AppConfig;
pub use db::Store;
pub use domain::location::GeoFix;
