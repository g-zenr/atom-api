//! Public contracts for the `notes` module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::notes::models::Note;
use crate::pagination::{Page, Pagination};

/// Maximum accepted note title length.
///
/// Mirrors the database constraint `notes_title_length_chk`
/// (`CHECK (CHAR_LENGTH(title) BETWEEN 1 AND 200)`) in
/// `migrations/0002_notes.sql`. The migration is the enforced copy; this
/// constant is the application-side mirror of it.
pub const MAX_TITLE_LENGTH: usize = 200;

/// Maximum accepted note content length.
///
/// Mirrors the database constraint `notes_content_length_chk`
/// (`CHECK (CHAR_LENGTH(content) <= 100000)`) in `migrations/0002_notes.sql`.
/// The migration is the enforced copy; this constant is the application-side
/// mirror of it.
pub const MAX_CONTENT_LENGTH: usize = 100_000;

/// The public representation of a note.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct NoteResponse {
    /// Stable public identifier.
    pub id: Uuid,
    /// Note title.
    pub title: String,
    /// Note body.
    pub content: String,
    /// Whether the note is archived.
    pub is_archived: bool,
    /// Creation timestamp, RFC 3339 UTC.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp, RFC 3339 UTC.
    pub updated_at: DateTime<Utc>,
}

/// Builds the public representation of a note.
///
/// The model is consumed rather than borrowed, so that the strings of a
/// listed page are moved into the response instead of being copied. A page
/// at the published limits carries megabytes of text, and a borrowed
/// conversion would copy all of it.
impl From<Note> for NoteResponse {
    fn from(note: Note) -> Self {
        Self {
            id: note.id,
            title: note.title,
            content: note.content,
            is_archived: note.is_archived,
            created_at: note.created_at,
            updated_at: note.updated_at,
        }
    }
}

/// Request body for creating a note.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateNoteRequest {
    /// Note title.
    #[schema(example = "Release checklist")]
    pub title: String,
    /// Note body. Defaults to an empty string.
    #[schema(example = "Cut the tag, run the smoke tests, publish the notes.")]
    #[serde(default)]
    pub content: String,
}

/// Request body for partially updating a note.
///
/// Every field is optional. Omitted fields are left untouched; they are never
/// written as `null`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateNoteRequest {
    /// New title.
    #[schema(example = "Release checklist v2")]
    pub title: Option<String>,
    /// New body.
    pub content: Option<String>,
    /// New archive state.
    pub is_archived: Option<bool>,
}

/// Query parameters accepted by `GET /api/v1/notes`.
///
/// Only the feature filters live here. The pagination parameters live in
/// [`crate::pagination::PaginationQuery`], which the handler extracts
/// separately: declaring `limit` and `offset` in both places would publish
/// them twice in the generated OpenAPI document.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct NoteQuery {
    /// Free-text search over title and content.
    ///
    /// At most 200 characters. A longer term is a validation failure.
    #[param(example = "release")]
    pub search: Option<String>,
    /// Filter by archive state. When omitted, only active notes are returned.
    /// `true` returns archived notes, `false` returns active notes.
    pub archived: Option<bool>,
}

/// The response body of `GET /api/v1/notes`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct NotePage {
    /// The notes on this page.
    pub items: Vec<NoteResponse>,
    /// Pagination metadata.
    pub pagination: Pagination,
}

impl From<Page<NoteResponse>> for NotePage {
    fn from(page: Page<NoteResponse>) -> Self {
        Self {
            items: page.items,
            pagination: page.pagination,
        }
    }
}
