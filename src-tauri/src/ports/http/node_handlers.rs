use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use super::{error::app_error, extract::ApiJson};
use crate::{
    domain::{
        errors::{ApplicationError, InfrastructureError},
        node::NodeVersion,
        ports::task::TaskState,
    },
    infrastructure::{
        container::AppContainer,
        dto::{NodeInstallProgress, NodeReleaseEntry, NodeStatus},
    },
};

type ContainerRef = Arc<AppContainer>;

// ── GET /api/v1/node ──────────────────────────────────────────────────────────

pub async fn node_status(State(container): State<ContainerRef>) -> Response {
    match container.get_node_status_uc.execute().await {
        Ok(env) => Json(NodeStatus::from(env)).into_response(),
        Err(e)  => app_error(&e),
    }
}

// ── GET /api/v1/node/releases[?refresh=1] ─────────────────────────────────────

#[derive(Deserialize)]
pub struct ReleasesQuery {
    #[serde(default)]
    refresh: Option<String>,
}

pub async fn node_releases(State(container): State<ContainerRef>, Query(q): Query<ReleasesQuery>) -> Response {
    let refresh = q.refresh.is_some_and(|r| r != "0" && r != "false");
    match container.list_node_releases_uc.execute(refresh).await {
        Ok(views) => Json(views.into_iter().map(NodeReleaseEntry::from).collect::<Vec<_>>()).into_response(),
        // nodejs.org unreachable: an upstream failure, not ours.
        Err(e @ ApplicationError::Infrastructure(InfrastructureError::DownloadFailed(_))) => {
            (StatusCode::BAD_GATEWAY, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
        Err(e) => app_error(&e),
    }
}

// ── POST /api/v1/node/install ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct InstallNodeRequest {
    version: String,
}

pub async fn install_node(
    State(container): State<ContainerRef>,
    ApiJson(InstallNodeRequest { version }): ApiJson<InstallNodeRequest>,
) -> Response {
    match container.install_node_uc.execute(&version).await {
        Ok(state) => {
            if state == TaskState::Pending { container.logger.log(format!("Starting Node {version} download")); }
            Json(serde_json::json!({ "ok": true, "state": state.as_str() })).into_response()
        }
        Err(e) => app_error(&e),
    }
}

// ── GET /api/v1/node/install/{version}/progress ───────────────────────────────

pub async fn install_node_progress(State(container): State<ContainerRef>, Path(raw): Path<String>) -> Response {
    let version = match NodeVersion::parse(&raw) {
        Ok(v) => v,
        Err(e) => return app_error(&e.into()),
    };
    let progress = match container.downloads.node_progress(&version).await {
        Some(p) => {
            let error = if p.state == TaskState::Error { Some(p.message.clone()) } else { p.error };
            NodeInstallProgress { version: version.to_string(), state: p.state, message: p.message, percent: p.percent, error }
        }
        None => NodeInstallProgress { version: version.to_string(), state: TaskState::Idle, message: String::new(), percent: 0, error: None },
    };
    Json(progress).into_response()
}

// ── POST /api/v1/node/versions/{version}/activate ─────────────────────────────

pub async fn activate_node(State(container): State<ContainerRef>, Path(version): Path<String>) -> Response {
    match container.activate_node_uc.execute(&version).await {
        Ok(env) => {
            container.logger.log(format!("Node {version} is now the active version"));
            Json(NodeStatus::from(env)).into_response()
        }
        Err(e) => app_error(&e),
    }
}

// ── DELETE /api/v1/node/versions/{version} ────────────────────────────────────

pub async fn uninstall_node(State(container): State<ContainerRef>, Path(version): Path<String>) -> Response {
    match container.uninstall_node_uc.execute(&version).await {
        Ok(()) => {
            container.logger.log(format!("Node {version} uninstalled"));
            Json(serde_json::json!({ "ok": true })).into_response()
        }
        Err(e) => app_error(&e),
    }
}
