//! Price-monitoring persistence.
//!
//! Every statement is scoped by `user_id` in its own `WHERE` clause, never by
//! a check in the handler (`ADR-0005`). A row belonging to another user is
//! therefore invisible to the repository, which is what makes the `404`-not-
//! `403` guarantee enforceable in one place.
//!
//! # Money
//!
//! Every money column is selected as `amount::text` / `target_amount::text`
//! and decodes into a [`String`]; see [`crate::monitoring::models`]. The `AS`
//! alias is not strictly required today — PostgreSQL names a cast expression
//! after its column — but it makes the `FromRow` mapping independent of that
//! detail. An amount is bound back as a string cast to `numeric`; no Rust
//! code ever parses money (`ADR-0007`).
//!
//! # The composite keys
//!
//! Every child statement carries both the parent id and the owner, so a child
//! addressed under a parent it does not belong to matches no row and becomes
//! the child's `404` — exactly the pattern `0004`/`0005` established.
//!
//! # Idempotency and the rule evaluation
//!
//! The observation write is the one multi-statement transaction of this
//! module. It locks the listing row (`FOR UPDATE`), absorbs a repeated
//! fingerprint through `ON CONFLICT … DO NOTHING` against
//! `price_observations_user_id_listing_fingerprint_unique_idx`, evaluates the
//! product's enabled rule against the new observation, inserts the alert with
//! `ON CONFLICT (rule_id, observation_id) DO NOTHING` against
//! `price_alerts_rule_observation_unique`, and maintains the listing's
//! `last_observed_at` — all inside one transaction, so a rule fires exactly
//! once per new observation or not at all. The duplicate-alert guarantee is
//! the database's unique index, not a check the service ran first.

use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppResult;
use crate::monitoring::models::{
    AutomationRun, ListingAvailability, ListingValidationState, MonitoringRule, NotificationStatus,
    PriceAlert, PriceObservation, ProductListing, ProductStatus, RunOutcome, TrackedProduct,
};
use crate::monitoring::service::derive_identity;
/// Persistence operations for the price-monitoring domain.
#[derive(Debug, Clone)]
pub struct MonitoringRepository {
    pool: PgPool,
}

impl MonitoringRepository {
    /// Creates a repository bound to a connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // -----------------------------------------------------------------------
    // Tracked products
    // -----------------------------------------------------------------------

