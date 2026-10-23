//! Tracing / logging initialisation.
//!
//! Logs are structured. In production the JSON formatter is used so that log
//! shippers can parse fields without regular expressions; in development the
//! human-readable formatter is used.
//!
//! The formatter follows the **environment** the process runs in, not the
//! build profile: a release build that runs a development deployment gets
//! readable logs, and a debug build that runs a production deployment gets
//! JSON.

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

use crate::config::Environment;

/// Installs the global tracing subscriber.
///
/// # Errors
///
/// Returns an error when the filter is not a valid directive, or when a
/// subscriber is already installed. The filter was validated when the
/// configuration was loaded, so an invalid one is a start-up error and not a
/// silent fallback to `info`.
pub fn init(filter: &str, environment: Environment) -> Result<(), Box<dyn std::error::Error>> {
    let env_filter = EnvFilter::try_new(filter)?;
    let registry = tracing_subscriber::registry().with(env_filter);

    match environment {
        Environment::Production => registry
            .with(fmt::layer().json().flatten_event(true))
            .try_init()?,
        Environment::Development => registry
            .with(
                fmt::layer()
                    .with_target(true)
                    .with_level(true)
                    .with_file(false)
                    .with_line_number(false),
            )
            .try_init()?,
    }

    Ok(())
}
