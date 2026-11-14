use std::borrow::Cow;

use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

#[derive(Debug, Clone)]
pub struct ApiError(pub StatusCode, pub Cow<'static, str>);
pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn bad_request(message: impl Into<Cow<'static, str>>) -> Self {
        Self(StatusCode::BAD_REQUEST, message.into())
    }

    pub fn unauthorized(message: impl Into<Cow<'static, str>>) -> Self {
        Self(StatusCode::UNAUTHORIZED, message.into())
    }

    pub fn forbidden(message: impl Into<Cow<'static, str>>) -> Self {
        Self(StatusCode::FORBIDDEN, message.into())
    }

    pub fn conflict(message: impl Into<Cow<'static, str>>) -> Self {
        Self(StatusCode::CONFLICT, message.into())
    }

    pub fn not_found(message: impl Into<Cow<'static, str>>) -> Self {
        Self(StatusCode::NOT_FOUND, message.into())
    }

    pub fn precondition_failed(message: impl Into<Cow<'static, str>>) -> Self {
        Self(StatusCode::PRECONDITION_FAILED, message.into())
    }

    pub fn precondition_required(message: impl Into<Cow<'static, str>>) -> Self {
        Self(StatusCode::PRECONDITION_REQUIRED, message.into())
    }

    pub fn unsupported_media_type(message: impl Into<Cow<'static, str>>) -> Self {
        Self(StatusCode::UNSUPPORTED_MEDIA_TYPE, message.into())
    }

    pub fn internal(message: impl Into<Cow<'static, str>>) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, message.into())
    }

    pub fn unprocessable(message: impl Into<Cow<'static, str>>) -> Self {
        Self(StatusCode::UNPROCESSABLE_ENTITY, message.into())
    }

    pub fn service_unavailable(message: impl Into<Cow<'static, str>>) -> Self {
        Self(StatusCode::SERVICE_UNAVAILABLE, message.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.0,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            self.1,
        )
            .into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        eprintln!("SQLite: {error}");
        Self::internal("Không truy cập được dữ liệu.")
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        eprintln!("JSON: {error}");
        Self::internal("Dữ liệu block không hợp lệ.")
    }
}
