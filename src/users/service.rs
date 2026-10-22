//! User use cases.
//!
//! The service owns the business rules that are not tied to a single request:
//! email normalization, the password policy and the safe public representation.
//!
//! Every client-facing string comes from the message catalogue, so the wording
//! of an error is defined once for the whole API.
//!
//! The rules are free functions, so that they can be read and tested without
//! a database. The service exposes them as methods, bound to its own
//! catalogue and password policy.

use std::sync::Arc;

use uuid::Uuid;

use crate::config::PasswordPolicy;
use crate::config::messages::{Messages, render};
use crate::error::{AppError, AppResult, FieldViolation};
use crate::helpers::field_error;
use crate::users::dto::UserResponse;
use crate::users::models::User;
use crate::users::repository::UserRepository;
use crate::utils::text::{char_len, is_blank};

/// The maximum length of an email address, in characters.
///
/// 254 is the path limit of RFC 5321. An address longer than that cannot be
/// delivered, whatever the local part would allow.
pub const MAX_EMAIL_LENGTH: usize = 254;

/// The minimum accepted length of a display name.
pub const MIN_DISPLAY_NAME_LENGTH: usize = 1;

/// The maximum accepted length of a display name.
pub const MAX_DISPLAY_NAME_LENGTH: usize = 120;

/// Normalizes an email address for storage and comparison.
///
/// The address is trimmed and lower-cased. The database has a matching check
/// constraint, so the two can never drift.
pub fn normalize_email(raw: &str) -> String {
    raw.trim().to_lowercase()
}

/// Validates an email address and returns its normalized form.
///
/// The check is deliberately permissive: the only authoritative test of an
/// address is whether mail can be delivered to it.
///
/// # Errors
///
/// Returns [`AppError`] with a field violation when the address is not shaped
/// like an address, or when it is longer than [`MAX_EMAIL_LENGTH`].
pub fn validate_email(email: &str, messages: &Messages) -> AppResult<String> {
    let normalized = normalize_email(email);

    let well_formed = match normalized.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && !domain.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
        }
        None => false,
    };

    if !well_formed {
        return Err(field_error(
            "email",
            messages.validation.email_invalid.clone(),
            messages,
        ));
    }

    if char_len(&normalized) > MAX_EMAIL_LENGTH {
        return Err(field_error(
            "email",
            render(
                &messages.validation.email_too_long,
                &[MAX_EMAIL_LENGTH.to_string()],
            ),
            messages,
        ));
    }

    Ok(normalized)
}

/// Validates a password against the given policy.
///
/// Every unmet rule is reported, so that a client can show all of them at
/// once instead of one per round trip.
///
/// # Errors
///
/// Returns [`AppError`] with one field violation per unmet rule.
pub fn validate_password(
    password: &str,
    policy: &PasswordPolicy,
    messages: &Messages,
) -> AppResult<()> {
    let mut violations = Vec::new();
    let length = char_len(password);

    if length < policy.min_length {
        violations.push(FieldViolation::new(
            "password",
            render(
                &messages.validation.password_too_short,
                &[policy.min_length.to_string()],
            ),
        ));
    }

    if length > policy.max_length {
        violations.push(FieldViolation::new(
            "password",
            render(
                &messages.validation.password_too_long,
                &[policy.max_length.to_string()],
            ),
        ));
    }

    if !password.chars().any(char::is_uppercase) {
        violations.push(FieldViolation::new(
            "password",
            messages.validation.password_needs_uppercase.clone(),
        ));
    }

    if !password.chars().any(char::is_lowercase) {
        violations.push(FieldViolation::new(
            "password",
            messages.validation.password_needs_lowercase.clone(),
        ));
    }

    if !password.chars().any(char::is_numeric) {
        violations.push(FieldViolation::new(
            "password",
            messages.validation.password_needs_digit.clone(),
        ));
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(AppError::validation(violations)
            .with_message(messages.general.validation_failed.clone()))
    }
}

/// Validates a display name and returns its trimmed form.
///
/// The display name keeps its own message: it names both limits, and it is
/// part of the documented `422` body.
///
/// # Errors
///
/// Returns [`AppError`] with a field violation when the name is empty or
/// longer than [`MAX_DISPLAY_NAME_LENGTH`].
pub fn validate_display_name(display_name: &str, messages: &Messages) -> AppResult<String> {
    let trimmed = display_name.trim();
    let length = char_len(trimmed);

    if is_blank(display_name) || length > MAX_DISPLAY_NAME_LENGTH {
        return Err(field_error(
            "display_name",
            render(
                &messages.validation.display_name_length,
                &[
                    MIN_DISPLAY_NAME_LENGTH.to_string(),
                    MAX_DISPLAY_NAME_LENGTH.to_string(),
                ],
            ),
            messages,
        ));
    }

    Ok(trimmed.to_string())
}

/// User use cases.
#[derive(Debug, Clone)]
pub struct UserService {
    repository: UserRepository,
    messages: Arc<Messages>,
    password_policy: PasswordPolicy,
}

