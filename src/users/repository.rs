//! User persistence.
//!
//! Every query is parameterised. Email uniqueness is enforced by a database
//! index, so the repository maps the unique-violation error to a conflict
//! rather than performing a racy "check then insert".

use sqlx::PgPool;
use uuid::Uuid;

use crate::config::messages::Messages;
use crate::error::{AppError, AppResult, code};
use crate::users::models::User;

/// The SQLSTATE code PostgreSQL uses for a unique-constraint violation.
const UNIQUE_VIOLATION: &str = "23505";

/// The SQLSTATE code PostgreSQL uses for a check-constraint violation.
const CHECK_VIOLATION: &str = "23514";

/// Persistence operations for users.
#[derive(Debug, Clone)]
pub struct UserRepository {
    pool: PgPool,
}

impl UserRepository {
    /// Creates a repository bound to a connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Inserts a new user.
    ///
    /// The catalogue is a parameter because this method is the only layer
    /// that recognises the SQLSTATE codes, and the two failures it can tell
    /// apart have a client-facing wording.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] when the email is already registered and
    /// [`AppError::Database`] for any other database failure.
    pub async fn insert(
        &self,
        id: Uuid,
        email: &str,
        password_hash: &str,
        display_name: &str,
        messages: &Messages,
    ) -> AppResult<User> {
        let result = sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (id, email, password_hash, display_name)
            VALUES ($1, $2, $3, $4)
            RETURNING id, email, password_hash, display_name, is_active, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(email)
        .bind(password_hash)
        .bind(display_name)
        .fetch_one(&self.pool)
        .await;

        match result {
            Ok(user) => Ok(user),
            Err(sqlx::Error::Database(error))
                if error.code().as_deref() == Some(UNIQUE_VIOLATION) =>
            {
                Err(AppError::Conflict(
                    messages.auth.email_already_registered.clone(),
                ))
            }
            // A `CHECK` violation means the application and the database
            // disagree about a rule. It is reported as a validation failure,
            // never as a server error, so that the database message - which
            // can contain the failing row - is never logged.
            Err(sqlx::Error::Database(error))
                if error.code().as_deref() == Some(CHECK_VIOLATION) =>
            {
                Err(AppError::validation_with(
                    code::VALIDATION_FAILED,
                    messages.auth.database_constraint.clone(),
                ))
            }
            Err(error) => Err(AppError::Database(error)),
        }
    }

    /// Finds a user by id.
    pub async fn find_by_id(&self, id: Uuid) -> AppResult<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, password_hash, display_name, is_active, created_at, updated_at
            FROM users
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }

    /// Finds a user by normalized email address.
    pub async fn find_by_email(&self, email: &str) -> AppResult<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, password_hash, display_name, is_active, created_at, updated_at
            FROM users
            WHERE email = $1
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }
}
