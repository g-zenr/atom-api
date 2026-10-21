//! The `projects` module: GitHub-style project tracking with single-owner
//! isolation.

pub mod dto;
pub mod handlers;
pub mod models;
pub mod repository;
pub mod service;

pub use dto::{CreateProjectRequest, ProjectQuery, ProjectResponse, UpdateProjectRequest};
pub use models::Project;
pub use repository::ProjectRepository;
pub use service::ProjectService;
