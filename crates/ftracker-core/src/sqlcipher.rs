use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, Context, Result};
use rand::RngCore;
use rusqlite::{Connection, OptionalExtension};
use std::fs;
use std::path::Path;

/// Encrypted-at-rest SQLite: live in-memory DB, AES-GCM snapshots to `ftracker.enc`.
pub struct EncryptedDb {
    pub conn: Connection,
    key: Vec<u8>,
    enc_path: std::path::PathBuf,
}

impl EncryptedDb {
    pub fn open(key: &[u8], enc_path: &Path) -> Result<Self> {
        if key.len() != 32 {
            return Err(anyhow!("DB_CORRUPT: bad key"));
        }
        let conn = Connection::open_in_memory().context("DB_CORRUPT")?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let mut db = Self {
            conn,
            key: key.to_vec(),
            enc_path: enc_path.to_path_buf(),
        };
        if enc_path.exists() {
            db.restore().context("DB_CORRUPT")?;
        }
        Ok(db)
    }

    fn restore(&mut self) -> Result<()> {
        let blob = fs::read(&self.enc_path)?;
        if blob.len() < 12 + 16 {
            return Err(anyhow!("ciphertext too small"));
        }
        let (nonce_b, ct) = blob.split_at(12);
        let cipher = Aes256Gcm::new_from_slice(&self.key)?;
        let plain = cipher
            .decrypt(Nonce::from_slice(nonce_b), ct)
            .map_err(|_| anyhow!("decrypt failed"))?;
        let tmp = self.enc_path.with_extension("restore.db");
        fs::write(&tmp, &plain)?;
        {
            let src = Connection::open(&tmp)?;
            let backup = rusqlite::backup::Backup::new(&src, &mut self.conn)?;
            backup.run_to_completion(100, std::time::Duration::from_millis(1), None)?;
        }
        let _ = fs::remove_file(&tmp);
        Ok(())
    }

    pub fn persist(&self) -> Result<()> {
        if let Some(parent) = self.enc_path.parent() {
            fs::create_dir_all(parent)?;
            crate::paths::apply_admin_acl(parent)?;
        }
        let tmp = self.enc_path.with_extension("snap.db");
        {
            let mut dst = Connection::open(&tmp)?;
            let backup = rusqlite::backup::Backup::new(&self.conn, &mut dst)?;
            backup.run_to_completion(100, std::time::Duration::from_millis(1), None)?;
        }
        let plain = fs::read(&tmp)?;
        let _ = fs::remove_file(&tmp);
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        let cipher = Aes256Gcm::new_from_slice(&self.key)?;
        let ct = cipher
            .encrypt(Nonce::from_slice(&nonce), plain.as_ref())
            .map_err(|_| anyhow!("encrypt failed"))?;
        let mut out = Vec::with_capacity(12 + ct.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        let tmp_enc = self.enc_path.with_extension("enc.tmp");
        fs::write(&tmp_enc, &out)?;
        fs::rename(&tmp_enc, &self.enc_path)?;
        crate::paths::apply_admin_acl(&self.enc_path)?;
        Ok(())
    }

    pub fn size_mb(&self) -> f64 {
        fs::metadata(&self.enc_path)
            .map(|m| m.len() as f64 / (1024.0 * 1024.0))
            .unwrap_or(0.0)
    }
}

pub fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    let n: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
            [name],
            |r| r.get(0),
        )
        .optional()?;
    Ok(n.is_some())
}
