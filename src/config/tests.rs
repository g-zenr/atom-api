//! The configuration tests.
//!
//! The configuration is the one place where a mistake is invisible until a
//! deployment starts, so the rules are covered here: the defaults, the
//! precedence between the three layers, the required secrets, the validation
//! rules and the message catalogue.
//!
//! The environment is injected rather than read from the process, so the tests
//! never mutate the environment of the test process and can run in parallel.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use super::loader::{CONFIG_DIR_VAR, ConfigLoader, EnvironmentSource};
use super::messages::render;
use super::validation::ConfigError;
use super::{Config, Environment};

/// A secret that satisfies the production rule.
const TEST_SECRET: &str = "test-signing-secret-that-is-long-enough-32b";

/// The database URL the tests use. It is never connected to.
const TEST_DATABASE_URL: &str = "postgres://atom:atom@127.0.0.1:5432/atom_test";

/// A counter that makes every test directory unique.
static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A loader that reads the environment from a map and no files.
fn loader_with(variables: &[(&str, &str)]) -> ConfigLoader {
    let map: BTreeMap<String, String> = variables
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    ConfigLoader::with_variables(None, map)
}

/// A loader with the two required secrets already set.
fn loader_with_secrets(variables: &[(&str, &str)]) -> ConfigLoader {
    let mut all: Vec<(&str, &str)> = vec![
        ("DATABASE_URL", TEST_DATABASE_URL),
        ("APP_JWT_SECRET", TEST_SECRET),
    ];
    all.extend_from_slice(variables);
    loader_with(&all)
}

/// A loader that reads the given files from a temporary directory.
fn loader_with_files(directory: &Path, variables: &[(&str, &str)]) -> ConfigLoader {
    let map: BTreeMap<String, String> = variables
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    ConfigLoader::with_variables(Some(directory.to_path_buf()), map)
}

/// A unique temporary directory for a test.
fn temporary_directory() -> PathBuf {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "atom-config-test-{}-{}",
        std::process::id(),
        counter
    ));
    fs::create_dir_all(&path).expect("the temporary directory must be creatable");
    path
}

/// Removes a temporary directory.
fn remove_directory(path: &PathBuf) {
    let _ = fs::remove_dir_all(path);
}

/// Asserts that the error is an invalid-value error for the given key.
fn assert_invalid(error: ConfigError, expected_key: &str) {
    match error {
        ConfigError::Invalid { key, .. } => assert_eq!(key, expected_key),
        other => panic!("expected an invalid-value error for {expected_key}, got {other:?}"),
    }
}

#[test]
fn the_base_configuration_carries_the_documented_defaults() {
    let config = Config::defaults().expect("the base configuration must be valid");

    assert_eq!(config.environment, Environment::Development);
    assert_eq!(config.app.bind_addr.to_string(), "127.0.0.1:3000");
    assert_eq!(config.app.limits.max_body_bytes, 1024 * 1024);
    assert_eq!(
        config.app.limits.request_timeout,
        std::time::Duration::from_secs(30)
    );
    assert_eq!(
        config.app.limits.readiness_probe_timeout,
        std::time::Duration::from_secs(1)
    );
    assert_eq!(config.api.pagination.default_limit, 20);
    assert_eq!(config.api.pagination.max_limit, 100);
    assert_eq!(config.api.rate_limit.max_requests, 10);
    assert_eq!(
        config.api.rate_limit.window,
        std::time::Duration::from_secs(60)
    );
    assert_eq!(
        config.auth.access_token_ttl,
        std::time::Duration::from_secs(900)
    );
    assert_eq!(
        config.auth.refresh_token_ttl,
        std::time::Duration::from_secs(1_209_600)
    );
    assert_eq!(config.auth.clock_skew, std::time::Duration::from_secs(30));
    assert_eq!(config.auth.argon2.memory_kib, 19_456);
    assert_eq!(config.auth.argon2.iterations, 2);
    assert_eq!(config.auth.argon2.parallelism, 1);
    assert_eq!(config.auth.password.min_length, 12);
    assert_eq!(config.auth.password.max_length, 256);
    assert_eq!(config.database.max_connections, 10);
    assert_eq!(
        config.database.acquire_timeout,
        std::time::Duration::from_secs(5)
    );
    assert_eq!(
        config.database.connect_timeout,
        std::time::Duration::from_secs(10)
    );
}

