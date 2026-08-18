use std::{sync::Mutex, time::Instant};

use crate::{database::Database, native_macos::FrontmostApp};

pub struct AppState {
    pub database: Database,
    pub runtime: Mutex<RuntimeState>,
}

pub struct RuntimeState {
    pub clipboard_started: bool,
    pub last_change_count: i64,
    pub suppress_change_count: Option<i64>,
    pub suppress_until: Instant,
    pub expected_hash: Option<String>,
    pub target_app: Option<FrontmostApp>,
    pub last_clipboard_change: Instant,
}

impl RuntimeState {
    pub fn new(last_change_count: i64) -> Self {
        let now = Instant::now();
        Self {
            clipboard_started: false,
            last_change_count,
            suppress_change_count: None,
            suppress_until: now,
            expected_hash: None,
            target_app: None,
            last_clipboard_change: now,
        }
    }
}
