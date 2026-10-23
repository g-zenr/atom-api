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

use std::future::IntoFuture;
use std::process::ExitCode;

use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;

use atom_api::api;
use atom_api::config::Config;
use atom_api::error::AppError;
use atom_api::state::AppState;
use atom_api::telemetry;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // The readable message, not the debug form: the operator has to
            // be able to act on it.
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Starts the server.
///
/// # Errors
///
/// Returns the error that stopped the process. Every failure is a start-up
/// failure: an invalid configuration, an unreachable database, a migration
/// that cannot be applied, or a socket that cannot be bound.
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    // `.env` is optional; a missing file is not an error.
    dotenvy::dotenv().ok();

    let config = Config::load()?;
    telemetry::init(&config.app.log_filter, config.environment)?;

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
    .map_err(|_| -> Box<dyn std::error::Error> {
        format!("the database did not answer within {connect_timeout:?}").into()
    })?
    .map_err(|error| -> Box<dyn std::error::Error> {
        format!(
            "could not connect to the database: {}",
            describe_database_error(&error)
        )
        .into()
    })?;

    // Migrations are embedded in the binary, so a deployed instance never
    // depends on files that ship next to it.
    sqlx::migrate!("./migrations").run(&pool).await.map_err(
        |error| -> Box<dyn std::error::Error> {
            format!(
                "could not apply the migrations: {}",
                describe_migration_error(&error)
            )
            .into()
        },
    )?;
    tracing::info!("database migrations are up to date");

    let bind_addr = config.app.bind_addr;
    let shutdown_grace = config.app.limits.shutdown_grace;
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
    let server = axum::serve(
        listener,
        api::router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .into_future();

    tokio::pin!(server);

    // The signal future above makes the server stop accepting and start
    // draining. This bounds that drain: a request that never finishes would
    // otherwise keep the process alive indefinitely. The second subscription
    // observes the same OS signal, so the clock starts when the drain does.
    let drain_deadline = async move {
        wait_for_signal().await;
        tokio::time::sleep(shutdown_grace).await;
    };

    tokio::select! {
        result = &mut server => result?,
        () = drain_deadline => {
            tracing::warn!(
                grace_seconds = shutdown_grace.as_secs(),
                "the shutdown grace elapsed with requests still in flight"
            );
        }
    }

    tracing::info!("ATOM API stopped");
    Ok(())
}

/// Describes a database failure without revealing the driver message.
///
/// The driver message can contain the connection string or a row, so only the
/// error kind and the SQLSTATE are reported. This is the same rule the
/// request path applies in `src/error.rs`.
fn describe_database_error(error: &sqlx::Error) -> String {
    match error.as_database_error() {
        Some(database) => match database.code() {
            Some(code) => format!("{:?} (SQLSTATE {code})", database.kind()),
            None => format!("{:?}", database.kind()),
        },
        // Every other variant carries a message the driver wrote, and a
        // configuration error can carry the connection string, so the
        // message is not repeated.
        None => match error {
            sqlx::Error::Io(inner) => format!("the connection could not be established: {inner}"),
            _ => "the connection could not be established".to_string(),
        },
    }
}

/// Describes a migration failure without revealing the driver message.
fn describe_migration_error(error: &sqlx::migrate::MigrateError) -> String {
    match error {
        sqlx::migrate::MigrateError::Execute(inner) => describe_database_error(inner),
        sqlx::migrate::MigrateError::ExecuteMigration(inner, version) => format!(
            "migration {version} failed: {}",
            describe_database_error(inner)
        ),
        sqlx::migrate::MigrateError::Source(_) => "the migration source failed".to_string(),
        other => other.to_string(),
    }
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

        // The configured grace is bounded by the configuration validation, so
        // the conversion cannot fail. If it ever did, a zero-length grace
        // prunes more, never less.
        let grace = chrono::Duration::from_std(grace).unwrap_or_default();
        match state.auth.prune_expired_sessions(grace).await {
            Ok(removed) if removed > 0 => {
                tracing::info!(removed, "pruned expired refresh sessions");
            }
            Ok(_) => {}
            Err(AppError::Database(error)) => {
                // The driver message can contain a row, so only the SQLSTATE
                // is logged.
                tracing::warn!(
                    sqlstate = ?error.as_database_error().and_then(|db| db.code()),
                    "could not prune expired refresh sessions"
                );
            }
            Err(error) => {
                tracing::warn!(error = ?error, "could not prune expired refresh sessions");
            }
        }
    }
}

/// Resolves when the process receives Ctrl-C or a termination signal.
///
/// It resolves as soon as the signal arrives, and does not wait: the future
/// passed to `with_graceful_shutdown` is what *starts* the shutdown, so
/// sleeping here would keep the listener accepting new connections for the
/// whole grace period and only begin to drain afterwards.
async fn wait_for_signal() {
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
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

/// Resolves when the process receives a shutdown signal, and records it.
async fn shutdown_signal() {
    wait_for_signal().await;
    tracing::info!("shutdown signal received, draining in-flight requests");
}
