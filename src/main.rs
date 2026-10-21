//! ATOM API — process entry point.
//!
//! Startup order matters:
//! 1. Load `.env` (development convenience only; production uses real env vars).
//! 2. Read and validate configuration. Invalid configuration stops the process
//!    before it binds a port.
//! 3. Connect to PostgreSQL and apply pending migrations.
//! 4. Bind the socket and serve.
//!
//! Every failure is reported on stderr with a non-zero exit code. The process
//! never starts in a partially configured state.

use std::net::SocketAddr;
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;

use atom_api::api;
use atom_api::config::Config;
use atom_api::state::AppState;
use atom_api::telemetry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // `.env` is optional; a missing file is not an error.
    dotenvy::dotenv().ok();

    let config = Config::from_env()?;
    telemetry::init(&config.log_filter);

    tracing::info!(
        bind_addr = %config.bind_addr,
        environment = ?config.environment,
        "starting ATOM API"
    );

    let pool = PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .acquire_timeout(config.database.acquire_timeout)
        .connect(&config.database.url)
        .await?;

    // Migrations are embedded in the binary, so a deployed instance never
    // depends on files that ship next to it.
    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("database migrations are up to date");

    let state = AppState::new(config.clone(), pool);

    // Prune expired refresh sessions in the background, so that the table
    // cannot grow without bound: every login and every rotation writes a row.
    tokio::spawn(prune_expired_sessions(state.clone()));

    let listener = TcpListener::bind(config.bind_addr).await?;
    tracing::info!(address = %config.bind_addr, "ATOM API is listening");

    // `into_make_service_with_connect_info` puts the peer address into the
    // request extensions. The `ClientIp` extractor reads it from there, and
    // the authentication rate limiter is keyed by that address. Without this
    // wiring every request would share a single rate-limit bucket.
    axum::serve(
        listener,
        api::router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    tracing::info!("ATOM API stopped");
    Ok(())
}

/// How often expired refresh sessions are pruned.
const SESSION_PRUNE_INTERVAL: Duration = Duration::from_secs(3600);

/// How long an expired session is kept before it is deleted.
const SESSION_PRUNE_GRACE: Duration = Duration::from_secs(86400);

/// Deletes refresh sessions that expired more than a day ago.
///
/// A revoked session is kept for a while so that a replayed token is still
/// recognised as a reuse rather than as an unknown token.
async fn prune_expired_sessions(state: AppState) {
    let mut ticker = tokio::time::interval(SESSION_PRUNE_INTERVAL);
    // The first tick completes immediately; skip it so that start-up does not
    // run a query before the server is ready.
    ticker.tick().await;

    loop {
        ticker.tick().await;

        let grace =
            chrono::Duration::from_std(SESSION_PRUNE_GRACE).unwrap_or(chrono::Duration::days(1));
        match state.prune_expired_sessions(grace).await {
            Ok(removed) if removed > 0 => {
                tracing::info!(removed, "pruned expired refresh sessions");
            }
            Ok(_) => {}
            Err(error) => {
                // The driver message can contain a row, so only the SQLSTATE
                // and the kind are logged.
                tracing::warn!(
                    sqlstate = ?error.as_database_error().and_then(|db| db.code()),
                    "could not prune expired refresh sessions"
                );
            }
        }
    }
}

/// Resolves when the process receives Ctrl-C or a termination signal.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(error) => tracing::warn!(%error, "cannot install SIGTERM handler"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("received Ctrl-C, shutting down"),
        _ = terminate => tracing::info!("received termination signal, shutting down"),
    }

    // Give in-flight requests a bounded amount of time to finish.
    tokio::time::sleep(Duration::from_millis(100)).await;
}
