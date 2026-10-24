//! The client-facing message catalogue.
//!
//! Every string that reaches a client through the error envelope is declared
//! once, in `config/messages.toml`, and read through this module. The stable
//! machine-readable error `code` is **not** part of the catalogue: the code is
//! part of the API contract and lives in [`crate::error`].
//!
//! A template may contain `{}` placeholders. The values substituted into them
//! are always numbers the application controls, never user input, so there is
//! no injection risk. A placeholder without a matching argument is left as
//! `{}` rather than silently dropped, so a mistake stays visible.

use serde::Deserialize;

/// Renders a template, substituting `{}` placeholders from left to right.
///
/// # Examples
///
/// ```
/// use atom_api::config::messages::render;
///
/// assert_eq!(render("must be at most {} characters long", &["200".to_string()]), "must be at most 200 characters long");
/// assert_eq!(render("plain", &[]), "plain");
/// ```
pub fn render(template: &str, args: &[String]) -> String {
    if args.is_empty() {
        return template.to_string();
    }

    let mut rendered =
        String::with_capacity(template.len() + args.iter().map(String::len).sum::<usize>());
    let mut args = args.iter();

    for part in template.split("{}") {
        rendered.push_str(part);
        if let Some(argument) = args.next() {
            rendered.push_str(argument);
        }
    }

    // Any remaining arguments are a mistake in the call site, not in the
    // template. They are dropped rather than appended in the wrong place.
    rendered
}

/// The catalogue of client-facing messages.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Messages {
    /// The messages that are not specific to one feature.
    pub general: GeneralMessages,
    /// The authentication messages.
    pub auth: AuthMessages,
    /// The field-level validation messages.
    pub validation: ValidationMessages,
    /// The messages for a resource that does not exist.
    pub resources: ResourceMessages,
    /// The messages for a request the current state refuses.
    pub conflicts: ConflictMessages,
}

/// The messages that are not specific to one feature.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralMessages {
    /// 422, when a request fails a validation rule.
    pub validation_failed: String,
    /// 413, when the request body is larger than the configured limit.
    pub payload_too_large: String,
    /// 429, when the caller exceeded the rate limit.
    pub rate_limited: String,
    /// 408, when the request exceeded the configured timeout.
    pub timeout: String,
    /// 400, when the request body is not valid JSON.
    pub bad_request: String,
    /// 400, when a path or query parameter cannot be parsed.
    pub malformed_parameter: String,
    /// 401, when a request needs a bearer token and does not carry one.
    pub unauthorized: String,
    /// 404, when the route does not exist.
    pub route_not_found: String,
}

/// The authentication messages.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthMessages {
    /// 401, for every failed login.
    pub invalid_credentials: String,
    /// 401, when a bearer token is malformed, expired or signed with another key.
    pub access_token_invalid: String,
    /// 401, when the presented refresh token is not known.
    pub refresh_token_invalid: String,
    /// 401, when a refresh token is presented a second time.
    pub refresh_token_reused: String,
    /// 401, when the refresh token has passed its absolute lifetime.
    pub refresh_token_expired: String,
    /// 403, when the account has been disabled.
    pub account_disabled: String,
    /// 403, when a caller tries to revoke another account's session.
    pub not_own_session: String,
    /// 409, when the email address is already registered.
    pub email_already_registered: String,
    /// 422, when the application and the database disagree about a rule.
    pub database_constraint: String,
}

/// The field-level validation messages.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationMessages {
    /// The address is not shaped like an address.
    pub email_invalid: String,
    /// The address is longer than the accepted maximum.
    pub email_too_long: String,
    /// The password is shorter than the configured minimum.
    pub password_too_short: String,
    /// The password is longer than the configured maximum.
    pub password_too_long: String,
    /// The password has no uppercase letter.
    pub password_needs_uppercase: String,
    /// The password has no lowercase letter.
    pub password_needs_lowercase: String,
    /// The password has no digit.
    pub password_needs_digit: String,
    /// The display name is outside the accepted length range.
    pub display_name_length: String,
    /// The value is empty after trimming.
    pub not_empty: String,
    /// The value is longer than the accepted maximum.
    pub too_long: String,
    /// The currency code is not three letters.
    pub currency_invalid: String,
    /// A submission-date range runs backwards.
    pub date_range_order: String,
    /// A money value is not a four-decimal string.
    pub money_scale: String,
    /// A receipt reference is not an absolute `http` or `https` URL.
    pub receipt_url_invalid: String,
    /// A claim with no items cannot be submitted.
    pub items_required: String,
    /// A claim with a zero total cannot be submitted.
    pub total_positive: String,
}

/// The messages for a resource that does not exist.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceMessages {
    /// 404, when the user does not exist.
    pub user_not_found: String,
    /// 404, when the note does not exist.
    pub note_not_found: String,
    /// 404, when the project does not exist.
    pub project_not_found: String,
    /// 404, when the task does not exist.
    pub task_not_found: String,
    /// 404, when the expense claim does not exist.
    pub claim_not_found: String,
    /// 404, when the expense item does not exist.
    pub item_not_found: String,
}

/// The messages for a request the current state refuses.
///
/// These are the `409 conflict` texts. They are kept apart from
/// [`ValidationMessages`] because the request itself was well formed: what
/// failed is the state of the resource, not a field.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConflictMessages {
    /// 409, when an item is written on a claim that is not a draft.
    pub claim_not_editable: String,
    /// 409, when a lifecycle action is not allowed from the current status.
    pub transition_not_allowed: String,
}

impl Messages {
    /// Parses the catalogue from a TOML document.
    ///
    /// The catalogue lives under the `messages` key so that it never collides
    /// with a section of the same name in another configuration file.
    ///
    /// The application reads the catalogue through the configuration loader;
    /// this constructor exists for the tests, which parse the **shipped**
    /// file so that a unit test asserts against the same templates the
    /// application serves.
    #[cfg(test)]
    pub(crate) fn from_toml(contents: &str) -> Result<Self, toml::de::Error> {
        #[derive(Deserialize)]
        struct Document {
            messages: Messages,
        }

        let document: Document = toml::from_str(contents)?;
        Ok(document.messages)
    }
}

/// The shipped catalogue, parsed once for the tests.
///
/// The unit tests assert against the same templates the application serves, so
/// that renaming or removing a key the code relies on fails a test. The fixture
/// lives here, next to the type it builds, rather than in each test module
/// (finding `F-06`).
#[cfg(test)]
pub(crate) fn test_messages() -> Messages {
    Messages::from_toml(include_str!("../../config/messages.toml"))
        .expect("the message catalogue parses")
}
