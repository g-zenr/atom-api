//! The shared text validators.
//!
//! Three feature modules each validate a title, a name or a body with the same
//! two rules. The rules live here so that the order of the checks, the
//! trimming behaviour and the message text are defined once.
//!
//! The trimming behaviour is **not** uniform across the API and is
//! deliberate:
//!
//! * a title, a name and a display name are trimmed, and a value that is
//!   empty after trimming is rejected;
//! * a body and a description are kept verbatim, so that a body of spaces is
//!   stored as it was sent.
//!
//! Merging the two would change observable behaviour and would break the
//! correspondence with the database `CHECK` constraints, which measure the
//! stored value.

use crate::config::messages::{Messages, render};
use crate::error::{AppError, AppResult, FieldViolation};
use crate::utils::text::{char_len, is_blank};

/// A trimmed value that must not be empty and must not be longer than `max`.
///
/// `too_long` is the template for the "too long" message, for example
/// `&messages.validation.too_long`.
///
/// # Errors
///
/// Returns [`AppError`] with a field violation when the value is empty after
/// trimming or when it is longer than `max`.
pub fn required_text(
    field: &str,
    value: &str,
    max: usize,
    too_long: &str,
    messages: &Messages,
) -> AppResult<String> {
    if is_blank(value) {
        return Err(field_error(
            field,
            messages.validation.not_empty.clone(),
            messages,
        ));
    }

    let trimmed = value.trim();

    if char_len(trimmed) > max {
        return Err(field_error(
            field,
            render(too_long, &[max.to_string()]),
            messages,
        ));
    }

    Ok(trimmed.to_string())
}

/// A value that is kept verbatim and must not be longer than `max`.
///
/// # Errors
///
/// Returns [`AppError`] with a field violation when the value is longer than
/// `max`.
pub fn bounded_text(
    field: &str,
    value: &str,
    max: usize,
    too_long: &str,
    messages: &Messages,
) -> AppResult<String> {
    if char_len(value) > max {
        return Err(field_error(
            field,
            render(too_long, &[max.to_string()]),
            messages,
        ));
    }

    Ok(value.to_string())
}

/// Builds the validation error for one field.
///
/// Every feature module reports a rejected field the same way: one field
/// violation in the details and the shared `validation_failed` message. The
/// helper is public so that a module which validates something the shared
/// validators do not cover reports it in the same shape.
pub fn field_error(field: &str, message: String, messages: &Messages) -> AppError {
    AppError::validation(vec![FieldViolation::new(field, message)])
        .with_message(messages.general.validation_failed.clone())
}

/// The longest accepted search term, in characters.
///
/// A search term is not stored, so it has no database constraint to mirror.
/// The bound is what keeps one request from making PostgreSQL tokenise an
/// arbitrarily long string. Every other request input has a bound, so this
/// one has one too.
pub const MAX_SEARCH_LENGTH: usize = 200;

/// Validates a search term.
///
/// The term is trimmed, and a term that is empty after trimming means "no
/// filter" rather than an error: the repository already treats it that way.
///
/// # Errors
///
/// Returns [`AppError`] with a field violation when the term is longer than
/// [`MAX_SEARCH_LENGTH`].
pub fn validate_search(term: &str, messages: &Messages) -> AppResult<String> {
    let trimmed = term.trim();

    if char_len(trimmed) > MAX_SEARCH_LENGTH {
        return Err(field_error(
            "search",
            render(
                &messages.validation.too_long,
                &[MAX_SEARCH_LENGTH.to_string()],
            ),
            messages,
        ));
    }

    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::messages::test_messages;

    #[test]
    fn required_text_trims_and_rejects_an_empty_value() {
        let messages = test_messages();
        let too_long = messages.validation.too_long.clone();

        assert_eq!(
            required_text("title", "  hello  ", 200, &too_long, &messages).unwrap(),
            "hello"
        );

        let error = required_text("title", "   ", 200, &too_long, &messages).unwrap_err();
        assert_eq!(error.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn required_text_rejects_a_value_over_the_maximum() {
        let messages = test_messages();
        let too_long = messages.validation.too_long.clone();

        let error =
            required_text("title", &"x".repeat(201), 200, &too_long, &messages).unwrap_err();
        let body = error.body();
        assert_eq!(
            body.error.details[0].message,
            "must be at most 200 characters long"
        );
    }

    #[test]
    fn bounded_text_keeps_the_value_verbatim() {
        let messages = test_messages();
        let too_long = messages.validation.too_long.clone();

        assert_eq!(
            bounded_text("description", "  spaced  ", 100, &too_long, &messages).unwrap(),
            "  spaced  "
        );
        assert!(bounded_text("description", &"x".repeat(101), 100, &too_long, &messages).is_err());
    }

    #[test]
    fn a_search_term_is_trimmed_and_bounded() {
        let messages = test_messages();

        assert_eq!(validate_search("  hello  ", &messages).unwrap(), "hello");
        assert_eq!(validate_search("   ", &messages).unwrap(), "");

        // A term at the bound is accepted.
        assert!(
            validate_search(&"x".repeat(MAX_SEARCH_LENGTH), &messages).is_ok(),
            "a term at the bound must be accepted"
        );

        // One character more is refused, with the field named.
        let error = validate_search(&"x".repeat(MAX_SEARCH_LENGTH + 1), &messages).unwrap_err();
        let body = error.body();
        assert_eq!(body.error.details[0].field, "search");
        assert_eq!(
            body.error.details[0].message,
            format!("must be at most {MAX_SEARCH_LENGTH} characters long")
        );
    }
}
