//! Tiny append-only diagnostic log at `%APPDATA%\brightvol\brightvol.log`.
//!
//! Used to capture what happens during brightness steps, since the app has no
//! console in release builds. Logging failures are ignored: diagnostics must
//! never break the app.

use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Config;

/// Append a line to the log file. Never panics; failures are silently dropped.
pub fn log(message: &str) {
    let path = Config::config_path().with_file_name("brightvol.log");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "[{secs}] {message}");
    }
}
