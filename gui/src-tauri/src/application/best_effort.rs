/// Runs a step whose failure must not abort the use case (DNS entry, nginx
/// reload…): a failure is logged as a warning — which reaches the GUI's
/// daemon log — and reported back as `false`.
pub(crate) fn best_effort<E: std::fmt::Display>(result: Result<(), E>, step: std::fmt::Arguments<'_>) -> bool {
    match result {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("{step}: {e}");
            false
        }
    }
}
