//! Task use cases.

use uuid::Uuid;

use crate::error::{AppError, AppResult, FieldViolation};
use crate::pagination::{Page, PaginationQuery};
use crate::tasks::dto::{CreateTaskRequest, TaskQuery, TaskResponse, UpdateTaskRequest};
use crate::tasks::models::{TaskPriority, TaskStatus};
use crate::tasks::repository::{NewTask, TaskFilters, TaskPatch, TaskRepository};

/// Task use cases.
#[derive(Debug, Clone)]
pub struct TaskService {
    repository: TaskRepository,
}

impl TaskService {
    /// Builds the service.
    pub fn new(repository: TaskRepository) -> Self {
        Self { repository }
    }

    /// Creates a task inside a project owned by `owner`.
    pub async fn create(
        &self,
        owner: Uuid,
        project_id: Uuid,
        request: CreateTaskRequest,
    ) -> AppResult<TaskResponse> {
        let title = validate_title(&request.title)?;
        let description = validate_description(&request.description)?;

        let status = request.status.unwrap_or(TaskStatus::Open);
        let priority = request.priority.unwrap_or(TaskPriority::Medium);

        let task = self
            .repository
            .insert(NewTask {
                id: Uuid::new_v4(),
                user_id: owner,
                project_id,
                title,
                description,
                status,
                priority,
                position: request.position,
            })
            .await?
            .ok_or(AppError::NotFound("The requested project does not exist."))?;

        Ok(TaskResponse::from(&task))
    }

    /// Lists the tasks of a project owned by `owner`.
    pub async fn list_for_project(
        &self,
        owner: Uuid,
        project_id: Uuid,
        query: TaskQuery,
        pagination: PaginationQuery,
    ) -> AppResult<Page<TaskResponse>> {
        let limit = pagination.effective_limit();
        let offset = pagination.effective_offset();

        let filters = TaskFilters {
            status: query.status,
            priority: query.priority,
        };

        let (tasks, total) = self
            .repository
            .list_for_project(owner, project_id, &filters, limit, offset)
            .await?
            .ok_or(AppError::NotFound("The requested project does not exist."))?;

        let items = tasks.iter().map(TaskResponse::from).collect();

        Ok(Page::new(items, limit, offset, total))
    }

    /// Loads one task owned by `owner`.
    pub async fn get(&self, owner: Uuid, id: Uuid) -> AppResult<TaskResponse> {
        let task = self
            .repository
            .find_owned(id, owner)
            .await?
            .ok_or(AppError::NotFound("The requested task does not exist."))?;

        Ok(TaskResponse::from(&task))
    }

    /// Applies a partial update to one task owned by `owner`.
    pub async fn update(
        &self,
        owner: Uuid,
        id: Uuid,
        request: UpdateTaskRequest,
    ) -> AppResult<TaskResponse> {
        let title = request.title.as_deref().map(validate_title).transpose()?;
        let description = request
            .description
            .as_deref()
            .map(validate_description)
            .transpose()?;

        let task = self
            .repository
            .update_owned(
                id,
                owner,
                TaskPatch {
                    title,
                    description,
                    status: request.status,
                    priority: request.priority,
                    position: request.position,
                },
            )
            .await?
            .ok_or(AppError::NotFound("The requested task does not exist."))?;

        Ok(TaskResponse::from(&task))
    }

    /// Deletes one task owned by `owner`.
    pub async fn delete(&self, owner: Uuid, id: Uuid) -> AppResult<()> {
        let deleted = self.repository.delete_owned(id, owner).await?;

        if !deleted {
            return Err(AppError::NotFound("The requested task does not exist."));
        }

        Ok(())
    }
}

/// Validates a task title.
pub fn validate_title(title: &str) -> AppResult<String> {
    let trimmed = title.trim();
    let length = trimmed.chars().count();

    if length == 0 {
        return Err(AppError::validation(vec![FieldViolation::new(
            "title",
            "must not be empty",
        )]));
    }

    if length > crate::tasks::dto::MAX_TITLE_LENGTH {
        return Err(AppError::validation(vec![FieldViolation::new(
            "title",
            format!(
                "must be at most {} characters long",
                crate::tasks::dto::MAX_TITLE_LENGTH
            ),
        )]));
    }

    Ok(trimmed.to_string())
}

/// Validates a task description.
pub fn validate_description(description: &str) -> AppResult<String> {
    if description.chars().count() > crate::tasks::dto::MAX_DESCRIPTION_LENGTH {
        return Err(AppError::validation(vec![FieldViolation::new(
            "description",
            format!(
                "must be at most {} characters long",
                crate::tasks::dto::MAX_DESCRIPTION_LENGTH
            ),
        )]));
    }

    Ok(description.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_trimmed_and_required() {
        assert_eq!(validate_title("  Ship it  ").unwrap(), "Ship it");
        assert!(validate_title("  ").is_err());
    }

    #[test]
    fn status_and_priority_have_stable_wire_names() {
        assert_eq!(TaskStatus::InProgress.as_str(), "in_progress");
        assert_eq!(TaskPriority::Urgent.as_str(), "urgent");
    }

    #[test]
    fn status_round_trips_through_serde() {
        let json = serde_json::to_string(&TaskStatus::InProgress).unwrap();
        assert_eq!(json, "\"in_progress\"");
        let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, TaskStatus::InProgress);
    }
}
