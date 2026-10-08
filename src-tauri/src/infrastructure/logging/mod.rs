use parking_lot::RwLock;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::Arc;
use tracing::{field::{Field, Visit}, Event, Level, Subscriber};
use tracing_subscriber::{layer::Context, Layer};

use crate::domain::ports::logger::LoggerPort;

const CAPACITY: usize = 200;

pub struct InMemoryLogger {
    entries: RwLock<VecDeque<String>>,
}

impl InMemoryLogger {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { entries: RwLock::new(VecDeque::with_capacity(CAPACITY)) })
    }
}

impl LoggerPort for InMemoryLogger {
    fn log(&self, message: String) {
        let entry = format!("[{}] {}", chrono::Local::now().format("%H:%M:%S"), message);
        eprintln!("{}", entry);
        let mut log = self.entries.write();
        if log.len() == CAPACITY { log.pop_front(); }
        log.push_back(entry);
    }

    fn recent(&self, limit: usize) -> Vec<String> {
        let log = self.entries.read();
        let start = log.len().saturating_sub(limit);
        log.range(start..).cloned().collect()
    }
}

/// Forwards this crate's `tracing` events (INFO and above) into the daemon
/// log the GUI shows. Release builds have no console, so without this every
/// `tracing::warn!` from the use cases would be invisible.
pub struct ForwardToLogger { logger: Arc<dyn LoggerPort> }

impl ForwardToLogger {
    pub fn new(logger: Arc<dyn LoggerPort>) -> Self { Self { logger } }
}

impl<S: Subscriber> Layer<S> for ForwardToLogger {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        if *meta.level() > Level::INFO || !meta.target().starts_with(env!("CARGO_CRATE_NAME")) {
            return;
        }
        let mut text = EventText::default();
        event.record(&mut text);
        let line = if *meta.level() == Level::INFO { text.0 } else { format!("{}: {}", meta.level(), text.0) };
        self.logger.log(line);
    }
}

#[derive(Default)]
struct EventText(String);

impl Visit for EventText {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        } else {
            let _ = write!(self.0, " {}={value:?}", field.name());
        }
    }
}

/// Installs the global subscriber. Call once, after the logger exists.
pub fn init_tracing(logger: Arc<dyn LoggerPort>) {
    use tracing_subscriber::layer::SubscriberExt;
    let subscriber = tracing_subscriber::registry().with(ForwardToLogger::new(logger));
    let _ = tracing::subscriber::set_global_default(subscriber);
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

    #[test]
    fn warnings_from_this_crate_reach_the_gui_log() {
        use tracing_subscriber::layer::SubscriberExt;
        let logger = InMemoryLogger::new();
        let subscriber = tracing_subscriber::registry().with(ForwardToLogger::new(logger.clone()));

        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!("DNS add_entry failed for {}: {}", "app.test", "access denied");
            tracing::debug!("too verbose for the GUI");
            tracing::warn!(target: "some_dependency", "not ours");
        });

        let log = logger.recent(10);
        assert_eq!(log.len(), 1, "got: {log:?}");
        assert!(log[0].ends_with("WARN: DNS add_entry failed for app.test: access denied"));
    }
}
