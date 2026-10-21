//! HTTP handlers for the notes endpoints.

use axum::extract::State;
use axum::http::StatusCode;
use uuid::Uuid;

use crate::api::extract::{Json, Path, Query};
use crate::api::middleware::AuthUser;
use crate::error::AppResult;
use crate::notes::dto::{CreateNoteRequest, NotePage, NoteQuery, NoteResponse, UpdateNoteRequest};
use crate::pagination::PaginationQuery;
use crate::state::AppState;

/// `GET /api/v1/notes` — list the caller's notes.
#[utoipa::path(
    get,
    path = "/api/v1/notes",
    tag = "notes",
    security(("bearer" = [])),
    params(
        NoteQuery,
        PaginationQuery
    ),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The caller's notes.", body = crate::notes::dto::NotePage),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn list_notes(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<NoteQuery>,
    Query(pagination): Query<PaginationQuery>,
) -> AppResult<Json<NotePage>> {
    let page = state.notes.list(user.user_id, query, pagination).await?;
    Ok(Json(NotePage::from(page)))
}

/// `POST /api/v1/notes` — create a note.
#[utoipa::path(
    post,
    path = "/api/v1/notes",
    tag = "notes",
    security(("bearer" = [])),
    request_body = CreateNoteRequest,
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 201, description = "The note was created.", body = crate::notes::dto::NoteResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn create_note(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<CreateNoteRequest>,
) -> AppResult<(StatusCode, Json<NoteResponse>)> {
    let note = state.notes.create(user.user_id, payload).await?;
    Ok((StatusCode::CREATED, Json(note)))
}

/// `GET /api/v1/notes/{note_id}` — fetch one note.
#[utoipa::path(
    get,
    path = "/api/v1/notes/{note_id}",
    tag = "notes",
    security(("bearer" = [])),
    params(("note_id" = Uuid, Path, description = "Note identifier")),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The note.", body = crate::notes::dto::NoteResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The resource does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn get_note(
    State(state): State<AppState>,
    user: AuthUser,
    Path(note_id): Path<Uuid>,
) -> AppResult<Json<NoteResponse>> {
    let note = state.notes.get(user.user_id, note_id).await?;
    Ok(Json(note))
}

/// `PATCH /api/v1/notes/{note_id}` — partially update a note.
#[utoipa::path(
    patch,
    path = "/api/v1/notes/{note_id}",
    tag = "notes",
    security(("bearer" = [])),
    params(("note_id" = Uuid, Path, description = "Note identifier")),
    request_body = UpdateNoteRequest,
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The updated note.", body = crate::notes::dto::NoteResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The resource does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn update_note(
    State(state): State<AppState>,
    user: AuthUser,
    Path(note_id): Path<Uuid>,
    Json(payload): Json<UpdateNoteRequest>,
) -> AppResult<Json<NoteResponse>> {
    let note = state.notes.update(user.user_id, note_id, payload).await?;
    Ok(Json(note))
}

/// `DELETE /api/v1/notes/{note_id}` — delete a note.
#[utoipa::path(
    delete,
    path = "/api/v1/notes/{note_id}",
    tag = "notes",
    security(("bearer" = [])),
    params(("note_id" = Uuid, Path, description = "Note identifier")),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 204, description = "The note was deleted."),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The resource does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn delete_note(
    State(state): State<AppState>,
    user: AuthUser,
    Path(note_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    state.notes.delete(user.user_id, note_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
