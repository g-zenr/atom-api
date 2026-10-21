//! Project use cases.

use uuid::Uuid;

use crate::error::{AppError, AppResult, FieldViolation};
use crate::pagination::{Page, PaginationQuery};
use crate::projects::dto::{
    CreateProjectRequest, ProjectQuery, ProjectResponse, UpdateProjectRequest,
};
use crate::projects::repository::ProjectRepository;

/// Project use cases.
#[derive(Debug, Clone)]
pub struct ProjectService {
    repository: ProjectRepository,
}

impl ProjectService {
    /// Builds the service.
    pub fn new(repository: ProjectRepository) -> Self {
        Self { repository }
    }

    /// Creates a project owned by `owner`.
    pub async fn create(
        &self,
        owner: Uuid,
        request: CreateProjectRequest,
    ) -> AppResult<ProjectResponse> {
        let name = validate_name(&request.name)?;
        let description = validate_description(&request.description)?;

        let project = self
            .repository
            .insert(Uuid::new_v4(), owner, &name, &description)
            .await?;

        Ok(ProjectResponse::from(&project))
    }

    /// Lists the owner's projects.
    pub async fn list(
        &self,
        owner: Uuid,
        query: ProjectQuery,
        pagination: PaginationQuery,
    ) -> AppResult<Page<ProjectResponse>> {
        let limit = pagination.effective_limit();
        let offset = pagination.effective_offset();

        let (projects, total) = self
            .repository
            .list_owned(owner, query.search.as_deref(), limit, offset)
            .await?;

        let items = projects.iter().map(ProjectResponse::from).collect();

        Ok(Page::new(items, limit, offset, total))
    }

    /// Loads one of the owner's projects.
    pub async fn get(&self, owner: Uuid, id: Uuid) -> AppResult<ProjectResponse> {
        let project = self
            .repository
            .find_owned(id, owner)
            .await?
            .ok_or(AppError::NotFound("The requested project does not exist."))?;

        Ok(ProjectResponse::from(&project))
    }

    /// Applies a partial update to one of the owner's projects.
    pub async fn update(
        &self,
        owner: Uuid,
        id: Uuid,
        request: UpdateProjectRequest,
    ) -> AppResult<ProjectResponse> {
        let name = request.name.as_deref().map(validate_name).transpose()?;
        let description = request
            .description
            .as_deref()
            .map(validate_description)
            .transpose()?;

        let project = self
            .repository
            .update_owned(id, owner, name.as_deref(), description.as_deref())
            .await?
            .ok_or(AppError::NotFound("The requested project does not exist."))?;

        Ok(ProjectResponse::from(&project))
    }

    /// Deletes one of the owner's projects. Its tasks are removed with it.
    pub async fn delete(&self, owner: Uuid, id: Uuid) -> AppResult<()> {
        let deleted = self.repository.delete_owned(id, owner).await?;

        if !deleted {
            return Err(AppError::NotFound("The requested project does not exist."));
        }

        Ok(())
    }
}

/// Validates a project name.
pub fn validate_name(name: &str) -> AppResult<String> {
    let trimmed = name.trim();
    let length = trimmed.chars().count();

    if length == 0 {
        return Err(AppError::validation(vec![FieldViolation::new(
            "name",
            "must not be empty",
        )]));
    }

    if length > crate::projects::dto::MAX_NAME_LENGTH {
        return Err(AppError::validation(vec![FieldViolation::new(
            "name",
            format!(
                "must be at most {} characters long",
                crate::projects::dto::MAX_NAME_LENGTH
            ),
        )]));
    }

    Ok(trimmed.to_string())
}

/// Validates a project description.
pub fn validate_description(description: &str) -> AppResult<String> {
    if description.chars().count() > crate::projects::dto::MAX_DESCRIPTION_LENGTH {
        return Err(AppError::validation(vec![FieldViolation::new(
            "description",
            format!(
                "must be at most {} characters long",
                crate::projects::dto::MAX_DESCRIPTION_LENGTH
            ),
        )]));
    }

    Ok(description.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_trimmed_and_required() {
        assert_eq!(validate_name("  ATOM  ").unwrap(), "ATOM");
        assert!(validate_name("   ").is_err());
    }

    #[test]
    fn oversized_descriptions_are_rejected() {
        let long = "x".repeat(crate::projects::dto::MAX_DESCRIPTION_LENGTH + 1);
        assert!(validate_description(&long).is_err());
    }
}
