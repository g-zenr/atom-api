//! Behaviour tests for the expired-session pruning.
//!
//! `src/main.rs` runs `prune_expired_sessions` on a ticker; the loop itself is
//! runtime machinery — a spawn and an interval — and is not tested here. What
//! the product promises is the statement underneath it: a refresh session that
//! expired more than the configured grace ago is deleted, a session that
//! expired inside the grace survives, and a live session is never touched. The
//! grace is a security window, not a tuning knob: a revoked session has to
//! survive long enough that a replayed token is still recognised as reuse.
//!
//! The grace is a parameter of the service call, so the test drives the call
//! directly — once, as one pruning pass would — with a grace large enough that
//! the three fixtures sit far apart on the clock. That keeps the test
//! deterministic without sleeping.

pub mod common;

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Inserts an account and returns its id.
async fn insert_account(pool: &PgPool, prefix: &str) -> Uuid {
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO users (id, email, password_hash, display_name) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(common::unique_email(prefix))
    .bind("prune-fixture-not-a-real-hash")
    .bind("Prune Fixture")
    .execute(pool)
    .await
    .expect("the fixture account must be inserted");

    id
}

/// Inserts one refresh session and returns its id.
///
/// The stored hash is the real SHA-256 of a unique token string, so the row has
/// the shape the repository writes and cannot collide with another row: the
/// table holds a unique index on the hash.
async fn insert_session(
    pool: &PgPool,
    user_id: Uuid,
    token: &str,
    expires_at: DateTime<Utc>,
) -> Uuid {
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO refresh_sessions (id, user_id, token_hash, expires_at)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(user_id)
    .bind(common::hash_of(token))
    .bind(expires_at)
    .execute(pool)
    .await
    .expect("the fixture session must be inserted");

    id
}

/// Whether a session row still exists.
async fn session_exists(pool: &PgPool, id: Uuid) -> bool {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM refresh_sessions WHERE id = $1)")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("the session must be lookable")
}

#[tokio::test]
async fn the_prune_removes_only_sessions_that_expired_beyond_the_grace() {
    let state = common::test_state().await;
    let owner = insert_account(&state.db, "prune-owner").await;

    let now = Utc::now();
    // The same grace `main` computes from the configuration before every pass.
    // Forty-eight hours puts each fixture a full day away from the cutoff, so
    // the time a test takes to run cannot move a row across it.
    let grace = Duration::hours(48);

    // Expired well beyond the grace: the reuse-detection window has closed,
    // and this is exactly the row the pruning exists to remove.
    let beyond = insert_session(
        &state.db,
        owner,
        &format!("prune-beyond-{}", Uuid::new_v4()),
        now - grace - Duration::hours(24),
    )
    .await;

    // Expired, but inside the grace: still recognisable as reuse on replay, so
    // it must survive every pass.
    let within = insert_session(
        &state.db,
        owner,
        &format!("prune-within-{}", Uuid::new_v4()),
        now - grace / 2,
    )
    .await;

    // Still live: never a prune candidate.
    let active = insert_session(
        &state.db,
        owner,
        &format!("prune-active-{}", Uuid::new_v4()),
        now + Duration::days(7),
    )
    .await;

    let removed = state
        .auth
        .prune_expired_sessions(grace)
        .await
        .expect("the prune must run");

    // The statement prunes the table, not one owner's rows, so the count may
    // include rows this test did not create — but this test's own expired
    // session must be among them.
    assert!(
        removed >= 1,
        "the session that expired beyond the grace must be among the removed rows"
    );

    assert!(
        !session_exists(&state.db, beyond).await,
        "a session that expired beyond the grace must be pruned"
    );
    assert!(
        session_exists(&state.db, within).await,
        "a session that expired inside the grace must survive the prune"
    );
    assert!(
        session_exists(&state.db, active).await,
        "a live session must survive the prune"
    );

    // Removing the fixture account takes its remaining sessions with it, so
    // the shared database is left as this test found it.
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(owner)
        .execute(&state.db)
        .await
        .expect("the fixture account must be removable");
}
