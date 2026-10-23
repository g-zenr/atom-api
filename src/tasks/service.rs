//! Task use cases.
//!
//! The service owns the business rules of a task: the length rules of a title
//! and a description, the ownership rules that are inherited through the
//! project, and the completion timestamp.
//!
//! Every client-facing string comes from the message catalogue, so the wording
//! of an error is defined once for the whole API.

use std::sync::Arc;

use uuid::Uuid;

use crate::config::messages::Messages;
use crate::config::settings::PaginationSettings;
use crate::error::{AppError, AppResult};
use crate::helpers::{bounded_text, required_text};
use crate::pagination::{Page, PaginationQuery};
use crate::tasks::dto::{
    CreateTaskRequest, MAX_DESCRIPTION_LENGTH, MAX_TITLE_LENGTH, TaskQuery, TaskResponse,
    UpdateTaskRequest,
};
use crate::tasks::models::{TaskPriority, TaskStatus};
use crate::tasks::repository::{NewTask, TaskFilters, TaskPatch, TaskRepository};

/// Task use cases.
#[derive(Debug, Clone)]
pub struct TaskService {
    repository: TaskRepository,
    messages: Arc<Messages>,
    pagination: PaginationSettings,
}

impl TaskService {
    /// Builds the service.
    pub fn new(
        repository: TaskRepository,
        messages: Arc<Messages>,
        pagination: PaginationSettings,
    ) -> Self {
        Self {
            repository,
            messages,
            pagination,
        }
    }

    /// Creates a task inside a project owned by `owner`.
    ///
    /// A project that does not exist, or that belongs to another account, is
    /// a `404`: the insert and the ownership check are one statement, so a
    /// task can never be attached to a project the caller does not own.
    pub async fn create(
        &self,
        owner: Uuid,
        project_id: Uuid,
        request: CreateTaskRequest,
    ) -> AppResult<TaskResponse> {
        let title = validate_title(&request.title, &self.messages)?;
        let description = validate_description(&request.description, &self.messages)?;

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
            .ok_or_else(|| AppError::NotFound(self.messages.resources.project_not_found.clone()))?;

        Ok(TaskResponse::from(task))
    }

    /// Lists the tasks of a project owned by `owner`.
    ///
    /// A project that exists but has no matching task is an empty page, not a
    /// `404`. The repository tells the two cases apart.
    pub async fn list_for_project(
        &self,
        owner: Uuid,
        project_id: Uuid,
        query: TaskQuery,
        pagination: PaginationQuery,
    ) -> AppResult<Page<TaskResponse>> {
        let limit = pagination.effective_limit(&self.pagination);
        let offset = pagination.effective_offset();

        let filters = TaskFilters {
            status: query.status,
            priority: query.priority,
        };

        let (tasks, total) = self
            .repository
            .list_for_project(owner, project_id, &filters, limit, offset)
            .await?
            .ok_or_else(|| AppError::NotFound(self.messages.resources.project_not_found.clone()))?;

        // The page is consumed: the strings of every task are moved into the
        // response rather than copied.
        let items = tasks.into_iter().map(TaskResponse::from).collect();

        Ok(Page::new(items, limit, offset, total))
    }

    /// Loads one task owned by `owner`.
    pub async fn get(&self, owner: Uuid, id: Uuid) -> AppResult<TaskResponse> {
        let task = self
            .repository
            .find_owned(id, owner)
            .await?
            .ok_or_else(|| AppError::NotFound(self.messages.resources.task_not_found.clone()))?;

        Ok(TaskResponse::from(task))
    }

    /// Applies a partial update to one task owned by `owner`.
    ///
    /// An omitted field is left untouched. An explicit empty description is
    /// kept: it clears the description.
    pub async fn update(
        &self,
        owner: Uuid,
        id: Uuid,
        request: UpdateTaskRequest,
    ) -> AppResult<TaskResponse> {
        let title = request
            .title
            .as_deref()
            .map(|title| validate_title(title, &self.messages))
            .transpose()?;
        let description = request
            .description
            .as_deref()
            .map(|description| validate_description(description, &self.messages))
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
            .ok_or_else(|| AppError::NotFound(self.messages.resources.task_not_found.clone()))?;

        Ok(TaskResponse::from(task))
    }

    /// Deletes one task owned by `owner`.
    pub async fn delete(&self, owner: Uuid, id: Uuid) -> AppResult<()> {
        let deleted = self.repository.delete_owned(id, owner).await?;

        if !deleted {
            return Err(AppError::NotFound(
                self.messages.resources.task_not_found.clone(),
            ));
        }

        Ok(())
    }
}

/// Validates a task title.
///
/// The title is trimmed, and a title that is empty after trimming is
/// rejected. The stored value is therefore the trimmed one, which is what the
/// database constraint `tasks_title_length_chk` measures.
pub fn validate_title(title: &str, messages: &Messages) -> AppResult<String> {
    required_text(
        "title",
        title,
        MAX_TITLE_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

/// Validates a task description.
///
/// The description is kept verbatim: a description of spaces is stored as it
/// was sent. The stored value is what the database constraint
/// `tasks_description_length_chk` measures.
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
    fn titles_are_trimmed_and_required() {
        let messages = test_messages();

        assert_eq!(validate_title("  Ship it  ", &messages).unwrap(), "Ship it");
        assert!(validate_title("  ", &messages).is_err());
    }

    #[test]
    fn descriptions_are_kept_verbatim() {
        let messages = test_messages();

        assert_eq!(
            validate_description("  spaced  ", &messages).unwrap(),
            "  spaced  "
        );
    }

    #[test]
    fn oversized_titles_and_descriptions_are_rejected() {
        let messages = test_messages();

        let long_title = "x".repeat(MAX_TITLE_LENGTH + 1);
        assert!(validate_title(&long_title, &messages).is_err());

        let long_description = "x".repeat(MAX_DESCRIPTION_LENGTH + 1);
        assert!(validate_description(&long_description, &messages).is_err());
    }

    #[test]
    fn a_validation_failure_carries_the_catalogue_wording() {
        let messages = test_messages();

        let error = validate_title("  ", &messages).unwrap_err();
        let body = error.body();

        assert_eq!(body.error.code, code::VALIDATION_FAILED);
        assert_eq!(body.error.message, messages.general.validation_failed);
        assert_eq!(body.error.details[0].field, "title");
        assert_eq!(body.error.details[0].message, messages.validation.not_empty);
    }

    #[test]
    fn a_too_long_title_names_the_limit() {
        let messages = test_messages();

        let long_title = "x".repeat(MAX_TITLE_LENGTH + 1);
        let error = validate_title(&long_title, &messages).unwrap_err();
        let body = error.body();

        assert_eq!(body.error.details[0].field, "title");
        assert_eq!(
            body.error.details[0].message,
            "must be at most 200 characters long"
        );
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
