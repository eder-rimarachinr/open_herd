use parking_lot::RwLock;
use std::sync::Arc;

use crate::domain::ports::logger::LoggerPort;

pub struct InMemoryLogger {
    entries: RwLock<Vec<String>>,
}

impl InMemoryLogger {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { entries: RwLock::new(Vec::new()) })
    }
}

impl LoggerPort for InMemoryLogger {
    fn log(&self, message: String) {
        let mut log = self.entries.write();
        let entry = format!("[{}] {}", chrono::Local::now().format("%H:%M:%S"), message);
        eprintln!("{}", entry);
        log.push(entry);
        if log.len() > 200 {
            let excess = log.len() - 200;
            log.drain(0..excess);
        }
    }

    fn recent(&self, limit: usize) -> Vec<String> {
        let log = self.entries.read();
        let start = log.len().saturating_sub(limit);
        log[start..].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_is_empty_before_any_log() {
        let logger = InMemoryLogger::new();
        assert!(logger.recent(10).is_empty());
    }

    #[test]
    fn recent_returns_entries_in_chronological_order_capped_at_limit() {
        let logger = InMemoryLogger::new();
        logger.log("first".into());
        logger.log("second".into());
        logger.log("third".into());

        let last_two = logger.recent(2);
        assert_eq!(last_two.len(), 2);
        assert!(last_two[0].contains("second"));
        assert!(last_two[1].contains("third"));
    }

    #[test]
    fn buffer_is_capped_at_200_entries() {
        let logger = InMemoryLogger::new();
        for i in 0..250 {
            logger.log(format!("entry {i}"));
        }
        let all = logger.recent(1000);
        assert_eq!(all.len(), 200);
        assert!(all[0].contains("entry 50"), "oldest 50 entries should have been dropped");
    }
}
