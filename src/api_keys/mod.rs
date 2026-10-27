//! The `api_keys` module: long-lived credentials for machine clients.
//!
//! A user JWT lives fifteen minutes and rotates its refresh token on every use,
//! which is the wrong shape for a workflow that runs unattended: a workflow
//! holding a refresh token would either lock the account out or break on
//! rotation. An API key is therefore a second, independent credential: a random
//! secret the account owner creates once, sends as `Authorization: Bearer …`,
//! and revokes when it is no longer needed.
//!
//! The module follows the Phase 1 layering:
//!
//! * `handlers` — HTTP extraction, status codes, response serialization.
//! * `service` — validation, the ownership mapping and the authentication rule.
//! * `repository` — the SQL; the only file in the module that holds it.
//! * `models` — row structs, including the caller a key resolves to.
//! * `dto` — the published request and response contracts.
//! * `secret` — the format of the secret, its hash and its stored prefix; it is
//!   the only file that knows how a secret is built.
//!
//! Three properties are carried by the structure rather than by a convention:
//!
//! * **The secret is returned once.** Only [`dto::ApiKeyCreatedResponse`] has a
//!   `key` field; every other type in the module is built from
//!   [`models::ApiKey`], which does not carry the hash, let alone the secret.
//! * **The hash is never selected into a response type.** `api_keys.key_hash`
//!   is read by exactly one statement, the one that resolves a presented
//!   credential, and it binds a hash, never returns one.
//! * **Every statement is owner-scoped.** `user_id` is a predicate of the
//!   listing, the revocation and the usage stamp, so a foreign key id is a
//!   `404` from the SQL and not from a comparison in the handler (`ADR-0005`).

pub mod dto;
pub mod handlers;
pub mod models;
pub mod repository;
pub mod secret;
pub mod service;

pub use dto::{ApiKeyCreatedResponse, ApiKeyPage, ApiKeyResponse, CreateApiKeyRequest};
pub use models::{ApiKey, KeyIdentity};
pub use repository::ApiKeyRepository;
pub use service::{ApiKeyService, KeyPrincipal, validate_name};
