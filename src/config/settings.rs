//! The typed settings structures.
//!
//! Every structure here is deserialised from one of the TOML files in
//! `config/`. The field names are the TOML keys, and the structures use
//! `deny_unknown_fields`, so a typo in a configuration file is a start-up
//! error rather than a silently ignored setting.

use std::net::SocketAddr;
use std::time::Duration;

use serde::Deserialize;

use crate::config::messages::Messages;

/// Deserialises a number of seconds into a [`Duration`].
///
/// The configuration files express every duration in seconds, because a bare
/// integer is the only thing a TOML reader can be relied upon to parse
/// unambiguously. The typed settings keep a `Duration`, so the conversion
/// happens in exactly one place.
fn seconds<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let seconds = u64::deserialize(deserializer)?;
    Ok(Duration::from_secs(seconds))
}

/// The settings that are not stored in a file.
///
/// The two secrets are deliberately absent from the TOML files: a signing
/// secret and a database URL usually contain credentials, and the
/// configuration directory is committed.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// The application settings.
    pub app: AppSettings,
    /// The API settings.
    pub api: ApiSettings,
    /// The authentication settings.
    pub auth: AuthSettings,
    /// The database settings.
    pub database: DatabaseSettings,
    /// The client-facing message catalogue.
    pub messages: Messages,
}

/// Application-level settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppSettings {
    /// The product name, used in the OpenAPI document and the start-up log.
    #[serde(default = "default_app_name")]
    pub name: String,
    /// The socket the HTTP server binds to.
    #[serde(default = "default_bind_addr")]
    pub bind_addr: SocketAddr,
    /// The `tracing` filter directive.
    #[serde(default = "default_log_filter")]
    pub log_filter: String,
    /// The request limits.
    #[serde(default)]
    pub limits: RequestLimits,
    /// The refresh-session housekeeping settings.
    #[serde(default)]
    pub sessions: SessionSettings,
}

/// The limits applied to an in-flight request.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestLimits {
    /// The largest accepted request body, in bytes.
    #[serde(default = "default_max_body_bytes")]
    pub max_body_bytes: usize,
    /// How long a request may take before it is abandoned.
    #[serde(
        default = "default_request_timeout",
        deserialize_with = "seconds",
        rename = "request_timeout_seconds"
    )]
    pub request_timeout: Duration,
    /// How long the readiness probe waits for the database.
    #[serde(
        default = "default_readiness_timeout",
        deserialize_with = "seconds",
        rename = "readiness_probe_timeout_seconds"
    )]
    pub readiness_probe_timeout: Duration,
}

impl Default for RequestLimits {
    fn default() -> Self {
        Self {
            max_body_bytes: default_max_body_bytes(),
            request_timeout: default_request_timeout(),
            readiness_probe_timeout: default_readiness_timeout(),
        }
    }
}

/// The housekeeping of expired refresh sessions.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionSettings {
    /// How often the pruning task runs.
    #[serde(
        default = "default_prune_interval",
        deserialize_with = "seconds",
        rename = "prune_interval_seconds"
    )]
    pub prune_interval: Duration,
    /// How long an expired session is kept.
    ///
    /// This is a security window, not a tuning knob: a revoked session has to
    /// survive long enough that a replayed token is still recognised as reuse.
    #[serde(
        default = "default_prune_grace",
        deserialize_with = "seconds",
        rename = "prune_grace_seconds"
    )]
    pub prune_grace: Duration,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            prune_interval: default_prune_interval(),
            prune_grace: default_prune_grace(),
        }
    }
}

/// API-level settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiSettings {
    /// The title of the generated OpenAPI document.
    #[serde(default = "default_api_title")]
    pub title: String,
    /// Whether the OpenAPI document and the API explorer are served.
    #[serde(default = "default_true")]
    pub docs_enabled: bool,
    /// The cross-origin settings.
    #[serde(default)]
    pub cors: CorsSettings,
    /// The pagination settings.
    #[serde(default)]
    pub pagination: PaginationSettings,
    /// The rate-limit settings for the authentication endpoints.
    #[serde(default)]
    pub rate_limit: RateLimitSettings,
}

/// The cross-origin settings.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorsSettings {
    /// The origins allowed to make cross-origin requests.
    #[serde(default)]
    pub allowed_origins: Vec<String>,
}

/// The pagination settings.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaginationSettings {
    /// The page size used when a caller does not ask for one.
    #[serde(default = "default_page_size")]
    pub default_limit: i64,
    /// The largest page a caller may ask for.
    #[serde(default = "default_max_page_size")]
    pub max_limit: i64,
}

/// The rate-limit settings.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitSettings {
    /// How many attempts one client may make per window.
    #[serde(default = "default_rate_limit_requests")]
    pub max_requests: u32,
    /// The length of the window.
    #[serde(
        default = "default_rate_limit_window",
        deserialize_with = "seconds",
        rename = "window_seconds"
    )]
    pub window: Duration,
}

