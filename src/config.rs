//! Typed application configuration.
//!
//! Every environment variable is read exactly once, here, and validated before
//! the server starts. Modules downstream receive a concrete `Config` value
//! instead of reading the process environment themselves, which keeps
//! configuration testable and prevents scattered `env::var` calls.

use std::fmt;
use std::net::SocketAddr;
use std::time::Duration;

/// Deployment environment. Drives fail-fast behaviour for secrets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Development,
    Production,
}

impl Environment {
    fn parse(raw: &str) -> Result<Self, ConfigError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "development" | "dev" | "local" => Ok(Self::Development),
            "production" | "prod" => Ok(Self::Production),
            other => Err(ConfigError::Invalid {
                key: "APP_ENV".to_string(),
                value: other.to_string(),
                expected: "one of: development, production".to_string(),
            }),
        }
    }
}

/// Database connectivity and pool settings.
#[derive(Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub acquire_timeout: Duration,
}

// The connection URL can contain a password, so it is redacted in logs.
impl fmt::Debug for DatabaseConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DatabaseConfig")
            .field("url", &"[REDACTED]")
            .field("max_connections", &self.max_connections)
            .field("acquire_timeout", &self.acquire_timeout)
            .finish()
    }
}

/// Argon2id parameters. Exposed so that cost can be raised per environment.
#[derive(Debug, Clone, Copy)]
pub struct Argon2Params {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

/// Token lifetimes and signing material.
#[derive(Clone)]
pub struct AuthConfig {
    /// HMAC signing secret. Never logged, never rendered by `Debug`.
    pub jwt_secret: String,
    pub access_token_ttl: Duration,
    pub refresh_token_ttl: Duration,
    pub issuer: String,
    pub audience: String,
    pub argon2: Argon2Params,
}

// The signing secret must not reach log output.
impl fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthConfig")
            .field("jwt_secret", &"[REDACTED]")
            .field("access_token_ttl", &self.access_token_ttl)
            .field("refresh_token_ttl", &self.refresh_token_ttl)
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("argon2", &self.argon2)
            .finish()
    }
}

/// In-process rate limiting configuration.
#[derive(Debug, Clone, Copy)]
pub struct RateLimitConfig {
    pub max_requests: u32,
    pub window: Duration,
}

/// Fully validated application configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub environment: Environment,
    pub bind_addr: SocketAddr,
    pub cors_allowed_origins: Vec<String>,
    pub max_body_bytes: usize,
    /// How long a request may take before it is abandoned.
    pub request_timeout: Duration,
    pub log_filter: String,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub rate_limit: RateLimitConfig,
}

impl Config {
    /// Reads and validates configuration from the process environment.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when a required variable is missing, a value
    /// cannot be parsed, or a value fails a safety rule (for example a short
    /// signing secret in production).
    pub fn from_env() -> Result<Self, ConfigError> {
        let environment = match optional("APP_ENV") {
            Some(raw) => Environment::parse(&raw)?,
            None => Environment::Development,
        };

        let bind_addr = optional("APP_BIND_ADDR")
            .unwrap_or_else(|| "127.0.0.1:3000".to_string())
            .parse::<SocketAddr>()
            .map_err(|_| ConfigError::Invalid {
                key: "APP_BIND_ADDR".to_string(),
                value: optional("APP_BIND_ADDR").unwrap_or_default(),
                expected: "a socket address such as 127.0.0.1:3000".to_string(),
            })?;

        let cors_allowed_origins = parse_csv(optional("APP_CORS_ALLOWED_ORIGINS").as_deref());

        let max_body_bytes = parse_or("APP_MAX_BODY_BYTES", 1024 * 1024usize)?;
        if max_body_bytes == 0 {
            return Err(ConfigError::Invalid {
                key: "APP_MAX_BODY_BYTES".to_string(),
                value: "0".to_string(),
                expected: "a positive number of bytes".to_string(),
            });
        }

        let request_timeout = Duration::from_secs(parse_or("APP_REQUEST_TIMEOUT_SECONDS", 30u64)?);
        if request_timeout.is_zero() {
            return Err(ConfigError::Invalid {
                key: "APP_REQUEST_TIMEOUT_SECONDS".to_string(),
                value: "0".to_string(),
                expected: "a positive number of seconds".to_string(),
            });
        }

        let log_filter = optional("APP_LOG_FILTER").unwrap_or_else(|| "info".to_string());

        let database_url = required("DATABASE_URL")?;
        let database = DatabaseConfig {
            url: database_url,
            max_connections: parse_or("DATABASE_MAX_CONNECTIONS", 10u32)?,
            acquire_timeout: Duration::from_secs(parse_or("DATABASE_ACQUIRE_TIMEOUT_SECS", 5u64)?),
        };

        let jwt_secret = required("APP_JWT_SECRET")?;
        validate_secret(&jwt_secret, environment)?;
        let access_token_ttl =
            Duration::from_secs(parse_or("APP_ACCESS_TOKEN_TTL_SECONDS", 900u64)?);
        let refresh_token_ttl =
            Duration::from_secs(parse_or("APP_REFRESH_TOKEN_TTL_SECONDS", 1_209_600u64)?);

        if access_token_ttl.is_zero() {
            return Err(ConfigError::Invalid {
                key: "APP_ACCESS_TOKEN_TTL_SECONDS".to_string(),
                value: "0".to_string(),
                expected: "a positive number of seconds".to_string(),
            });
        }
        if refresh_token_ttl <= access_token_ttl {
            return Err(ConfigError::Invalid {
                key: "APP_REFRESH_TOKEN_TTL_SECONDS".to_string(),
                value: refresh_token_ttl.as_secs().to_string(),
                expected: "a lifetime greater than the access token lifetime".to_string(),
            });
        }

        let argon2 = Argon2Params {
            memory_kib: parse_or("APP_ARGON2_MEMORY_KIB", 19_456u32)?,
            iterations: parse_or("APP_ARGON2_ITERATIONS", 2u32)?,
            parallelism: parse_or("APP_ARGON2_PARALLELISM", 1u32)?,
        };
        if argon2.memory_kib < 8_192 {
            return Err(ConfigError::Invalid {
                key: "APP_ARGON2_MEMORY_KIB".to_string(),
                value: argon2.memory_kib.to_string(),
                expected: "at least 8192 KiB (8 MiB)".to_string(),
            });
        }

        let rate_limit = RateLimitConfig {
            max_requests: parse_or("APP_AUTH_RATE_LIMIT_MAX_REQUESTS", 10u32)?,
            window: Duration::from_secs(parse_or("APP_AUTH_RATE_LIMIT_WINDOW_SECONDS", 60u64)?),
        };

        Ok(Self {
            environment,
            bind_addr,
            cors_allowed_origins,
            max_body_bytes,
            request_timeout,
            log_filter,
            database,
            auth: AuthConfig {
                jwt_secret,
                access_token_ttl,
                refresh_token_ttl,
                issuer: optional("APP_JWT_ISSUER").unwrap_or_else(|| "atom-api".to_string()),
                audience: optional("APP_JWT_AUDIENCE").unwrap_or_else(|| "atom-api".to_string()),
                argon2,
            },
            rate_limit,
        })
    }
}

