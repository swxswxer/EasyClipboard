use std::{sync::Mutex, time::Instant};

use crate::{database::Database, platform::TargetApplication};

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
    pub target_app: Option<TargetApplication>,
    pub last_clipboard_change: Instant,
}

impl RuntimeState {
    pub fn new(last_change_count: i64) -> Self {
        let now = Instant::now();
        Self {
            clipboard_started: crate::platform::RECORDING_STARTS_AUTOMATICALLY,
            last_change_count,
            suppress_change_count: None,
            suppress_until: now,
            expected_hash: None,
            target_app: None,
            last_clipboard_change: now,
        }
    }

    pub fn should_capture_change(&mut self, change_token: i64, now: Instant) -> bool {
        if change_token == self.last_change_count {
            return false;
        }
        self.last_change_count = change_token;
        self.last_clipboard_change = now;
        self.suppress_change_count != Some(change_token) || now > self.suppress_until
    }
}

#[cfg(test)]
mod tests {
    use super::RuntimeState;
    use std::time::{Duration, Instant};

    #[test]
    fn unchanged_and_self_written_tokens_are_not_captured() {
        let now = Instant::now();
        let mut runtime = RuntimeState::new(10);
        assert!(!runtime.should_capture_change(10, now));

        runtime.suppress_change_count = Some(11);
        runtime.suppress_until = now + Duration::from_secs(1);
        assert!(!runtime.should_capture_change(11, now));
        assert!(!runtime.should_capture_change(11, now + Duration::from_secs(2)));
        assert!(runtime.should_capture_change(12, now + Duration::from_secs(2)));
    }
}
