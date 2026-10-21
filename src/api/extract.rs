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

use axum::RequestExt;
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{AppError, FieldViolation};

/// The outer bound on the body the extractor will read.
///
/// The effective limit is the one the router installs with
/// `DefaultBodyLimit`, which is `APP_MAX_BODY_BYTES`. That limit is applied
/// by wrapping the body in `http_body_util::Limited`; this constant is the
/// backstop for a request that reaches the extractor without the layer.
const MAX_JSON_BODY_BYTES: usize = 4 * 1024 * 1024;

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
        // `into_limited_body` applies the limit the router installed with
        // `DefaultBodyLimit`, which is `APP_MAX_BODY_BYTES`. Reading it here
        // is what makes that setting the effective ceiling.
        let body = req.into_limited_body();

        let bytes = axum::body::to_bytes(body, MAX_JSON_BODY_BYTES)
            .await
            .map_err(|_| AppError::PayloadTooLarge)?;

        match serde_json::from_slice::<T>(&bytes) {
            Ok(value) => Ok(Json(value)),
            // A body that is not JSON at all is a malformed request.
            Err(error) if error.is_syntax() || error.is_eof() => Err(AppError::BadRequest(
                "The request body is not valid JSON.".to_string(),
            )),
            // A well-formed body that does not match the contract fails a
            // business rule.
            Err(error) => Err(AppError::validation(vec![FieldViolation::new(
                "body",
                error.to_string(),
            )])),
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
        match axum::extract::Path::<T>::from_request_parts(parts, state).await {
            Ok(value) => Ok(Path(value.0)),
            Err(rejection) => Err(AppError::BadRequest(rejection.body_text())),
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
        match axum::extract::Query::<T>::from_request_parts(parts, state).await {
            Ok(value) => Ok(Query(value.0)),
            Err(rejection) => Err(AppError::BadRequest(rejection.body_text())),
        }
    }
}
