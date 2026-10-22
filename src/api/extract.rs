//! Extractors that report rejections through the ATOM error contract.
//!
//! Axum's built-in `Json`, `Path` and `Query` extractors reject with
//! `text/plain` bodies and framework-specific wording. That would break the
//! documented error envelope, so the API uses these wrappers instead: every
//! rejection becomes an [`AppError`] and therefore the same JSON shape as
//! every other error response.
//!
//! The mapping is deliberate:
//!
//! * A malformed request — invalid JSON syntax, an unparsable path parameter,
//!   an unparsable query parameter — is `400 Bad Request`.
//! * A well-formed request that fails a business rule — an unknown enum
//!   variant, a too-long title — is `422 Unprocessable Entity`.
//!
//! The message text comes from the catalogue in `config/messages.toml`; the
//! stable error code comes from [`crate::error::code`].

use axum::RequestExt;
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::Extensions;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::sync::Arc;

use crate::api::RequestContext;
use crate::config::messages::Messages;
use crate::error::{AppError, FieldViolation};

/// A JSON request body.
#[derive(Debug, Clone, Copy, Default)]
pub struct Json<T>(pub T);

/// A JSON response body.
///
/// The wrapper is transparent for responses: it delegates to `axum::Json` so
/// that handlers can return `Json<T>` without importing two different `Json`
/// types.
impl<T> IntoResponse for Json<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

impl<T, S> FromRequest<S> for Json<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        let messages = request_messages(req.extensions())?;

        // `into_limited_body` applies the limit the router installed with
        // `DefaultBodyLimit`, which is the configured `max_body_bytes`. The
        // backstop below only bounds a body that reached the extractor
        // without the layer installed, or one whose configured limit is
        // larger than the backstop.
        let body = req.into_limited_body();

        let bytes = axum::body::to_bytes(body, crate::config::MAX_BODY_BYTES_BACKSTOP)
            .await
            .map_err(|_| AppError::PayloadTooLarge(messages.general.payload_too_large.clone()))?;

        match serde_json::from_slice::<T>(&bytes) {
            Ok(value) => Ok(Json(value)),
            // A body that is not JSON at all is a malformed request.
            Err(error) if error.is_syntax() || error.is_eof() => {
                Err(AppError::BadRequest(messages.general.bad_request.clone()))
            }
            // A well-formed body that does not match the contract fails a
            // business rule. The specific reason stays in the field detail,
            // where a client wants it.
            Err(error) => Err(AppError::validation(vec![FieldViolation::new(
                "body",
                error.to_string(),
            )])
            .with_message(messages.general.validation_failed.clone())),
        }
    }
}

/// A path parameter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Path<T>(pub T);

impl<T, S> FromRequestParts<S> for Path<T>
where
    T: DeserializeOwned + Send + 'static,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let messages = request_messages(&parts.extensions)?;

        match axum::extract::Path::<T>::from_request_parts(parts, state).await {
            Ok(value) => Ok(Path(value.0)),
            Err(rejection) => {
                tracing::debug!(detail = %rejection.body_text(), "path parameter rejected");
                Err(AppError::BadRequest(
                    messages.general.malformed_parameter.clone(),
                ))
            }
        }
    }
}

/// A query string.
#[derive(Debug, Clone, Copy, Default)]
pub struct Query<T>(pub T);

impl<T, S> FromRequestParts<S> for Query<T>
where
    T: DeserializeOwned + Send + 'static,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let messages = request_messages(&parts.extensions)?;

        match axum::extract::Query::<T>::from_request_parts(parts, state).await {
            Ok(value) => Ok(Query(value.0)),
            Err(rejection) => {
                tracing::debug!(detail = %rejection.body_text(), "query parameter rejected");
                Err(AppError::BadRequest(
                    messages.general.malformed_parameter.clone(),
                ))
            }
        }
    }
}

/// Reads the message catalogue from the request extensions.
///
/// The three extractors reject through the same catalogue, so they read it
/// through the same helper. A missing context is an internal error: the
/// router installs it before any route runs.
///
/// # Errors
///
/// Returns [`AppError::Internal`] when the request carries no
/// [`RequestContext`].
fn request_messages(extensions: &Extensions) -> Result<Arc<Messages>, AppError> {
    extensions
        .get::<RequestContext>()
        .map(|context| context.messages.clone())
        .ok_or_else(|| AppError::Internal("the request context is missing".to_string()))
}
