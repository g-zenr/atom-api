//! The `auth` module: registration, login, token lifecycle and session
//! revocation.

pub mod dto;
pub mod handlers;
pub mod models;
pub mod password;
pub mod repository;
pub mod service;
pub mod token;

pub use dto::{AuthResponse, LoginRequest, LogoutRequest, RefreshRequest, TokenPair};
pub use models::RefreshSession;
pub use password::PasswordHasher;
pub use repository::RefreshSessionRepository;
pub use service::{AuthService, ClientMeta};
pub use token::{TokenService, generate_refresh_token, hash_refresh_token};