#[test]
fn the_catalogue_is_loaded_and_complete() {
    let config = Config::defaults().expect("the base configuration must be valid");
    let messages = &config.messages;

    // Every key the code uses is present and non-empty.
    for (name, value) in [
        (
            "general.validation_failed",
            &messages.general.validation_failed,
        ),
        (
            "general.payload_too_large",
            &messages.general.payload_too_large,
        ),
        ("general.rate_limited", &messages.general.rate_limited),
        ("general.timeout", &messages.general.timeout),
        ("general.bad_request", &messages.general.bad_request),
        (
            "general.malformed_parameter",
            &messages.general.malformed_parameter,
        ),
        ("general.body_mismatch", &messages.general.body_mismatch),
        ("general.unauthorized", &messages.general.unauthorized),
        ("general.route_not_found", &messages.general.route_not_found),
        (
            "auth.invalid_credentials",
            &messages.auth.invalid_credentials,
        ),
        (
            "auth.access_token_invalid",
            &messages.auth.access_token_invalid,
        ),
        (
            "auth.refresh_token_invalid",
            &messages.auth.refresh_token_invalid,
        ),
        (
            "auth.refresh_token_reused",
            &messages.auth.refresh_token_reused,
        ),
        (
            "auth.refresh_token_expired",
            &messages.auth.refresh_token_expired,
        ),
        ("auth.account_disabled", &messages.auth.account_disabled),
        ("auth.not_own_session", &messages.auth.not_own_session),
        (
            "auth.email_already_registered",
            &messages.auth.email_already_registered,
        ),
        (
            "auth.database_constraint",
            &messages.auth.database_constraint,
        ),
        (
            "validation.email_invalid",
            &messages.validation.email_invalid,
        ),
        (
            "validation.email_too_long",
            &messages.validation.email_too_long,
        ),
        (
            "validation.password_too_short",
            &messages.validation.password_too_short,
        ),
        (
            "validation.password_too_long",
            &messages.validation.password_too_long,
        ),
        (
            "validation.password_needs_uppercase",
            &messages.validation.password_needs_uppercase,
        ),
        (
            "validation.password_needs_lowercase",
            &messages.validation.password_needs_lowercase,
        ),
        (
            "validation.password_needs_digit",
            &messages.validation.password_needs_digit,
        ),
        (
            "validation.display_name_length",
            &messages.validation.display_name_length,
        ),
        ("validation.not_empty", &messages.validation.not_empty),
        (
            "validation.title_too_long",
            &messages.validation.title_too_long,
        ),
        (
            "validation.content_too_long",
            &messages.validation.content_too_long,
        ),
        (
            "validation.description_too_long",
            &messages.validation.description_too_long,
        ),
        (
            "resources.user_not_found",
            &messages.resources.user_not_found,
        ),
        (
            "resources.note_not_found",
            &messages.resources.note_not_found,
        ),
        (
            "resources.project_not_found",
            &messages.resources.project_not_found,
        ),
        (
            "resources.task_not_found",
            &messages.resources.task_not_found,
        ),
    ] {
        assert!(!value.is_empty(), "the catalogue key {name} is empty");
    }
}

#[test]
fn a_message_template_renders_its_placeholders() {
    let config = Config::defaults().expect("the base configuration must be valid");

    let rendered = render(
        &config.messages.validation.title_too_long,
        &["200".to_string()],
    );
    assert_eq!(rendered, "must be at most 200 characters long");

    // A placeholder with no argument stays visible instead of vanishing.
    let rendered = render(&config.messages.validation.title_too_long, &[]);
    assert_eq!(rendered, "must be at most {} characters long");
}

