use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;

use super::error::app_error;
use crate::{
    domain::{
        errors::{ApplicationError, InfrastructureError},
        ports::task::TaskState,
    },
    infrastructure::{
        container::AppContainer,
        dto::{ComposerInstallProgress, ComposerStatus},
    },
};

type ContainerRef = Arc<AppContainer>;

async fn status_response(container: &AppContainer) -> Response {
    let default_php = container.config.get().default_php;
    match container.get_composer_status_uc.execute(&default_php).await {
        Ok(view) => Json(ComposerStatus::from(view)).into_response(),
        Err(e)   => app_error(&e),
    }
}

// ── GET /api/v1/composer ──────────────────────────────────────────────────────

pub async fn composer_status(State(container): State<ContainerRef>) -> Response {
    status_response(&container).await
}

// ── POST /api/v1/composer/install ─────────────────────────────────────────────

/// Install, update or repair: whatever brings Composer to the newest release
/// the default PHP runs, with the command pointing at that PHP.
pub async fn install_composer(State(container): State<ContainerRef>) -> Response {
    let default_php = container.config.get().default_php;
    match container.install_composer_uc.execute(&default_php).await {
        Ok(state) => {
            match state {
                TaskState::Pending => container.logger.log("Starting Composer download".to_string()),
                TaskState::Done => container.logger.log(format!("composer command now runs PHP {default_php}")),
                _ => {}
            }
            Json(serde_json::json!({ "ok": true, "state": state.as_str() })).into_response()
        }
        // getcomposer.org unreachable: an upstream failure, not ours.
        Err(e @ ApplicationError::Infrastructure(InfrastructureError::DownloadFailed(_))) => {
            (StatusCode::BAD_GATEWAY, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
        Err(e) => app_error(&e),
    }
}

// ── GET /api/v1/composer/install/progress ─────────────────────────────────────

pub async fn install_composer_progress(State(container): State<ContainerRef>) -> Response {
    let progress = match container.downloads.composer_progress().await {
        Some(p) => {
            let error = if p.state == TaskState::Error { Some(p.message.clone()) } else { p.error };
            ComposerInstallProgress { state: p.state, message: p.message, percent: p.percent, error }
        }
        None => ComposerInstallProgress { state: TaskState::Idle, message: String::new(), percent: 0, error: None },
    };
    Json(progress).into_response()
}

// ── POST /api/v1/composer/extensions ──────────────────────────────────────────

/// Enables the extensions Composer needs in the default PHP's php.ini and
/// returns the new status (which re-checks them with `php -m`).
pub async fn enable_composer_extensions(State(container): State<ContainerRef>) -> Response {
    let default_php = container.config.get().default_php;
    if let Err(e) = container.enable_composer_extensions_uc.execute(&default_php).await {
        return app_error(&e);
    }
    container.logger.log(format!("Enabled Composer's extensions in PHP {default_php}'s php.ini"));
    status_response(&container).await
}
