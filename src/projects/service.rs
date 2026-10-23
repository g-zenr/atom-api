//! Project use cases.
//!
//! Every client-facing string comes from the message catalogue, so the wording
//! of an error is defined once for the whole API.

use std::sync::Arc;

use uuid::Uuid;

use crate::config::messages::Messages;
use crate::config::settings::PaginationSettings;
use crate::error::{AppError, AppResult};
use crate::helpers::{bounded_text, required_text, validate_search};
use crate::pagination::{Page, PaginationQuery};
use crate::projects::dto::{
    CreateProjectRequest, MAX_DESCRIPTION_LENGTH, MAX_NAME_LENGTH, ProjectQuery, ProjectResponse,
    UpdateProjectRequest,
};
use crate::projects::repository::ProjectRepository;

/// Project use cases.
#[derive(Debug, Clone)]
pub struct ProjectService {
    repository: ProjectRepository,
    messages: Arc<Messages>,
    pagination: PaginationSettings,
}

impl ProjectService {
    /// Builds the service.
    pub fn new(
        repository: ProjectRepository,
        messages: Arc<Messages>,
        pagination: PaginationSettings,
    ) -> Self {
        Self {
            repository,
            messages,
            pagination,
        }
    }

    /// Creates a project owned by `owner`.
    pub async fn create(
        &self,
        owner: Uuid,
        request: CreateProjectRequest,
    ) -> AppResult<ProjectResponse> {
        let name = validate_name(&request.name, &self.messages)?;
        let description = validate_description(&request.description, &self.messages)?;

        let project = self
            .repository
            .insert(Uuid::new_v4(), owner, &name, &description)
            .await?;

        Ok(ProjectResponse::from(project))
    }

    /// Lists the owner's projects.
    pub async fn list(
        &self,
        owner: Uuid,
        query: ProjectQuery,
        pagination: PaginationQuery,
    ) -> AppResult<Page<ProjectResponse>> {
        let limit = pagination.effective_limit(&self.pagination);
        let offset = pagination.effective_offset();

        let search = query
            .search
            .as_deref()
            .map(|term| validate_search(term, &self.messages))
            .transpose()?
            .filter(|term| !term.is_empty());

        let (projects, total) = self
            .repository
            .list_owned(owner, search.as_deref(), limit, offset)
            .await?;

        // The page is consumed: the strings of every project are moved into
        // the response rather than copied.
        let items = projects.into_iter().map(ProjectResponse::from).collect();

        Ok(Page::new(items, limit, offset, total))
    }

    /// Loads one of the owner's projects.
    pub async fn get(&self, owner: Uuid, id: Uuid) -> AppResult<ProjectResponse> {
        let project = self
            .repository
            .find_owned(id, owner)
            .await?
            .ok_or_else(|| AppError::NotFound(self.messages.resources.project_not_found.clone()))?;

        Ok(ProjectResponse::from(project))
    }

    /// Applies a partial update to one of the owner's projects.
    pub async fn update(
        &self,
        owner: Uuid,
        id: Uuid,
        request: UpdateProjectRequest,
    ) -> AppResult<ProjectResponse> {
        let name = request
            .name
            .as_deref()
            .map(|name| validate_name(name, &self.messages))
            .transpose()?;
        let description = request
            .description
            .as_deref()
            .map(|description| validate_description(description, &self.messages))
            .transpose()?;

        let project = self
            .repository
            .update_owned(id, owner, name.as_deref(), description.as_deref())
            .await?
            .ok_or_else(|| AppError::NotFound(self.messages.resources.project_not_found.clone()))?;

        Ok(ProjectResponse::from(project))
    }

    /// Deletes one of the owner's projects. Its tasks are removed with it.
    pub async fn delete(&self, owner: Uuid, id: Uuid) -> AppResult<()> {
        let deleted = self.repository.delete_owned(id, owner).await?;

        if !deleted {
            return Err(AppError::NotFound(
                self.messages.resources.project_not_found.clone(),
            ));
        }

        Ok(())
    }
}

/// Validates a project name.
///
/// The name is trimmed, and a name that is empty after trimming is rejected.
/// The stored value is therefore the trimmed one, which is what the database
/// constraint `projects_name_length_chk` measures.
pub fn validate_name(name: &str, messages: &Messages) -> AppResult<String> {
    required_text(
        "name",
        name,
        MAX_NAME_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

/// Validates a project description.
///
/// The description is kept verbatim: a description of spaces is stored as it
/// was sent. The stored value is what the database constraint
/// `projects_description_length_chk` measures.
pub fn validate_description(description: &str, messages: &Messages) -> AppResult<String> {
    bounded_text(
        "description",
        description,
        MAX_DESCRIPTION_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::messages::test_messages;
    use crate::error::code;

    #[test]
    fn names_are_trimmed_and_required() {
        let messages = test_messages();
        assert_eq!(validate_name("  ATOM  ", &messages).unwrap(), "ATOM");
        assert!(validate_name("   ", &messages).is_err());
    }

    #[test]
    fn oversized_names_are_rejected() {
        let messages = test_messages();
        let long = "x".repeat(MAX_NAME_LENGTH + 1);
        assert!(validate_name(&long, &messages).is_err());
    }

    #[test]
    fn the_name_limit_in_the_message_comes_from_the_constant() {
        let messages = test_messages();
        let long = "x".repeat(MAX_NAME_LENGTH + 1);

        let error = validate_name(&long, &messages).unwrap_err();
        let body = error.body();

        assert_eq!(body.error.code, code::VALIDATION_FAILED);
        assert_eq!(body.error.details[0].field, "name");
        assert_eq!(
            body.error.details[0].message,
            format!("must be at most {MAX_NAME_LENGTH} characters long")
        );
    }

    #[test]
    fn oversized_descriptions_are_rejected() {
        let messages = test_messages();
        let long = "x".repeat(MAX_DESCRIPTION_LENGTH + 1);
        assert!(validate_description(&long, &messages).is_err());
    }

    #[test]
    fn descriptions_are_kept_verbatim() {
        let messages = test_messages();
        assert_eq!(
            validate_description("   ", &messages).unwrap(),
            "   ",
            "a description of spaces is stored as it was sent"
        );
    }
}