#[test]
fn an_environment_variable_overrides_the_file() {
    let config = loader_with_secrets(&[("APP_MAX_BODY_BYTES", "2048")])
        .load()
        .expect("the configuration must be valid");

    assert_eq!(config.app.limits.max_body_bytes, 2048);
    // The values that were not overridden keep the file value.
    assert_eq!(config.api.pagination.default_limit, 20);
}

#[test]
fn a_missing_required_secret_is_an_error() {
    let error = loader_with(&[("DATABASE_URL", TEST_DATABASE_URL), ("APP_JWT_SECRET", "")])
        .load()
        .expect_err("the signing secret is required");

    match error {
        ConfigError::Missing(ref key) => assert_eq!(key, "APP_JWT_SECRET"),
        other => panic!("expected a missing-value error, got {other:?}"),
    }
}

#[test]
fn a_missing_database_url_is_an_error() {
    let error = loader_with(&[("APP_JWT_SECRET", TEST_SECRET)])
        .load()
        .expect_err("the database URL is required");

    match error {
        ConfigError::Missing(ref key) => assert_eq!(key, "DATABASE_URL"),
        other => panic!("expected a missing-value error, got {other:?}"),
    }
}

#[test]
fn a_value_that_is_not_a_number_is_an_error() {
    let error = loader_with_secrets(&[("APP_MAX_BODY_BYTES", "not-a-number")])
        .load()
        .expect_err("the value must be a number");

    assert_invalid(error, "APP_MAX_BODY_BYTES");
}

#[test]
fn a_value_that_is_not_a_boolean_is_an_error() {
    let error = loader_with_secrets(&[("APP_API_DOCS_ENABLED", "maybe")])
        .load()
        .expect_err("the value must be a boolean");

    assert_invalid(error, "APP_API_DOCS_ENABLED");
}

#[test]
fn a_file_on_disk_overrides_the_embedded_copy() {
    let directory = temporary_directory();
    fs::write(
        directory.join("app.toml"),
        "[app]\nname = \"ATOM on disk\"\n",
    )
    .expect("the file must be writable");

    let loader = loader_with_files(
        &directory,
        &[
            ("DATABASE_URL", TEST_DATABASE_URL),
            ("APP_JWT_SECRET", TEST_SECRET),
        ],
    );

    let config = loader.load().expect("the configuration must be valid");
    // The key the file mentions takes the file's value.
    assert_eq!(config.app.name, "ATOM on disk");
    // The keys the file does not mention keep the embedded value.
    assert_eq!(config.app.limits.max_body_bytes, 1024 * 1024);
    // The environment overlay still wins over the base layer, including over an
    // on-disk copy of the base file. The precedence is base, overlay,
    // environment variables.
    assert_eq!(
        config.app.log_filter,
        "atom_api=debug,tower_http=debug,sqlx=warn"
    );

    remove_directory(&directory);
}

#[test]
fn the_environment_overlay_overrides_the_base_file() {
    let directory = temporary_directory();
    fs::create_dir_all(directory.join("environments")).expect("the directory must be creatable");
    fs::write(
        directory.join("environments/production.toml"),
        "[app]\nlog_filter = \"error\"\n",
    )
    .expect("the file must be writable");

    let loader = loader_with_files(
        &directory,
        &[
            ("APP_ENV", "production"),
            ("DATABASE_URL", TEST_DATABASE_URL),
            ("APP_JWT_SECRET", TEST_SECRET),
        ],
    );

    let config = loader.load().expect("the configuration must be valid");
    assert_eq!(config.environment, Environment::Production);
    assert_eq!(config.app.log_filter, "error");
    // The production overlay also disables the API explorer.
    assert!(!config.api.docs_enabled);

    remove_directory(&directory);
}

