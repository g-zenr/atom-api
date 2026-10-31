//! Public contracts for the `monitoring` module.
//!
//! The shapes mirror `docs/features/price-monitoring/api.md` field for field.
//! The same two properties the `reimbursements` contracts carry are carried by
//! these types themselves:
//!
//! * **`user_id` never reaches the wire.** The response types are built from
//!   the row models and drop it, exactly like every other feature.
//! * **Money is a `String` with four decimals.** `amount` and `target_amount`
//!   are `NUMERIC(19,4)::text` on the server side and are never parsed,
//!   formatted or arithmetically combined in Rust (`ADR-0007`): the target
//!   comparison and the percentage of the rule evaluation are computed by
//!   `SQL`, in the transaction that inserts the observation.
//!
//! The observation create response deserves its own note. The endpoint is
//! **idempotent on the fingerprint** (`ADR-0012` §Decision 5): a body whose
//! `(owner, listing, fingerprint)` already has an observation is absorbed and
//! answers `200` with the **existing** row, while a new one answers `201`
//! with the row it wrote. The body is the same [`ObservationResponse`] in
//! both cases — the status code is the whole difference, so a client needs
//! no extra field to tell them apart.
//!
//! Patch bodies are partial: an omitted field is left untouched. The two
//! nullable columns (`product_listings.external_id`,
//! `automation_runs.error_summary`, `monitoring_rules.percent_drop`) are
//! tri-states, because "absent" and "cleared" must stay two instructions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::monitoring::models::{
    AutomationRun, ListingAvailability, ListingValidationState,
    MonitoringRule as MonitoringRuleModel, NotificationStatus, NotifyVia, PriceAlert,
    PriceObservation, ProductListing, ProductStatus, RunOutcome, TrackedProduct,
};
use crate::pagination::{Page, Pagination, PaginationQuery};

/// Maximum accepted brand length.
///
/// Mirrors `tracked_products_brand_length_chk`
/// (`CHECK (CHAR_LENGTH(brand) BETWEEN 1 AND 100)`) in
/// `migrations/0008_price_monitoring.sql`. The migration is the enforced copy;
/// this constant is the application-side mirror of it.
pub const MAX_BRAND_LENGTH: usize = 100;

/// Maximum accepted model length. Mirrors `tracked_products_model_length_chk`.
pub const MAX_MODEL_LENGTH: usize = 100;

/// Maximum accepted configuration text. Mirrors
/// `tracked_products_specs_length_chk`.
pub const MAX_SPECS_LENGTH: usize = 2_000;

/// Maximum accepted length of the derived normalised name. Mirrors
/// `tracked_products_normalized_name_length_chk`; the service derives the name
/// and refuses an over-long result with a `422` before any statement runs.
pub const MAX_NORMALIZED_NAME_LENGTH: usize = 255;

/// Maximum accepted retailer label. Mirrors
/// `product_listings_retailer_length_chk`.
pub const MAX_RETAILER_LENGTH: usize = 100;

/// Maximum accepted listing URL. Mirrors the length half of
/// `product_listings_url_chk`; the scheme half is enforced by
/// [`crate::monitoring::service::validate_listing_url`].
pub const MAX_LISTING_URL_LENGTH: usize = 2_048;

/// Maximum accepted external identifier. Mirrors
/// `product_listings_external_id_length_chk`.
pub const MAX_EXTERNAL_ID_LENGTH: usize = 200;

/// Maximum accepted observation fingerprint. Mirrors
/// `price_observations_fingerprint_length_chk`.
pub const MAX_FINGERPRINT_LENGTH: usize = 200;

/// Maximum accepted automation-run error summary. Mirrors
/// `automation_runs_error_summary_length_chk`.
pub const MAX_ERROR_SUMMARY_LENGTH: usize = 1_000;

/// The lowest percent drop a rule may watch. Mirrors
/// `monitoring_rules_percent_drop_chk`.
pub const MIN_PERCENT_DROP: i32 = 1;

/// The highest percent drop a rule may watch. Mirrors
/// `monitoring_rules_percent_drop_chk`: 100 can never be reached.
pub const MAX_PERCENT_DROP: i32 = 99;

// ---------------------------------------------------------------------------
// Tracked products
// ---------------------------------------------------------------------------

