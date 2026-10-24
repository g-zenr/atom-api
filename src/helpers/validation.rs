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

/// The number of decimal places a money value carries.
///
/// Mirrors the `4` of `NUMERIC(19,4)` in `migrations/0004_expense_claims.sql`
/// and the wire form of `ADR-0007`: four decimals, always, in both directions.
pub const MONEY_SCALE: usize = 4;

/// The maximum number of integer digits a money value may carry.
///
/// Mirrors the `15` of `NUMERIC(19,4)`: nineteen digits in total, four of them
/// fractional. The largest accepted magnitude is therefore
/// `"999999999999999.9999"`, which is the bound the published contract states.
pub const MONEY_MAX_INTEGER_DIGITS: usize = 15;

/// Validates the textual form of a money value and returns it unchanged.
///
/// The accepted form is `^[0-9]{1,15}\.[0-9]{4}$`: one to fifteen ASCII
/// digits, one decimal point, exactly four digits. No sign, no leading or
/// trailing space, no grouping separator, no exponent, no currency symbol and
/// no empty string.
///
/// Nothing parses, rounds, compares or does arithmetic on the value. The
/// database sums money, in SQL (`ADR-0007`). This check exists because
/// `NUMERIC(19,4)` **rounds** a value it is given rather than refusing it —
/// `"12.50001"` would be stored as `"12.5000"` — so the four-decimal rule on
/// **input** is the service's job. Accepting `"12.5"` would also break the
/// round trip the decision promises: the client would send one string and read
/// back another.
///
/// # Errors
///
/// Returns [`AppError`] with a field violation when the value is not in the
/// accepted form.
pub fn validate_money(field: &str, value: &str, messages: &Messages) -> AppResult<String> {
    let valid = match value.split_once('.') {
        Some((integer, fraction)) => {
            !integer.is_empty()
                && integer.len() <= MONEY_MAX_INTEGER_DIGITS
                && integer.bytes().all(|byte| byte.is_ascii_digit())
                && fraction.len() == MONEY_SCALE
                && fraction.bytes().all(|byte| byte.is_ascii_digit())
        }
        None => false,
    };

    if !valid {
        return Err(field_error(
            field,
            messages.validation.money_scale.clone(),
            messages,
        ));
    }

    Ok(value.to_string())
}

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

    /// The four-decimal rule, enforced by the service because the column is
    /// permissive: `NUMERIC(19,4)` rounds what it is given instead of
    /// refusing it, so `"1.00000"` would silently become `"1.0000"`.
    #[test]
    fn money_requires_exactly_four_decimals() {
        let messages = test_messages();

        for accepted in ["0.0000", "1.0000", "125.8000", "999999999999999.9999"] {
            assert_eq!(
                validate_money("amount", accepted, &messages).unwrap(),
                accepted,
                "{accepted:?} is in the accepted form and must round-trip unchanged"
            );
        }

        // Three decimals, five decimals, a missing point, a sign, a grouping
        // separator, an exponent, a currency symbol and the empty string.
        for refused in [
            "1",
            "1.0",
            "1.000",
            "1.00000",
            "",
            "abc",
            "-1.0000",
            "+1.0000",
            "1,000.0000",
            "1e3",
            "€1.0000",
            " 1.0000",
            "1.0000 ",
            "1.0000.0000",
        ] {
            let error = validate_money("amount", refused, &messages)
                .expect_err("a value that is not four-decimal money must be refused");
            let body = error.body();
            assert_eq!(body.error.details[0].field, "amount");
            assert_eq!(
                body.error.details[0].message, messages.validation.money_scale,
                "{refused:?} must be refused with the money-scale message"
            );
        }
    }

    /// The column holds nineteen digits in total, four of them fractional, so
    /// the integer part may carry fifteen digits and no more.
    #[test]
    fn money_is_bounded_to_fifteen_integer_digits() {
        let messages = test_messages();

        assert!(validate_money("amount", "999999999999999.0000", &messages).is_ok());
        assert!(validate_money("amount", "1000000000000000.0000", &messages).is_err());
    }

    /// `NUMERIC(19,4)` refuses a negative amount through
    /// `expense_items_amount_chk`; the service refuses it without a round trip
    /// so that the client gets a `422` naming the field.
    #[test]
    fn a_negative_amount_is_refused() {
        let messages = test_messages();

        assert!(validate_money("amount", "-0.0100", &messages).is_err());
        assert!(validate_money("amount", "0.0000", &messages).is_ok());
    }
}
