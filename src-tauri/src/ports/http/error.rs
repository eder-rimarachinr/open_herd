use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};

use crate::domain::errors::{ApplicationError, DomainError};

/// Maps a use-case error to an HTTP response with the `{"error": "..."}` body
/// the GUI shows. User mistakes are 4xx; anything the user can't fix is 500.
pub fn app_error(e: &ApplicationError) -> Response {
    let status = match e {
        ApplicationError::Domain(de) => match de {
            DomainError::SiteNotFound(_) | DomainError::PhpIniNotFound(_) => StatusCode::NOT_FOUND,
            DomainError::DomainAlreadyExists(_) => StatusCode::CONFLICT,
            DomainError::InvalidTld
            | DomainError::InvalidDomain(_)
            | DomainError::InvalidPath(_)
            | DomainError::InvalidPhpIniChange(_) => StatusCode::BAD_REQUEST,
            DomainError::Persistence(_) => StatusCode::INTERNAL_SERVER_ERROR,
            DomainError::SslAlreadyActive(_) | DomainError::SslCannotEnable(_) => StatusCode::UNPROCESSABLE_ENTITY,
        },
        ApplicationError::Infrastructure(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
}