/// The public representation of a tracked product.
///
/// `user_id` is on the row and is deliberately absent here.
/// `normalized_name` and `config_hash` are published because they are the two
/// halves of the tracking identity: a client that wants to know why a create
/// answered `409` can see exactly what collided.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrackedProductResponse {
    /// Stable public identifier.
    pub id: Uuid,
    /// The maker, 1 to 100 characters.
    pub brand: String,
    /// The model, 1 to 100 characters.
    pub model: String,
    /// The configuration text; may be empty.
    pub specs: String,
    /// The deterministic identity, derived from brand and model.
    pub normalized_name: String,
    /// The lower-case SHA-256 hex digest of the configuration text.
    pub config_hash: String,
    /// Whether the product is being collected.
    pub status: ProductStatus,
    /// Creation timestamp, RFC 3339 UTC.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp, RFC 3339 UTC.
    pub updated_at: DateTime<Utc>,
}

impl From<TrackedProduct> for TrackedProductResponse {
    fn from(product: TrackedProduct) -> Self {
        Self {
            id: product.id,
            brand: product.brand,
            model: product.model,
            specs: product.specs,
            normalized_name: product.normalized_name,
            config_hash: product.config_hash,
            status: product.status,
            created_at: product.created_at,
            updated_at: product.updated_at,
        }
    }
}

/// The response body of `GET /api/v1/products`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrackedProductPage {
    /// The products on this page.
    pub items: Vec<TrackedProductResponse>,
    /// Pagination metadata.
    pub pagination: Pagination,
}

impl From<Page<TrackedProductResponse>> for TrackedProductPage {
    fn from(page: Page<TrackedProductResponse>) -> Self {
        Self {
            items: page.items,
            pagination: page.pagination,
        }
    }
}

/// Request body for tracking a product.
///
/// A product is always created `active`. `normalized_name` and `config_hash`
/// are derived by the service and are not request fields; `id`, `user_id` and
/// the timestamps are server-owned.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateProductRequest {
    /// The maker, trimmed to 1 to 100 characters.
    #[schema(example = "Lenovo")]
    pub brand: String,
    /// The model, trimmed to 1 to 100 characters.
    #[schema(example = "ThinkPad X1 Carbon")]
    pub model: String,
    /// The configuration that distinguishes otherwise identical models —
    /// RAM, storage, GPU, CPU, screen. Kept verbatim; defaults to empty.
    #[schema(example = "32GB RAM, 1TB SSD, Ultra 7 155H")]
    #[serde(default)]
    pub specs: String,
}

/// Request body for partially updating a tracked product.
///
/// Every field is optional; an omitted field is left untouched. `specs` is
/// kept verbatim, so `""` clears it. `normalized_name` and `config_hash` are
/// **not** fields: they are re-derived from whatever `brand`, `model` and
/// `specs` the patch leaves behind, so a client cannot make the stored
/// identity disagree with the fields it was derived from.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateProductRequest {
    /// New brand. Trimmed; an empty brand is a validation failure.
    pub brand: Option<String>,
    /// New model. Trimmed; an empty model is a validation failure.
    pub model: Option<String>,
    /// New configuration text. `""` clears it; an absent field leaves it
    /// untouched.
    pub specs: Option<String>,
    /// New status. Omitted leaves the current one.
    pub status: Option<ProductStatus>,
}

/// The filters accepted by `GET /api/v1/products`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ProductQuery {
    /// Only products in this status. An unknown value is a `422`.
    pub status: Option<ProductStatus>,
    /// Case-insensitive substring over brand, model and the normalised name.
    ///
    /// At most 200 characters. An empty term means "no filter".
    #[param(example = "thinkpad")]
    pub search: Option<String>,
}

/// The whole query string of `GET /api/v1/products`, deserialized once.
///
/// Written out instead of composing [`ProductQuery`] and
/// [`PaginationQuery`] with `#[serde(flatten)]`, for the reason the
/// reimbursements contracts record: `serde_urlencoded` feeds strings to a
/// flattened `Option<i64>`, which fails at run time on a real request.
/// Deliberately absent from the generated document: the published parameter
/// set stays [`ProductQuery`] and [`PaginationQuery`].
#[derive(Debug, Clone, Deserialize)]
pub struct ProductsListQuery {
    /// Maximum number of products to return.
    pub limit: Option<i64>,
    /// Number of products to skip.
    pub offset: Option<i64>,
    /// Filter by status.
    pub status: Option<ProductStatus>,
    /// Free-text search over brand, model and the normalised name.
    pub search: Option<String>,
}

