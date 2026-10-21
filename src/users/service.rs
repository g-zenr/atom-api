//! User use cases.
//!
//! The service owns the business rules that are not tied to a single request:
//! email normalization, password policy and the safe public representation.

use uuid::Uuid;

use crate::error::{AppError, AppResult, FieldViolation};
use crate::users::dto::UserResponse;
use crate::users::models::User;
use crate::users::repository::UserRepository;

/// Minimum accepted password length.
pub const MIN_PASSWORD_LENGTH: usize = 12;
/// Maximum accepted password length. Bounded so that Argon2 cost cannot be
/// used as a denial-of-service vector.
pub const MAX_PASSWORD_LENGTH: usize = 256;
/// Minimum accepted display-name length.
pub const MIN_DISPLAY_NAME_LENGTH: usize = 1;
/// Maximum accepted display-name length.
pub const MAX_DISPLAY_NAME_LENGTH: usize = 120;

/// Normalizes an email address for storage and comparison.
///
/// The address is trimmed and lower-cased. The database has a matching check
/// constraint, so the two can never drift.
pub fn normalize_email(raw: &str) -> String {
    raw.trim().to_lowercase()
}

/// Validates a password against the Phase 1 policy.
///
/// The policy is length-based with a small set of composition rules. It is
/// documented in `docs/features/authentication/specification.md`.
pub fn validate_password(password: &str) -> AppResult<()> {
    let mut violations = Vec::new();

    if password.chars().count() < MIN_PASSWORD_LENGTH {
        violations.push(FieldViolation::new(
            "password",
            format!("must be at least {MIN_PASSWORD_LENGTH} characters long"),
        ));
    }

    if password.chars().count() > MAX_PASSWORD_LENGTH {
        violations.push(FieldViolation::new(
            "password",
            format!("must be at most {MAX_PASSWORD_LENGTH} characters long"),
        ));
    }

    if !password.chars().any(char::is_uppercase) {
        violations.push(FieldViolation::new(
            "password",
            "must contain at least one uppercase letter",
        ));
    }

    if !password.chars().any(char::is_lowercase) {
        violations.push(FieldViolation::new(
            "password",
            "must contain at least one lowercase letter",
        ));
    }

    if !password.chars().any(char::is_numeric) {
        violations.push(FieldViolation::new(
            "password",
            "must contain at least one digit",
        ));
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(AppError::validation(violations))
    }
}

/// Validates a display name.
pub fn validate_display_name(display_name: &str) -> AppResult<String> {
    let trimmed = display_name.trim();
    let length = trimmed.chars().count();

    if !(MIN_DISPLAY_NAME_LENGTH..=MAX_DISPLAY_NAME_LENGTH).contains(&length) {
        return Err(AppError::validation(vec![FieldViolation::new(
            "display_name",
            format!(
                "must be between {MIN_DISPLAY_NAME_LENGTH} and {MAX_DISPLAY_NAME_LENGTH} characters long"
            ),
        )]));
    }

    Ok(trimmed.to_string())
}

/// Validates an email address.
pub fn validate_email(email: &str) -> AppResult<String> {
    let normalized = normalize_email(email);

    if normalized.is_empty() || !normalized.contains('@') {
        return Err(AppError::validation(vec![FieldViolation::new(
            "email",
            "must be a valid email address",
        )]));
    }

    // A deliberately permissive check: the only authoritative test of an
    // address is whether mail can be delivered to it.
    let Some((local, domain)) = normalized.split_once('@') else {
        return Err(AppError::validation(vec![FieldViolation::new(
            "email",
            "must be a valid email address",
        )]));
    };

    if local.is_empty() || domain.is_empty() || !domain.contains('.') || domain.starts_with('.') {
        return Err(AppError::validation(vec![FieldViolation::new(
            "email",
            "must be a valid email address",
        )]));
    }

    if normalized.chars().count() > 254 {
        return Err(AppError::validation(vec![FieldViolation::new(
            "email",
            "must be at most 254 characters long",
        )]));
    }

    Ok(normalized)
}

/// User use cases.
#[derive(Debug, Clone)]
pub struct UserService {
    repository: UserRepository,
}

impl UserService {
    /// Builds the service.
    pub fn new(repository: UserRepository) -> Self {
        Self { repository }
    }

    /// Creates a user from already validated inputs.
    pub async fn create_user(
        &self,
        email: String,
        password_hash: String,
        display_name: String,
    ) -> AppResult<User> {
        self.repository
            .insert(Uuid::new_v4(), &email, &password_hash, &display_name)
            .await
    }

    /// Loads a user by id.
    pub async fn get_user(&self, id: Uuid) -> AppResult<User> {
        self.repository
            .find_by_id(id)
            .await?
            .ok_or(AppError::NotFound("The requested user does not exist."))
    }

    /// Returns the safe public representation of a user.
    pub async fn get_user_response(&self, id: Uuid) -> AppResult<UserResponse> {
        Ok(UserResponse::from(&self.get_user(id).await?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails_are_normalized() {
        assert_eq!(normalize_email("  Ada@Example.COM "), "ada@example.com");
    }

    #[test]
    fn valid_emails_are_accepted() {
        assert_eq!(
            validate_email("Ada@Example.com").unwrap(),
            "ada@example.com"
        );
    }

    #[test]
    fn invalid_emails_are_rejected() {
        for candidate in ["", "no-at-sign", "@example.com", "ada@", "ada@nodot"] {
            assert!(
                validate_email(candidate).is_err(),
                "{candidate} should be rejected"
            );
        }
    }

    #[test]
    fn weak_passwords_are_rejected_with_field_details() {
        let error = validate_password("short").unwrap_err();
        let body = error.body();
        assert!(body.error.details.iter().any(|d| d.field == "password"));
    }

    #[test]
    fn strong_passwords_are_accepted() {
        assert!(validate_password("Correct-Horse-42").is_ok());
    }

    #[test]
    fn display_names_are_trimmed_and_length_checked() {
        assert_eq!(validate_display_name("  Ada  ").unwrap(), "Ada");
        assert!(validate_display_name("   ").is_err());
        assert!(validate_display_name(&"x".repeat(121)).is_err());
    }
}
