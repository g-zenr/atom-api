//! Note use cases: validation, ownership and listing rules.
//!
//! Every client-facing string comes from the message catalogue, so the wording
//! of an error is defined once for the whole API.

use std::sync::Arc;

use uuid::Uuid;

use crate::config::messages::Messages;
use crate::error::{AppError, AppResult};
use crate::helpers::{bounded_text, required_text};
use crate::notes::dto::{
    CreateNoteRequest, MAX_CONTENT_LENGTH, MAX_TITLE_LENGTH, NoteQuery, NoteResponse,
    UpdateNoteRequest,
};
use crate::notes::repository::{NoteFilters, NoteRepository};
use crate::pagination::{Page, PaginationQuery};

/// Note use cases.
#[derive(Debug, Clone)]
pub struct NoteService {
    repository: NoteRepository,
    messages: Arc<Messages>,
}

impl NoteService {
    /// Builds the service.
    pub fn new(repository: NoteRepository, messages: Arc<Messages>) -> Self {
        Self {
            repository,
            messages,
        }
    }

    /// Creates a note for the given owner.
    pub async fn create(&self, owner: Uuid, request: CreateNoteRequest) -> AppResult<NoteResponse> {
        let title = validate_title(&request.title, &self.messages)?;
        let content = validate_content(&request.content, &self.messages)?;

        let note = self
            .repository
            .insert(Uuid::new_v4(), owner, &title, &content)
            .await?;

        Ok(NoteResponse::from(&note))
    }

    /// Lists the owner's notes.
    pub async fn list(
        &self,
        owner: Uuid,
        query: NoteQuery,
        pagination: PaginationQuery,
    ) -> AppResult<Page<NoteResponse>> {
        let filters = NoteFilters {
            search: query.search,
            archived: query.archived,
        };

        let limit = pagination.effective_limit();
        let offset = pagination.effective_offset();

        let (notes, total) = self
            .repository
            .list_owned(owner, &filters, limit, offset)
            .await?;

        let items = notes.iter().map(NoteResponse::from).collect();

        Ok(Page::new(items, limit, offset, total))
    }

    /// Loads one of the owner's notes.
    pub async fn get(&self, owner: Uuid, id: Uuid) -> AppResult<NoteResponse> {
        let note = self
            .repository
            .find_owned(id, owner)
            .await?
            .ok_or_else(|| AppError::NotFound(self.messages.resources.note_not_found.clone()))?;

        Ok(NoteResponse::from(&note))
    }

    /// Applies a partial update to one of the owner's notes.
    pub async fn update(
        &self,
        owner: Uuid,
        id: Uuid,
        request: UpdateNoteRequest,
    ) -> AppResult<NoteResponse> {
        let title = request
            .title
            .as_deref()
            .map(|title| validate_title(title, &self.messages))
            .transpose()?;
        let content = request
            .content
            .as_deref()
            .map(|content| validate_content(content, &self.messages))
            .transpose()?;

        let note = self
            .repository
            .update_owned(
                id,
                owner,
                title.as_deref(),
                content.as_deref(),
                request.is_archived,
            )
            .await?
            .ok_or_else(|| AppError::NotFound(self.messages.resources.note_not_found.clone()))?;

        Ok(NoteResponse::from(&note))
    }

    /// Deletes one of the owner's notes.
    pub async fn delete(&self, owner: Uuid, id: Uuid) -> AppResult<()> {
        let deleted = self.repository.delete_owned(id, owner).await?;

        if !deleted {
            return Err(AppError::NotFound(
                self.messages.resources.note_not_found.clone(),
            ));
        }

        Ok(())
    }
}

/// Validates a note title.
///
/// The title is trimmed, and a title that is empty after trimming is rejected.
/// The stored value is therefore the trimmed one, which is what the database
/// constraint `notes_title_length_chk` measures.
pub fn validate_title(title: &str, messages: &Messages) -> AppResult<String> {
    required_text(
        "title",
        title,
        MAX_TITLE_LENGTH,
        &messages.validation.title_too_long,
        messages,
    )
}

/// Validates note content.
///
/// The body is kept verbatim: a body of spaces is stored as it was sent. The
/// stored value is what the database constraint `notes_content_length_chk`
/// measures.
pub fn validate_content(content: &str, messages: &Messages) -> AppResult<String> {
    bounded_text(
        "content",
        content,
        MAX_CONTENT_LENGTH,
        &messages.validation.content_too_long,
        messages,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::code;

    fn messages() -> Messages {
        // The shipped catalogue, so that the tests exercise the same templates
        // the application serves.
        Messages::from_toml(include_str!("../../config/messages.toml"))
            .expect("the message catalogue parses")
    }

    #[test]
    fn titles_are_trimmed_and_must_not_be_empty() {
        let messages = messages();
        assert_eq!(validate_title("  hello  ", &messages).unwrap(), "hello");
        assert!(validate_title("   ", &messages).is_err());
    }

    #[test]
    fn oversized_titles_are_rejected() {
        let messages = messages();
        let long = "x".repeat(MAX_TITLE_LENGTH + 1);
        assert!(validate_title(&long, &messages).is_err());
    }

    #[test]
    fn the_title_limit_in_the_message_comes_from_the_constant() {
        let messages = messages();
        let long = "x".repeat(MAX_TITLE_LENGTH + 1);

        let error = validate_title(&long, &messages).unwrap_err();
        let body = error.body();

        assert_eq!(body.error.code, code::VALIDATION_FAILED);
        assert_eq!(body.error.details[0].field, "title");
        assert_eq!(
            body.error.details[0].message,
            format!("must be at most {MAX_TITLE_LENGTH} characters long")
        );
    }

    #[test]
    fn oversized_content_is_rejected() {
        let messages = messages();
        let long = "x".repeat(MAX_CONTENT_LENGTH + 1);
        assert!(validate_content(&long, &messages).is_err());
    }

    #[test]
    fn content_is_kept_verbatim() {
        let messages = messages();
        assert_eq!(validate_content("   ", &messages).unwrap(), "   ");
    }
}
