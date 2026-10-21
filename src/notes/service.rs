//! Note use cases: validation, ownership and listing rules.

use uuid::Uuid;

use crate::error::{AppError, AppResult, FieldViolation};
use crate::notes::dto::{CreateNoteRequest, NoteQuery, NoteResponse, UpdateNoteRequest};
use crate::notes::repository::{NoteFilters, NoteRepository};
use crate::pagination::{Page, PaginationQuery};

/// Note use cases.
#[derive(Debug, Clone)]
pub struct NoteService {
    repository: NoteRepository,
}

impl NoteService {
    /// Builds the service.
    pub fn new(repository: NoteRepository) -> Self {
        Self { repository }
    }

    /// Creates a note for the given owner.
    pub async fn create(&self, owner: Uuid, request: CreateNoteRequest) -> AppResult<NoteResponse> {
        let title = validate_title(&request.title)?;
        let content = validate_content(&request.content)?;

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
            search: query.search.clone(),
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
            .ok_or(AppError::NotFound("The requested note does not exist."))?;

        Ok(NoteResponse::from(&note))
    }

    /// Applies a partial update to one of the owner's notes.
    pub async fn update(
        &self,
        owner: Uuid,
        id: Uuid,
        request: UpdateNoteRequest,
    ) -> AppResult<NoteResponse> {
        let title = request.title.as_deref().map(validate_title).transpose()?;
        let content = request
            .content
            .as_deref()
            .map(validate_content)
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
            .ok_or(AppError::NotFound("The requested note does not exist."))?;

        Ok(NoteResponse::from(&note))
    }

    /// Deletes one of the owner's notes.
    pub async fn delete(&self, owner: Uuid, id: Uuid) -> AppResult<()> {
        let deleted = self.repository.delete_owned(id, owner).await?;

        if !deleted {
            return Err(AppError::NotFound("The requested note does not exist."));
        }

        Ok(())
    }
}

/// Validates a note title.
pub fn validate_title(title: &str) -> AppResult<String> {
    let trimmed = title.trim();
    let length = trimmed.chars().count();

    if length == 0 {
        return Err(AppError::validation(vec![FieldViolation::new(
            "title",
            "must not be empty",
        )]));
    }

    if length > crate::notes::dto::MAX_TITLE_LENGTH {
        return Err(AppError::validation(vec![FieldViolation::new(
            "title",
            format!(
                "must be at most {} characters long",
                crate::notes::dto::MAX_TITLE_LENGTH
            ),
        )]));
    }

    Ok(trimmed.to_string())
}

/// Validates note content.
pub fn validate_content(content: &str) -> AppResult<String> {
    if content.chars().count() > crate::notes::dto::MAX_CONTENT_LENGTH {
        return Err(AppError::validation(vec![FieldViolation::new(
            "content",
            format!(
                "must be at most {} characters long",
                crate::notes::dto::MAX_CONTENT_LENGTH
            ),
        )]));
    }

    Ok(content.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_trimmed_and_must_not_be_empty() {
        assert_eq!(validate_title("  hello  ").unwrap(), "hello");
        assert!(validate_title("   ").is_err());
    }

    #[test]
    fn oversized_titles_are_rejected() {
        let long = "x".repeat(crate::notes::dto::MAX_TITLE_LENGTH + 1);
        assert!(validate_title(&long).is_err());
    }

    #[test]
    fn oversized_content_is_rejected() {
        let long = "x".repeat(crate::notes::dto::MAX_CONTENT_LENGTH + 1);
        assert!(validate_content(&long).is_err());
    }
}
