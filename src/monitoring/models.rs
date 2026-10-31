//! Persistence models for the `monitoring` module.
//!
//! The shapes mirror `docs/database/schema.md` §12 column for column, so a row
//! read by a repository statement decodes without a projection that renames
//! anything.
//!
//! # Money
//!
//! Every money column is `NUMERIC(19,4)`. SQLx cannot decode `NUMERIC` with
//! this crate's feature set, and `ADR-0007` forbids a decimal crate, so the
//! repository selects the column as `amount::text` / `target_amount::text` and
//! the value is carried here as it arrives. `NUMERIC(19,4)::text` renders
//! exactly four decimal places, which is the published wire form. The type is
//! a plain `String` on purpose: nothing in Rust adds, rounds, parses or
//! compares money — the target comparison and the percentage are computed by
//! `SQL`, in the transaction that inserts the observation.
//!
//! # The controlled vocabularies
//!
//! Every controlled value in this chain is a `TEXT` column with a `CHECK`,
//! not a PostgreSQL enum type (`migrations/0008_price_monitoring.sql` header).
//! The Rust enums below are the typed half of the same rule: `as_str` is the
//! stored form, [`TryFrom`] is the decode, and the [`sqlx`] impls at the
//! bottom delegate to `String`, so a `FromRow` struct can carry the enum
//! directly and a bind can pass it as text.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

/// Whether a tracked product is being collected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProductStatus {
    /// The owner wants this product collected.
    Active,
    /// The owner stopped collection, keeping the product and its history.
    Paused,
}

impl ProductStatus {
    /// The stored value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
        }
    }
}

impl TryFrom<String> for ProductStatus {
    type Error = String;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        match raw.as_str() {
            "active" => Ok(Self::Active),
            "paused" => Ok(Self::Paused),
            other => Err(other.to_string()),
        }
    }
}

/// Whether a listing was purchasable when it was last collected.
///
/// Shared by `product_listings` (the latest report) and `price_observations`
/// (the report at the moment of collection); both columns carry the same
/// `CHECK`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ListingAvailability {
    /// Nothing has reported availability yet.
    Unknown,
    /// The source offered the product for purchase.
    Available,
    /// The source did not offer the product for purchase.
    Unavailable,
}

impl ListingAvailability {
    /// The stored value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Available => "available",
            Self::Unavailable => "unavailable",
        }
    }
}

impl TryFrom<String> for ListingAvailability {
    type Error = String;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        match raw.as_str() {
            "unknown" => Ok(Self::Unknown),
            "available" => Ok(Self::Available),
            "unavailable" => Ok(Self::Unavailable),
            other => Err(other.to_string()),
        }
    }
}

/// How much the listing itself is still trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ListingValidationState {
    /// The URL is believed to still describe the product.
    Valid,
    /// The page changed and the mapping is doubted.
    Stale,
    /// The mapping is known wrong.
    Rejected,
}

impl ListingValidationState {
    /// The stored value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Stale => "stale",
            Self::Rejected => "rejected",
        }
    }
}

impl TryFrom<String> for ListingValidationState {
    type Error = String;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        match raw.as_str() {
            "valid" => Ok(Self::Valid),
            "stale" => Ok(Self::Stale),
            "rejected" => Ok(Self::Rejected),
            other => Err(other.to_string()),
        }
    }
}

/// The lifecycle state of one automation run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    /// Started, not finished yet.
    Running,
    /// Finished; every collection step succeeded.
    Ok,
    /// Finished; at least one step failed and the rest succeeded.
    Partial,
    /// Finished; the run did not do its work.
    Failed,
}

impl RunOutcome {
    /// The stored value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Ok => "ok",
            Self::Partial => "partial",
            Self::Failed => "failed",
        }
    }

    /// Whether the value names a state a **finish** may set.
    ///
    /// `running` is what a create produces, not what a finish may write, so
    /// the finish request type refuses it before any statement runs.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }
}

impl TryFrom<String> for RunOutcome {
    type Error = String;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        match raw.as_str() {
            "running" => Ok(Self::Running),
            "ok" => Ok(Self::Ok),
            "partial" => Ok(Self::Partial),
            "failed" => Ok(Self::Failed),
            other => Err(other.to_string()),
        }
    }
}

/// How the owner wants to be told about an alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotifyVia {
    /// The ATOM inbox: the one channel the API owns today.
    Inbox,
}

