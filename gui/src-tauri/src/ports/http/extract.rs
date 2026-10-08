use axum::{
    extract::{FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::de::DeserializeOwned;

/// `Json<T>` whose rejections (malformed JSON, missing field, wrong type) are
/// answered as `400 {"error": "..."}` — the shape the GUI's API client reads —
/// instead of axum's default plain-text 400/415/422.
pub struct ApiJson<T>(pub T);

impl<T, S> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(rejection) => Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": rejection.body_text() })),
            ).into_response()),
        }
    }
}
