//! Configuration loading.
//!
//! The loader resolves three layers, in this order:
//!
//! 1. the base files in `config/`,
//! 2. the environment overlay in `config/environments/<APP_ENV>.toml`,
//! 3. the environment variables.
//!
//! A later layer wins over an earlier one, key by key. The files are embedded
//! in the binary, so a deployment needs no configuration directory; setting
//! `ATOM_CONFIG_DIR` points the loader at a directory whose files are merged
//! on top of the embedded ones.
//!
//! The environment source is injectable, so that the precedence and the
//! validation rules can be tested without touching the process environment.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use toml::Table;

use crate::config::settings::{MlBackend, Settings};
use crate::config::validation::{ConfigError, validate};
use crate::config::{Config, Environment};

/// The base configuration files, in the order they are merged.
const BASE_FILES: [&str; 6] = [
    "app.toml",
    "api.toml",
    "auth.toml",
    "database.toml",
    "ml.toml",
    "messages.toml",
];

/// The environment variable that points at a configuration directory.
pub const CONFIG_DIR_VAR: &str = "ATOM_CONFIG_DIR";

/// Where the environment variables come from.
///
/// The production loader reads the process environment. A test injects a map,
/// so that the tests never mutate the environment of the test process.
pub(super) type EnvironmentSource = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Reads one variable from the process environment.
///
/// A value that is empty or only whitespace counts as unset.
fn process_environment(key: &str) -> Option<String> {
    normalise(std::env::var(key).ok()?)
}

/// Normalises a value read from the environment.
///
/// A value that is empty or only whitespace counts as unset. The rule is
/// applied in one place, so that the test seam and the real environment
/// cannot drift apart.
fn normalise(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();

    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Loads the configuration for this process.
#[derive(Clone)]
pub struct ConfigLoader {
    /// The directory the configuration files are read from, when one was
    /// found.
    directory: Option<PathBuf>,
    /// Where the environment variables come from.
    environment: EnvironmentSource,
}

impl std::fmt::Debug for ConfigLoader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigLoader")
            .field("directory", &self.directory)
            .finish_non_exhaustive()
    }
}

impl ConfigLoader {
    /// Creates a loader that reads the process environment, and discovers the
    /// configuration directory.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when `ATOM_CONFIG_DIR` is set but does not point
    /// at a directory.
    pub fn new() -> Result<Self, ConfigError> {
        let environment: EnvironmentSource = Arc::new(process_environment);

        Ok(Self {
            directory: discover_directory(&environment)?,
            environment,
        })
    }

    /// Creates a loader that reads the environment from a map.
    ///
    /// This is the seam the tests use. The directory is given explicitly, so a
    /// test can point the loader at a directory of its own.
    pub fn with_variables(directory: Option<PathBuf>, variables: BTreeMap<String, String>) -> Self {
        let map = Arc::new(variables);
        Self {
            directory,
            environment: Arc::new(move |key: &str| {
                map.get(key).and_then(|value| normalise(value.clone()))
            }),
        }
    }

    /// Reads one environment variable.
    fn variable(&self, key: &str) -> Option<String> {
        (self.environment)(key)
    }

    /// Loads the configuration for this process.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when a file is malformed, a required
    /// environment variable is absent, a value cannot be parsed, or the
    /// resolved configuration violates a rule.
    pub fn load(&self) -> Result<Config, ConfigError> {
        let environment = match self.variable("APP_ENV") {
            Some(raw) => Environment::parse(&raw)?,
            None => Environment::Development,
        };

        let mut table = self.read_base()?;

        if let Some(overlay) = self.read_overlay(environment)? {
            merge(&mut table, overlay);
        }

        let mut settings: Settings =
            table
                .try_into()
                .map_err(|error: toml::de::Error| ConfigError::Parse {
                    source: "the merged configuration".to_string(),
                    message: error.message().to_string(),
                })?;

        self.apply_environment(&mut settings)?;

        let config = Config {
            environment,
            app: settings.app,
            api: settings.api,
            auth: settings.auth,
            database: settings.database,
            ml: settings.ml,
            // The catalogue is read on every error path, so the services share
            // one copy behind an `Arc`.
            messages: Arc::new(settings.messages),
        };

        validate(&config)?;

        Ok(config)
    }

