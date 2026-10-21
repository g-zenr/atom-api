//! ATOM API — process entry point.
//!
//! Startup order matters:
//! 1. Load `.env` (a development convenience; production uses real env vars).
//! 2. Read and validate the configuration. Invalid configuration stops the
//!    process before it binds a port.
//! 3. Connect to PostgreSQL and apply pending migrations.
//! 4. Bind the socket and serve.
//!
//! Every failure is reported on stderr with a non-zero exit code. The process
//! never starts in a partially configured state.

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

    let config = Config::load()?;
    telemetry::init(&config.app.log_filter);

    tracing::info!(
        bind_addr = %config.app.bind_addr,
        environment = ?config.environment,
        "starting ATOM API"
    );

    // SQLx 0.8 has no connect timeout of its own, so the bound is applied
    // here: without it an unreachable database hangs the process before it
    // ever binds a port.
    let connect_timeout = config.database.connect_timeout;
    let pool = tokio::time::timeout(
        connect_timeout,
        PgPoolOptions::new()
            .max_connections(config.database.max_connections)
            .acquire_timeout(config.database.acquire_timeout)
            .connect(&config.database.url),
    )
    .await
    .map_err(|_| {
        Box::<dyn std::error::Error>::from(format!(
            "the database did not answer within {connect_timeout:?}"
        ))
    })??;

    // Migrations are embedded in the binary, so a deployed instance never
    // depends on files that ship next to it.
    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("database migrations are up to date");

    let bind_addr = config.app.bind_addr;
    let state = AppState::new(config, pool)?;

    // Prune expired refresh sessions in the background, so that the table
    // cannot grow without bound: every login and every rotation writes a row.
    tokio::spawn(prune_expired_sessions(state.clone()));

    let listener = TcpListener::bind(bind_addr).await?;
    tracing::info!(address = %bind_addr, "ATOM API is listening");

    // `into_make_service_with_connect_info` puts the peer address into the
    // request extensions. The `ClientIp` extractor reads it from there, and
    // the authentication rate limiter is keyed by that address. Without this
    // wiring every request would share a single rate-limit bucket.
    axum::serve(
        listener,
        api::router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    tracing::info!("ATOM API stopped");
    Ok(())
}

/// Deletes refresh sessions that expired more than a day ago.
async fn prune_expired_sessions(state: AppState) {
    let interval = state.config.app.sessions.prune_interval;
    let grace = state.config.app.sessions.prune_grace;

    let mut ticker = tokio::time::interval(interval);
    // The first tick completes immediately; skip it so that start-up does not
    // run a query before the server is ready.
    ticker.tick().await;

    loop {
        ticker.tick().await;

        let grace = chrono::Duration::from_std(grace).unwrap_or(chrono::Duration::days(1));
        match state.prune_expired_sessions(grace).await {
            Ok(removed) if removed > 0 => {
                tracing::info!(removed, "pruned expired refresh sessions");
            }
            Ok(_) => {}
            Err(error) => {
                // The driver message can contain a row, so only the SQLSTATE
                // is logged.
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
