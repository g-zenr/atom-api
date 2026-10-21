//! The `notes` module: personal notes with ownership isolation, search,
//! archiving and pagination.

pub mod dto;
pub mod handlers;
pub mod models;
pub mod repository;
pub mod service;

pub use dto::{CreateNoteRequest, NoteQuery, NoteResponse, UpdateNoteRequest};
pub use models::Note;
pub use repository::{NoteFilters, NoteRepository};
pub use service::NoteService;
