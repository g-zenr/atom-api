//! Public contracts for the `notes` module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::notes::models::Note;
use crate::pagination::{Page, Pagination, PaginationQuery};

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
/// [`crate::pagination::PaginationQuery`], and the handler reads both halves
/// from a single [`NotesListQuery`]: declaring `limit` and `offset` in both
/// places would publish them twice in the generated OpenAPI document.
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

/// The whole query string of `GET /api/v1/notes`, deserialized once.
///
/// The handler takes a single `Query<…>` extractor over this type. The query
/// string is therefore parsed exactly once, and [`Self::into_parts`] splits the
/// result into the pair the service layer already works with.
///
/// The fields are written out instead of composing [`NoteQuery`] and
/// [`PaginationQuery`] with `#[serde(flatten)]`. The extractor deserializes
/// with `serde_urlencoded`, whose map access yields strings, so a flattened
/// `Option<i64>` would be asked to deserialize itself from a string and would
/// fail at run time, on a real request, instead of at compile time. The
/// duplication is the price of parsing the string once.
///
/// It derives `Deserialize` only, and is deliberately absent from the
/// generated OpenAPI document: the published parameter set stays [`NoteQuery`]
/// and [`PaginationQuery`], which the handler's `utoipa` attribute names.
#[derive(Debug, Clone, Deserialize)]
pub struct NotesListQuery {
    /// Maximum number of notes to return.
    pub limit: Option<i64>,
    /// Number of notes to skip.
    pub offset: Option<i64>,
    /// Free-text search over title and content.
    pub search: Option<String>,
    /// Filter by archive state.
    pub archived: Option<bool>,
}

impl NotesListQuery {
    /// Splits the parsed query into its filter and pagination halves.
    pub fn into_parts(self) -> (NoteQuery, PaginationQuery) {
        let Self {
            limit,
            offset,
            search,
            archived,
        } = self;

        (
            NoteQuery { search, archived },
            PaginationQuery { limit, offset },
        )
    }
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
