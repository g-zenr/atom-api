//! The single error type and its HTTP representation.
//!
//! Every fallible path in the service and repository layers returns
//! [`AppError`]. Handlers never construct HTTP responses directly, which
//! guarantees that the public error contract stays consistent.
//!
//! The public contract is documented in `docs/api/error-contract.md`.
//!
//! Two things are deliberately kept apart:
//!
//! * the **stable error code**, which is part of the API contract and lives
//!   in [`code`] — one definition per code, never two;
//! * the **message text**, which is client-facing wording and lives in the
//!   message catalogue in `config/messages.toml`.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use tracing::error;
use utoipa::ToSchema;

/// The stable, machine-readable error codes.
///
/// A client switches on these, never on the message text. Each code has
/// exactly one definition here.
pub mod code {
    /// A request failed a validation rule.
    pub const VALIDATION_FAILED: &str = "validation_failed";
    /// The request is malformed.
    pub const BAD_REQUEST: &str = "bad_request";
    /// The request needs credentials and does not carry valid ones.
    pub const UNAUTHORIZED: &str = "unauthorized";
    /// The caller is authenticated but not allowed to perform the operation.
    pub const FORBIDDEN: &str = "forbidden";
    /// The resource does not exist, or the caller may not know that it does.
    pub const NOT_FOUND: &str = "not_found";
    /// The request conflicts with the current state of the resource.
    pub const CONFLICT: &str = "conflict";
    /// The request body is larger than the server accepts.
    pub const PAYLOAD_TOO_LARGE: &str = "payload_too_large";
    /// The caller exceeded the rate limit.
    pub const RATE_LIMITED: &str = "rate_limited";
    /// The request took too long.
    pub const TIMEOUT: &str = "timeout";
    /// A database failure.
    pub const DATABASE_ERROR: &str = "database_error";
    /// An unexpected failure.
    pub const INTERNAL_ERROR: &str = "internal_error";
}

/// A single field-level validation failure.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct FieldViolation {
    /// The field that failed.
    pub field: String,
    /// Why it failed, in words meant for a developer.
    pub message: String,
}

impl FieldViolation {
    /// Builds one field-level failure.
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
    /// The error itself.
    pub error: ErrorDetail,
}