impl ProductsListQuery {
    /// Splits the parsed query into its filter and pagination halves.
    pub fn into_parts(self) -> (ProductQuery, PaginationQuery) {
        let Self {
            limit,
            offset,
            status,
            search,
        } = self;

        (
            ProductQuery { status, search },
            PaginationQuery { limit, offset },
        )
    }
}

// ---------------------------------------------------------------------------
// Product listings
// ---------------------------------------------------------------------------

/// The public representation of one retailer listing.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ListingResponse {
    /// Stable public identifier.
    pub id: Uuid,
    /// The tracked product this listing sells.
    pub product_id: Uuid,
    /// Where the price is collected, 1 to 100 characters.
    pub retailer: String,
    /// The canonical listing URL, unique per owner.
    pub url: String,
    /// The retailer's own product identifier, when one was recorded.
    pub external_id: Option<String>,
    /// ISO 4217 currency code, three upper-case letters. Immutable once
    /// created.
    pub currency: String,
    /// The availability the most recent collection reported.
    pub availability: ListingAvailability,
    /// How much the listing itself is still trusted.
    pub validation_state: ListingValidationState,
    /// The `observed_at` of the newest observation, when there is one.
    pub last_observed_at: Option<DateTime<Utc>>,
    /// Creation timestamp, RFC 3339 UTC.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp, RFC 3339 UTC.
    pub updated_at: DateTime<Utc>,
}

impl From<ProductListing> for ListingResponse {
    fn from(listing: ProductListing) -> Self {
        Self {
            id: listing.id,
            product_id: listing.product_id,
            retailer: listing.retailer,
            url: listing.url,
            external_id: listing.external_id,
            currency: listing.currency,
            availability: listing.availability,
            validation_state: listing.validation_state,
            last_observed_at: listing.last_observed_at,
            created_at: listing.created_at,
            updated_at: listing.updated_at,
        }
    }
}

/// The response body of `GET /api/v1/products/{product_id}/listings`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ListingPage {
    /// The listings on this page.
    pub items: Vec<ListingResponse>,
    /// Pagination metadata.
    pub pagination: Pagination,
}

impl From<Page<ListingResponse>> for ListingPage {
    fn from(page: Page<ListingResponse>) -> Self {
        Self {
            items: page.items,
            pagination: page.pagination,
        }
    }
}

/// Request body for tracking a listing of a product.
///
/// `availability` and `validation_state` are not request fields: a listing is
/// born `unknown` and `valid`, and both change through the patch, which is
/// the workflow's report about the listing rather than a property the owner
/// declares at create time. `id`, `product_id` and the timestamps are
/// server-owned.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateListingRequest {
    /// Where the price is collected, trimmed to 1 to 100 characters.
    #[schema(example = "Notebooksbilliger")]
    pub retailer: String,
    /// The canonical listing URL: an absolute `http`/`https` reference of at
    /// most 2048 characters. Unique per owner.
    #[schema(example = "https://www.notebooksbilliger.de/product/lenovo-thinkpad-x1-carbon")]
    pub url: String,
    /// ISO 4217 code. Trimmed and upper-cased; exactly three letters.
    #[schema(example = "EUR")]
    pub currency: String,
    /// The retailer's own product identifier, at most 200 characters.
    #[schema(example = "2000510704135")]
    pub external_id: Option<String>,
}

/// Request body for partially updating a listing.
///
/// Only the workflow's report about the listing is patchable. `retailer`,
/// `url` and `currency` are **not** fields: the URL is the listing's identity
/// per owner (the unique index makes a second row for it a `409`), and
/// moving a listing to a new URL is a new listing, not a mutation. A body
/// carrying them is accepted and ignored.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateListingRequest {
    /// The retailer's own product identifier, as a tri-state.
    ///
    /// * key absent — leave it untouched;
    /// * `"external_id": null` — clear it;
    /// * `"external_id": "<text>"` — set it.
    #[serde(default, deserialize_with = "double_option_string")]
    #[schema(value_type = Option<String>, nullable)]
    pub external_id: Option<Option<String>>,
    /// New availability. Omitted leaves the stored one.
    pub availability: Option<ListingAvailability>,
    /// New validation state. Omitted leaves the stored one.
    pub validation_state: Option<ListingValidationState>,
}

