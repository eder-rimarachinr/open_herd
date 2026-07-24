use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

use crate::domain::ports::ssl_task::{SslTaskPort, SslTaskProgress};

pub struct InMemorySslTaskTracker {
    tasks: Mutex<HashMap<String, SslTaskProgress>>,
}

impl InMemorySslTaskTracker {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { tasks: Mutex::new(HashMap::new()) })
    }
}

impl SslTaskPort for InMemorySslTaskTracker {
    fn set(&self, site_id: &str, state: &str, message: &str, error: Option<String>) {
        self.tasks.lock().insert(
            site_id.to_string(),
            SslTaskProgress { state: state.into(), message: message.into(), error },
        );
    }

    fn get(&self, site_id: &str) -> Option<SslTaskProgress> {
        self.tasks.lock().get(site_id).cloned()
    }

    fn remove(&self, site_id: &str) {
        self.tasks.lock().remove(site_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_site_has_no_task() {
        let tracker = InMemorySslTaskTracker::new();
        assert!(tracker.get("nope").is_none());
    }

    #[test]
    fn set_then_get_then_remove_roundtrip() {
        let tracker = InMemorySslTaskTracker::new();
        tracker.set("site-1", "running", "Issuing…", None);
        let progress = tracker.get("site-1");
        assert!(progress.is_some());
        if let Some(progress) = progress {
            assert_eq!(progress.state, "running");
            assert_eq!(progress.message, "Issuing…");
        }

        tracker.remove("site-1");
        assert!(tracker.get("site-1").is_none());
    }
}
