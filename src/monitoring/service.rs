//! Price-monitoring use cases: validation, the identity rule, the idempotency
//! mapping and the rule-evaluation outcome.
//!
//! Every method takes `owner` first, exactly as the other Phase 2 services
//! do, so a monitoring operation cannot be called without saying whose data
//! is meant. The service never compares identifiers and never returns `403`:
//! a repository miss becomes [`AppError::NotFound`] with the same message for
//! "does not exist" and "belongs to somebody else" (`ADR-0005` §3).
//!
//! Three rules hold in this order:
//!
//! 1. **Validate the whole body first, then touch the database.** A request
//!    that is both malformed and aimed at a foreign resource is a `422`, not
//!    a `404`: the body never had to be read.
//! 2. **Every miss is a catalogue message**, never a message assembled here.
//! 3. **Every state change is guarded by the statement, not by a preceding
//!    read.** The duplicate identities, the duplicate URLs, the one-rule-per-
//!    product limit, the observation idempotency and the duplicate alerts are
//!    all unique indexes; the run finish is guarded by `WHERE outcome =
//!    'running'`.
//!
//! Money is validated here but never parsed: [`crate::helpers::validation::
//! validate_money`] is a character check that returns the string unchanged,
//! and the target comparison and the percentage of the rule evaluation are
//! computed by `SQL`, inside the observation transaction (`ADR-0007`).

use std::sync::Arc;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::config::messages::{Messages, render};
use crate::config::settings::PaginationSettings;
use crate::error::{AppError, AppResult};
use crate::helpers::validation::validate_money;
use crate::helpers::{bounded_text, field_error, required_text, validate_search};
use crate::monitoring::dto::{
    AlertPage, AlertQuery, AlertResponse, CreateListingRequest, CreateObservationRequest,
    CreateProductRequest, CreateRuleRequest, FinishRunRequest, ListingPage, ListingResponse,
    MAX_BRAND_LENGTH, MAX_ERROR_SUMMARY_LENGTH, MAX_FINGERPRINT_LENGTH, MAX_LISTING_URL_LENGTH,
    MAX_MODEL_LENGTH, MAX_NORMALIZED_NAME_LENGTH, MAX_PERCENT_DROP, MAX_RETAILER_LENGTH,
    MAX_SPECS_LENGTH, MIN_PERCENT_DROP, ObservationPage, ObservationResponse, ProductQuery,
    RulePage, RuleResponse, RunPage, RunResponse, TrackedProductPage, TrackedProductResponse,
    UpdateAlertRequest, UpdateListingRequest, UpdateProductRequest, UpdateRuleRequest,
};
use crate::monitoring::models::ListingAvailability;
use crate::monitoring::repository::{
    ListingPatch, ListingWrite, MonitoringRepository, NewListing, NewObservation, NewProduct,
    ObservationWrite, ProductPatch, ProductWrite, RulePatch, RuleWrite, RunFinish,
};
use crate::pagination::{Page, PaginationQuery};

/// The outcome of one observation request.
///
/// The idempotency rule of `ADR-0012` §Decision 5 is carried by this pair
/// rather than by two service methods: the handler picks the status code
/// from `created` — `201` for a written observation, `200` for an absorbed
/// duplicate — and the body is the same [`ObservationResponse`] either way.
#[derive(Debug, Clone)]
pub struct RecordedObservation {
    /// The observation, whether it was just written or already existed.
    pub observation: ObservationResponse,
    /// Whether this request is the one that wrote the row.
    pub created: bool,
}

/// The monitoring use cases.
#[derive(Debug, Clone)]
pub struct MonitoringService {
    repository: MonitoringRepository,
    messages: Arc<Messages>,
    pagination: PaginationSettings,
}

impl MonitoringService {
    /// Builds the service.
    pub fn new(
        repository: MonitoringRepository,
        messages: Arc<Messages>,
        pagination: PaginationSettings,
    ) -> Self {
        Self {
            repository,
            messages,
            pagination,
        }
    }

    // -----------------------------------------------------------------------
    // Tracked products
    // -----------------------------------------------------------------------

    /// Lists the owner's products, newest first.
    pub async fn list_products(
        &self,
        owner: Uuid,
        query: ProductQuery,
        pagination: PaginationQuery,
    ) -> AppResult<TrackedProductPage> {
        let status = query.status;
        let search = query
            .search
            .as_deref()
            .map(|term| normalize_search(term, &self.messages))
            .transpose()?
            .flatten();

        let limit = pagination.effective_limit(&self.pagination);
        let offset = pagination.effective_offset();

        let (products, total) = self
            .repository
            .list_products_owned(owner, status, search.as_deref(), limit, offset)
            .await?;

        Ok(TrackedProductPage::from(Page::new(
            products
                .into_iter()
                .map(TrackedProductResponse::from)
                .collect(),
            limit,
            offset,
            total,
        )))
    }

