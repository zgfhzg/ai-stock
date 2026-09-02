use axum::{http::StatusCode, Json};
use serde::Serialize;

pub type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;

#[derive(Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

pub fn api_error(
    status: StatusCode,
    code: impl Into<String>,
    error: impl std::fmt::Display,
) -> (StatusCode, Json<ApiError>) {
    (
        status,
        Json(ApiError {
            code: code.into(),
            message: error.to_string(),
        }),
    )
}
