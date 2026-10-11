use anyhow::Result;
use rusqlite::params;

use crate::Store;

impl Store {
    pub fn get_subscriber(&self, user: &str) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT chat_id FROM subscriber WHERE windows_user=?1",
                [user],
                |r| r.get(0),
            )
            .ok())
    }

    pub fn set_subscriber(&self, user: &str, chat_id: &str) -> Result<()> {
        let ts = chrono::Local::now().timestamp();
        self.conn().execute(
            "INSERT INTO subscriber(windows_user, chat_id, updated_at) VALUES(?1,?2,?3)
             ON CONFLICT(windows_user) DO UPDATE SET chat_id=excluded.chat_id, updated_at=excluded.updated_at",
            params![user, chat_id, ts],
        )?;
        Ok(())
    }

    pub fn clear_subscriber(&self, user: &str) -> Result<()> {
        self.conn()
            .execute("DELETE FROM subscriber WHERE windows_user=?1", [user])?;
        Ok(())
    }
}