    /// Tracks a product for the owner.
    ///
    /// The identity is derived from the validated fields, so a `409` is the
    /// unique index's verdict about the same (name, configuration) pair a
    /// client would recompute — never about an unnormalised spelling.
    pub async fn create_product(
        &self,
        owner: Uuid,
        request: CreateProductRequest,
    ) -> AppResult<TrackedProductResponse> {
        let brand = validate_brand(&request.brand, &self.messages)?;
        let model = validate_model(&request.model, &self.messages)?;
        let specs = validate_specs(&request.specs, &self.messages)?;
        let (normalized_name, config_hash) =
            ensure_identity(&brand, &model, &specs, &self.messages)?;

        let product = match self
            .repository
            .insert_product(
                owner,
                &NewProduct {
                    id: Uuid::new_v4(),
                    brand: &brand,
                    model: &model,
                    specs: &specs,
                    normalized_name: &normalized_name,
                    config_hash: &config_hash,
                },
            )
            .await?
        {
            ProductWrite::Applied(product) => product,
            ProductWrite::DuplicateIdentity => {
                return Err(AppError::Conflict(
                    self.messages.conflicts.product_already_tracked.clone(),
                ));
            }
            // Unreachable on this path: the insert derives the identity from
            // values [`ensure_identity`] has already bounded.
            ProductWrite::IdentityTooLong => {
                return Err(field_error(
                    "normalized_name",
                    render(
                        &self.messages.validation.too_long,
                        &[MAX_NORMALIZED_NAME_LENGTH.to_string()],
                    ),
                    &self.messages,
                ));
            }
            // Unreachable: the insert has no ownership predicate that can
            // filter, because the owner comes from the credential.
            ProductWrite::NotFound => {
                return Err(AppError::NotFound(
                    self.messages.resources.product_not_found.clone(),
                ));
            }
        };

        Ok(TrackedProductResponse::from(product))
    }

    /// Loads one of the owner's products.
    pub async fn get_product(
        &self,
        owner: Uuid,
        product_id: Uuid,
    ) -> AppResult<TrackedProductResponse> {
        let product = self
            .repository
            .find_product_owned(product_id, owner)
            .await?
            .ok_or_else(|| AppError::NotFound(self.messages.resources.product_not_found.clone()))?;

        Ok(TrackedProductResponse::from(product))
    }

    /// Applies a partial update to one of the owner's products.
    ///
    /// The merge and the identity re-derivation happen inside the update
    /// transaction, under the row lock, so a patch that renames a product
    /// can never leave `normalized_name` or `config_hash` disagreeing with
    /// the fields they were derived from.
    pub async fn update_product(
        &self,
        owner: Uuid,
        product_id: Uuid,
        request: UpdateProductRequest,
    ) -> AppResult<TrackedProductResponse> {
        let brand = request
            .brand
            .as_deref()
            .map(|brand| validate_brand(brand, &self.messages))
            .transpose()?;
        let model = request
            .model
            .as_deref()
            .map(|model| validate_model(model, &self.messages))
            .transpose()?;
        let specs = request
            .specs
            .as_deref()
            .map(|specs| validate_specs(specs, &self.messages))
            .transpose()?;
        let status = request.status;

        let product = match self
            .repository
            .update_product_owned(
                product_id,
                owner,
                &ProductPatch {
                    brand: brand.as_deref(),
                    model: model.as_deref(),
                    specs: specs.as_deref(),
                    status,
                },
            )
            .await?
        {
            ProductWrite::Applied(product) => product,
            ProductWrite::NotFound => {
                return Err(AppError::NotFound(
                    self.messages.resources.product_not_found.clone(),
                ));
            }
            ProductWrite::DuplicateIdentity => {
                return Err(AppError::Conflict(
                    self.messages.conflicts.product_already_tracked.clone(),
                ));
            }
            // The patch derived a normalised name the column's `CHECK`
            // refuses — reachable only through Unicode lower-casing that
            // expands. The create path refuses the same value with the same
            // `422`, so the two paths report one rule.
            ProductWrite::IdentityTooLong => {
                return Err(field_error(
                    "normalized_name",
                    render(
                        &self.messages.validation.too_long,
                        &[MAX_NORMALIZED_NAME_LENGTH.to_string()],
                    ),
                    &self.messages,
                ));
            }
        };

        Ok(TrackedProductResponse::from(product))
    }