#[test]
fn an_environment_variable_wins_over_the_environment_overlay() {
    let directory = temporary_directory();
    fs::create_dir_all(directory.join("environments")).expect("the directory must be creatable");
    fs::write(
        directory.join("environments/production.toml"),
        "[app]\nlog_filter = \"error\"\n",
    )
    .expect("the file must be writable");

    let loader = loader_with_files(
        &directory,
        &[
            ("APP_ENV", "production"),
            ("APP_LOG_FILTER", "trace"),
            ("DATABASE_URL", TEST_DATABASE_URL),
            ("APP_JWT_SECRET", TEST_SECRET),
        ],
    );

    let config = loader.load().expect("the configuration must be valid");
    assert_eq!(config.app.log_filter, "trace");

    remove_directory(&directory);
}

#[test]
fn a_malformed_file_is_a_parse_error() {
    let directory = temporary_directory();
    fs::write(directory.join("app.toml"), "[app\nname = ").expect("the file must be writable");

    let loader = loader_with_files(
        &directory,
        &[
            ("DATABASE_URL", TEST_DATABASE_URL),
            ("APP_JWT_SECRET", TEST_SECRET),
        ],
    );

    let error = loader.load().expect_err("the file must not parse");
    assert!(matches!(error, ConfigError::Parse { .. }));

    remove_directory(&directory);
}

#[test]
fn an_unknown_key_is_a_parse_error() {
    let directory = temporary_directory();
    fs::write(directory.join("app.toml"), "[app]\nnot_a_setting = 1\n")
        .expect("the file must be writable");

    let loader = loader_with_files(
        &directory,
        &[
            ("DATABASE_URL", TEST_DATABASE_URL),
            ("APP_JWT_SECRET", TEST_SECRET),
        ],
    );

    let error = loader.load().expect_err("the key must be refused");
    assert!(matches!(error, ConfigError::Parse { .. }));

    remove_directory(&directory);
}

#[test]
fn a_zero_rate_limit_is_refused() {
    let error = loader_with_secrets(&[("APP_AUTH_RATE_LIMIT_MAX_REQUESTS", "0")])
        .load()
        .expect_err("a zero rate limit is refused");

    assert_invalid(error, "api.rate_limit.max_requests");
}

#[test]
fn a_wildcard_cors_origin_is_refused() {
    let error = loader_with_secrets(&[("APP_CORS_ALLOWED_ORIGINS", "*")])
        .load()
        .expect_err("a wildcard origin is refused");

    assert_invalid(error, "api.cors.allowed_origins");
}

#[test]
fn a_short_secret_is_refused() {
    let error = loader_with(&[
        ("DATABASE_URL", TEST_DATABASE_URL),
        ("APP_JWT_SECRET", "too-short"),
    ])
    .load()
    .expect_err("a short secret is refused");

    assert_invalid(error, "APP_JWT_SECRET");
}

#[test]
fn the_placeholder_secret_is_refused_in_production_only() {
    let placeholder = "replace-me-with-a-development-only-secret-at-least-32-bytes-long";

    // Development accepts the documented placeholder, so a contributor can
    // start without generating a secret.
    let config = loader_with_secrets(&[("APP_JWT_SECRET", placeholder)])
        .load()
        .expect("the placeholder is allowed in development");
    assert_eq!(config.environment, Environment::Development);

    // Production refuses it.
    let error = loader_with_secrets(&[("APP_ENV", "production"), ("APP_JWT_SECRET", placeholder)])
        .load()
        .expect_err("the placeholder is refused in production");
    assert!(matches!(error, ConfigError::Invalid { .. }));
}

#[test]
fn a_refresh_lifetime_that_is_not_longer_is_refused() {
    let error = loader_with_secrets(&[
        ("APP_ACCESS_TOKEN_TTL_SECONDS", "900"),
        ("APP_REFRESH_TOKEN_TTL_SECONDS", "900"),
    ])
    .load()
    .expect_err("the refresh lifetime must be longer");

    assert_invalid(error, "auth.refresh_token_ttl");
}