impl UpdateListingRequest {
    /// The external identifier as `(clear, value)`, the pair the statement
    /// binds. A single `COALESCE($n, external_id)` cannot express "set to
    /// `NULL`", because `NULL` already means "leave alone".
    pub fn external_id_change(&self) -> (bool, Option<&str>) {
        match &self.external_id {
            None => (false, None),
            Some(None) => (true, None),
            Some(Some(value)) => (false, Some(value)),
        }
    }
}

// ---------------------------------------------------------------------------
// Price observations
// ---------------------------------------------------------------------------

/// The public representation of one collected price.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ObservationResponse {
    /// Stable public identifier.
    pub id: Uuid,
    /// The listing this observation was collected from.
    pub listing_id: Uuid,
    /// The collected price, as a four-decimal string.
    #[schema(
        format = "decimal",
        pattern = "^[0-9]{1,15}\\.[0-9]{4}$",
        example = "1289.9900"
    )]
    pub amount: String,
    /// The currency the price was quoted in.
    pub currency: String,
    /// When the source was observed, RFC 3339 UTC.
    pub observed_at: DateTime<Utc>,
    /// The idempotency identity the workflow supplied.
    pub fingerprint: String,
    /// The availability reported with this observation.
    pub availability: ListingAvailability,
    /// The automation run that collected it, when one was named.
    pub run_id: Option<Uuid>,
    /// Write timestamp, RFC 3339 UTC.
    pub created_at: DateTime<Utc>,
}

impl From<PriceObservation> for ObservationResponse {
    fn from(observation: PriceObservation) -> Self {
        Self {
            id: observation.id,
            listing_id: observation.listing_id,
            amount: observation.amount,
            currency: observation.currency,
            observed_at: observation.observed_at,
            fingerprint: observation.fingerprint,
            availability: observation.availability,
            run_id: observation.run_id,
            created_at: observation.created_at,
        }
    }
}

/// The response body of `GET /api/v1/listings/{listing_id}/observations`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ObservationPage {
    /// The observations on this page, newest first.
    pub items: Vec<ObservationResponse>,
    /// Pagination metadata.
    pub pagination: Pagination,
}

impl From<Page<ObservationResponse>> for ObservationPage {
    fn from(page: Page<ObservationResponse>) -> Self {
        Self {
            items: page.items,
            pagination: page.pagination,
        }
    }
}

/// Request body for recording one collected price.
///
/// The endpoint is idempotent on `fingerprint` (`ADR-0012` §Decision 5): a
/// retried workflow re-sends the same fingerprint and is answered `200` with
/// the existing observation, so no field of this request is ever written
/// twice. `id` and `created_at` are server-owned; `user_id` comes from the
/// credential.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateObservationRequest {
    /// The collected price, as a four-decimal string. A JSON number is
    /// refused by the extractor (`ADR-0007`).
    #[schema(
        format = "decimal",
        pattern = "^[0-9]{1,15}\\.[0-9]{4}$",
        example = "1289.9900"
    )]
    pub amount: String,
    /// The currency the price was quoted in. Trimmed and upper-cased.
    #[schema(example = "EUR")]
    pub currency: String,
    /// When the source was observed — the collected instant, not the write
    /// instant.
    #[schema(example = "2026-10-31T09:15:00Z")]
    pub observed_at: DateTime<Utc>,
    /// The idempotency identity, 1 to 200 characters: a value the workflow
    /// derives from product, listing, observed price and collected-at.
    #[schema(example = "sha256:9f2b1c")]
    pub fingerprint: String,
    /// The availability at the moment of collection. Defaults to `unknown`.
    pub availability: Option<ListingAvailability>,
    /// The automation run that collected this observation. It must belong to
    /// the caller; a foreign or unknown run is a `404`.
    pub run_id: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// Monitoring rules
// ---------------------------------------------------------------------------

/// The public representation of one target-price rule.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RuleResponse {
    /// Stable public identifier.
    pub id: Uuid,
    /// The product this rule watches.
    pub product_id: Uuid,
    /// The target price, as a four-decimal string.
    #[schema(
        format = "decimal",
        pattern = "^[0-9]{1,15}\\.[0-9]{4}$",
        example = "1000.0000"
    )]
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
    /// Creation timestamp, RFC 3339 UTC.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp, RFC 3339 UTC.
    pub updated_at: DateTime<Utc>,
}

