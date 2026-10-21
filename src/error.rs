//! The single application error type and its HTTP representation.
//!
//! Every fallible path in the service and repository layers returns
//! [`AppError`]. Handlers never construct HTTP responses directly, which
//! guarantees that the public error contract stays consistent.
//!
//! The public contract is documented in `docs/api/error-contract.md`.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use tracing::error;
use utoipa::ToSchema;

/// A single field-level validation failure.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct FieldViolation {
    pub field: String,
    pub message: String,
}

impl FieldViolation {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

/// The body returned for every non-2xx response.
///
/// ```json
/// { "error": { "code": "validation_failed", "message": "...", "details": [] } }
/// ```
#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct ErrorDetail {
    /// Stable, machine-readable code. Clients switch on this, never on the
    /// human-readable message.
    pub code: &'static str,
    /// Human-readable summary. Not intended for end users verbatim.
    pub message: String,
    /// Field-level details. Present only for validation failures.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<FieldViolation>,
}

/// Application error.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The request body or parameters failed validation.
    #[error("validation failed")]
    Validation {
        code: &'static str,
        message: String,
        details: Vec<FieldViolation>,
    },

    /// Credentials are missing, malformed or expired.
    #[error("authentication failed: {0}")]
    Unauthorized(&'static str),

    /// The caller is authenticated but not allowed to perform the operation.
    #[error("forbidden: {0}")]
    Forbidden(&'static str),

    /// The resource does not exist, or the caller may not know that it does.
    #[error("not found: {0}")]
    NotFound(&'static str),

    /// The request conflicts with the current state of the resource.
    #[error("conflict: {0}")]
    Conflict(&'static str),

    /// The request body exceeds the configured limit.
    #[error("payload too large")]
    PayloadTooLarge,

    /// The request is malformed: invalid JSON, an unparsable path parameter
    /// or an unparsable query parameter.
    #[error("bad request: {0}")]
    BadRequest(String),

    /// The caller exceeded the configured rate limit.
    #[error("rate limit exceeded")]
    RateLimited { retry_after_seconds: u64 },

    /// The request took too long to process.
    #[error("request timeout")]
    Timeout,

    /// A database failure. Details are logged, never returned to the caller.
    #[error("database error")]
    Database(#[from] sqlx::Error),
    /// An unexpected internal failure.
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    /// Builds a validation error from a list of field violations.
    pub fn validation(details: Vec<FieldViolation>) -> Self {
        Self::Validation {
            code: "validation_failed",
            message: "The request failed validation.".to_string(),
            details,
        }
    }

    /// Builds a validation error with a custom stable code and message.
    pub fn validation_with(code: &'static str, message: impl Into<String>) -> Self {
        Self::Validation {
            code,
            message: message.into(),
            details: Vec::new(),
        }
    }

    /// Stable error code exposed to clients.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation { code, .. } => code,
            Self::Unauthorized(_) => "unauthorized",
            Self::Forbidden(_) => "forbidden",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::PayloadTooLarge => "payload_too_large",
            Self::BadRequest(_) => "bad_request",
            Self::RateLimited { .. } => "rate_limited",
            Self::Timeout => "timeout",
            Self::Database(_) => "database_error",
            Self::Internal(_) => "internal_error",
        }
    }

    /// HTTP status code for this error.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Validation { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Timeout => StatusCode::REQUEST_TIMEOUT,
            Self::Database(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Renders the public error body.
    pub fn body(&self) -> ErrorBody {
        let (message, details) = match self {
            Self::Validation {
                message, details, ..
            } => (message.clone(), details.clone()),
            Self::Unauthorized(message) => (message.to_string(), Vec::new()),
            Self::Forbidden(message) => (message.to_string(), Vec::new()),
            Self::NotFound(message) => (message.to_string(), Vec::new()),
            Self::Conflict(message) => (message.to_string(), Vec::new()),
            Self::PayloadTooLarge => (
                "The request body is larger than the server accepts.".to_string(),
                Vec::new(),
            ),
            Self::BadRequest(message) => (message.clone(), Vec::new()),
            Self::RateLimited { .. } => (
                "Too many requests. Try again later.".to_string(),
                Vec::new(),
            ),
            Self::Timeout => ("The request timed out.".to_string(), Vec::new()),
            // Internal and database details stay in the logs.
            Self::Database(_) => (
                "The request could not be completed.".to_string(),
                Vec::new(),
            ),
            Self::Internal(_) => (
                "The request could not be completed.".to_string(),
                Vec::new(),
            ),
        };

        ErrorBody {
            error: ErrorDetail {
                code: self.code(),
                message,
                details,
            },
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();

        if status.is_server_error() {
            // A PostgreSQL error message can contain the failing row, which for
            // the `users` table includes the password hash. Only the SQLSTATE
            // and the error kind are logged, never the driver message.
            match &self {
                Self::Database(error) => error!(
                    sqlstate = ?error.as_database_error().and_then(|db| db.code()),
                    kind = ?error.as_database_error().map(|db| db.kind()),
                    "request failed with a database error"
                ),
                _ => error!(error = ?self, "request failed with a server error"),
            }
        } else {
            tracing::debug!(error = ?self, status = status.as_u16(), "request rejected");
        }

        let mut response = (status, Json(self.body())).into_response();

        if let Self::RateLimited {
            retry_after_seconds,
        } = self
        {
            if let Ok(value) = retry_after_seconds.to_string().parse() {
                response.headers_mut().insert("retry-after", value);
            }
        }

        response
    }
}

/// Convenient result alias for service and repository functions.
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_errors_use_422_and_expose_field_details() {
        let error = AppError::validation(vec![FieldViolation::new("email", "is required")]);
        assert_eq!(error.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = error.body();
        assert_eq!(body.error.code, "validation_failed");
        assert_eq!(body.error.details.len(), 1);
        assert_eq!(body.error.details[0].field, "email");
    }

    #[test]
    fn internal_errors_do_not_leak_details() {
        let error = AppError::Internal("connection string with password=hunter2".to_string());
        let rendered = serde_json::to_string(&error.body()).unwrap();
        assert!(!rendered.contains("hunter2"));
        assert!(rendered.contains("internal_error"));
    }

    #[test]
    fn database_errors_do_not_leak_driver_messages() {
        let error = AppError::Database(sqlx::Error::RowNotFound);
        let rendered = serde_json::to_string(&error.body()).unwrap();
        assert!(!rendered.contains("RowNotFound"));
        assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn not_found_and_forbidden_map_to_distinct_codes() {
        assert_eq!(AppError::NotFound("note").code(), "not_found");
        assert_eq!(AppError::Forbidden("other user").code(), "forbidden");
    }
}
