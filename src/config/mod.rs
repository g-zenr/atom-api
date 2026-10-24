//! Typed application configuration.
//!
//! Configuration is resolved once, at start-up, from three layers:
//!
//! 1. the base files in `config/`,
//! 2. the environment overlay in `config/environments/<APP_ENV>.toml`,
//! 3. the environment variables.
//!
//! Later layers win. Every layer is read exactly once, here, and the result
//! is validated before the server binds a port. Modules downstream receive a
//! concrete [`Config`] value instead of reading the process environment
//! themselves, which keeps configuration testable and prevents scattered
//! `env::var` calls.
//!
//! The layout of this module is:
//!
//! * [`settings`] — the typed settings structures.
//! * [`loader`] — discovery, parsing, merging and the environment overrides.
//! * [`validation`] — the rules a valid configuration must satisfy.
//! * [`messages`] — the client-facing message catalogue.

pub mod loader;
pub mod messages;
pub mod settings;
pub mod validation;

#[cfg(test)]
mod tests;

use std::sync::Arc;

pub use loader::ConfigLoader;
pub use messages::Messages;
pub use settings::{
    ApiSettings, AppSettings, Argon2Params, AuthSettings, CorsSettings, DatabaseSettings,
    MlBackend, MlSettings, PaginationSettings, PasswordPolicy, RateLimitSettings, RequestLimits,
    SessionSettings, Settings,
};
pub use validation::ConfigError;
pub use validation::MAX_BODY_BYTES_BACKSTOP;

/// The fully resolved configuration.
///
/// A `Config` is immutable once it has been built. It is shared through the
/// application state, which every handler already has.
#[derive(Debug, Clone)]
pub struct Config {
    /// Which environment the process is running in.
    pub environment: Environment,
    /// The application settings.
    pub app: AppSettings,
    /// The API settings.
    pub api: ApiSettings,
    /// The authentication settings.
    pub auth: AuthSettings,
    /// The database settings.
    pub database: DatabaseSettings,
    /// The local machine-learning settings.
    pub ml: MlSettings,
    /// The client-facing message catalogue.
    pub messages: Arc<Messages>,
}

impl Config {
    /// Loads the configuration for this process.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when a configuration file is missing or
    /// malformed, a required environment variable is absent, a value cannot be
    /// parsed, or the resolved configuration violates a rule.
    pub fn load() -> Result<Self, ConfigError> {
        ConfigLoader::new()?.load()
    }

    /// The base configuration, with no environment variables applied.
    ///
    /// This is the configuration a test harness starts from. It exercises the
    /// real loader and the real files, so a broken configuration file fails the
    /// test suite rather than a deployment.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when the base configuration files are missing
    /// or malformed.
    pub fn defaults() -> Result<Self, ConfigError> {
        ConfigLoader::new()?.load_base()
    }
}

/// The environment the process runs in.
///
/// The environment selects the overlay file and decides whether the
/// documented placeholder secrets are refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    /// A local or shared development deployment.
    Development,
    /// A production deployment.
    Production,
}

impl Environment {
    /// The canonical name of this environment.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Production => "production",
        }
    }

    /// The file name of the overlay for this environment.
    pub fn overlay_file(&self) -> String {
        format!("{}.toml", self.name())
    }

    /// Parses an environment name.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when the name is not a known environment.
    pub fn parse(raw: &str) -> Result<Self, ConfigError> {
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