    /// Inserts a tracked product for `owner`.
    ///
    /// The id and the identity columns come from [`NewProduct`]; the identity
    /// was derived by the service, so a `409` is the unique index's verdict
    /// about the same pair a client would recompute. The only two outcomes
    /// are the row and the duplicate: every input was validated before this
    /// statement runs.
    pub async fn insert_product(
        &self,
        owner: Uuid,
        product: &NewProduct<'_>,
    ) -> AppResult<ProductWrite<TrackedProduct>> {
        const INSERT: &str = r#"
            INSERT INTO tracked_products
                (id, user_id, brand, model, specs, normalized_name, config_hash, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'active')
            RETURNING id, user_id, brand, model, specs, normalized_name, config_hash,
                      status, created_at, updated_at
        "#;

        match sqlx::query_as::<_, TrackedProduct>(INSERT)
            .bind(product.id)
            .bind(owner)
            .bind(product.brand)
            .bind(product.model)
            .bind(product.specs)
            .bind(product.normalized_name)
            .bind(product.config_hash)
            .fetch_optional(&self.pool)
            .await
        {
            Ok(Some(product)) => Ok(ProductWrite::Applied(product)),
            Ok(None) => Ok(ProductWrite::NotFound),
            Err(error)
                if is_unique_violation(&error, "tracked_products_user_id_identity_unique_idx") =>
            {
                Ok(ProductWrite::DuplicateIdentity)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Finds one product of the caller.
    pub async fn find_product_owned(
        &self,
        product_id: Uuid,
        owner: Uuid,
    ) -> AppResult<Option<TrackedProduct>> {
        const FIND: &str = r#"
            SELECT id, user_id, brand, model, specs, normalized_name, config_hash,
                   status, created_at, updated_at
            FROM tracked_products
            WHERE id = $1 AND user_id = $2
        "#;

        let product = sqlx::query_as::<_, TrackedProduct>(FIND)
            .bind(product_id)
            .bind(owner)
            .fetch_optional(&self.pool)
            .await?;

        Ok(product)
    }

    /// The product page statement.
    ///
    /// Volumes per owner are small (a human tracks a handful of laptops), so
    /// one statement shape serves the listing with and without the status
    /// filter: a second shape, in the style of `0004`'s two page statements,
    /// would buy a planner benefit this table's indexes do not offer.
    const PAGE_PRODUCTS: &'static str = r#"
        SELECT id, user_id, brand, model, specs, normalized_name, config_hash,
               status, created_at, updated_at
        FROM tracked_products
        WHERE user_id = $1
          AND ($2::text IS NULL OR status = $2)
          AND ($3::text IS NULL
               OR brand ILIKE '%' || $3 || '%'
               OR model ILIKE '%' || $3 || '%'
               OR normalized_name ILIKE '%' || $3 || '%')
        ORDER BY created_at DESC, id DESC
        LIMIT $4 OFFSET $5
    "#;

    /// The count statement matching [`Self::PAGE_PRODUCTS`].
    const COUNT_PRODUCTS: &'static str = r#"
        SELECT COUNT(*)
        FROM tracked_products
        WHERE user_id = $1
          AND ($2::text IS NULL OR status = $2)
          AND ($3::text IS NULL
               OR brand ILIKE '%' || $3 || '%'
               OR model ILIKE '%' || $3 || '%'
               OR normalized_name ILIKE '%' || $3 || '%')
    "#;

    /// Lists the products owned by `owner`, newest first, with the total of
    /// the filtered set.
    ///
    /// `status` is bound as text and cast in the statement, because the
    /// column is a `CHECK`ed `TEXT` and not a PostgreSQL enum type.
    pub async fn list_products_owned(
        &self,
        owner: Uuid,
        status: Option<ProductStatus>,
        search: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> AppResult<(Vec<TrackedProduct>, i64)> {
        let status_text = status.map(|status| status.as_str().to_string());

        let items = sqlx::query_as::<_, TrackedProduct>(Self::PAGE_PRODUCTS)
            .bind(owner)
            .bind(status_text.clone())
            .bind(search)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        let total = sqlx::query_scalar::<_, i64>(Self::COUNT_PRODUCTS)
            .bind(owner)
            .bind(status_text)
            .bind(search)
            .fetch_one(&self.pool)
            .await?;

        Ok((items, total))
    }

    /// Applies a partial update to one product of the caller.
    ///
    /// One transaction, in this order: lock the row and read it (`404` if
    /// absent or foreign); merge the patch into the row; re-derive the
    /// identity from the **effective** fields; write the full row. The merge
    /// happens under the lock, so the read-modify-write is atomic — a
    /// `COALESCE`-shaped single statement cannot express "re-derive from the
    /// effective brand and model", which is why this write is a transaction
    /// instead.
    pub async fn update_product_owned(
        &self,
        product_id: Uuid,
        owner: Uuid,
        patch: &ProductPatch<'_>,
    ) -> AppResult<ProductWrite<TrackedProduct>> {
        const LOCK: &str = r#"
            SELECT id, user_id, brand, model, specs, normalized_name, config_hash,
                   status, created_at, updated_at
            FROM tracked_products
            WHERE id = $1 AND user_id = $2
            FOR UPDATE
        "#;

        const UPDATE: &str = r#"
            UPDATE tracked_products
            SET brand = $3,
                model = $4,
                specs = $5,
                status = $6,
                normalized_name = $7,
                config_hash = $8,
                updated_at = clock_timestamp()
            WHERE id = $1 AND user_id = $2
            RETURNING id, user_id, brand, model, specs, normalized_name, config_hash,
                      status, created_at, updated_at
        "#;

        let mut transaction = self.pool.begin().await?;

        let Some(current) = sqlx::query_as::<_, TrackedProduct>(LOCK)
            .bind(product_id)
            .bind(owner)
            .fetch_optional(&mut *transaction)
            .await?
        else {
            transaction.commit().await?;
            return Ok(ProductWrite::NotFound);
        };

        let brand = patch.brand.unwrap_or(&current.brand);
        let model = patch.model.unwrap_or(&current.model);
        let specs = patch.specs.unwrap_or(&current.specs);
        let status = patch.status.unwrap_or(current.status);
        let (normalized_name, config_hash) = derive_identity(brand, model, specs);

        // Lower-casing is not length-preserving in Unicode, so an over-long
        // derived name is a real (if pathological) outcome of a legal patch.
        // The refusal is a variant, not an error: the messages are the
        // service's, and the same value is a `422` on the create path.
        if crate::utils::text::char_len(&normalized_name)
            > crate::monitoring::dto::MAX_NORMALIZED_NAME_LENGTH
        {
            transaction.commit().await?;
            return Ok(ProductWrite::IdentityTooLong);
        }

        let updated = match sqlx::query_as::<_, TrackedProduct>(UPDATE)
            .bind(product_id)
            .bind(owner)
            .bind(brand)
            .bind(model)
            .bind(specs)
            .bind(status.as_str())
            .bind(&normalized_name)
            .bind(&config_hash)
            .fetch_optional(&mut *transaction)
            .await
        {
            Ok(updated) => updated,
            // A patch can collide the identity with **another** product of
            // the same owner — the same brand, model and configuration under
            // a different id. The index refuses it, and the refusal is the
            // same `409` a create would have produced.
            Err(error)
                if is_unique_violation(&error, "tracked_products_user_id_identity_unique_idx") =>
            {
                transaction.commit().await?;
                return Ok(ProductWrite::DuplicateIdentity);
            }
            Err(error) => return Err(error.into()),
        };

        transaction.commit().await?;

        match updated {
            Some(product) => Ok(ProductWrite::Applied(product)),
            None => Ok(ProductWrite::NotFound),
        }
    }

    /// Deletes one product of the caller. Its listings, observations, rules
    /// and alerts go with it through the composite keys' cascades, in the
    /// same statement.
    pub async fn delete_product_owned(&self, product_id: Uuid, owner: Uuid) -> AppResult<bool> {
        const DELETE: &str = "DELETE FROM tracked_products WHERE id = $1 AND user_id = $2";

        let result = sqlx::query(DELETE)
            .bind(product_id)
            .bind(owner)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    // -----------------------------------------------------------------------
    // Product listings
    // -----------------------------------------------------------------------

    /// Inserts one listing of the caller's product.
    pub async fn insert_listing(
        &self,
        owner: Uuid,
        product_id: Uuid,
        listing: &NewListing<'_>,
    ) -> AppResult<ListingWrite<ProductListing>> {
        const INSERT: &str = r#"
            INSERT INTO product_listings
                (id, user_id, product_id, retailer, url, external_id, currency)
            VALUES ($1, $2, $3, $4, $5, $6, $7::char(3))
            RETURNING id, user_id, product_id, retailer, url, external_id, currency,
                      availability, validation_state, last_observed_at, created_at, updated_at
        "#;

        match sqlx::query_as::<_, ProductListing>(INSERT)
            .bind(listing.id)
            .bind(owner)
            .bind(product_id)
            .bind(listing.retailer)
            .bind(listing.url)
            .bind(listing.external_id)
            .bind(listing.currency)
            .fetch_optional(&self.pool)
            .await
        {
            Ok(Some(listing)) => Ok(ListingWrite::Applied(listing)),
            Ok(None) => Ok(ListingWrite::ProductNotFound),
            Err(error)
                if is_unique_violation(&error, "product_listings_user_id_url_unique_idx") =>
            {
                Ok(ListingWrite::DuplicateUrl)
            }
            // A composite foreign key violation is the insert naming a
            // product that does not belong to the caller: the statement
            // cannot tell it apart from a missing product, and both are the
            // product's `404`.
            Err(error) if is_foreign_violation(&error, "product_listings_product_fk") => {
                Ok(ListingWrite::ProductNotFound)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// The listing page statement, for one product of the caller.
    const PAGE_LISTINGS: &'static str = r#"
        SELECT id, user_id, product_id, retailer, url, external_id, currency,
               availability, validation_state, last_observed_at, created_at, updated_at
        FROM product_listings
        WHERE product_id = $1 AND user_id = $2
        ORDER BY created_at DESC, id DESC
        LIMIT $3 OFFSET $4
    "#;

    /// The count statement matching [`Self::PAGE_LISTINGS`].
    const COUNT_LISTINGS: &'static str =
        "SELECT COUNT(*) FROM product_listings WHERE product_id = $1 AND user_id = $2";

    /// Lists the listings of one product owned by `owner`, newest first.
    pub async fn list_listings_owned(
        &self,
        product_id: Uuid,
        owner: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<(Vec<ProductListing>, i64)> {
        let items = sqlx::query_as::<_, ProductListing>(Self::PAGE_LISTINGS)
            .bind(product_id)
            .bind(owner)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        let total = sqlx::query_scalar::<_, i64>(Self::COUNT_LISTINGS)
            .bind(product_id)
            .bind(owner)
            .fetch_one(&self.pool)
            .await?;

        Ok((items, total))
    }

    /// Applies a partial update to one listing of the caller's product.
    ///
    /// Both the product and the listing are bound, so a listing id that
    /// exists under another product of the same caller matches no row and
    /// becomes the listing's `404` — never a `200` and never a `403`. The
    /// write is one statement: the tri-state of the external identifier is
    /// the pair the service resolved, and the two controlled vocabularies
    /// are bound as their stored text.
    pub async fn update_listing_owned(
        &self,
        product_id: Uuid,
        owner: Uuid,
        listing_id: Uuid,
        patch: &ListingPatch<'_>,
    ) -> AppResult<Option<ProductListing>> {
        const UPDATE: &str = r#"
            UPDATE product_listings
            SET external_id = CASE WHEN $4 THEN NULL ELSE COALESCE($5, external_id) END,
                availability = COALESCE($6, availability),
                validation_state = COALESCE($7, validation_state),
                updated_at = clock_timestamp()
            WHERE id = $1
              AND product_id = $2
              AND user_id = $3
            RETURNING id, user_id, product_id, retailer, url, external_id, currency,
                      availability, validation_state, last_observed_at, created_at, updated_at
        "#;

        let listing = sqlx::query_as::<_, ProductListing>(UPDATE)
            .bind(listing_id)
            .bind(product_id)
            .bind(owner)
            .bind(patch.clear_external_id)
            .bind(patch.external_id)
            .bind(patch.availability.map(|value| value.as_str().to_string()))
            .bind(
                patch
                    .validation_state
                    .map(|value| value.as_str().to_string()),
            )
            .fetch_optional(&self.pool)
            .await?;

        Ok(listing)
    }

    /// Deletes one listing of the caller's product. Its observations — and
    /// the alerts those observations triggered — go with it through the
    /// cascades.
    pub async fn delete_listing_owned(
        &self,
        product_id: Uuid,
        owner: Uuid,
        listing_id: Uuid,
    ) -> AppResult<bool> {
        const DELETE: &str =
            "DELETE FROM product_listings WHERE id = $1 AND product_id = $2 AND user_id = $3";

        let result = sqlx::query(DELETE)
            .bind(listing_id)
            .bind(product_id)
            .bind(owner)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    // -----------------------------------------------------------------------
    // Price observations
    // -----------------------------------------------------------------------

    /// Locks one listing of the caller for the observation transaction.
    ///
    /// `FOR UPDATE` is what serialises observations on one listing: the rule
    /// evaluation reads the "previous observation" of the same listing, so
    /// two concurrent collectors must not both compute a previous that
    /// misses the other. It is also the existence check: `Ok(None)` is
    /// exactly the listing's `404`, and `Ok(Some(product_id))` means the
    /// caller may write — and hands back the one id the rule evaluation
    /// needs, so it does not have to trust a second read.
    async fn lock_listing(
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        listing_id: Uuid,
        owner: Uuid,
    ) -> AppResult<Option<Uuid>> {
        const LOCK: &str =
            "SELECT product_id FROM product_listings WHERE id = $1 AND user_id = $2 FOR UPDATE";

        let product_id = sqlx::query_scalar::<_, Uuid>(LOCK)
            .bind(listing_id)
            .bind(owner)
            .fetch_optional(&mut **transaction)
            .await?;

        Ok(product_id)
    }

    /// Records one collected price and evaluates the product's rule.
    ///
    /// One transaction, in this order:
    ///
    /// 1. lock the listing row (`404` if absent or foreign);
    /// 2. when the body names a run, read it owner-scoped (`404` if absent
    ///    or foreign) — the composite key would refuse the insert anyway,
    ///    and the read names the resource instead of surfacing a `23503`;
    /// 3. insert the observation with `ON CONFLICT
    ///    (user_id, listing_id, fingerprint) DO NOTHING`. A row returned
    ///    means the observation is new: evaluate the rule (step 4) and touch
    ///    the listing. No row, with the listing already locked and the run
    ///    already verified, can only mean the conflict fired: read the
    ///    existing row and report it absorbed;
    /// 4. evaluate the product's **enabled** rule — the target reached, or
    ///    the configured percentage dropped against the previous observation
    ///    of the same listing — and insert the alert with `ON CONFLICT
    ///    (rule_id, observation_id) DO NOTHING`. Both triggers compare
    ///    currencies explicitly: money is never converted, and a price
    ///    collected in another currency than the rule's target simply does
    ///    not trigger (`ADR-0007`: the database computes, Rust carries
    ///    text);
    /// 5. stamp the listing's `last_observed_at` with the new observation,
    ///    when it is newer than what is stored.
    ///
    /// A duplicate is absorbed **without** re-running the evaluation: the
    /// alert it could produce was written with the original observation, and
    /// writing it again is exactly what the unique index refuses.
    pub async fn insert_observation(
        &self,
        owner: Uuid,
        listing_id: Uuid,
        observation: &NewObservation<'_>,
    ) -> AppResult<ObservationWrite> {
        const INSERT: &str = r#"
            INSERT INTO price_observations
                (id, user_id, listing_id, amount, currency, observed_at, fingerprint,
                 availability, run_id)
            VALUES ($1, $2, $3, $4::text::numeric, $5::char(3), $6, $7, $8, $9)
            ON CONFLICT (user_id, listing_id, fingerprint) DO NOTHING
            RETURNING id, user_id, listing_id, amount::text AS amount, currency,
                      observed_at, fingerprint, availability, run_id, created_at
        "#;

        const FIND_ABSORBED: &str = r#"
            SELECT id, user_id, listing_id, amount::text AS amount, currency,
                   observed_at, fingerprint, availability, run_id, created_at
            FROM price_observations
            WHERE user_id = $1 AND listing_id = $2 AND fingerprint = $3
        "#;

        const FIND_RUN_OWNED: &str =
            "SELECT id FROM automation_runs WHERE id = $1 AND user_id = $2";

        const EVALUATE_RULE: &str = r#"
            WITH rule AS (
                SELECT id, target_amount, target_currency, percent_drop
                FROM monitoring_rules
                WHERE product_id = $1
                  AND user_id = $2
                  AND enabled
            ),
            previous AS (
                SELECT amount, currency
                FROM price_observations
                WHERE listing_id = $3
                  AND user_id = $2
                  AND id <> $4
                ORDER BY observed_at DESC, created_at DESC, id DESC
                LIMIT 1
            )
            INSERT INTO price_alerts (id, user_id, rule_id, observation_id)
            SELECT $5, $2, rule.id, $4
            FROM rule
            LEFT JOIN previous ON TRUE
            WHERE (rule.target_currency = $7 AND rule.target_amount >= $6::text::numeric)
               OR (
                    rule.percent_drop IS NOT NULL
                    AND previous.currency = $7
                    AND previous.amount > 0
                    AND (previous.amount - $6::text::numeric) * 100
                        >= rule.percent_drop * previous.amount
               )
            ON CONFLICT (rule_id, observation_id) DO NOTHING
            RETURNING id
        "#;

        const TOUCH_LISTING: &str = r#"
            UPDATE product_listings
            SET last_observed_at = GREATEST(COALESCE(last_observed_at, $3), $3),
                updated_at = clock_timestamp()
            WHERE id = $1 AND user_id = $2
        "#;

        let mut transaction = self.pool.begin().await?;

        let Some(product_id) = Self::lock_listing(&mut transaction, listing_id, owner).await?
        else {
            transaction.commit().await?;
            return Ok(ObservationWrite::ListingNotFound);
        };

        if let Some(run_id) = observation.run_id {
            let owned = sqlx::query_scalar::<_, Uuid>(FIND_RUN_OWNED)
                .bind(run_id)
                .bind(owner)
                .fetch_optional(&mut *transaction)
                .await?;

            if owned.is_none() {
                transaction.commit().await?;
                return Ok(ObservationWrite::RunNotFound);
            }
        }

        // The alert id is captured before the returned row shadows the
        // request: the evaluation needs the identifier it was given, not one
        // of the row it produced.
        let alert_id = observation.alert_id;

        let inserted = sqlx::query_as::<_, PriceObservation>(INSERT)
            .bind(observation.id)
            .bind(owner)
            .bind(listing_id)
            .bind(observation.amount)
            .bind(observation.currency)
            .bind(observation.observed_at)
            .bind(observation.fingerprint)
            .bind(observation.availability.as_str().to_string())
            .bind(observation.run_id)
            .fetch_optional(&mut *transaction)
            .await?;

        let Some(observation) = inserted else {
            // The listing is locked and the run verified, so the only way
            // the insert can yield no row is the idempotency conflict. Read
            // the existing row and absorb the request.
            let existing = sqlx::query_as::<_, PriceObservation>(FIND_ABSORBED)
                .bind(owner)
                .bind(listing_id)
                .bind(observation.fingerprint)
                .fetch_optional(&mut *transaction)
                .await?;

            transaction.commit().await?;

            return Ok(match existing {
                Some(observation) => ObservationWrite::Absorbed(observation),
                // Unreachable while the lock and the verify above hold: the
                // insert conflicted with a row this read must see. A `None`
                // here means the database disagreed with the transaction's
                // own snapshot, which no caller can act on — report it as
                // the listing's `404` rather than invent a row.
                None => ObservationWrite::ListingNotFound,
            });
        };

        let alert = sqlx::query(EVALUATE_RULE)
            .bind(product_id)
            .bind(owner)
            .bind(listing_id)
            .bind(observation.id)
            .bind(alert_id)
            .bind(&observation.amount)
            .bind(&observation.currency)
            .execute(&mut *transaction)
            .await?;

        sqlx::query(TOUCH_LISTING)
            .bind(listing_id)
            .bind(owner)
            .bind(observation.observed_at)
            .execute(&mut *transaction)
            .await?;

        transaction.commit().await?;

        Ok(ObservationWrite::Applied(
            observation,
            alert.rows_affected() > 0,
        ))
    }

    /// The observation page statement, newest first.
    ///
    /// `created_at DESC, id DESC` are the total-order tiebreakers; the index
    /// `price_observations_user_id_listing_observed_at_idx` supplies the
    /// leading order, so only ties at the same instant are sorted.
    const PAGE_OBSERVATIONS: &'static str = r#"
        SELECT id, user_id, listing_id, amount::text AS amount, currency,
               observed_at, fingerprint, availability, run_id, created_at
        FROM price_observations
        WHERE listing_id = $1 AND user_id = $2
        ORDER BY observed_at DESC, created_at DESC, id DESC
        LIMIT $3 OFFSET $4
    "#;

    /// The count statement matching [`Self::PAGE_OBSERVATIONS`].
    const COUNT_OBSERVATIONS: &'static str =
        "SELECT COUNT(*) FROM price_observations WHERE listing_id = $1 AND user_id = $2";

    /// Lists the observations of one listing owned by `owner`, newest first.
    pub async fn list_observations_owned(
        &self,
        listing_id: Uuid,
        owner: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<(Vec<PriceObservation>, i64)> {
        let items = sqlx::query_as::<_, PriceObservation>(Self::PAGE_OBSERVATIONS)
            .bind(listing_id)
            .bind(owner)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        let total = sqlx::query_scalar::<_, i64>(Self::COUNT_OBSERVATIONS)
            .bind(listing_id)
            .bind(owner)
            .fetch_one(&self.pool)
            .await?;

        Ok((items, total))
    }

    /// Reads the newest observation of one listing owned by `owner`.
    pub async fn find_latest_observation(
        &self,
        listing_id: Uuid,
        owner: Uuid,
    ) -> AppResult<Option<PriceObservation>> {
        const FIND_LATEST: &str = r#"
            SELECT id, user_id, listing_id, amount::text AS amount, currency,
                   observed_at, fingerprint, availability, run_id, created_at
            FROM price_observations
            WHERE listing_id = $1 AND user_id = $2
            ORDER BY observed_at DESC, created_at DESC, id DESC
            LIMIT 1
        "#;

        let observation = sqlx::query_as::<_, PriceObservation>(FIND_LATEST)
            .bind(listing_id)
            .bind(owner)
            .fetch_optional(&self.pool)
            .await?;

        Ok(observation)
    }

    /// Whether the listing exists for the caller — the error-path read of
    /// [`Self::find_latest_observation`], and the observation routes' "which
    /// resource was the problem" probe.
    pub async fn listing_is_owned(&self, listing_id: Uuid, owner: Uuid) -> AppResult<bool> {
        const IS_OWNED: &str =
            "SELECT EXISTS (SELECT 1 FROM product_listings WHERE id = $1 AND user_id = $2)";

        let owned = sqlx::query_scalar::<_, bool>(IS_OWNED)
            .bind(listing_id)
            .bind(owner)
            .fetch_one(&self.pool)
            .await?;

        Ok(owned)
    }

    // -----------------------------------------------------------------------
    // Monitoring rules
    // -----------------------------------------------------------------------

    /// Inserts the rule for the caller's product.
    pub async fn insert_rule(
        &self,
        id: Uuid,
        owner: Uuid,
        product_id: Uuid,
        target_amount: &str,
        target_currency: &str,
        percent_drop: Option<i32>,
    ) -> AppResult<RuleWrite<MonitoringRule>> {
        const INSERT: &str = r#"
            INSERT INTO monitoring_rules
                (id, user_id, product_id, target_amount, target_currency, percent_drop)
            VALUES ($1, $2, $3, $4::text::numeric, $5::char(3), $6)
            RETURNING id, user_id, product_id, target_amount::text AS target_amount,
                      target_currency, percent_drop, enabled, notify_via,
                      created_at, updated_at
        "#;

        match sqlx::query_as::<_, MonitoringRule>(INSERT)
            .bind(id)
            .bind(owner)
            .bind(product_id)
            .bind(target_amount)
            .bind(target_currency)
            .bind(percent_drop)
            .fetch_optional(&self.pool)
            .await
        {
            Ok(Some(rule)) => Ok(RuleWrite::Applied(rule)),
            Ok(None) => Ok(RuleWrite::ProductNotFound),
            Err(error)
                if is_unique_violation(&error, "monitoring_rules_user_id_product_unique_idx") =>
            {
                Ok(RuleWrite::RuleExists)
            }
            Err(error) if is_foreign_violation(&error, "monitoring_rules_product_fk") => {
                Ok(RuleWrite::ProductNotFound)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// The rule page statement, for one product of the caller. At most one
    /// row can match: the unique index allows exactly one rule per product.
    const PAGE_RULES: &'static str = r#"
        SELECT id, user_id, product_id, target_amount::text AS target_amount,
               target_currency, percent_drop, enabled, notify_via,
               created_at, updated_at
        FROM monitoring_rules
        WHERE product_id = $1 AND user_id = $2
        ORDER BY created_at DESC, id DESC
        LIMIT $3 OFFSET $4
    "#;

    /// The count statement matching [`Self::PAGE_RULES`].
    const COUNT_RULES: &'static str =
        "SELECT COUNT(*) FROM monitoring_rules WHERE product_id = $1 AND user_id = $2";

    /// Lists the rules of one product owned by `owner`.
    pub async fn list_rules_owned(
        &self,
        product_id: Uuid,
        owner: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<(Vec<MonitoringRule>, i64)> {
        let items = sqlx::query_as::<_, MonitoringRule>(Self::PAGE_RULES)
            .bind(product_id)
            .bind(owner)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        let total = sqlx::query_scalar::<_, i64>(Self::COUNT_RULES)
            .bind(product_id)
            .bind(owner)
            .fetch_one(&self.pool)
            .await?;

        Ok((items, total))
    }

    /// Applies a partial update to one rule of the caller's product.
    ///
    /// Both the product and the rule are bound, so a rule id that exists
    /// under another product of the same caller matches no row and becomes
    /// the rule's `404`. The write is one statement: there are no derived
    /// columns here, so `COALESCE($n, column)` is the partial-update rule
    /// made mechanical, and the boolean is what says "clear the percent
    /// drop".
    pub async fn update_rule_owned(
        &self,
        product_id: Uuid,
        owner: Uuid,
        rule_id: Uuid,
        patch: &RulePatch<'_>,
    ) -> AppResult<Option<MonitoringRule>> {
        const UPDATE: &str = r#"
            UPDATE monitoring_rules
            SET enabled = COALESCE($4, enabled),
                target_amount = COALESCE($5::text::numeric, target_amount),
                target_currency = COALESCE($6::char(3), target_currency),
                percent_drop = CASE WHEN $7 THEN NULL ELSE COALESCE($8, percent_drop) END,
                updated_at = clock_timestamp()
            WHERE id = $1
              AND product_id = $2
              AND user_id = $3
            RETURNING id, user_id, product_id, target_amount::text AS target_amount,
                      target_currency, percent_drop, enabled, notify_via,
                      created_at, updated_at
        "#;

        let rule = sqlx::query_as::<_, MonitoringRule>(UPDATE)
            .bind(rule_id)
            .bind(product_id)
            .bind(owner)
            .bind(patch.enabled)
            .bind(patch.target_amount)
            .bind(patch.target_currency)
            .bind(patch.clear_percent_drop)
            .bind(patch.percent_drop)
            .fetch_optional(&self.pool)
            .await?;

        Ok(rule)
    }

    /// Deletes one rule of the caller's product. The alerts it raised go
    /// with it through the composite key's cascade.
    pub async fn delete_rule_owned(
        &self,
        product_id: Uuid,
        owner: Uuid,
        rule_id: Uuid,
    ) -> AppResult<bool> {
        const DELETE: &str =
            "DELETE FROM monitoring_rules WHERE id = $1 AND product_id = $2 AND user_id = $3";

        let result = sqlx::query(DELETE)
            .bind(rule_id)
            .bind(product_id)
            .bind(owner)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    // -----------------------------------------------------------------------
    // Automation runs
    // -----------------------------------------------------------------------

    /// Starts one automation run for the caller.
    ///
    /// A start carries no state the server keeps: the run is a record of
    /// what the workflow did, and its shape is fixed by the schema. `NOW()`
    /// is the started instant, because the insert is the start.
    pub async fn insert_run(&self, id: Uuid, owner: Uuid) -> AppResult<AutomationRun> {
        const INSERT: &str = r#"
            INSERT INTO automation_runs (id, user_id, started_at)
            VALUES ($1, $2, NOW())
            RETURNING id, user_id, started_at, finished_at, outcome, error_summary,
                      observations, alerts, failures
        "#;

        let run = sqlx::query_as::<_, AutomationRun>(INSERT)
            .bind(id)
            .bind(owner)
            .fetch_one(&self.pool)
            .await?;

        Ok(run)
    }

    /// Finds one run of the caller.
    pub async fn find_run_owned(
        &self,
        run_id: Uuid,
        owner: Uuid,
    ) -> AppResult<Option<AutomationRun>> {
        const FIND: &str = r#"
            SELECT id, user_id, started_at, finished_at, outcome, error_summary,
                   observations, alerts, failures
            FROM automation_runs
            WHERE id = $1 AND user_id = $2
        "#;

        let run = sqlx::query_as::<_, AutomationRun>(FIND)
            .bind(run_id)
            .bind(owner)
            .fetch_optional(&self.pool)
            .await?;

        Ok(run)
    }

    /// The run page statement, newest first.
    const PAGE_RUNS: &'static str = r#"
        SELECT id, user_id, started_at, finished_at, outcome, error_summary,
               observations, alerts, failures
        FROM automation_runs
        WHERE user_id = $1
        ORDER BY started_at DESC, id DESC
        LIMIT $2 OFFSET $3
    "#;

    /// The count statement matching [`Self::PAGE_RUNS`].
    const COUNT_RUNS: &'static str = "SELECT COUNT(*) FROM automation_runs WHERE user_id = $1";

    /// Lists the runs owned by `owner`, newest first.
    pub async fn list_runs_owned(
        &self,
        owner: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<(Vec<AutomationRun>, i64)> {
        let items = sqlx::query_as::<_, AutomationRun>(Self::PAGE_RUNS)
            .bind(owner)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        let total = sqlx::query_scalar::<_, i64>(Self::COUNT_RUNS)
            .bind(owner)
            .fetch_one(&self.pool)
            .await?;

        Ok((items, total))
    }

    /// Finishes one **running** run of the caller.
    ///
    /// The guard is the `WHERE outcome = 'running'`, not a preceding read,
    /// so two concurrent finishes cannot both win: the second re-evaluates
    /// the guard against the committed row and touches nothing. A zero-row
    /// result is then ambiguous — absent or foreign, or already finished —
    /// and the service disambiguates on its error path with one owner-scoped
    /// read.
    ///
    /// `clock_timestamp()` rather than `NOW()`: `NOW()` is the transaction's
    /// start instant, and `automation_runs_lifecycle_chk` requires the
    /// finish not to predate the start — the same reasoning `0004` documents
    /// for its decision stamp.
    pub async fn finish_run_owned(
        &self,
        run_id: Uuid,
        owner: Uuid,
        finish: &RunFinish<'_>,
    ) -> AppResult<Option<AutomationRun>> {
        const FINISH: &str = r#"
            UPDATE automation_runs
            SET outcome = $3,
                finished_at = clock_timestamp(),
                observations = COALESCE($4, observations),
                alerts = COALESCE($5, alerts),
                failures = COALESCE($6, failures),
                error_summary = CASE WHEN $7 THEN NULL ELSE COALESCE($8, error_summary) END
            WHERE id = $1
              AND user_id = $2
              AND outcome = 'running'
            RETURNING id, user_id, started_at, finished_at, outcome, error_summary,
                      observations, alerts, failures
        "#;

        let run = sqlx::query_as::<_, AutomationRun>(FINISH)
            .bind(run_id)
            .bind(owner)
            .bind(finish.outcome.as_str())
            .bind(finish.observations)
            .bind(finish.alerts)
            .bind(finish.failures)
            .bind(finish.clear_error_summary)
            .bind(finish.error_summary)
            .fetch_optional(&self.pool)
            .await?;

        Ok(run)
    }

    // -----------------------------------------------------------------------
    // Price alerts
    // -----------------------------------------------------------------------

    /// The alert page statement, newest first.
    ///
    /// The unread filter is `$2 IS NOT TRUE OR read_at IS NULL`: `true`
    /// keeps only the unread, and `false` or an omitted parameter keeps
    /// everything.
    const PAGE_ALERTS: &'static str = r#"
        SELECT id, user_id, rule_id, observation_id, triggered_at,
               notification_status, read_at
        FROM price_alerts
        WHERE user_id = $1
          AND ($2 IS NOT TRUE OR read_at IS NULL)
        ORDER BY triggered_at DESC, id DESC
        LIMIT $3 OFFSET $4
    "#;

    /// The count statement matching [`Self::PAGE_ALERTS`].
    const COUNT_ALERTS: &'static str = r#"
        SELECT COUNT(*)
        FROM price_alerts
        WHERE user_id = $1
          AND ($2 IS NOT TRUE OR read_at IS NULL)
    "#;

    /// Lists the alerts owned by `owner`, newest first, with the total of
    /// the filtered set.
    pub async fn list_alerts_owned(
        &self,
        owner: Uuid,
        unread_only: bool,
        limit: i64,
        offset: i64,
    ) -> AppResult<(Vec<PriceAlert>, i64)> {
        let items = sqlx::query_as::<_, PriceAlert>(Self::PAGE_ALERTS)
            .bind(owner)
            .bind(unread_only)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        let total = sqlx::query_scalar::<_, i64>(Self::COUNT_ALERTS)
            .bind(owner)
            .bind(unread_only)
            .fetch_one(&self.pool)
            .await?;

        Ok((items, total))
    }

    /// Finds one alert of the caller.
    pub async fn find_alert_owned(
        &self,
        alert_id: Uuid,
        owner: Uuid,
    ) -> AppResult<Option<PriceAlert>> {
        const FIND: &str = r#"
            SELECT id, user_id, rule_id, observation_id, triggered_at,
                   notification_status, read_at
            FROM price_alerts
            WHERE id = $1 AND user_id = $2
        "#;

        let alert = sqlx::query_as::<_, PriceAlert>(FIND)
            .bind(alert_id)
            .bind(owner)
            .fetch_optional(&self.pool)
            .await?;

        Ok(alert)
    }

    /// Applies a partial update to one alert of the caller.
    ///
    /// `set_read` stamps `read_at` with the write instant, `clear_read`
    /// clears it, and the two are mutually exclusive by construction — the
    /// service resolves the `Option<bool>` into at most one of them. The
    /// `price_alerts_read_after_triggered_chk` constraint is satisfied by
    /// both: a read stamp is written after the alert exists, so it cannot
    /// predate the trigger.
    pub async fn update_alert_owned(
        &self,
        alert_id: Uuid,
        owner: Uuid,
        set_read: bool,
        clear_read: bool,
        notification_status: Option<NotificationStatus>,
    ) -> AppResult<Option<PriceAlert>> {
        const UPDATE: &str = r#"
            UPDATE price_alerts
            SET read_at = CASE WHEN $3 THEN clock_timestamp()
                               WHEN $4 THEN NULL
                               ELSE read_at END,
                notification_status = COALESCE($5, notification_status)
            WHERE id = $1 AND user_id = $2
            RETURNING id, user_id, rule_id, observation_id, triggered_at,
                      notification_status, read_at
        "#;

        let alert = sqlx::query_as::<_, PriceAlert>(UPDATE)
            .bind(alert_id)
            .bind(owner)
            .bind(set_read)
            .bind(clear_read)
            .bind(notification_status.map(|status| status.as_str().to_string()))
            .fetch_optional(&self.pool)
            .await?;

        Ok(alert)
    }
}

/// The outcome of one tracked-product write.
///
/// The repository never builds an error: the messages, and therefore the
/// statuses, are the service's. Each variant names exactly one thing the
/// caller has to translate.
#[derive(Debug)]
pub enum ProductWrite<T> {
    /// The write applied.
    Applied(T),
    /// The unique index `tracked_products_user_id_identity_unique_idx`
    /// refused the row: this owner already tracks that name and that exact
    /// configuration. Detected by the `23505`, never by a pre-check, so two
    /// concurrent creates cannot both pass.
    DuplicateIdentity,
    /// The derived normalised name is longer than the column's own bound —
    /// reachable only through Unicode lower-casing that expands, never
    /// through the validators the service runs first.
    IdentityTooLong,
    /// No product of that id belongs to the caller.
    NotFound,
}

/// The outcome of one listing write. The URL is the listing's identity per
/// owner, and the duplicate is again the index's verdict, not a pre-check's.
#[derive(Debug)]
pub enum ListingWrite<T> {
    /// The write applied.
    Applied(T),
    /// The unique index `product_listings_user_id_url_unique_idx` refused
    /// the row: this owner already tracks that URL.
    DuplicateUrl,
    /// The product does not exist or is not the caller's.
    ProductNotFound,
    /// The listing does not exist under that product of the caller.
    ListingNotFound,
}

/// The outcome of one rule write. A product has at most one rule, and the
/// unique index is what says so.
#[derive(Debug)]
pub enum RuleWrite<T> {
    /// The write applied.
    Applied(T),
    /// The unique index `monitoring_rules_user_id_product_unique_idx`
    /// refused the row: the product already has a rule.
    RuleExists,
    /// The product does not exist or is not the caller's.
    ProductNotFound,
}

/// The outcome of one observation write.
#[derive(Debug)]
pub enum ObservationWrite {
    /// A new observation was written; the flag says whether the rule
    /// evaluation inserted an alert in the same transaction.
    Applied(PriceObservation, bool),
    /// The fingerprint was already recorded: the write was absorbed, and the
    /// existing observation is the one the caller answers with.
    Absorbed(PriceObservation),
    /// No listing of that id belongs to the caller.
    ListingNotFound,
    /// The run named in the body does not exist or is not the caller's.
    RunNotFound,
}

/// A partial product update, already validated by the service.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProductPatch<'a> {
    /// New brand, validated, or `None` to keep the stored one.
    pub brand: Option<&'a str>,
    /// New model, validated, or `None` to keep the stored one.
    pub model: Option<&'a str>,
    /// New configuration text, validated, or `None` to keep the stored one.
    pub specs: Option<&'a str>,
    /// New status, or `None` to keep the stored one.
    pub status: Option<ProductStatus>,
}

/// A new tracked product, as the service validated it.
///
/// A struct rather than eight positional arguments: an argument order that
/// can be silently swapped is a bug generator, and
/// `clippy::too_many_arguments` fires at eight. Every value has already been
/// validated and normalised by [`crate::monitoring::service`].
#[derive(Debug, Clone, Copy)]
pub struct NewProduct<'a> {
    /// The freshly generated identifier.
    pub id: Uuid,
    /// Trimmed brand, 1 to 100 characters.
    pub brand: &'a str,
    /// Trimmed model, 1 to 100 characters.
    pub model: &'a str,
    /// Configuration text, verbatim, at most 2000 characters.
    pub specs: &'a str,
    /// The derived normalised name.
    pub normalized_name: &'a str,
    /// The derived lower-case SHA-256 hex digest of the configuration.
    pub config_hash: &'a str,
}

/// A new listing, as the service validated it.
#[derive(Debug, Clone, Copy)]
pub struct NewListing<'a> {
    /// The freshly generated identifier.
    pub id: Uuid,
    /// Trimmed retailer label, 1 to 100 characters.
    pub retailer: &'a str,
    /// Trimmed absolute `http`/`https` URL, at most 2048 characters.
    pub url: &'a str,
    /// Trimmed, upper-cased ISO 4217 code.
    pub currency: &'a str,
    /// The retailer's own identifier, validated, or `None`.
    pub external_id: Option<&'a str>,
}

/// The resolved tri-state of a listing patch, as the statement binds it.
#[derive(Debug, Clone, Copy, Default)]
pub struct ListingPatch<'a> {
    /// Whether the external identifier is cleared.
    pub clear_external_id: bool,
    /// New external identifier, validated, or `None` to keep the stored one.
    pub external_id: Option<&'a str>,
    /// New availability, or `None` to keep the stored one.
    pub availability: Option<ListingAvailability>,
    /// New validation state, or `None` to keep the stored one.
    pub validation_state: Option<ListingValidationState>,
}

/// One new observation, as the service validated it.
///
/// The two freshly generated identifiers — the observation's and the alert's,
/// for the evaluation to use if it fires — travel with the write, because
/// both rows are born in the same transaction.
#[derive(Debug, Clone, Copy)]
pub struct NewObservation<'a> {
    /// The freshly generated observation identifier.
    pub id: Uuid,
    /// The freshly generated alert identifier, for the evaluation.
    pub alert_id: Uuid,
    /// Four-decimal money string, never parsed in Rust.
    pub amount: &'a str,
    /// Trimmed, upper-cased ISO 4217 code.
    pub currency: &'a str,
    /// When the source was observed, as the workflow reported it.
    pub observed_at: chrono::DateTime<chrono::Utc>,
    /// The idempotency identity, 1 to 200 characters.
    pub fingerprint: &'a str,
    /// The availability at the moment of collection.
    pub availability: ListingAvailability,
    /// The run that collected it, already known to belong to the caller or
    /// `None`.
    pub run_id: Option<Uuid>,
}

/// The resolved finish of a run, as the statement binds it.
#[derive(Debug, Clone, Copy)]
pub struct RunFinish<'a> {
    /// The terminal outcome, already checked by the service.
    pub outcome: RunOutcome,
    /// New counts, validated non-negative, or `None` to keep the stored ones.
    pub observations: Option<i32>,
    /// New counts, validated non-negative, or `None` to keep the stored ones.
    pub alerts: Option<i32>,
    /// New counts, validated non-negative, or `None` to keep the stored ones.
    pub failures: Option<i32>,
    /// Whether the error summary is cleared.
    pub clear_error_summary: bool,
    /// New summary, validated, or `None` to keep the stored one.
    pub error_summary: Option<&'a str>,
}

/// A partial rule update, already validated by the service.
#[derive(Debug, Clone, Copy, Default)]
pub struct RulePatch<'a> {
    /// New switch state, or `None` to keep the stored one.
    pub enabled: Option<bool>,
    /// New target price, validated, or `None` to keep the stored one.
    pub target_amount: Option<&'a str>,
    /// New target currency, validated, or `None` to keep the stored one.
    pub target_currency: Option<&'a str>,
    /// Whether the percent drop is cleared.
    pub clear_percent_drop: bool,
    /// New percent drop, validated, or `None` to keep the stored one.
    pub percent_drop: Option<i32>,
}

/// Whether a database error is the named unique index.
///
/// `23505` alone is not enough: `users_email_unique_idx` reaches the same
/// code, and reporting one of those as a duplicate product would be a lie a
/// client acts on. The constraint name is what makes the identification
/// exact.
fn is_unique_violation(error: &sqlx::Error, constraint: &str) -> bool {
    error
        .as_database_error()
        .filter(|database| database.code().as_deref() == Some("23505"))
        .and_then(|database| database.constraint())
        .is_some_and(|reported| reported == constraint)
}

/// Whether a database error is the named foreign-key violation.
///
/// A composite foreign key is what makes a cross-owner child unrepresentable;
/// when an insert is refused by one, the only cause is a parent that does not
/// exist for this caller, which is the parent's `404` and never a `500`.
fn is_foreign_violation(error: &sqlx::Error, constraint: &str) -> bool {
    error
        .as_database_error()
        .filter(|database| database.code().as_deref() == Some("23503"))
        .and_then(|database| database.constraint())
        .is_some_and(|reported| reported == constraint)
}