    /// Loads the base configuration, without the environment overlay and
    /// without the environment variables.
    ///
    /// This is the starting point for a test harness: it exercises the real
    /// files and the real parser, and the caller fills in the two secrets.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when a base file is malformed.
    pub fn load_base(&self) -> Result<Config, ConfigError> {
        let table = self.read_base()?;

        let settings: Settings =
            table
                .try_into()
                .map_err(|error: toml::de::Error| ConfigError::Parse {
                    source: "the base configuration".to_string(),
                    message: error.message().to_string(),
                })?;

        let config = Config {
            environment: Environment::Development,
            app: settings.app,
            api: settings.api,
            auth: settings.auth,
            database: settings.database,
            ml: settings.ml,
            messages: Arc::new(settings.messages),
        };

        crate::config::validation::validate_settings(&config)?;

        Ok(config)
    }

    /// Reads and merges the base configuration files.
    fn read_base(&self) -> Result<Table, ConfigError> {
        let mut table = Table::new();

        for name in BASE_FILES {
            let mut file = parse_file(name, embedded(name))?;

            if let Some(directory) = &self.directory {
                let path = directory.join(name);
                match fs::read_to_string(&path) {
                    Ok(contents) => {
                        let overlay = parse_file(name, &contents)?;
                        merge(&mut file, overlay);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        // The embedded copy is used. A file that is present on
                        // disk replaces the embedded one key by key; a file
                        // that is absent leaves the embedded one in place.
                    }
                    Err(error) => {
                        return Err(ConfigError::Io {
                            path: path.display().to_string(),
                            message: error.to_string(),
                        });
                    }
                }
            }

            merge(&mut table, file);
        }

        Ok(table)
    }

    /// Reads the environment overlay, when there is one.
    fn read_overlay(&self, environment: Environment) -> Result<Option<Table>, ConfigError> {
        let name = format!("environments/{}", environment.overlay_file());

        let mut table = match embedded_overlay(environment) {
            Some(contents) => parse_file(&name, contents)?,
            None => Table::new(),
        };

        if let Some(directory) = &self.directory {
            let path = directory.join(&name);
            match fs::read_to_string(&path) {
                Ok(contents) => {
                    let overlay = parse_file(&name, &contents)?;
                    merge(&mut table, overlay);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(ConfigError::Io {
                        path: path.display().to_string(),
                        message: error.to_string(),
                    });
                }
            }
        }

        Ok(Some(table))
    }

