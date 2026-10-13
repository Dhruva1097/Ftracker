pub mod events;
pub mod hours;
pub mod outbox;
pub mod registration;
pub mod schema;
pub mod subscriber;

use anyhow::Result;
use rusqlite::Connection;

use crate::config::AppConfig;
use crate::db_lock::DbLock;
use crate::paths;
use crate::sqlcipher::EncryptedDb;

pub struct Store {
    enc: EncryptedDb,
    _lock: DbLock,
}

impl Store {
    pub fn open(cfg: &AppConfig) -> Result<Self> {
        Self::open_wait(cfg, 60_000)
    }

    pub fn try_open(cfg: &AppConfig) -> Result<Self> {
        Self::open_wait(cfg, 0)
    }

    fn open_wait(cfg: &AppConfig, wait_ms: u32) -> Result<Self> {
        crate::paths::ensure_data_dir()?;
        let lock = DbLock::acquire(wait_ms)?;
        let enc_path = paths::db_enc_path()?;
        let enc = EncryptedDb::open(&cfg.db_key, &enc_path)?;
        schema::apply(&enc.conn)?;
        Ok(Self { enc, _lock: lock })
    }

    pub fn conn(&self) -> &Connection {
        &self.enc.conn
    }

    pub fn persist(&self) -> Result<()> {
        self.enc.persist()
    }

    pub fn size_mb(&self) -> f64 {
        self.enc.size_mb()
    }

    pub fn trim_if_needed(&self, max_mb: f64) -> Result<()> {
        if self.size_mb() <= max_mb {
            return Ok(());
        }
        let days: Vec<String> = {
            let mut stmt = self
                .conn()
                .prepare("SELECT day FROM daily_reports WHERE telegram_ok=1 ORDER BY day ASC")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        for day in days {
            if self.size_mb() <= max_mb {
                break;
            }
            self.conn()
                .execute("DELETE FROM events WHERE day=?1", [&day])?;
            self.conn()
                .execute("DELETE FROM locations WHERE day=?1", [&day])?;
            self.conn()
                .execute("DELETE FROM screenshots WHERE day=?1", [&day])?;
            self.conn()
                .execute("DELETE FROM daily_reports WHERE day=?1", [&day])?;
            self.conn()
                .execute("DELETE FROM day_seconds WHERE day=?1", [&day])?;
            self.persist()?;
        }
        Ok(())
    }
}
