//! Tracing / logging initialisation.
//!
//! Logs are structured. In development the human-readable formatter is used;
//! in production the JSON formatter is used so that log shippers can parse
//! fields without regular expressions.

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

/// Installs the global tracing subscriber.
///
/// # Panics
///
/// Panics when a subscriber is already installed, which can only happen if the
/// process initialises tracing twice. That is a programming error.
pub fn init(filter: &str) {
    let env_filter = EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("info"));

    let registry = tracing_subscriber::registry().with(env_filter);

    if cfg!(debug_assertions) {
        registry
            .with(
                fmt::layer()
                    .with_target(true)
                    .with_level(true)
                    .with_file(false)
                    .with_line_number(false),
            )
            .try_init()
            .expect("tracing subscriber must not already be installed");
    } else {
        registry
            .with(fmt::layer().json().flatten_event(true))
            .try_init()
            .expect("tracing subscriber must not already be installed");
    }
}