    /// Applies the environment-variable overrides.
    ///
    /// A variable that is not set leaves the value from the file layers alone.
    /// The two secrets have no file layer at all, so they are always required.
    fn apply_environment(&self, settings: &mut Settings) -> Result<(), ConfigError> {
        if let Some(value) = self.variable("APP_BIND_ADDR") {
            settings.app.bind_addr = value.parse().map_err(|_| ConfigError::Invalid {
                key: "APP_BIND_ADDR".to_string(),
                value,
                expected: "a socket address such as 127.0.0.1:3000".to_string(),
            })?;
        }

        if let Some(value) = self.variable("APP_LOG_FILTER") {
            settings.app.log_filter = value;
        }

        if let Some(value) = self.variable("APP_MAX_BODY_BYTES") {
            settings.app.limits.max_body_bytes = parse_number("APP_MAX_BODY_BYTES", &value)?;
        }

        if let Some(value) = self.variable("APP_REQUEST_TIMEOUT_SECONDS") {
            settings.app.limits.request_timeout =
                Duration::from_secs(parse_number("APP_REQUEST_TIMEOUT_SECONDS", &value)?);
        }

        if let Some(value) = self.variable("APP_READINESS_PROBE_TIMEOUT_SECONDS") {
            settings.app.limits.readiness_probe_timeout =
                Duration::from_secs(parse_number("APP_READINESS_PROBE_TIMEOUT_SECONDS", &value)?);
        }

        if let Some(value) = self.variable("APP_SHUTDOWN_GRACE_SECONDS") {
            settings.app.limits.shutdown_grace =
                Duration::from_secs(parse_number("APP_SHUTDOWN_GRACE_SECONDS", &value)?);
        }

        if let Some(value) = self.variable("APP_SESSION_PRUNE_INTERVAL_SECONDS") {
            settings.app.sessions.prune_interval =
                Duration::from_secs(parse_number("APP_SESSION_PRUNE_INTERVAL_SECONDS", &value)?);
        }

        if let Some(value) = self.variable("APP_SESSION_PRUNE_GRACE_SECONDS") {
            settings.app.sessions.prune_grace =
                Duration::from_secs(parse_number("APP_SESSION_PRUNE_GRACE_SECONDS", &value)?);
        }

        if let Some(value) = self.variable("APP_CORS_ALLOWED_ORIGINS") {
            settings.api.cors.allowed_origins = parse_csv(&value);
        }

        if let Some(value) = self.variable("APP_API_DOCS_ENABLED") {
            settings.api.docs_enabled = parse_bool("APP_API_DOCS_ENABLED", &value)?;
        }

        if let Some(value) = self.variable("APP_AUTH_RATE_LIMIT_MAX_REQUESTS") {
            settings.api.rate_limit.max_requests =
                parse_number("APP_AUTH_RATE_LIMIT_MAX_REQUESTS", &value)?;
        }

        if let Some(value) = self.variable("APP_AUTH_RATE_LIMIT_WINDOW_SECONDS") {
            settings.api.rate_limit.window =
                Duration::from_secs(parse_number("APP_AUTH_RATE_LIMIT_WINDOW_SECONDS", &value)?);
        }

        settings.database.url = self.required("DATABASE_URL")?;

        if let Some(value) = self.variable("DATABASE_MAX_CONNECTIONS") {
            settings.database.max_connections = parse_number("DATABASE_MAX_CONNECTIONS", &value)?;
        }

        if let Some(value) = self.variable("DATABASE_ACQUIRE_TIMEOUT_SECS") {
            settings.database.acquire_timeout =
                Duration::from_secs(parse_number("DATABASE_ACQUIRE_TIMEOUT_SECS", &value)?);
        }

        if let Some(value) = self.variable("DATABASE_CONNECT_TIMEOUT_SECS") {
            settings.database.connect_timeout =
                Duration::from_secs(parse_number("DATABASE_CONNECT_TIMEOUT_SECS", &value)?);
        }

        settings.auth.jwt_secret = self.required("APP_JWT_SECRET")?;

        if let Some(value) = self.variable("APP_ACCESS_TOKEN_TTL_SECONDS") {
            settings.auth.access_token_ttl =
                Duration::from_secs(parse_number("APP_ACCESS_TOKEN_TTL_SECONDS", &value)?);
        }

        if let Some(value) = self.variable("APP_REFRESH_TOKEN_TTL_SECONDS") {
            settings.auth.refresh_token_ttl =
                Duration::from_secs(parse_number("APP_REFRESH_TOKEN_TTL_SECONDS", &value)?);
        }

        if let Some(value) = self.variable("APP_ACCESS_TOKEN_CLOCK_SKEW_SECONDS") {
            settings.auth.clock_skew =
                Duration::from_secs(parse_number("APP_ACCESS_TOKEN_CLOCK_SKEW_SECONDS", &value)?);
        }

        if let Some(value) = self.variable("APP_JWT_ISSUER") {
            settings.auth.issuer = value;
        }

        if let Some(value) = self.variable("APP_JWT_AUDIENCE") {
            settings.auth.audience = value;
        }

        if let Some(value) = self.variable("APP_ARGON2_MEMORY_KIB") {
            settings.auth.argon2.memory_kib = parse_number("APP_ARGON2_MEMORY_KIB", &value)?;
        }

        if let Some(value) = self.variable("APP_ARGON2_ITERATIONS") {
            settings.auth.argon2.iterations = parse_number("APP_ARGON2_ITERATIONS", &value)?;
        }

        if let Some(value) = self.variable("APP_ARGON2_PARALLELISM") {
            settings.auth.argon2.parallelism = parse_number("APP_ARGON2_PARALLELISM", &value)?;
        }

        // The seven `ml.*` settings. `ml.model_path` and `ml.tokenizer_path`
        // are not secrets, so a literal value in the committed file is allowed;
        // a machine-specific path belongs here, in the environment, because a
        // committed file is shared by every host.
        if let Some(value) = self.variable("APP_ML_ENABLED") {
            settings.ml.enabled = parse_bool("APP_ML_ENABLED", &value)?;
        }

        if let Some(value) = self.variable("APP_ML_BACKEND") {
            settings.ml.backend = MlBackend::parse(&value)?;
        }

        if let Some(value) = self.variable("APP_ML_MODEL_PATH") {
            settings.ml.model_path = value;
        }

        if let Some(value) = self.variable("APP_ML_TOKENIZER_PATH") {
            settings.ml.tokenizer_path = value;
        }

        if let Some(value) = self.variable("APP_ML_DIMENSION") {
            settings.ml.dimension = parse_number("APP_ML_DIMENSION", &value)?;
        }

        if let Some(value) = self.variable("APP_ML_TIMEOUT_SECONDS") {
            settings.ml.timeout =
                Duration::from_secs(parse_number("APP_ML_TIMEOUT_SECONDS", &value)?);
        }

        if let Some(value) = self.variable("APP_ML_CANDIDATE_LIMIT") {
            settings.ml.candidate_limit = parse_number("APP_ML_CANDIDATE_LIMIT", &value)?;
        }

        Ok(())
    }

