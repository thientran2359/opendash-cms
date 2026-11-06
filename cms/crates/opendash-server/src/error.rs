use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Clone, Copy)]
pub struct ApiError(pub StatusCode, pub &'static str);
pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn bad_request(message: &'static str) -> Self {
        Self(StatusCode::BAD_REQUEST, message)
    }

    pub fn unauthorized(message: &'static str) -> Self {
        Self(StatusCode::UNAUTHORIZED, message)
    }

    pub fn forbidden(message: &'static str) -> Self {
        Self(StatusCode::FORBIDDEN, message)
    }

    pub fn conflict(message: &'static str) -> Self {
        Self(StatusCode::CONFLICT, message)
    }

    pub fn internal(message: &'static str) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        eprintln!("SQLite: {error}");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Không truy cập được dữ liệu.",
        )
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        eprintln!("JSON: {error}");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Dữ liệu block không hợp lệ.",
        )
    }
}