/// Minimum acceptable length of the HMAC signing secret.
const MIN_SECRET_BYTES: usize = 32;

/// Placeholder secrets that must never reach production.
const PLACEHOLDER_SECRETS: [&str; 2] = [
    "replace-me-with-a-development-only-secret-at-least-32-bytes-long",
    "change-me",
];

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

fn required(key: &str) -> Result<String, ConfigError> {
    match optional(key) {
        Some(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(ConfigError::Missing(key.to_string())),
    }
}

fn optional(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
}

fn parse_or<T>(key: &str, default: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
    T::Err: fmt::Display,
{
    match optional(key) {
        None => Ok(default),
        Some(raw) => raw.parse::<T>().map_err(|err| ConfigError::Invalid {
            key: key.to_string(),
            value: raw,
            expected: err.to_string(),
        }),
    }
}

fn parse_csv(raw: Option<&str>) -> Vec<String> {
    raw.map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

/// Configuration failure. Raised before the server starts listening.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("required environment variable {0} is missing or empty")]
    Missing(String),
    #[error("environment variable {key} has invalid value {value}: expected {expected}")]
    Invalid {
        key: String,
        value: String,
        expected: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_csv_splits_and_trims() {
        assert_eq!(
            parse_csv(Some(" http://a.test , http://b.test ,")),
            vec!["http://a.test".to_string(), "http://b.test".to_string()]
        );
    }

    #[test]
    fn parse_csv_handles_absent_value() {
        assert!(parse_csv(None).is_empty());
    }

    #[test]
    fn environment_aliases_are_accepted() {
        assert_eq!(Environment::parse("prod").unwrap(), Environment::Production);
        assert_eq!(
            Environment::parse("LOCAL").unwrap(),
            Environment::Development
        );
        assert!(Environment::parse("staging").is_err());
    }

    #[test]
    fn short_secrets_are_rejected() {
        let err = validate_secret("too-short", Environment::Development).unwrap_err();
        assert!(matches!(err, ConfigError::Invalid { .. }));
    }

    #[test]
    fn placeholder_secret_is_rejected_in_production_only() {
        let placeholder = PLACEHOLDER_SECRETS[0];
        assert!(
            validate_secret(placeholder, Environment::Development).is_ok(),
            "development may use the documented placeholder"
        );
        assert!(
            validate_secret(placeholder, Environment::Production).is_err(),
            "production must refuse the documented placeholder"
        );
    }

    #[test]
    fn auth_config_debug_redacts_the_secret() {
        let config = AuthConfig {
            jwt_secret: "s".repeat(40),
            access_token_ttl: Duration::from_secs(60),
            refresh_token_ttl: Duration::from_secs(120),
            issuer: "atom-api".to_string(),
            audience: "atom-api".to_string(),
            argon2: Argon2Params {
                memory_kib: 19_456,
                iterations: 2,
                parallelism: 1,
            },
        };
        let rendered = format!("{config:?}");
        assert!(!rendered.contains(&"s".repeat(40)));
        assert!(rendered.contains("[REDACTED]"));
    }
}
