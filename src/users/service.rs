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

/// The maximum length of the local part of an email address, in characters.
///
/// RFC 5321 §4.5.3.1.1 caps the local part at 64 octets. Like every length
/// rule in this API, the check counts characters rather than bytes.
pub const MAX_EMAIL_LOCAL_LENGTH: usize = 64;

/// The maximum length of the domain part of an email address, in characters.
///
/// The textual form of a fully qualified domain name is at most 253
/// characters (RFC 1035 §2.3.4, carried into RFC 5321 §4.5.3.1.2).
pub const MAX_EMAIL_DOMAIN_LENGTH: usize = 253;

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

/// Decides whether an already normalized address is well formed.
///
/// The grammar is deliberately small — it is not an RFC 5322 parser, because
/// the only authoritative test of an address is whether mail can be
/// delivered to it. It is, however, stricter than it once was: since the
/// owner answered Q-18 (2026-10-31), it refuses the shapes no mailbox can
/// have. The rules:
///
/// * exactly one `@`, with a non-empty local part and a non-empty domain;
/// * the local part has no empty dot-separated segment, so it has no
///   leading, trailing or consecutive dots;
/// * the domain carries at least one dot, and every label is non-empty and
///   neither starts nor ends with a hyphen;
/// * the local part is at most [`MAX_EMAIL_LOCAL_LENGTH`] and the domain at
///   most [`MAX_EMAIL_DOMAIN_LENGTH`] characters.
///
/// Everything else stays permissive on purpose: `+` tags, subdomains,
/// punycode labels and one-character TLDs are accepted, because refusing a
/// real mailbox is worse than accepting an unusual shape.
fn is_well_formed_email(address: &str) -> bool {
    let Some((local, domain)) = address.split_once('@') else {
        return false;
    };

    // A second `@` cannot belong to an address: `a@b@c.com` is refused.
    if domain.contains('@') {
        return false;
    }

    if local.is_empty() || domain.is_empty() {
        return false;
    }

    if char_len(local) > MAX_EMAIL_LOCAL_LENGTH || char_len(domain) > MAX_EMAIL_DOMAIN_LENGTH {
        return false;
    }

    // An empty segment is a leading, a trailing or two consecutive dots.
    if local.split('.').any(|segment| segment.is_empty()) {
        return false;
    }

    // The domain needs at least one dot, and every label must be a non-empty
    // piece that neither starts nor ends with a hyphen.
    domain.contains('.')
        && domain
            .split('.')
            .all(|label| !label.is_empty() && !label.starts_with('-') && !label.ends_with('-'))
}

/// Validates an email address and returns its normalized form.
///
/// The address is trimmed and lower-cased, its total length is checked
/// against [`MAX_EMAIL_LENGTH`], and its shape is checked against
/// [`is_well_formed_email`]. The shape check is not a full RFC 5322 parser:
/// the only authoritative test of an address is whether mail can be
/// delivered to it.
///
/// # Errors
///
/// Returns [`AppError`] with a field violation when the address is longer
/// than [`MAX_EMAIL_LENGTH`] or is not shaped like an address. The total
/// length is checked first, so an address that breaks both rules is reported
/// as too long: the limit is the contract the database's
/// `users_email_length_chk` constraint mirrors, and the shape rules would
/// otherwise pre-empt it with a message about a part the caller cannot fix
/// without shortening the address anyway.
pub fn validate_email(email: &str, messages: &Messages) -> AppResult<String> {
    let normalized = normalize_email(email);

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

    if !is_well_formed_email(&normalized) {
        return Err(field_error(
            "email",
            messages.validation.email_invalid.clone(),
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
        let messages = test_messages();

        assert_eq!(
            validate_email("Ada@Example.com", &messages).unwrap(),
            "ada@example.com"
        );

        // Shapes the tightened grammar must keep accepting: a `+` tag, a dot
        // in the local part, a subdomain, a punycode label and a
        // one-character TLD.
        for accepted in [
            "ada+q3@example.com",
            "ada.lovelace@example.com",
            "ada@mail.example.com",
            "ada@xn--exmple-cua.com",
            "ada@example.x",
        ] {
            assert!(
                validate_email(accepted, &messages).is_ok(),
                "{accepted} is a legitimate address and must be accepted"
            );
        }
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

    /// Each address below breaks one named rule of the tightened grammar
    /// (Q-18, answered 2026-10-31). The rule is named in the assertion
    /// message, so a failure points at the rule that stopped holding.
    #[test]
    fn every_tightened_email_rule_is_enforced() {
        let messages = test_messages();

        for (candidate, rule) in [
            ("a@b@c.com", "the address carries exactly one @"),
            ("a..b@example.com", "the local part has no consecutive dots"),
            (".a@example.com", "the local part does not start with a dot"),
            ("a.@example.com", "the local part does not end with a dot"),
            ("a@ex..ample.com", "the domain has no empty label"),
            ("a@.example.com", "the domain has no empty label"),
            ("a@example.com.", "the domain has no empty label"),
            ("a@-example.com", "no domain label starts with a hyphen"),
            ("a@example-.com", "no domain label ends with a hyphen"),
        ] {
            let error = validate_email(candidate, &messages)
                .expect_err("a malformed address must be rejected");
            let body = error.body();

            assert_eq!(body.error.details[0].field, "email", "{candidate}");
            assert_eq!(
                body.error.details[0].message, messages.validation.email_invalid,
                "{candidate} must be refused by the rule: {rule}"
            );
        }
    }

    /// The part-length rules of the tightened grammar: the local part is at
    /// most 64 and the domain at most 253 characters. The total limit stays
    /// the binding contract with the database, so the longest well-formed
    /// address is exactly [`MAX_EMAIL_LENGTH`] characters.
    #[test]
    fn email_part_lengths_are_bounded() {
        let messages = test_messages();

        // A local part of exactly 64 characters is the RFC 5321 limit.
        let local_at_limit = format!("{}@example.com", "a".repeat(MAX_EMAIL_LOCAL_LENGTH));
        assert!(
            validate_email(&local_at_limit, &messages).is_ok(),
            "a 64-character local part must be accepted"
        );

        let local_over_limit = format!("{}@example.com", "a".repeat(MAX_EMAIL_LOCAL_LENGTH + 1));
        let error = validate_email(&local_over_limit, &messages)
            .expect_err("a 65-character local part must be rejected");
        assert_eq!(
            error.body().error.details[0].message,
            messages.validation.email_invalid,
            "an over-long local part is a shape failure, not a total-length failure"
        );

        // A domain of exactly 253 characters is the textual DNS limit. With
        // any non-empty local part it would pass the 254-character total, so
        // the shape predicate is asserted directly.
        let domain_at_limit = format!("{}.com", "a".repeat(MAX_EMAIL_DOMAIN_LENGTH - 4));
        assert_eq!(char_len(&domain_at_limit), MAX_EMAIL_DOMAIN_LENGTH);
        assert!(
            is_well_formed_email(&format!("a@{domain_at_limit}")),
            "a 253-character domain is well formed"
        );
        assert!(
            !is_well_formed_email(&format!("a@{domain_at_limit}x")),
            "a 254-character domain must be rejected by the domain-length rule"
        );

        // The total limit binds first: 1 + 1 + 252 = 254 is the longest
        // address that both the shape rules and the database CHECK accept.
        let longest = format!("a@{}.com", "a".repeat(MAX_EMAIL_DOMAIN_LENGTH - 5));
        assert_eq!(char_len(&longest), MAX_EMAIL_LENGTH);
        assert!(validate_email(&longest, &messages).is_ok());
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