impl From<MonitoringRuleModel> for RuleResponse {
    fn from(rule: MonitoringRuleModel) -> Self {
        Self {
            id: rule.id,
            product_id: rule.product_id,
            target_amount: rule.target_amount,
            target_currency: rule.target_currency,
            percent_drop: rule.percent_drop,
            enabled: rule.enabled,
            notify_via: rule.notify_via,
            created_at: rule.created_at,
            updated_at: rule.updated_at,
        }
    }
}

/// The response body of `GET /api/v1/products/{product_id}/rules`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RulePage {
    /// The rules on this page; at most one, because a product has at most
    /// one rule.
    pub items: Vec<RuleResponse>,
    /// Pagination metadata.
    pub pagination: Pagination,
}

impl From<Page<RuleResponse>> for RulePage {
    fn from(page: Page<RuleResponse>) -> Self {
        Self {
            items: page.items,
            pagination: page.pagination,
        }
    }
}

/// Request body for creating a product's rule.
///
/// A product has at most one rule; creating a second is a `409`. Both
/// triggers may be set, exactly one, or — for `percent_drop` — neither. The
/// rule is born `enabled`, on the `inbox` channel.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateRuleRequest {
    /// The target price, as a four-decimal string.
    #[schema(
        format = "decimal",
        pattern = "^[0-9]{1,15}\\.[0-9]{4}$",
        example = "1000.0000"
    )]
    pub target_amount: String,
    /// The currency the target is denominated in. Trimmed and upper-cased.
    #[schema(example = "EUR")]
    pub target_currency: String,
    /// The alternative trigger: a drop of at least this many percent against
    /// the previous observation of the same listing. 1 to 99.
    pub percent_drop: Option<i32>,
}

/// Request body for partially updating a rule.
///
/// Every field is optional. `percent_drop` is a tri-state, because `None` is
/// a real state of the rule ("does not watch percentages") and not a missing
/// value: absent leaves it, an explicit `null` clears it.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateRuleRequest {
    /// New switch state. Omitted leaves the stored one.
    pub enabled: Option<bool>,
    /// New target price, as a four-decimal string.
    #[schema(
        format = "decimal",
        pattern = "^[0-9]{1,15}\\.[0-9]{4}$",
        example = "950.0000"
    )]
    pub target_amount: Option<String>,
    /// New target currency. Trimmed and upper-cased.
    pub target_currency: Option<String>,
    /// The percent drop as a tri-state: absent leaves it, `null` clears it.
    #[serde(default, deserialize_with = "double_option_i32")]
    #[schema(value_type = Option<i32>, nullable)]
    pub percent_drop: Option<Option<i32>>,
}

impl UpdateRuleRequest {
    /// The percent drop as `(clear, value)`, the pair the statement binds.
    pub fn percent_drop_change(&self) -> (bool, Option<i32>) {
        match self.percent_drop {
            None => (false, None),
            Some(None) => (true, None),
            Some(Some(value)) => (false, Some(value)),
        }
    }
}

// ---------------------------------------------------------------------------
// Automation runs
// ---------------------------------------------------------------------------

/// The public representation of one automation run.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RunResponse {
    /// Stable public identifier.
    pub id: Uuid,
    /// When the run started, RFC 3339 UTC.
    pub started_at: DateTime<Utc>,
    /// When the run finished, RFC 3339 UTC; `null` while it is running.
    pub finished_at: Option<DateTime<Utc>>,
    /// The lifecycle state.
    pub outcome: RunOutcome,
    /// The bounded summary of what went wrong, when the run reported one.
    pub error_summary: Option<String>,
    /// The count the run reported.
    pub observations: i32,
    /// The count the run reported.
    pub alerts: i32,
    /// The count the run reported.
    pub failures: i32,
}

impl From<AutomationRun> for RunResponse {
    fn from(run: AutomationRun) -> Self {
        Self {
            id: run.id,
            started_at: run.started_at,
            finished_at: run.finished_at,
            outcome: run.outcome,
            error_summary: run.error_summary,
            observations: run.observations,
            alerts: run.alerts,
            failures: run.failures,
        }
    }
}

/// The response body of `GET /api/v1/automation/runs`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RunPage {
    /// The runs on this page, newest first.
    pub items: Vec<RunResponse>,
    /// Pagination metadata.
    pub pagination: Pagination,
}

impl From<Page<RunResponse>> for RunPage {
    fn from(page: Page<RunResponse>) -> Self {
        Self {
            items: page.items,
            pagination: page.pagination,
        }
    }
}