    /// Deletes one of the owner's products, with its listings, observations,
    /// rules and alerts.
    pub async fn delete_product(&self, owner: Uuid, product_id: Uuid) -> AppResult<()> {
        let deleted = self
            .repository
            .delete_product_owned(product_id, owner)
            .await?;

        if !deleted {
            return Err(AppError::NotFound(
                self.messages.resources.product_not_found.clone(),
            ));
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Product listings
    // -----------------------------------------------------------------------

    /// Lists the listings of one of the owner's products.
    ///
    /// An empty page probes the product once on the error path, so a foreign
    /// product is the product's `404` and a product with no listing is an
    /// honest empty page — the same rule the tasks list applies to its
    /// project.
    pub async fn list_listings(
        &self,
        owner: Uuid,
        product_id: Uuid,
        pagination: PaginationQuery,
    ) -> AppResult<ListingPage> {
        let limit = pagination.effective_limit(&self.pagination);
        let offset = pagination.effective_offset();

        let (listings, total) = self
            .repository
            .list_listings_owned(product_id, owner, limit, offset)
            .await?;

        if total == 0 {
            let product_exists = self
                .repository
                .find_product_owned(product_id, owner)
                .await?
                .is_some();

            if !product_exists {
                return Err(AppError::NotFound(
                    self.messages.resources.product_not_found.clone(),
                ));
            }
        }

        Ok(ListingPage::from(Page::new(
            listings.into_iter().map(ListingResponse::from).collect(),
            limit,
            offset,
            total,
        )))
    }

    /// Tracks a listing of one of the owner's products.
    ///
    /// A missing or foreign product is the product's `404`; a URL this owner
    /// already tracks is the index's `409`.
    pub async fn create_listing(
        &self,
        owner: Uuid,
        product_id: Uuid,
        request: CreateListingRequest,
    ) -> AppResult<ListingResponse> {
        let retailer = validate_retailer(&request.retailer, &self.messages)?;
        let url = validate_listing_url(&request.url, &self.messages)?;
        let currency = validate_currency_code(&request.currency, &self.messages)?;
        let external_id = request
            .external_id
            .as_deref()
            .map(|value| validate_external_id(value, &self.messages))
            .transpose()?;

        let listing = match self
            .repository
            .insert_listing(
                owner,
                product_id,
                &NewListing {
                    id: Uuid::new_v4(),
                    retailer: &retailer,
                    url: &url,
                    currency: &currency,
                    external_id: external_id.as_deref(),
                },
            )
            .await?
        {
            ListingWrite::Applied(listing) => listing,
            ListingWrite::DuplicateUrl => {
                return Err(AppError::Conflict(
                    self.messages.conflicts.listing_already_tracked.clone(),
                ));
            }
            ListingWrite::ProductNotFound | ListingWrite::ListingNotFound => {
                return Err(AppError::NotFound(
                    self.messages.resources.product_not_found.clone(),
                ));
            }
        };

        Ok(ListingResponse::from(listing))
    }

    /// Applies a partial update to one listing of the owner's product.
    ///
    /// The listing id alone is never enough to address a row: the statement
    /// also binds the product and the owner, so a listing that exists under
    /// another product of the same caller is a `404`.
    pub async fn update_listing(
        &self,
        owner: Uuid,
        product_id: Uuid,
        listing_id: Uuid,
        request: UpdateListingRequest,
    ) -> AppResult<ListingResponse> {
        let (clear_external_id, external_id) =
            resolve_external_id(request.external_id_change(), &self.messages)?;

        let listing = self
            .repository
            .update_listing_owned(
                product_id,
                owner,
                listing_id,
                &ListingPatch {
                    clear_external_id,
                    external_id: external_id.as_deref(),
                    availability: request.availability,
                    validation_state: request.validation_state,
                },
            )
            .await?
            .ok_or_else(|| AppError::NotFound(self.messages.resources.listing_not_found.clone()))?;

        Ok(ListingResponse::from(listing))
    }

    /// Deletes one listing of the owner's product, with its observations and
    /// the alerts those observations triggered.
    pub async fn delete_listing(
        &self,
        owner: Uuid,
        product_id: Uuid,
        listing_id: Uuid,
    ) -> AppResult<()> {
        let deleted = self
            .repository
            .delete_listing_owned(product_id, owner, listing_id)
            .await?;

        if !deleted {
            return Err(AppError::NotFound(
                self.messages.resources.listing_not_found.clone(),
            ));
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Price observations
    // -----------------------------------------------------------------------

    /// Records one collected price, or absorbs a repeated fingerprint.
    ///
    /// The rule evaluation runs inside the same transaction as the insert,
    /// which is why this method carries no evaluation logic of its own: the
    /// alert exists exactly when the transaction committed a new observation
    /// that triggered, and a duplicate can never re-fire it.
    pub async fn record_observation(
        &self,
        owner: Uuid,
        listing_id: Uuid,
        request: CreateObservationRequest,
    ) -> AppResult<RecordedObservation> {
        let amount = validate_money("amount", &request.amount, &self.messages)?;
        let currency = validate_currency_code(&request.currency, &self.messages)?;
        let fingerprint = validate_fingerprint(&request.fingerprint, &self.messages)?;
        let availability = request.availability.unwrap_or(ListingAvailability::Unknown);

        let outcome = self
            .repository
            .insert_observation(
                owner,
                listing_id,
                &NewObservation {
                    id: Uuid::new_v4(),
                    alert_id: Uuid::new_v4(),
                    amount: &amount,
                    currency: &currency,
                    observed_at: request.observed_at,
                    fingerprint: &fingerprint,
                    availability,
                    run_id: request.run_id,
                },
            )
            .await?;

        match outcome {
            ObservationWrite::Applied(observation, _alert_inserted) => Ok(RecordedObservation {
                observation: ObservationResponse::from(observation),
                created: true,
            }),
            ObservationWrite::Absorbed(observation) => Ok(RecordedObservation {
                observation: ObservationResponse::from(observation),
                created: false,
            }),
            ObservationWrite::ListingNotFound => Err(AppError::NotFound(
                self.messages.resources.listing_not_found.clone(),
            )),
            ObservationWrite::RunNotFound => Err(AppError::NotFound(
                self.messages.resources.run_not_found.clone(),
            )),
        }
    }

    /// Lists the observations of one of the owner's listings, newest first.
    ///
    /// An empty page probes the listing once on the error path, so a foreign
    /// listing is the listing's `404` and a listing with no history is an
    /// honest empty page.
    pub async fn list_observations(
        &self,
        owner: Uuid,
        listing_id: Uuid,
        pagination: PaginationQuery,
    ) -> AppResult<ObservationPage> {
        let limit = pagination.effective_limit(&self.pagination);
        let offset = pagination.effective_offset();

        let (observations, total) = self
            .repository
            .list_observations_owned(listing_id, owner, limit, offset)
            .await?;

        if total == 0 {
            let listing_exists = self.repository.listing_is_owned(listing_id, owner).await?;

            if !listing_exists {
                return Err(AppError::NotFound(
                    self.messages.resources.listing_not_found.clone(),
                ));
            }
        }

        Ok(ObservationPage::from(Page::new(
            observations
                .into_iter()
                .map(ObservationResponse::from)
                .collect(),
            limit,
            offset,
            total,
        )))
    }

    /// Reads the newest observation of one of the owner's listings.
    ///
    /// A listing with no observations is a `404` on the observation, and a
    /// foreign or absent listing is a `404` on the listing: one owner-scoped
    /// read on the error path names which, without disclosing anything a
    /// caller may not know.
    pub async fn latest_observation(
        &self,
        owner: Uuid,
        listing_id: Uuid,
    ) -> AppResult<ObservationResponse> {
        let observation = self
            .repository
            .find_latest_observation(listing_id, owner)
            .await?;

        let Some(observation) = observation else {
            let listing_exists = self.repository.listing_is_owned(listing_id, owner).await?;

            return Err(AppError::NotFound(if listing_exists {
                self.messages.resources.observation_not_found.clone()
            } else {
                self.messages.resources.listing_not_found.clone()
            }));
        };

        Ok(ObservationResponse::from(observation))
    }

    // -----------------------------------------------------------------------
    // Monitoring rules
    // -----------------------------------------------------------------------

    /// Lists the rules of one of the owner's products. At most one rule can
    /// be on the page, because the schema allows one rule per product.
    ///
    /// An empty page probes the product once on the error path, so a foreign
    /// product is the product's `404`.
    pub async fn list_rules(
        &self,
        owner: Uuid,
        product_id: Uuid,
        pagination: PaginationQuery,
    ) -> AppResult<RulePage> {
        let limit = pagination.effective_limit(&self.pagination);
        let offset = pagination.effective_offset();

        let (rules, total) = self
            .repository
            .list_rules_owned(product_id, owner, limit, offset)
            .await?;

        if total == 0 {
            let product_exists = self
                .repository
                .find_product_owned(product_id, owner)
                .await?
                .is_some();

            if !product_exists {
                return Err(AppError::NotFound(
                    self.messages.resources.product_not_found.clone(),
                ));
            }
        }

        Ok(RulePage::from(Page::new(
            rules.into_iter().map(RuleResponse::from).collect(),
            limit,
            offset,
            total,
        )))
    }

    /// Creates the rule for one of the owner's products.
    ///
    /// A product has at most one rule; the unique index, not a pre-check, is
    /// what refuses a second, so two concurrent creates cannot both pass.
    pub async fn create_rule(
        &self,
        owner: Uuid,
        product_id: Uuid,
        request: CreateRuleRequest,
    ) -> AppResult<RuleResponse> {
        let target_amount =
            validate_money("target_amount", &request.target_amount, &self.messages)?;
        let target_currency = validate_currency_code(&request.target_currency, &self.messages)?;
        let percent_drop = request
            .percent_drop
            .map(|value| validate_percent_drop(value, &self.messages))
            .transpose()?;

        let rule = match self
            .repository
            .insert_rule(
                Uuid::new_v4(),
                owner,
                product_id,
                &target_amount,
                &target_currency,
                percent_drop,
            )
            .await?
        {
            RuleWrite::Applied(rule) => rule,
            RuleWrite::RuleExists => {
                return Err(AppError::Conflict(
                    self.messages.conflicts.rule_already_exists.clone(),
                ));
            }
            RuleWrite::ProductNotFound => {
                return Err(AppError::NotFound(
                    self.messages.resources.product_not_found.clone(),
                ));
            }
        };

        Ok(RuleResponse::from(rule))
    }

    /// Applies a partial update to one rule of the owner's product.
    ///
    /// Both the product and the rule are bound, so a rule id that exists
    /// under another product of the same caller is the rule's `404`.
    pub async fn update_rule(
        &self,
        owner: Uuid,
        product_id: Uuid,
        rule_id: Uuid,
        request: UpdateRuleRequest,
    ) -> AppResult<RuleResponse> {
        let target_amount = request
            .target_amount
            .as_deref()
            .map(|value| validate_money("target_amount", value, &self.messages))
            .transpose()?;
        let target_currency = request
            .target_currency
            .as_deref()
            .map(|value| validate_currency_code(value, &self.messages))
            .transpose()?;
        let percent_drop = request
            .percent_drop_change()
            .1
            .map(|value| validate_percent_drop(value, &self.messages))
            .transpose()?;

        let rule = self
            .repository
            .update_rule_owned(
                product_id,
                owner,
                rule_id,
                &RulePatch {
                    enabled: request.enabled,
                    target_amount: target_amount.as_deref(),
                    target_currency: target_currency.as_deref(),
                    clear_percent_drop: request.percent_drop_change().0,
                    percent_drop,
                },
            )
            .await?
            .ok_or_else(|| AppError::NotFound(self.messages.resources.rule_not_found.clone()))?;

        Ok(RuleResponse::from(rule))
    }

    /// Deletes one rule of the owner's product, with the alerts it raised.
    pub async fn delete_rule(&self, owner: Uuid, product_id: Uuid, rule_id: Uuid) -> AppResult<()> {
        let deleted = self
            .repository
            .delete_rule_owned(product_id, owner, rule_id)
            .await?;

        if !deleted {
            return Err(AppError::NotFound(
                self.messages.resources.rule_not_found.clone(),
            ));
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Automation runs
    // -----------------------------------------------------------------------

    /// Starts one automation run for the owner.
    pub async fn start_run(&self, owner: Uuid) -> AppResult<RunResponse> {
        let run = self.repository.insert_run(Uuid::new_v4(), owner).await?;

        Ok(RunResponse::from(run))
    }

    /// Lists the owner's runs, newest first.
    pub async fn list_runs(&self, owner: Uuid, pagination: PaginationQuery) -> AppResult<RunPage> {
        let limit = pagination.effective_limit(&self.pagination);
        let offset = pagination.effective_offset();

        let (runs, total) = self
            .repository
            .list_runs_owned(owner, limit, offset)
            .await?;

        Ok(RunPage::from(Page::new(
            runs.into_iter().map(RunResponse::from).collect(),
            limit,
            offset,
            total,
        )))
    }

    /// Finishes one running run of the owner.
    ///
    /// The guard is the statement's `WHERE outcome = 'running'`; the read
    /// below only names the reason a zero-row result produced — absent or
    /// foreign (`404`), or already finished (`409`).
    pub async fn finish_run(
        &self,
        owner: Uuid,
        run_id: Uuid,
        request: FinishRunRequest,
    ) -> AppResult<RunResponse> {
        if !request.outcome.is_terminal() {
            return Err(field_error(
                "outcome",
                self.messages.validation.run_outcome_not_terminal.clone(),
                &self.messages,
            ));
        }

        let observations = validate_count("observations", request.observations, &self.messages)?;
        let alerts = validate_count("alerts", request.alerts, &self.messages)?;
        let failures = validate_count("failures", request.failures, &self.messages)?;
        let (clear_error_summary, error_summary) =
            resolve_error_summary(request.error_summary_change(), &self.messages)?;

        let run = self
            .repository
            .finish_run_owned(
                run_id,
                owner,
                &RunFinish {
                    outcome: request.outcome,
                    observations,
                    alerts,
                    failures,
                    clear_error_summary,
                    error_summary: error_summary.as_deref(),
                },
            )
            .await?;

        let Some(run) = run else {
            let exists = self
                .repository
                .find_run_owned(run_id, owner)
                .await?
                .is_some();

            if exists {
                return Err(AppError::Conflict(
                    self.messages.conflicts.run_already_finished.clone(),
                ));
            }

            return Err(AppError::NotFound(
                self.messages.resources.run_not_found.clone(),
            ));
        };

        Ok(RunResponse::from(run))
    }

    // -----------------------------------------------------------------------
    // Price alerts
    // -----------------------------------------------------------------------

    /// Lists the owner's alerts, newest first, optionally unread only.
    pub async fn list_alerts(
        &self,
        owner: Uuid,
        query: AlertQuery,
        pagination: PaginationQuery,
    ) -> AppResult<AlertPage> {
        let unread_only = query.unread.unwrap_or(false);

        let limit = pagination.effective_limit(&self.pagination);
        let offset = pagination.effective_offset();

        let (alerts, total) = self
            .repository
            .list_alerts_owned(owner, unread_only, limit, offset)
            .await?;

        Ok(AlertPage::from(Page::new(
            alerts.into_iter().map(AlertResponse::from).collect(),
            limit,
            offset,
            total,
        )))
    }

    /// Loads one of the owner's alerts.
    pub async fn get_alert(&self, owner: Uuid, alert_id: Uuid) -> AppResult<AlertResponse> {
        let alert = self
            .repository
            .find_alert_owned(alert_id, owner)
            .await?
            .ok_or_else(|| AppError::NotFound(self.messages.resources.alert_not_found.clone()))?;

        Ok(AlertResponse::from(alert))
    }

    /// Applies a partial update to one of the owner's alerts: the read mark
    /// and the delivery status.
    pub async fn update_alert(
        &self,
        owner: Uuid,
        alert_id: Uuid,
        request: UpdateAlertRequest,
    ) -> AppResult<AlertResponse> {
        let (set_read, clear_read) = match request.read {
            Some(true) => (true, false),
            Some(false) => (false, true),
            None => (false, false),
        };

        let alert = self
            .repository
            .update_alert_owned(
                alert_id,
                owner,
                set_read,
                clear_read,
                request.notification_status,
            )
            .await?
            .ok_or_else(|| AppError::NotFound(self.messages.resources.alert_not_found.clone()))?;

        Ok(AlertResponse::from(alert))
    }
}

// ---------------------------------------------------------------------------
// Validators
// ---------------------------------------------------------------------------

/// Validates a brand.
///
/// Trimmed before it is measured, so 100 characters surrounded by spaces is
/// accepted and stored trimmed. A brand that is empty after trimming is
/// rejected: a product always has one, so an empty one is not a "clear".
pub fn validate_brand(brand: &str, messages: &Messages) -> AppResult<String> {
    required_text(
        "brand",
        brand,
        MAX_BRAND_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

/// Validates a model, with the same rules as the brand.
pub fn validate_model(model: &str, messages: &Messages) -> AppResult<String> {
    required_text(
        "model",
        model,
        MAX_MODEL_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

/// Validates the configuration text.
///
/// The specs are kept verbatim, so `""` clears them and a value of spaces is
/// stored as it was sent. That mirrors the column, which is
/// `NOT NULL DEFAULT ''` and measured by
/// `tracked_products_specs_length_chk`.
pub fn validate_specs(specs: &str, messages: &Messages) -> AppResult<String> {
    bounded_text(
        "specs",
        specs,
        MAX_SPECS_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

/// Validates a retailer label, with the same rules as the brand.
pub fn validate_retailer(retailer: &str, messages: &Messages) -> AppResult<String> {
    required_text(
        "retailer",
        retailer,
        MAX_RETAILER_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

/// Validates a listing URL and returns it trimmed.
///
/// The accepted form is an absolute `http` or `https` URL of at most
/// [`MAX_LISTING_URL_LENGTH`] characters: the column's own
/// `product_listings_url_chk` requires `~* '^https?://'`, and a relative path
/// or a `javascript:`/`file:`/`data:` scheme has no meaning a client could
/// follow. The scheme comparison is case-insensitive, matching the column's
/// `~*`, and the service additionally requires a non-empty authority — the
/// same narrowing `validate_receipt_url` applies to its column, which accepts
/// `"https:///"` and must not see it stored.
pub fn validate_listing_url(url: &str, messages: &Messages) -> AppResult<String> {
    let trimmed = url.trim();
    let lowercase = trimmed.to_ascii_lowercase();

    let rest = if lowercase.starts_with("https://") {
        &trimmed[8..]
    } else if lowercase.starts_with("http://") {
        &trimmed[7..]
    } else {
        ""
    };

    // An empty `rest` yields `None`, which is also a refusal: `"https://"`
    // has no authority either.
    let has_authority = rest
        .chars()
        .next()
        .is_some_and(|first| !first.is_whitespace() && !matches!(first, '/' | '?' | '#'));

    if !has_authority || crate::utils::text::char_len(trimmed) > MAX_LISTING_URL_LENGTH {
        return Err(field_error(
            "url",
            messages.validation.listing_url_invalid.clone(),
            messages,
        ));
    }

    Ok(trimmed.to_string())
}

/// Validates the retailer's external identifier.
pub fn validate_external_id(value: &str, messages: &Messages) -> AppResult<String> {
    required_text(
        "external_id",
        value,
        crate::monitoring::dto::MAX_EXTERNAL_ID_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

/// Validates an observation fingerprint.
///
/// The fingerprint is the idempotency identity the workflow computes; the
/// service only bounds it, because its content is the workflow's business.
pub fn validate_fingerprint(fingerprint: &str, messages: &Messages) -> AppResult<String> {
    required_text(
        "fingerprint",
        fingerprint,
        MAX_FINGERPRINT_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

/// Validates and normalises a currency code.
///
/// The same rule the reimbursements module applies to its currency: trimmed,
/// upper-cased, exactly three ASCII letters, matching the columns'
/// `'^[A-Z]{3}$'`. It is written here rather than imported from that
/// module's service, because a feature module does not call another
/// feature module's service — the rule is four lines, and the two copies
/// assert the same wire form in their own unit tests.
pub fn validate_currency_code(currency: &str, messages: &Messages) -> AppResult<String> {
    let normalised = currency.trim().to_ascii_uppercase();

    if normalised.len() != 3 || !normalised.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(field_error(
            "currency",
            messages.validation.currency_invalid.clone(),
            messages,
        ));
    }

    Ok(normalised)
}

/// Validates a percent drop against the column's `CHECK`.
///
/// 0 would fire on every observation and 100 can never be reached, which is
/// why the accepted range is 1..99 — the range
/// `monitoring_rules_percent_drop_chk` enforces.
pub fn validate_percent_drop(value: i32, messages: &Messages) -> AppResult<i32> {
    if !(MIN_PERCENT_DROP..=MAX_PERCENT_DROP).contains(&value) {
        return Err(field_error(
            "percent_drop",
            messages.validation.percent_drop_range.clone(),
            messages,
        ));
    }

    Ok(value)
}

/// Validates a reported count.
pub fn validate_count(
    field: &str,
    value: Option<i32>,
    messages: &Messages,
) -> AppResult<Option<i32>> {
    if value.is_some_and(|count| count < 0) {
        return Err(field_error(
            field,
            messages.validation.count_negative.clone(),
            messages,
        ));
    }

    Ok(value)
}

/// Normalises a search term: trimmed, and `None` when it is empty.
///
/// An empty term means "no filter", which is the published rule for this
/// parameter (and for no other). It is not an error.
pub fn normalize_search(term: &str, messages: &Messages) -> AppResult<Option<String>> {
    let trimmed = validate_search(term, messages)?;

    Ok(Some(trimmed).filter(|value| !value.is_empty()))
}

/// Derives the tracking identity from the effective fields.
///
/// The normalised name is the trimmed, lower-cased `brand` followed by the
/// `model`; the configuration hash is the lower-case SHA-256 hex digest of
/// the configuration text. Both are **pure functions of the effective
/// fields**, and they are the only two writers of those columns — which is
/// what keeps the create path and the patch path from ever disagreeing about
/// what the same (brand, model, specs) identifies as.
///
/// The derived name's length is **not** checked here: lower-casing is not
/// length-preserving in Unicode, so the two call sites check it in the shape
/// their callers can act on — [`ensure_identity`] turns it into a `422` on
/// the create path, and the repository's update transaction reports it as
/// [`ProductWrite::IdentityTooLong`], which this service maps to the same
/// `422`.
pub fn derive_identity(brand: &str, model: &str, specs: &str) -> (String, String) {
    let normalized_name = format!("{brand} {model}").trim().to_lowercase();

    let digest = Sha256::digest(specs.as_bytes());
    let config_hash = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    (normalized_name, config_hash)
}

/// Derives the tracking identity and refuses an over-long derived name.
///
/// This is the create path's gate: a `422` naming `normalized_name` before
/// any statement runs. The column's `CHECK` is the enforced copy of the same
/// bound.
///
/// # Errors
///
/// Returns a `422` naming `normalized_name` when the derived name is longer
/// than the column's own bound.
pub fn ensure_identity(
    brand: &str,
    model: &str,
    specs: &str,
    messages: &Messages,
) -> AppResult<(String, String)> {
    let (normalized_name, config_hash) = derive_identity(brand, model, specs);

    if crate::utils::text::char_len(&normalized_name) > MAX_NORMALIZED_NAME_LENGTH {
        return Err(field_error(
            "normalized_name",
            render(
                &messages.validation.too_long,
                &[MAX_NORMALIZED_NAME_LENGTH.to_string()],
            ),
            messages,
        ));
    }

    Ok((normalized_name, config_hash))
}

/// Resolves the external-identifier tri-state into the pair the statement
/// binds.
///
/// `(true, None)` clears the column; `(false, None)` leaves it untouched;
/// `(false, Some(value))` writes the validated value.
fn resolve_external_id(
    change: (bool, Option<&str>),
    messages: &Messages,
) -> AppResult<(bool, Option<String>)> {
    match change {
        (true, _) => Ok((true, None)),
        (false, None) => Ok((false, None)),
        (false, Some(raw)) => Ok((false, Some(validate_external_id(raw, messages)?))),
    }
}

/// Resolves the error-summary tri-state into the pair the statement binds.
///
/// An empty summary after trimming is a validation failure rather than a
/// clear: the column is `NULL`-able, so clearing is spelled with an explicit
/// `null`, and a `''` row would violate
/// `automation_runs_error_summary_length_chk`.
fn resolve_error_summary(
    change: (bool, Option<&str>),
    messages: &Messages,
) -> AppResult<(bool, Option<String>)> {
    match change {
        (true, _) => Ok((true, None)),
        (false, None) => Ok((false, None)),
        (false, Some(raw)) => {
            let summary = required_text(
                "error_summary",
                raw,
                MAX_ERROR_SUMMARY_LENGTH,
                &messages.validation.too_long,
                messages,
            )?;

            Ok((false, Some(summary)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::messages::test_messages;
    use crate::error::code;
    use crate::monitoring::models::RunOutcome;

    #[test]
    fn a_currency_is_three_letters_and_is_upper_cased() {
        let messages = test_messages();

        assert_eq!(validate_currency_code("eur", &messages).unwrap(), "EUR");
        assert_eq!(validate_currency_code("  Eur  ", &messages).unwrap(), "EUR");
        assert_eq!(validate_currency_code("USD", &messages).unwrap(), "USD");

        for rejected in ["EU", "EURO", "1US", "EU1", "", "   ", "€UR"] {
            assert!(
                validate_currency_code(rejected, &messages).is_err(),
                "{rejected:?} must be refused"
            );
        }
    }

    #[test]
    fn a_listing_url_must_be_an_absolute_http_url() {
        let messages = test_messages();

        for accepted in [
            "https://www.notebooksbilliger.de/product/x1-carbon",
            "http://example.test/a",
            "HTTPS://EXAMPLE.TEST/A",
        ] {
            assert_eq!(
                validate_listing_url(accepted, &messages).unwrap(),
                accepted,
                "{accepted:?} is an absolute http(s) URL and must be accepted"
            );
        }

        for refused in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/plain,hello",
            "example.test/listing",
            "/listings/1",
            "https://",
            "",
            "   ",
            // A scheme with an empty authority is not an absolute URL, even
            // though `product_listings_url_chk` accepts it.
            "https:///",
            "http:///listings/1",
            "https://?p=7",
            "https://#listing",
            "https:// /listings/1",
        ] {
            let error = validate_listing_url(refused, &messages)
                .expect_err("a listing must be an absolute http or https URL");
            assert_eq!(error.body().error.details[0].field, "url");
        }

        // The published bound is 2048 characters, measured on the trimmed
        // value.
        let long = format!(
            "https://example.test/{}",
            "a".repeat(MAX_LISTING_URL_LENGTH)
        );
        assert!(validate_listing_url(&long, &messages).is_err());
        let at_bound = format!(
            "https://example.test/{}",
            "a".repeat(MAX_LISTING_URL_LENGTH - "https://example.test/".len())
        );
        assert!(validate_listing_url(&at_bound, &messages).is_ok());
    }

    #[test]
    fn a_percent_drop_is_bounded_to_one_through_ninety_nine() {
        let messages = test_messages();

        assert_eq!(validate_percent_drop(1, &messages).unwrap(), 1);
        assert_eq!(validate_percent_drop(99, &messages).unwrap(), 99);

        for refused in [0, -1, 100, 1_000] {
            let error = validate_percent_drop(refused, &messages)
                .expect_err("a percent drop outside 1..99 must be refused");
            assert_eq!(error.body().error.details[0].field, "percent_drop");
        }
    }

    #[test]
    fn a_negative_count_is_refused_and_a_missing_one_passes() {
        let messages = test_messages();

        assert_eq!(validate_count("alerts", None, &messages).unwrap(), None);
        assert_eq!(
            validate_count("alerts", Some(0), &messages).unwrap(),
            Some(0)
        );
        assert_eq!(
            validate_count("alerts", Some(12), &messages).unwrap(),
            Some(12)
        );

        let error = validate_count("alerts", Some(-1), &messages)
            .expect_err("a negative count must be refused");
        assert_eq!(error.body().error.code, code::VALIDATION_FAILED);
        assert_eq!(error.body().error.details[0].field, "alerts");
    }

    #[test]
    fn the_identity_is_deterministic_and_the_hash_is_lower_case_hex() {
        let messages = test_messages();

        // The inputs are what the validators produced — trimmed — so the
        // derivation sees trimmed parts, exactly as the service passes them.
        let (name, hash) = derive_identity("Lenovo", "ThinkPad X1 Carbon", "32GB RAM");
        assert_eq!(name, "lenovo thinkpad x1 carbon");
        assert_eq!(hash.len(), 64);
        assert!(
            hash.bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        );

        // The same fields derive the same identity: the create path and the
        // patch path must never disagree.
        let (again, _) = derive_identity("Lenovo", "ThinkPad X1 Carbon", "32GB RAM");
        assert_eq!(again, name);

        // A different configuration is a different identity, even for the
        // same model.
        let (_, other_hash) = derive_identity("Lenovo", "ThinkPad X1 Carbon", "16GB RAM");
        assert_ne!(other_hash, hash);

        // An empty configuration still hashes: the identity of a product
        // with no specs is the hash of the empty string, not a null.
        let (bare_name, bare_hash) = derive_identity("Lenovo", "X1", "");
        assert_eq!(bare_name, "lenovo x1");
        assert_eq!(bare_hash.len(), 64);

        // The validating gate accepts what the column accepts and refuses
        // what the column would refuse.
        assert!(ensure_identity("Lenovo", "X1", "", &messages).is_ok());
    }

    /// The derived name is bounded because lower-casing is not
    /// length-preserving in Unicode: `U+0130` (`İ`) folds to two characters.
    #[test]
    fn an_over_long_derived_name_is_refused_before_the_database() {
        let messages = test_messages();

        // Two parts at their own bounds, each padded with a folding character
        // that expands, push the derived name past the column's bound.
        let brand = "İ".repeat(MAX_BRAND_LENGTH);
        let model = "İ".repeat(MAX_MODEL_LENGTH);

        let error = ensure_identity(&brand, &model, "", &messages)
            .expect_err("a derived name over its bound must be refused");
        let body = error.body();
        assert_eq!(error.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body.error.details[0].field, "normalized_name");
    }

    #[test]
    fn a_blank_search_means_no_filter() {
        let messages = test_messages();

        assert_eq!(normalize_search("   ", &messages).unwrap(), None);
        assert_eq!(normalize_search("", &messages).unwrap(), None);
        assert_eq!(
            normalize_search("  thinkpad  ", &messages).unwrap(),
            Some("thinkpad".to_string())
        );
    }

    #[test]
    fn an_empty_error_summary_is_a_failure_and_a_null_is_a_clear() {
        let messages = test_messages();

        let (clear, value) = resolve_error_summary((true, Some("   ")), &messages).unwrap();
        assert!(clear);
        assert_eq!(value, None);

        let (clear, value) = resolve_error_summary((false, None), &messages).unwrap();
        assert!(!clear);
        assert_eq!(value, None);

        let (clear, value) =
            resolve_error_summary((false, Some("  two sources refused  ")), &messages).unwrap();
        assert!(!clear);
        assert_eq!(value.as_deref(), Some("two sources refused"));

        let error = resolve_error_summary((false, Some("")), &messages)
            .expect_err("an empty summary would violate the column's length range");
        assert_eq!(error.body().error.details[0].field, "error_summary");

        let over_long = "x".repeat(MAX_ERROR_SUMMARY_LENGTH + 1);
        let error = resolve_error_summary((false, Some(&over_long)), &messages)
            .expect_err("an over-long summary must be refused");
        assert_eq!(error.body().error.details[0].field, "error_summary");
    }

    #[test]
    fn the_run_outcome_terminal_set_is_exactly_the_three_finished_states() {
        assert!(RunOutcome::Ok.is_terminal());
        assert!(RunOutcome::Partial.is_terminal());
        assert!(RunOutcome::Failed.is_terminal());
        assert!(!RunOutcome::Running.is_terminal());
    }
}