    /// Reads a required environment variable.
    fn required(&self, key: &str) -> Result<String, ConfigError> {
        self.variable(key)
            .ok_or_else(|| ConfigError::Missing(key.to_string()))
    }
}

/// The embedded copy of a base configuration file.
const fn embedded(name: &str) -> &'static str {
    match name.as_bytes() {
        b"app.toml" => include_str!("../../config/app.toml"),
        b"api.toml" => include_str!("../../config/api.toml"),
        b"auth.toml" => include_str!("../../config/auth.toml"),
        b"database.toml" => include_str!("../../config/database.toml"),
        b"ml.toml" => include_str!("../../config/ml.toml"),
        b"messages.toml" => include_str!("../../config/messages.toml"),
        _ => "",
    }
}

/// The embedded copy of an environment overlay.
fn embedded_overlay(environment: Environment) -> Option<&'static str> {
    match environment {
        Environment::Development => {
            Some(include_str!("../../config/environments/development.toml"))
        }
        Environment::Production => Some(include_str!("../../config/environments/production.toml")),
    }
}

/// Finds the directory the configuration files are read from.
///
/// The search order is the explicit `ATOM_CONFIG_DIR`, then a `config`
/// directory next to the executable, then a `config` directory in the working
/// directory.
pub(super) fn discover_directory(
    environment: &EnvironmentSource,
) -> Result<Option<PathBuf>, ConfigError> {
    if let Some(directory) = environment(CONFIG_DIR_VAR) {
        let path = PathBuf::from(&directory);
        if !path.is_dir() {
            return Err(ConfigError::Invalid {
                key: CONFIG_DIR_VAR.to_string(),
                value: directory,
                expected: "the path of an existing directory".to_string(),
            });
        }
        return Ok(Some(path));
    }

    if let Ok(executable) = std::env::current_exe()
        && let Some(parent) = executable.parent()
    {
        let candidate = parent.join("config");
        if candidate.is_dir() {
            return Ok(Some(candidate));
        }
    }

    let candidate = PathBuf::from("config");
    if candidate.is_dir() {
        return Ok(Some(candidate));
    }

    Ok(None)
}

/// Parses one TOML document.
fn parse_file(name: &str, contents: &str) -> Result<Table, ConfigError> {
    contents
        .parse::<Table>()
        .map_err(|error: toml::de::Error| ConfigError::Parse {
            source: name.to_string(),
            message: error.message().to_string(),
        })
}

/// Merges `overlay` into `base`, key by key.
///
/// A table in the overlay is merged recursively; every other value replaces
/// the one in the base.
fn merge(base: &mut Table, overlay: Table) {
    for (key, value) in overlay {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(existing)), toml::Value::Table(incoming)) => {
                merge(existing, incoming);
            }
            (_, incoming) => {
                base.insert(key, incoming);
            }
        }
    }
}

/// Parses a number, reporting the variable that held it.
fn parse_number<T>(key: &str, value: &str) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
{
    value.parse::<T>().map_err(|_| ConfigError::Invalid {
        key: key.to_string(),
        value: value.to_string(),
        expected: "a number".to_string(),
    })
}

/// Parses a boolean, reporting the variable that held it.
fn parse_bool(key: &str, value: &str) -> Result<bool, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(ConfigError::Invalid {
            key: key.to_string(),
            value: value.to_string(),
            expected: "one of: true, false".to_string(),
        }),
    }
}

/// Parses a comma-separated list.
fn parse_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}