/// The detail of an error.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct ErrorDetail {
    /// Stable, machine-readable code. A client switches on this.
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
        /// The stable error code.
        code: &'static str,
        /// The client-facing message.
        message: String,
        /// The fields that failed.
        details: Vec<FieldViolation>,
    },

    /// Credentials are missing, malformed or expired.
    #[error("authentication failed: {0}")]
    Unauthorized(String),

    /// The caller is authenticated but not allowed to perform the operation.
    #[error("forbidden: {0}")]
    Forbidden(String),

    /// The resource does not exist, or the caller may not know that it does.
    #[error("not found: {0}")]
    NotFound(String),

    /// The request conflicts with the current state of the resource.
    #[error("conflict: {0}")]
    Conflict(String),

    /// The request is malformed.
    #[error("bad request: {0}")]
    BadRequest(String),

    /// The request body exceeds the configured limit.
    #[error("payload too large")]
    PayloadTooLarge(String),

    /// The caller exceeded the configured rate limit.
    #[error("rate limit exceeded")]
    RateLimited {
        /// How long the caller should wait, in seconds.
        retry_after_seconds: u64,
        /// The client-facing message.
        message: String,
    },

    /// The request took too long.
    #[error("request timeout")]
    Timeout(String),

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
            code: code::VALIDATION_FAILED,
            message: String::new(),
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

    /// Sets the message of a validation error.
    ///
    /// The message comes from the catalogue; the code stays a compile-time
    /// constant.
    pub fn with_message(mut self, text: impl Into<String>) -> Self {
        if let Self::Validation { message, .. } = &mut self {
            *message = text.into();
        }
        self
    }

    /// Stable error code exposed to clients.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation { code, .. } => code,
            Self::Unauthorized(_) => code::UNAUTHORIZED,
            Self::Forbidden(_) => code::FORBIDDEN,
            Self::NotFound(_) => code::NOT_FOUND,
            Self::Conflict(_) => code::CONFLICT,
            Self::BadRequest(_) => code::BAD_REQUEST,
            Self::PayloadTooLarge(_) => code::PAYLOAD_TOO_LARGE,
            Self::RateLimited { .. } => code::RATE_LIMITED,
            Self::Timeout(_) => code::TIMEOUT,
            Self::Database(_) => code::DATABASE_ERROR,
            Self::Internal(_) => code::INTERNAL_ERROR,
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
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Timeout(_) => StatusCode::REQUEST_TIMEOUT,
            Self::Database(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Renders the public error body.
    pub fn body(&self) -> ErrorBody {
        let (message, details) = match self {
            Self::Validation {
                message, details, ..
            } => (message.clone(), details.clone()),
            Self::Unauthorized(message)
            | Self::Forbidden(message)
            | Self::NotFound(message)
            | Self::Conflict(message)
            | Self::BadRequest(message) => (message.clone(), Vec::new()),
            Self::PayloadTooLarge(message) => (message.clone(), Vec::new()),
            Self::RateLimited { message, .. } => (message.clone(), Vec::new()),
            Self::Timeout(message) => (message.clone(), Vec::new()),
            // Internal and database details stay in the logs.
            Self::Database(_) | Self::Internal(_) => (
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
            ..
        } = self
            && let Ok(value) = retry_after_seconds.to_string().parse()
        {
            response.headers_mut().insert("retry-after", value);
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
        let error = AppError::validation(vec![FieldViolation::new("email", "is required")])
            .with_message("The request failed validation.");
        assert_eq!(error.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = error.body();
        assert_eq!(body.error.code, code::VALIDATION_FAILED);
        assert_eq!(body.error.details.len(), 1);
        assert_eq!(body.error.details[0].field, "email");
    }

    #[test]
    fn internal_errors_do_not_leak_details() {
        let error = AppError::Internal("connection string with password=hunter2".to_string());
        let rendered = serde_json::to_string_pretty(&error.body()).unwrap();
        assert!(!rendered.contains("hunter2"));
        assert!(rendered.contains(code::INTERNAL_ERROR));
    }

    #[test]
    fn database_errors_do_not_leak_driver_messages() {
        let error = AppError::Database(sqlx::Error::RowNotFound);
        let rendered = serde_json::to_string_pretty(&error.body()).unwrap();
        assert!(!rendered.contains("RowNotFound"));
        assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn every_code_has_one_definition() {
        // The codes are the contract a client switches on. They are defined
        // once, in the `code` module, and nothing else may define one.
        assert_eq!(code::VALIDATION_FAILED, "validation_failed");
        assert_eq!(code::BAD_REQUEST, "bad_request");
        assert_eq!(code::UNAUTHORIZED, "unauthorized");
        assert_eq!(code::FORBIDDEN, "forbidden");
        assert_eq!(code::NOT_FOUND, "not_found");
        assert_eq!(code::CONFLICT, "conflict");
        assert_eq!(code::PAYLOAD_TOO_LARGE, "payload_too_large");
        assert_eq!(code::RATE_LIMITED, "rate_limited");
        assert_eq!(code::TIMEOUT, "timeout");
        assert_eq!(code::DATABASE_ERROR, "database_error");
        assert_eq!(code::INTERNAL_ERROR, "internal_error");
    }

    #[test]
    fn not_found_and_forbidden_map_to_distinct_codes() {
        assert_eq!(
            AppError::NotFound("note".to_string()).code(),
            code::NOT_FOUND
        );
        assert_eq!(
            AppError::Forbidden("other user".to_string()).code(),
            code::FORBIDDEN
        );
    }
}