/// Authentication settings.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthSettings {
    /// The HMAC signing secret.
    ///
    /// It is never written to a configuration file. The field is empty until
    /// the loader reads `APP_JWT_SECRET`, and validation refuses an empty
    /// value.
    #[serde(default)]
    pub jwt_secret: String,
    /// The lifetime of an access token.
    #[serde(
        default = "default_access_token_ttl",
        deserialize_with = "seconds",
        rename = "access_token_ttl_seconds"
    )]
    pub access_token_ttl: Duration,
    /// The absolute lifetime of a refresh token.
    #[serde(
        default = "default_refresh_token_ttl",
        deserialize_with = "seconds",
        rename = "refresh_token_ttl_seconds"
    )]
    pub refresh_token_ttl: Duration,
    /// The `iss` claim.
    #[serde(default = "default_issuer")]
    pub issuer: String,
    /// The `aud` claim.
    #[serde(default = "default_audience")]
    pub audience: String,
    /// The clock-skew allowance applied when validating an access token.
    #[serde(
        default = "default_clock_skew",
        deserialize_with = "seconds",
        rename = "clock_skew_seconds"
    )]
    pub clock_skew: Duration,
    /// The Argon2id cost parameters.
    #[serde(default)]
    pub argon2: Argon2Params,
    /// The password policy.
    #[serde(default)]
    pub password: PasswordPolicy,
}

// The signing secret must never reach log output.
impl std::fmt::Debug for AuthSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthSettings")
            .field("jwt_secret", &"[REDACTED]")
            .field("access_token_ttl", &self.access_token_ttl)
            .field("refresh_token_ttl", &self.refresh_token_ttl)
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("clock_skew", &self.clock_skew)
            .field("argon2", &self.argon2)
            .field("password", &self.password)
            .finish()
    }
}

/// The Argon2id cost parameters.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Argon2Params {
    /// The memory cost, in KiB.
    #[serde(default = "default_argon2_memory")]
    pub memory_kib: u32,
    /// The iteration count.
    #[serde(default = "default_argon2_iterations")]
    pub iterations: u32,
    /// The degree of parallelism.
    #[serde(default = "default_argon2_parallelism")]
    pub parallelism: u32,
}

impl Default for Argon2Params {
    fn default() -> Self {
        Self {
            memory_kib: default_argon2_memory(),
            iterations: default_argon2_iterations(),
            parallelism: default_argon2_parallelism(),
        }
    }
}

/// The password policy.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasswordPolicy {
    /// The shortest accepted password.
    #[serde(default = "default_password_min_length")]
    pub min_length: usize,
    /// The longest accepted password.
    #[serde(default = "default_password_max_length")]
    pub max_length: usize,
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            min_length: default_password_min_length(),
            max_length: default_password_max_length(),
        }
    }
}

/// Database settings.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseSettings {
    /// The PostgreSQL connection URL.
    ///
    /// It usually contains a password, so it is never written to a
    /// configuration file. The field is empty until the loader reads
    /// `DATABASE_URL`, and validation refuses an empty value.
    #[serde(default)]
    pub url: String,
    /// The largest number of pooled connections.
    #[serde(default = "default_pool_size")]
    pub max_connections: u32,
    /// How long a caller waits for a connection from the pool.
    #[serde(
        default = "default_acquire_timeout",
        deserialize_with = "seconds",
        rename = "acquire_timeout_seconds"
    )]
    pub acquire_timeout: Duration,
    /// How long the initial connection may take.
    #[serde(
        default = "default_connect_timeout",
        deserialize_with = "seconds",
        rename = "connect_timeout_seconds"
    )]
    pub connect_timeout: Duration,
}

// The connection URL can contain a password, so it is redacted in logs.
impl std::fmt::Debug for DatabaseSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatabaseSettings")
            .field("url", &"[REDACTED]")
            .field("max_connections", &self.max_connections)
            .field("acquire_timeout", &self.acquire_timeout)
            .field("connect_timeout", &self.connect_timeout)
            .finish()
    }
}

fn default_app_name() -> String {
    "ATOM API".to_string()
}

fn default_bind_addr() -> SocketAddr {
    "127.0.0.1:3000"
        .parse()
        .expect("the default address is valid")
}

fn default_log_filter() -> String {
    "info".to_string()
}

fn default_true() -> bool {
    true
}

fn default_api_title() -> String {
    "ATOM API".to_string()
}

fn default_max_body_bytes() -> usize {
    1024 * 1024
}

fn default_request_timeout() -> Duration {
    Duration::from_secs(30)
}

fn default_readiness_timeout() -> Duration {
    Duration::from_secs(1)
}

fn default_prune_interval() -> Duration {
    Duration::from_secs(3600)
}

fn default_prune_grace() -> Duration {
    Duration::from_secs(86_400)
}

fn default_page_size() -> i64 {
    20
}

fn default_max_page_size() -> i64 {
    100
}

fn default_rate_limit_requests() -> u32 {
    10
}

fn default_rate_limit_window() -> Duration {
    Duration::from_secs(60)
}

fn default_access_token_ttl() -> Duration {
    Duration::from_secs(900)
}

fn default_refresh_token_ttl() -> Duration {
    Duration::from_secs(1_209_600)
}

fn default_issuer() -> String {
    "atom-api".to_string()
}

fn default_audience() -> String {
    "atom-api".to_string()
}

fn default_clock_skew() -> Duration {
    Duration::from_secs(30)
}

fn default_argon2_memory() -> u32 {
    19_456
}

fn default_argon2_iterations() -> u32 {
    2
}

fn default_argon2_parallelism() -> u32 {
    1
}

fn default_password_min_length() -> usize {
    12
}

fn default_password_max_length() -> usize {
    256
}

fn default_pool_size() -> u32 {
    10
}

fn default_acquire_timeout() -> Duration {
    Duration::from_secs(5)
}

fn default_connect_timeout() -> Duration {
    Duration::from_secs(10)
}