impl UserService {
    /// Builds the service.
    pub fn new(
        repository: UserRepository,
        messages: Arc<Messages>,
        password_policy: PasswordPolicy,
    ) -> Self {
        Self {
            repository,
            messages,
            password_policy,
        }
    }

    /// Normalizes an email address for storage and comparison.
    ///
    /// The same rule as [`normalize_email`], exposed on the service so that a
    /// caller does not have to know where the rule lives.
    pub fn normalize_email(raw: &str) -> String {
        normalize_email(raw)
    }

    /// Validates an email address and returns its normalized form.
    pub fn validate_email(&self, email: &str) -> AppResult<String> {
        validate_email(email, self.messages.as_ref())
    }

    /// Validates a password against the configured policy.
    pub fn validate_password(&self, password: &str) -> AppResult<()> {
        validate_password(password, &self.password_policy, self.messages.as_ref())
    }

    /// Validates a display name and returns its trimmed form.
    pub fn validate_display_name(&self, display_name: &str) -> AppResult<String> {
        validate_display_name(display_name, self.messages.as_ref())
    }

    /// Creates a user from already validated inputs.
    pub async fn create_user(
        &self,
        email: String,
        password_hash: String,
        display_name: String,
    ) -> AppResult<User> {
        self.repository
            .insert(
                Uuid::new_v4(),
                &email,
                &password_hash,
                &display_name,
                self.messages.as_ref(),
            )
            .await
    }

    /// Loads a user by id.
    pub async fn get_user(&self, id: Uuid) -> AppResult<User> {
        self.repository
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(self.messages.resources.user_not_found.clone()))
    }

    /// Loads a user by email address.
    ///
    /// The address has to be normalised first, because that is how it is
    /// stored. A missing account is `None`, not an error: the caller decides
    /// what a missing account means.
    pub async fn find_user_by_email(&self, email: &str) -> AppResult<Option<User>> {
        self.repository.find_by_email(email).await
    }

    /// Returns the safe public representation of a user.
    pub async fn get_user_response(&self, id: Uuid) -> AppResult<UserResponse> {
        Ok(UserResponse::from(&self.get_user(id).await?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::messages::test_messages;

    #[test]
    fn emails_are_normalized() {
        assert_eq!(normalize_email("  Ada@Example.COM "), "ada@example.com");
        assert_eq!(
            UserService::normalize_email("  Ada@Example.COM "),
            "ada@example.com"
        );
    }

    #[test]
    fn valid_emails_are_accepted() {
        assert_eq!(
            validate_email("Ada@Example.com", &test_messages()).unwrap(),
            "ada@example.com"
        );
    }

    #[test]
    fn invalid_emails_are_rejected() {
        let messages = test_messages();

        for candidate in ["", "no-at-sign", "@example.com", "ada@", "ada@nodot"] {
            assert!(
                validate_email(candidate, &messages).is_err(),
                "{candidate} should be rejected"
            );
        }
    }

    #[test]
    fn an_over_long_email_is_rejected() {
        let messages = test_messages();
        let long = format!("{}@example.com", "a".repeat(MAX_EMAIL_LENGTH));

        let error = validate_email(&long, &messages).unwrap_err();
        let body = error.body();

        assert_eq!(body.error.details[0].field, "email");
        assert_eq!(
            body.error.details[0].message,
            format!("must be at most {MAX_EMAIL_LENGTH} characters long")
        );
    }

    #[test]
    fn weak_passwords_are_rejected_with_field_details() {
        let messages = test_messages();
        let error = validate_password("short", &PasswordPolicy::default(), &messages).unwrap_err();
        let body = error.body();

        assert!(
            body.error
                .details
                .iter()
                .any(|detail| detail.field == "password")
        );
    }

    #[test]
    fn every_unmet_password_rule_is_reported_at_once() {
        let messages = test_messages();
        let error = validate_password("short", &PasswordPolicy::default(), &messages).unwrap_err();
        let body = error.body();

        // "short" is too short, and it has no uppercase letter and no digit.
        assert_eq!(body.error.details.len(), 3);
    }

    #[test]
    fn strong_passwords_are_accepted() {
        let messages = test_messages();
        assert!(
            validate_password("Correct-Horse-42", &PasswordPolicy::default(), &messages).is_ok()
        );
    }

    #[test]
    fn display_names_are_trimmed_and_length_checked() {
        let messages = test_messages();

        assert_eq!(validate_display_name("  Ada  ", &messages).unwrap(), "Ada");
        assert!(validate_display_name("   ", &messages).is_err());
        assert!(validate_display_name(&"x".repeat(121), &messages).is_err());
    }

    #[test]
    fn the_display_name_keeps_its_own_message() {
        let messages = test_messages();
        let error = validate_display_name("   ", &messages).unwrap_err();
        let body = error.body();

        assert_eq!(body.error.details[0].field, "display_name");
        assert_eq!(
            body.error.details[0].message,
            format!(
                "must be between {MIN_DISPLAY_NAME_LENGTH} and {MAX_DISPLAY_NAME_LENGTH} characters long"
            )
        );
    }
}
