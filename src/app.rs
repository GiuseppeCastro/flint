use crate::config::Config;
use crate::secrets::SecretScanner;
use crate::{db, paths};
use rusqlite::Connection;

pub struct App {
    pub conn: Connection,
    pub config: Config,
}

impl App {
    pub fn open() -> rusqlite::Result<Self> {
        paths::ensure_data_dir().ok();
        let config = Config::load(&paths::config_path());
        let conn = db::open(&paths::db_path())?;
        Ok(Self { conn, config })
    }

    pub fn scanner(&self) -> SecretScanner {
        SecretScanner::new(&self.config.ignore_patterns)
    }
}