/// Request body for finishing a run.
///
/// The `outcome` is the point of the request and is required; it must be one
/// of the three terminal values, because `running` is what the run already
/// is. The three counts are optional and default to "leave the stored count"
/// (`0` for a fresh run). `error_summary` is a tri-state, because a summary
/// must be clearable when a partial run is re-explained.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct FinishRunRequest {
    /// The terminal outcome. `running` is refused with a `422`.
    pub outcome: RunOutcome,
    /// The number of observations the run collected.
    pub observations: Option<i32>,
    /// The number of alerts the run raised.
    pub alerts: Option<i32>,
    /// The number of failures the run recorded.
    pub failures: Option<i32>,
    /// The error summary, as a tri-state: absent leaves it, `null` clears it.
    #[serde(default, deserialize_with = "double_option_string")]
    #[schema(value_type = Option<String>, nullable)]
    pub error_summary: Option<Option<String>>,
}

impl FinishRunRequest {
    /// The error summary as `(clear, value)`, the pair the statement binds.
    pub fn error_summary_change(&self) -> (bool, Option<&str>) {
        match &self.error_summary {
            None => (false, None),
            Some(None) => (true, None),
            Some(Some(value)) => (false, Some(value)),
        }
    }
}

// ---------------------------------------------------------------------------
// Price alerts
// ---------------------------------------------------------------------------

/// The public representation of one raised alert.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AlertResponse {
    /// Stable public identifier.
    pub id: Uuid,
    /// The rule that fired.
    pub rule_id: Uuid,
    /// The observation that triggered it.
    pub observation_id: Uuid,
    /// When the rule fired, RFC 3339 UTC.
    pub triggered_at: DateTime<Utc>,
    /// The delivery state of the notification.
    pub notification_status: NotificationStatus,
    /// When the owner read the alert; `null` while unread.
    pub read_at: Option<DateTime<Utc>>,
}

impl From<PriceAlert> for AlertResponse {
    fn from(alert: PriceAlert) -> Self {
        Self {
            id: alert.id,
            rule_id: alert.rule_id,
            observation_id: alert.observation_id,
            triggered_at: alert.triggered_at,
            notification_status: alert.notification_status,
            read_at: alert.read_at,
        }
    }
}

/// The response body of `GET /api/v1/alerts`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AlertPage {
    /// The alerts on this page, newest first.
    pub items: Vec<AlertResponse>,
    /// Pagination metadata.
    pub pagination: Pagination,
}

impl From<Page<AlertResponse>> for AlertPage {
    fn from(page: Page<AlertResponse>) -> Self {
        Self {
            items: page.items,
            pagination: page.pagination,
        }
    }
}

/// The filters accepted by `GET /api/v1/alerts`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AlertQuery {
    /// When `true`, only alerts with no `read_at`. Omitted or `false` returns
    /// all alerts, exactly as the notes archive filter behaves.
    pub unread: Option<bool>,
}

/// The whole query string of `GET /api/v1/alerts`, deserialized once; see
/// [`ProductsListQuery`] for why it is written out.
#[derive(Debug, Clone, Deserialize)]
pub struct AlertsListQuery {
    /// Maximum number of alerts to return.
    pub limit: Option<i64>,
    /// Number of alerts to skip.
    pub offset: Option<i64>,
    /// Only unread alerts.
    pub unread: Option<bool>,
}

impl AlertsListQuery {
    /// Splits the parsed query into its filter and pagination halves.
    pub fn into_parts(self) -> (AlertQuery, PaginationQuery) {
        let Self {
            limit,
            offset,
            unread,
        } = self;

        (AlertQuery { unread }, PaginationQuery { limit, offset })
    }
}

/// Request body for partially updating an alert.
///
/// `read` is the mark the owner sets: `true` stamps `read_at` with the write
/// instant, `false` clears it. `notification_status` is the channel's
/// delivery report. Both are optional and an omitted field is left untouched.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateAlertRequest {
    /// `true` marks the alert read; `false` marks it unread again.
    pub read: Option<bool>,
    /// The delivery state the channel reports.
    pub notification_status: Option<NotificationStatus>,
}

// ---------------------------------------------------------------------------
// The two tri-state deserializers
// ---------------------------------------------------------------------------

/// Distinguishes `"key": null` from an absent key, for a text field.
///
/// The pair of attributes on the field does the work; see the reimbursements
/// contracts for the full argument. In short: `#[serde(default)]` handles
/// absence and short-circuits the function, and this function wraps an
/// explicit `null` into `Some(None)` — drop either one and the two states
/// collapse into one.
fn double_option_string<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