#[test]
fn an_argon2_cost_below_the_floor_is_refused() {
    let error = loader_with_secrets(&[("APP_ARGON2_MEMORY_KIB", "1024")])
        .load()
        .expect_err("the memory floor is enforced");

    assert_invalid(error, "auth.argon2.memory_kib");
}

#[test]
fn a_zero_argon2_iteration_count_is_refused() {
    let error = loader_with_secrets(&[("APP_ARGON2_ITERATIONS", "0")])
        .load()
        .expect_err("a zero iteration count is refused");

    assert_invalid(error, "auth.argon2.iterations");
}

#[test]
fn an_empty_issuer_is_refused() {
    // An empty environment variable counts as unset, so the only way to reach
    // an empty issuer is a configuration file.
    let directory = temporary_directory();
    fs::write(directory.join("auth.toml"), "[auth]\nissuer = \"\"\n")
        .expect("the file must be writable");

    let loader = loader_with_files(
        &directory,
        &[
            ("DATABASE_URL", TEST_DATABASE_URL),
            ("APP_JWT_SECRET", TEST_SECRET),
        ],
    );

    let error = loader.load().expect_err("an empty issuer is refused");
    assert_invalid(error, "auth.issuer");

    remove_directory(&directory);
}

#[test]
fn an_invalid_log_filter_is_refused() {
    let error = loader_with_secrets(&[("APP_LOG_FILTER", "atom_api=not_a_level")])
        .load()
        .expect_err("an invalid filter is refused");

    assert_invalid(error, "app.log_filter");
}

#[test]
fn a_page_maximum_below_the_default_is_refused() {
    let directory = temporary_directory();
    fs::write(
        directory.join("api.toml"),
        "[api.pagination]\ndefault_limit = 50\nmax_limit = 10\n",
    )
    .expect("the file must be writable");

    let loader = loader_with_files(
        &directory,
        &[
            ("DATABASE_URL", TEST_DATABASE_URL),
            ("APP_JWT_SECRET", TEST_SECRET),
        ],
    );

    let error = loader
        .load()
        .expect_err("the maximum must not be below the default");
    assert_invalid(error, "api.pagination.max_limit");

    remove_directory(&directory);
}

#[test]
fn the_environment_names_are_parsed() {
    assert_eq!(
        Environment::parse("development").unwrap(),
        Environment::Development
    );
    assert_eq!(Environment::parse("dev").unwrap(), Environment::Development);
    assert_eq!(
        Environment::parse("local").unwrap(),
        Environment::Development
    );
    assert_eq!(
        Environment::parse("PRODUCTION").unwrap(),
        Environment::Production
    );
    assert_eq!(
        Environment::parse(" prod ").unwrap(),
        Environment::Production
    );
    assert!(Environment::parse("staging").is_err());
    assert!(Environment::parse("").is_err());
}

#[test]
fn the_secret_is_never_rendered() {
    let config = Config::defaults().expect("the base configuration must be valid");

    let auth = format!("{:?}", config.auth);
    assert!(
        auth.contains("[REDACTED]"),
        "the secret must be redacted: {auth}"
    );

    let database = format!("{:?}", config.database);
    assert!(
        database.contains("[REDACTED]"),
        "the connection URL must be redacted: {database}"
    );
}

#[test]
fn the_configuration_directory_must_exist_when_it_is_named() {
    let map: BTreeMap<String, String> = BTreeMap::from([(
        CONFIG_DIR_VAR.to_string(),
        "/a/directory/that/does/not/exist".to_string(),
    )]);
    let source: EnvironmentSource = Arc::new(move |key: &str| map.get(key).cloned());

    let error = super::loader::discover_directory(&source)
        .expect_err("a named directory that does not exist is an error");

    assert_invalid(error, CONFIG_DIR_VAR);
}
