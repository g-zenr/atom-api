//! The rules a valid configuration must satisfy.
//!
//! Validation happens once, at start-up, before the server binds a port. A
//! misconfiguration is therefore a clear operator message rather than a
//! runtime surprise: a request that fails because a limit was set to zero is
//! a bug, not a configuration.

use std::fmt;

use crate::config::{Config, Environment};

/// Minimum acceptable length of the HMAC signing secret.
///
/// This is a security floor, not a setting.
pub const MIN_SECRET_BYTES: usize = 32;

/// Minimum acceptable Argon2id memory cost, in KiB.
///
/// The cost may be raised; it may not be lowered below this floor.
pub const MIN_ARGON2_MEMORY_KIB: u32 = 8_192;

/// Minimum acceptable password length, in characters.
///
/// This is a security floor, not a setting. The accepted requirement in
/// `docs/features/authentication/specification.md` is a password of at least
/// twelve characters, so a configuration may not lower the bar.
pub const MIN_PASSWORD_LENGTH: usize = 12;

/// Maximum acceptable clock-skew allowance, in seconds.
///
/// The allowance exists for a drifting host. It is not a way to keep an
/// expired access token alive, so it is bounded.
pub const MAX_CLOCK_SKEW_SECONDS: u64 = 300;

/// Minimum acceptable refresh-session prune grace, in seconds.
///
/// The grace is how long an expired session row survives. It must be long
/// enough that a token which expired recently is still recognised as expired
/// rather than as one that never existed.
pub const MIN_PRUNE_GRACE_SECONDS: u64 = 3_600;

/// Maximum acceptable access-token lifetime, in seconds.
///
/// A day is far beyond any usable session and keeps the duration arithmetic
/// below.
pub const MAX_ACCESS_TOKEN_TTL_SECONDS: u64 = 86_400;

/// Maximum acceptable refresh-token lifetime, in seconds.
///
/// A year is far beyond any usable session and keeps the duration arithmetic
/// below.
pub const MAX_REFRESH_TOKEN_TTL_SECONDS: u64 = 31_536_000;

/// The hard ceiling on the body the JSON extractor reads, in bytes.
///
/// The **effective** ceiling is the configured `max_body_bytes`: the router
/// installs it as `DefaultBodyLimit` and the extractor reads it through
/// `req.into_limited_body()`. This constant is the independent backstop below
/// it.
///
/// It is a floor and not a setting because it is a safety ceiling of the
/// parser rather than a deployment policy. The validation refuses a
/// configured limit above it, so the configuration can never promise more
/// than the API delivers.
pub const MAX_BODY_BYTES_BACKSTOP: usize = 4 * 1024 * 1024;

/// The largest page a configuration may allow.
///
/// The published maximum is 100. A deployment may raise it — the bound is
/// part of the published client convention, so changing it is a breaking
/// change — but not without limit, because the limit is what stops one
/// request from pulling an arbitrary slice of a table out of the database.
pub const MAX_PAGINATION_LIMIT: i64 = 1_000;

/// Placeholder secrets that must never reach production.
pub const PLACEHOLDER_SECRETS: [&str; 2] = [
    "replace-me-with-a-development-only-secret-at-least-32-bytes-long",
    "change-me",
];