/// The same tri-state for an integer field.
fn double_option_i32<'de, D>(deserializer: D) -> Result<Option<Option<i32>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<i32>::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three body forms of a nullable rule field. Drop either attribute
    /// of the pair and this test fails, because absence and an explicit
    /// `null` collapse into the same value.
    #[test]
    fn a_percent_drop_is_kept_cleared_or_set_by_the_three_body_forms() {
        let kept: UpdateRuleRequest = serde_json::from_str("{}").expect("an empty body");
        assert_eq!(kept.percent_drop_change(), (false, None));

        let cleared: UpdateRuleRequest =
            serde_json::from_str(r#"{"percent_drop": null}"#).expect("an explicit null");
        assert_eq!(cleared.percent_drop_change(), (true, None));

        let set: UpdateRuleRequest =
            serde_json::from_str(r#"{"percent_drop": 15}"#).expect("an integer");
        assert_eq!(set.percent_drop_change(), (false, Some(15)));
    }

    /// The same tri-state for the two text fields of the module: the
    /// listing's external identifier and the run's error summary.
    #[test]
    fn the_nullable_text_fields_are_kept_cleared_or_set_by_the_three_body_forms() {
        let kept: UpdateListingRequest = serde_json::from_str("{}").expect("an empty body");
        assert_eq!(kept.external_id_change(), (false, None));

        let cleared: UpdateListingRequest =
            serde_json::from_str(r#"{"external_id": null}"#).expect("an explicit null");
        assert_eq!(cleared.external_id_change(), (true, None));

        let set: UpdateListingRequest =
            serde_json::from_str(r#"{"external_id": "2000510704135"}"#).expect("a string");
        assert_eq!(set.external_id_change(), (false, Some("2000510704135")));

        let kept: FinishRunRequest = serde_json::from_str(r#"{"outcome":"ok"}"#)
            .expect("a body without the summary leaves it untouched");
        assert_eq!(kept.error_summary_change(), (false, None));

        let cleared: FinishRunRequest =
            serde_json::from_str(r#"{"outcome":"partial","error_summary": null}"#)
                .expect("an explicit null clears the summary");
        assert_eq!(cleared.error_summary_change(), (true, None));

        let set: FinishRunRequest =
            serde_json::from_str(r#"{"outcome":"failed","error_summary":"two sources refused"}"#)
                .expect("a string sets the summary");
        assert_eq!(
            set.error_summary_change(),
            (false, Some("two sources refused"))
        );
    }

    /// The controlled vocabularies round-trip through their snake_case wire
    /// form, and an unknown value is a deserialization failure rather than a
    /// silently accepted state.
    #[test]
    fn the_wire_form_of_every_vocabulary_is_snake_case() {
        assert_eq!(
            serde_json::from_str::<ProductStatus>(r#""active""#).unwrap(),
            ProductStatus::Active
        );
        assert_eq!(
            serde_json::from_str::<ListingAvailability>(r#""unavailable""#).unwrap(),
            ListingAvailability::Unavailable
        );
        assert_eq!(
            serde_json::from_str::<ListingValidationState>(r#""stale""#).unwrap(),
            ListingValidationState::Stale
        );
        assert_eq!(
            serde_json::from_str::<RunOutcome>(r#""partial""#).unwrap(),
            RunOutcome::Partial
        );
        assert_eq!(
            serde_json::from_str::<NotifyVia>(r#""inbox""#).unwrap(),
            NotifyVia::Inbox
        );
        assert_eq!(
            serde_json::from_str::<NotificationStatus>(r#""delivered""#).unwrap(),
            NotificationStatus::Delivered
        );

        // An out-of-contract value is a deserialization failure, never a
        // variant: the same closed set the database `CHECK` guards.
        assert!(serde_json::from_str::<ProductStatus>(r#""archived""#).is_err());
        assert!(serde_json::from_str::<RunOutcome>(r#""succeeded""#).is_err());

        // `running` is what a create produces, not what a finish may write;
        // the helper is the one definition of the terminal set.
        assert!(!RunOutcome::Running.is_terminal());
        assert!(RunOutcome::Ok.is_terminal());
        assert!(RunOutcome::Partial.is_terminal());
        assert!(RunOutcome::Failed.is_terminal());
    }
}