impl NotifyVia {
    /// The stored value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
        }
    }
}

impl TryFrom<String> for NotifyVia {
    type Error = String;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        match raw.as_str() {
            "inbox" => Ok(Self::Inbox),
            other => Err(other.to_string()),
        }
    }
}

/// The delivery state of an alert's notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationStatus {
    /// The notification has not been confirmed by its channel.
    Pending,
    /// The notification was delivered.
    Delivered,
}

impl NotificationStatus {
    /// The stored value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Delivered => "delivered",
        }
    }
}

impl TryFrom<String> for NotificationStatus {
    type Error = String;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        match raw.as_str() {
            "pending" => Ok(Self::Pending),
            "delivered" => Ok(Self::Delivered),
            other => Err(other.to_string()),
        }
    }
}

/// A tracked product row (`tracked_products`).
#[derive(Debug, Clone, FromRow)]
pub struct TrackedProduct {
    /// Primary key, generated by the application.
    pub id: Uuid,
    /// The owner. Every statement filters on this column.
    pub user_id: Uuid,
    /// The maker, 1 to 100 characters.
    pub brand: String,
    /// The model, 1 to 100 characters.
    pub model: String,
    /// The configuration that distinguishes otherwise identical models.
    pub specs: String,
    /// The deterministic identity the service derives from brand and model.
    pub normalized_name: String,
    /// The lower-case SHA-256 hex digest of `specs`.
    pub config_hash: String,
    /// The two-state switch.
    pub status: ProductStatus,
    /// Creation instant.
    pub created_at: DateTime<Utc>,
    /// Last modification instant.
    pub updated_at: DateTime<Utc>,
}

/// One retailer listing of a tracked product (`product_listings`).
#[derive(Debug, Clone, FromRow)]
pub struct ProductListing {
    /// Primary key, generated by the application.
    pub id: Uuid,
    /// The owner. The composite key to `tracked_products (id, user_id)` makes
    /// this equal to the product's owner by construction.
    pub user_id: Uuid,
    /// The tracked product this listing sells.
    pub product_id: Uuid,
    /// Where the price is collected, 1 to 100 characters.
    pub retailer: String,
    /// The canonical listing URL, unique per owner.
    pub url: String,
    /// The retailer's own product identifier, when the workflow recorded one.
    pub external_id: Option<String>,
    /// ISO 4217 currency code, three upper-case letters.
    pub currency: String,
    /// The availability the most recent collection reported.
    pub availability: ListingAvailability,
    /// How much the listing itself is still trusted.
    pub validation_state: ListingValidationState,
    /// The `observed_at` of the newest observation, maintained by the write.
    pub last_observed_at: Option<DateTime<Utc>>,
    /// Creation instant.
    pub created_at: DateTime<Utc>,
    /// Last modification instant.
    pub updated_at: DateTime<Utc>,
}

/// One collected price (`price_observations`).
///
/// An observation is immutable: it is written once, duplicates are absorbed,
/// and nothing about it is corrected in place — which is why there is no
/// `updated_at`, for the same reason `application_events` has none.
#[derive(Debug, Clone, FromRow)]
pub struct PriceObservation {
    /// Primary key, generated by the application.
    pub id: Uuid,
    /// The owner. The composite key to `product_listings (id, user_id)` makes
    /// this equal to the listing's owner by construction.
    pub user_id: Uuid,
    /// The listing this observation was collected from.
    pub listing_id: Uuid,
    /// The collected price, as `NUMERIC(19,4)::text`.
    pub amount: String,
    /// The currency the price was quoted in.
    pub currency: String,
    /// When the source was observed, as the workflow reported it.
    pub observed_at: DateTime<Utc>,
    /// The client-supplied idempotency identity.
    pub fingerprint: String,
    /// The availability reported with this observation.
    pub availability: ListingAvailability,
    /// The automation run that collected it, when the workflow said.
    pub run_id: Option<Uuid>,
    /// The write instant.
    pub created_at: DateTime<Utc>,
}

