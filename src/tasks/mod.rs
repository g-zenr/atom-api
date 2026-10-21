//! The `tasks` module: GitHub-style task tracking with status, priority,
//! ordering and filtering.

pub mod dto;
pub mod handlers;
pub mod models;
pub mod repository;
pub mod service;

pub use dto::{CreateTaskRequest, TaskQuery, TaskResponse, UpdateTaskRequest};
pub use models::{Task, TaskPriority, TaskStatus};
pub use repository::{NewTask, TaskFilters, TaskPatch, TaskRepository};
pub use service::TaskService;
