/// State of a background task (download, SSL issuance) as reported to the GUI.
/// `as_str` is the wire value the GUI already understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// No task has run (or its result was not kept).
    Idle,
    /// Accepted, not started yet.
    Pending,
    Running,
    Downloading,
    Extracting,
    Configuring,
    Done,
    Error,
}

impl TaskState {
    /// Whether a task in this state is still in flight, i.e. starting another
    /// one for the same target would race with it.
    pub fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Running | Self::Downloading | Self::Extracting | Self::Configuring)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle        => "idle",
            Self::Pending     => "pending",
            Self::Running     => "running",
            Self::Downloading => "downloading",
            Self::Extracting  => "extracting",
            Self::Configuring => "configuring",
            Self::Done        => "done",
            Self::Error       => "error",
        }
    }
}

impl std::fmt::Display for TaskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuring_counts_as_active() {
        // Used to be missed by the "already downloading" check, allowing a
        // second install to start while php.ini was being written.
        assert!(TaskState::Configuring.is_active());
        assert!(!TaskState::Done.is_active());
        assert!(!TaskState::Error.is_active());
    }
}
