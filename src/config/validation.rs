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
pub fn validate_settings(config: &Config) -> Result<(), ConfigError> {
    if config.app.limits.max_body_bytes == 0 {
        return Err(ConfigError::Invalid {
            key: "app.limits.max_body_bytes".to_string(),
            value: "0".to_string(),
            expected: "a positive number of bytes".to_string(),
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

    if tracing_subscriber::EnvFilter::try_new(&config.app.log_filter).is_err() {
        return Err(ConfigError::Invalid {
            key: "app.log_filter".to_string(),
            value: config.app.log_filter.clone(),
            expected: "a valid tracing filter directive, such as `info`".to_string(),
        });
    }

    validate_cors(&config.api.cors.allowed_origins)?;

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

    if config.auth.access_token_ttl.is_zero() {
        return Err(ConfigError::Invalid {
            key: "auth.access_token_ttl".to_string(),
            value: "0s".to_string(),
            expected: "a positive duration".to_string(),
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

    if config.auth.password.min_length == 0 {
        return Err(ConfigError::Invalid {
            key: "auth.password.min_length".to_string(),
            value: "0".to_string(),
            expected: "a positive number of characters".to_string(),
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