/// A configuration failure.
///
/// The message never contains a secret value: a rejected secret is reported by
/// its length, not by its content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// A required environment variable is missing or empty.
    Missing(String),
    /// A value is not acceptable.
    Invalid {
        /// The setting that is wrong.
        key: String,
        /// The offending value, with any secret redacted.
        value: String,
        /// What the value should have been.
        expected: String,
    },
    /// A configuration file could not be read.
    Io {
        /// The path that could not be read.
        path: String,
        /// What went wrong.
        message: String,
    },
    /// A configuration file could not be parsed.
    Parse {
        /// The source of the parse failure.
        source: String,
        /// What went wrong.
        message: String,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(key) => {
                write!(f, "required environment variable {key} is missing or empty")
            }
            Self::Invalid {
                key,
                value,
                expected,
            } => {
                write!(
                    f,
                    "environment variable {key} has invalid value {value}: expected {expected}"
                )
            }
            Self::Io { path, message } => {
                write!(f, "could not read the configuration file {path}: {message}")
            }
            Self::Parse { source, message } => {
                write!(
                    f,
                    "could not parse the configuration file {source}: {message}"
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// Validates a fully resolved configuration, including the two secrets.
pub fn validate(config: &Config) -> Result<(), ConfigError> {
    validate_settings(config)?;
    validate_secret(&config.auth.jwt_secret, config.environment)
}

/// Validates everything except the two required secrets.
///
/// The test harness supplies the secrets itself, so the base loader cannot
/// require them.
///
/// The rules are grouped by the settings they apply to, and the groups are
/// checked in a fixed order. A configuration that breaks more than one rule
/// is therefore always reported by the same one, which is what the tests
/// assert.
pub fn validate_settings(config: &Config) -> Result<(), ConfigError> {
    validate_request_limits(config)?;
    validate_sessions(config)?;
    validate_pagination(config)?;
    validate_rate_limit(config)?;
    validate_logging(config)?;
    validate_cors(&config.api.cors.allowed_origins)?;
    validate_database(config)?;
    validate_auth(config)?;

    Ok(())
}

/// Validates the request limits.
fn validate_request_limits(config: &Config) -> Result<(), ConfigError> {
    if config.app.limits.max_body_bytes == 0 {
        return Err(ConfigError::Invalid {
            key: "app.limits.max_body_bytes".to_string(),
            value: "0".to_string(),
            expected: "a positive number of bytes".to_string(),
        });
    }

    if config.app.limits.max_body_bytes > MAX_BODY_BYTES_BACKSTOP {
        return Err(ConfigError::Invalid {
            key: "app.limits.max_body_bytes".to_string(),
            value: config.app.limits.max_body_bytes.to_string(),
            expected: format!(
                "at most {MAX_BODY_BYTES_BACKSTOP} bytes, the ceiling of the body reader"
            ),
        });
    }

    if config.app.limits.request_timeout.is_zero() {
        return Err(ConfigError::Invalid {
            key: "app.limits.request_timeout".to_string(),
            value: "0s".to_string(),
            expected: "a positive duration".to_string(),
        });
    }

    if config.app.limits.readiness_probe_timeout.is_zero() {
        return Err(ConfigError::Invalid {
            key: "app.limits.readiness_probe_timeout".to_string(),
            value: "0s".to_string(),
            expected: "a positive duration".to_string(),
        });
    }

    if config.app.limits.shutdown_grace.is_zero() {
        return Err(ConfigError::Invalid {
            key: "app.limits.shutdown_grace".to_string(),
            value: "0s".to_string(),
            expected: "a positive duration".to_string(),
        });
    }

    Ok(())
}

/// Validates the refresh-session housekeeping.
fn validate_sessions(config: &Config) -> Result<(), ConfigError> {
    if config.app.sessions.prune_interval.is_zero() {
        return Err(ConfigError::Invalid {
            key: "app.sessions.prune_interval".to_string(),
            value: "0s".to_string(),
            expected: "a positive duration".to_string(),
        });
    }

    if config.app.sessions.prune_grace.is_zero() {
        return Err(ConfigError::Invalid {
            key: "app.sessions.prune_grace".to_string(),
            value: "0s".to_string(),
            expected: "a positive duration".to_string(),
        });
    }

    if config.app.sessions.prune_grace.as_secs() < MIN_PRUNE_GRACE_SECONDS {
        return Err(ConfigError::Invalid {
            key: "app.sessions.prune_grace".to_string(),
            value: format!("{}s", config.app.sessions.prune_grace.as_secs()),
            expected: format!("at least {MIN_PRUNE_GRACE_SECONDS}s"),
        });
    }

    Ok(())
}

/// Validates the pagination bounds.
fn validate_pagination(config: &Config) -> Result<(), ConfigError> {
    if config.api.pagination.default_limit <= 0 {
        return Err(ConfigError::Invalid {
            key: "api.pagination.default_limit".to_string(),
            value: config.api.pagination.default_limit.to_string(),
            expected: "a positive number of items".to_string(),
        });
    }

    if config.api.pagination.max_limit < config.api.pagination.default_limit {
        return Err(ConfigError::Invalid {
            key: "api.pagination.max_limit".to_string(),
            value: config.api.pagination.max_limit.to_string(),
            expected: format!(
                "a value of at least the default limit ({})",
                config.api.pagination.default_limit
            ),
        });
    }

    if config.api.pagination.max_limit > MAX_PAGINATION_LIMIT {
        return Err(ConfigError::Invalid {
            key: "api.pagination.max_limit".to_string(),
            value: config.api.pagination.max_limit.to_string(),
            expected: format!("at most {MAX_PAGINATION_LIMIT} items"),
        });
    }

    Ok(())
}

/// Validates the rate limit.
fn validate_rate_limit(config: &Config) -> Result<(), ConfigError> {
    if config.api.rate_limit.max_requests == 0 {
        return Err(ConfigError::Invalid {
            key: "api.rate_limit.max_requests".to_string(),
            value: "0".to_string(),
            expected: "a positive number of requests".to_string(),
        });
    }

    if config.api.rate_limit.window.is_zero() {
        return Err(ConfigError::Invalid {
            key: "api.rate_limit.window".to_string(),
            value: "0s".to_string(),
            expected: "a positive duration".to_string(),
        });
    }

    Ok(())
}

/// Validates the log filter.
fn validate_logging(config: &Config) -> Result<(), ConfigError> {
    if tracing_subscriber::EnvFilter::try_new(&config.app.log_filter).is_err() {
        return Err(ConfigError::Invalid {
            key: "app.log_filter".to_string(),
            value: config.app.log_filter.clone(),
            expected: "a valid tracing filter directive, such as `info`".to_string(),
        });
    }

    Ok(())
}

/// Validates the connection pool.
fn validate_database(config: &Config) -> Result<(), ConfigError> {
    if config.database.max_connections == 0 {
        return Err(ConfigError::Invalid {
            key: "database.max_connections".to_string(),
            value: "0".to_string(),
            expected: "a positive number of connections".to_string(),
        });
    }

    if config.database.acquire_timeout.is_zero() {
        return Err(ConfigError::Invalid {
            key: "database.acquire_timeout".to_string(),
            value: "0s".to_string(),
            expected: "a positive duration".to_string(),
        });
    }

    if config.database.connect_timeout.is_zero() {
        return Err(ConfigError::Invalid {
            key: "database.connect_timeout".to_string(),
            value: "0s".to_string(),
            expected: "a positive duration".to_string(),
        });
    }

    Ok(())
}

/// Validates the authentication settings.
fn validate_auth(config: &Config) -> Result<(), ConfigError> {
    if config.auth.access_token_ttl.is_zero() {
        return Err(ConfigError::Invalid {
            key: "auth.access_token_ttl".to_string(),
            value: "0s".to_string(),
            expected: "a positive duration".to_string(),
        });
    }

    if config.auth.access_token_ttl.as_secs() > MAX_ACCESS_TOKEN_TTL_SECONDS {
        return Err(ConfigError::Invalid {
            key: "auth.access_token_ttl".to_string(),
            value: format!("{}s", config.auth.access_token_ttl.as_secs()),
            expected: format!("at most {MAX_ACCESS_TOKEN_TTL_SECONDS}s"),
        });
    }

    if config.auth.refresh_token_ttl <= config.auth.access_token_ttl {
        return Err(ConfigError::Invalid {
            key: "auth.refresh_token_ttl".to_string(),
            value: format!("{}s", config.auth.refresh_token_ttl.as_secs()),
            expected: format!(
                "a lifetime greater than the access token lifetime ({}s)",
                config.auth.access_token_ttl.as_secs()
            ),
        });
    }

    if config.auth.refresh_token_ttl.as_secs() > MAX_REFRESH_TOKEN_TTL_SECONDS {
        return Err(ConfigError::Invalid {
            key: "auth.refresh_token_ttl".to_string(),
            value: format!("{}s", config.auth.refresh_token_ttl.as_secs()),
            expected: format!("at most {MAX_REFRESH_TOKEN_TTL_SECONDS}s"),
        });
    }

    if config.auth.issuer.trim().is_empty() {
        return Err(ConfigError::Invalid {
            key: "auth.issuer".to_string(),
            value: "<empty>".to_string(),
            expected: "a non-empty issuer".to_string(),
        });
    }

    if config.auth.audience.trim().is_empty() {
        return Err(ConfigError::Invalid {
            key: "auth.audience".to_string(),
            value: "<empty>".to_string(),
            expected: "a non-empty audience".to_string(),
        });
    }

    if config.auth.argon2.memory_kib < MIN_ARGON2_MEMORY_KIB {
        return Err(ConfigError::Invalid {
            key: "auth.argon2.memory_kib".to_string(),
            value: config.auth.argon2.memory_kib.to_string(),
            expected: format!("at least {MIN_ARGON2_MEMORY_KIB} KiB (8 MiB)"),
        });
    }

    if config.auth.argon2.iterations == 0 {
        return Err(ConfigError::Invalid {
            key: "auth.argon2.iterations".to_string(),
            value: "0".to_string(),
            expected: "at least one iteration".to_string(),
        });
    }

    if config.auth.argon2.parallelism == 0 {
        return Err(ConfigError::Invalid {
            key: "auth.argon2.parallelism".to_string(),
            value: "0".to_string(),
            expected: "at least one degree of parallelism".to_string(),
        });
    }

    if config.auth.password.min_length < MIN_PASSWORD_LENGTH {
        return Err(ConfigError::Invalid {
            key: "auth.password.min_length".to_string(),
            value: config.auth.password.min_length.to_string(),
            expected: format!("at least {MIN_PASSWORD_LENGTH} characters"),
        });
    }

    if config.auth.password.max_length < config.auth.password.min_length {
        return Err(ConfigError::Invalid {
            key: "auth.password.max_length".to_string(),
            value: config.auth.password.max_length.to_string(),
            expected: format!(
                "a value of at least the minimum length ({})",
                config.auth.password.min_length
            ),
        });
    }

    if config.auth.clock_skew.as_secs() > MAX_CLOCK_SKEW_SECONDS {
        return Err(ConfigError::Invalid {
            key: "auth.clock_skew".to_string(),
            value: format!("{}s", config.auth.clock_skew.as_secs()),
            expected: format!("at most {MAX_CLOCK_SKEW_SECONDS}s"),
        });
    }

    Ok(())
}

/// Validates the cross-origin allowlist.
///
/// A wildcard is refused: it cannot be combined with credentials safely, and
/// the failure belongs at start-up rather than in a panic in the router.
fn validate_cors(origins: &[String]) -> Result<(), ConfigError> {
    for origin in origins {
        if origin == "*" {
            return Err(ConfigError::Invalid {
                key: "api.cors.allowed_origins".to_string(),
                value: "*".to_string(),
                expected: "explicit origins, not a wildcard".to_string(),
            });
        }

        if origin.parse::<axum::http::HeaderValue>().is_err() {
            return Err(ConfigError::Invalid {
                key: "api.cors.allowed_origins".to_string(),
                value: origin.clone(),
                expected: "a valid origin, such as https://example.com".to_string(),
            });
        }
    }

    Ok(())
}

/// Validates the signing secret.
///
/// The value is never included in the error: only its length is reported.
fn validate_secret(secret: &str, environment: Environment) -> Result<(), ConfigError> {
    if secret.len() < MIN_SECRET_BYTES {
        return Err(ConfigError::Invalid {
            key: "APP_JWT_SECRET".to_string(),
            value: format!("<{} bytes>", secret.len()),
            expected: format!("at least {MIN_SECRET_BYTES} bytes of entropy"),
        });
    }

    if environment == Environment::Production && PLACEHOLDER_SECRETS.contains(&secret) {
        return Err(ConfigError::Invalid {
            key: "APP_JWT_SECRET".to_string(),
            value: "<placeholder>".to_string(),
            expected: "a real secret from a secret manager in production".to_string(),
        });
    }

    Ok(())
}