/// A target-price rule (`monitoring_rules`).
#[derive(Debug, Clone, FromRow)]
pub struct MonitoringRule {
    /// Primary key, generated by the application.
    pub id: Uuid,
    /// The owner. The composite key to `tracked_products (id, user_id)` makes
    /// this equal to the product's owner by construction.
    pub user_id: Uuid,
    /// The product this rule watches.
    pub product_id: Uuid,
    /// The target price, as `NUMERIC(19,4)::text`.
    pub target_amount: String,
    /// The currency the target is denominated in.
    pub target_currency: String,
    /// The alternative trigger, as a percentage; `None` means the rule does
    /// not watch percentages.
    pub percent_drop: Option<i32>,
    /// Whether the rule is evaluated when an observation arrives.
    pub enabled: bool,
    /// How the owner wants to be told.
    pub notify_via: NotifyVia,
    /// Creation instant.
    pub created_at: DateTime<Utc>,
    /// Last modification instant.
    pub updated_at: DateTime<Utc>,
}

/// One execution of the automation workflow (`automation_runs`).
#[derive(Debug, Clone, FromRow)]
pub struct AutomationRun {
    /// Primary key, generated by the application.
    pub id: Uuid,
    /// The owner. Every statement filters on this column.
    pub user_id: Uuid,
    /// When the run started.
    pub started_at: DateTime<Utc>,
    /// When the run finished; `None` while it is running.
    pub finished_at: Option<DateTime<Utc>>,
    /// The lifecycle state.
    pub outcome: RunOutcome,
    /// The bounded summary of what went wrong, for the two failure outcomes.
    pub error_summary: Option<String>,
    /// The count the run reported.
    pub observations: i32,
    /// The count the run reported.
    pub alerts: i32,
    /// The count the run reported.
    pub failures: i32,
}

/// One raised alert (`price_alerts`).
#[derive(Debug, Clone, FromRow)]
pub struct PriceAlert {
    /// Primary key, generated by the application.
    pub id: Uuid,
    /// The owner. The composite keys to `monitoring_rules` and
    /// `price_observations` make this equal to both of theirs by
    /// construction.
    pub user_id: Uuid,
    /// The rule that fired.
    pub rule_id: Uuid,
    /// The observation that triggered it.
    pub observation_id: Uuid,
    /// When the rule fired. Written once, by the evaluation.
    pub triggered_at: DateTime<Utc>,
    /// The delivery state of the notification.
    pub notification_status: NotificationStatus,
    /// When the owner read the alert; `None` while unread.
    pub read_at: Option<DateTime<Utc>>,
}

/// Implements the three `sqlx` traits a `CHECK`ed `TEXT` enum needs.
///
/// The `Type` derive cannot express "decode a `TEXT` column into this enum":
/// a weak enum (`repr`) casts unit variants to the repr, which only compiles
/// for integer reprs, and a strong enum (`type_name`) demands a PostgreSQL
/// enum type, which this schema deliberately does not create. These impls
/// delegate every trait to `String`, so the enum decodes from and encodes to
/// the very column the `CHECK` guards, and an out-of-contract value — which
/// the `CHECK` makes unreachable through the API — is a decode error rather
/// than a silently accepted state.
macro_rules! checked_text_enum {
    ($ty:ty) => {
        impl sqlx::Type<sqlx::Postgres> for $ty {
            fn type_info() -> sqlx::postgres::PgTypeInfo {
                <String as sqlx::Type<sqlx::Postgres>>::type_info()
            }

            fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
                <String as sqlx::Type<sqlx::Postgres>>::compatible(ty)
            }
        }

        impl sqlx::Encode<'_, sqlx::Postgres> for $ty {
            fn encode_by_ref(
                &self,
                buf: &mut sqlx::postgres::PgArgumentBuffer,
            ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                let stored = self.as_str().to_string();
                <String as sqlx::Encode<'_, sqlx::Postgres>>::encode_by_ref(&stored, buf)
            }

            fn size_hint(&self) -> usize {
                self.as_str().len()
            }
        }

        impl sqlx::Decode<'_, sqlx::Postgres> for $ty {
            fn decode(
                value: sqlx::postgres::PgValueRef<'_>,
            ) -> Result<Self, sqlx::error::BoxDynError> {
                let raw = <String as sqlx::Decode<'_, sqlx::Postgres>>::decode(value)?;
                <$ty>::try_from(raw).map_err(|unknown| {
                    let message = format!("unknown {} value: {unknown}", stringify!($ty));
                    sqlx::error::BoxDynError::from(message)
                })
            }
        }
    };
}

checked_text_enum!(ProductStatus);
checked_text_enum!(ListingAvailability);
checked_text_enum!(ListingValidationState);
checked_text_enum!(RunOutcome);
checked_text_enum!(NotifyVia);
checked_text_enum!(NotificationStatus);
