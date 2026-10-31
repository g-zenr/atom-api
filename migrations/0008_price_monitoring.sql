-- ATOM Phase 2 — price monitoring: tracked products, retailer listings,
-- price observations, monitoring rules, automation runs and price alerts.
--
-- The six tables implement the minimal data model of
-- docs/decisions/ADR-0012-automation-orchestration-scope.md §Decision, for the
-- first automation use case (laptop price monitoring). The orchestration
-- (scheduling, outbound collection, notifications) is external — an n8n
-- workflow that reaches ATOM only through the authenticated endpoints — so
-- this schema is written by ordinary authenticated requests and every
-- ownership rule below is the same one the rest of the API enforces: every
-- table carries `user_id`, and every statement filters on it without a join
-- (ADR-0005).
--
-- Every child table also carries the composite foreign key `(child_id,
-- user_id) REFERENCES parent (id, user_id)` introduced by `0004`/`0005`: the
-- direct `users` key states the ownership, and the composite key states that
-- the ownership **agrees with the parent's**, which makes a cross-owner child
-- unrepresentable in the database rather than merely rejected by the service.
--
-- The definition of every column, constraint and index here is
-- docs/database/schema.md §12. This file is the enforced copy of that
-- description; if the two ever disagree, the document is corrected in a new
-- change and the migration stays as it was applied.
--
-- Unlike `0004` and `0005`, no PostgreSQL enum type is created here. Every
-- controlled vocabulary in this chain is a `CHECK`ed `TEXT` column, for the
-- reason `0006` recorded for `embeddings.source_type` and §11.8 documents: a
-- `CHECK` expresses exactly the same closed set, and extending it is one
-- migration instead of a type change with a dependent-view audit. The
-- exceptions `0004`/`0005` made for a five-state machine that carries
-- transition semantics do not apply: none of these vocabularies has
-- transitions the database could not see anyway.

-- ---------------------------------------------------------------------------
-- tracked_products
-- ---------------------------------------------------------------------------

-- One row per product configuration the owner tracks. The identity of a
-- tracked product is (owner, normalised name, configuration hash): the same
-- laptop model with a different RAM/storage/GPU configuration is a different
-- product with its own price history, and tracking the same configuration
-- twice is a data-entry bug, not a second record (the unique index below).
CREATE TABLE tracked_products (
    id      UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    brand   TEXT NOT NULL,
    model   TEXT NOT NULL,
    -- The free-text configuration that distinguishes otherwise identical
    -- models — RAM, storage, GPU, CPU, screen. Extensible but simple: text,
    -- bounded, empty when the owner does not differentiate (`NOT NULL
    -- DEFAULT ''`, exactly like `expense_claims.description`). It is
    -- deliberately not `JSONB`: no query needs to look inside it, and a
    -- schema inside a schema is a second thing to migrate.
    specs   TEXT NOT NULL DEFAULT '',
    -- The deterministic identity, derived by the service from brand and model
    -- (trimmed, lower-cased). Stored rather than derived at read time so the
    -- unique index below can compare it, and compared only by the index: the
    -- column is a key, not a display form — the display form is `brand` and
    -- `model`.
    normalized_name TEXT NOT NULL,
    -- The lower-case SHA-256 hex digest of `specs`, computed by the service.
    -- Storing the digest rather than hashing at read time is what lets the
    -- unique index below distinguish configurations; storing the hash rather
    -- than a second copy of `specs` is what keeps the two from drifting. A
    -- 64-character lower-case hex string is the one form the service writes,
    -- so the `CHECK` refuses anything else — including an upper-case or
    -- truncated digest, which would silently fork the identity.
    config_hash     TEXT NOT NULL,
    -- The two-state switch the owner uses to stop tracking without losing the
    -- history. `paused` keeps the product, its listings and its observations;
    -- only new collection is expected to stop. Deleting is the other way to
    -- stop, and it loses the history by design.
    status          TEXT NOT NULL DEFAULT 'active',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT tracked_products_brand_length_chk           CHECK (CHAR_LENGTH(brand) BETWEEN 1 AND 100),
    CONSTRAINT tracked_products_model_length_chk           CHECK (CHAR_LENGTH(model) BETWEEN 1 AND 100),
    CONSTRAINT tracked_products_specs_length_chk           CHECK (CHAR_LENGTH(specs) <= 2000),
    -- The derived name is bounded on its own because lower-casing is not
    -- length-preserving in Unicode: the service derives it and refuses an
    -- over-long result with a `422`, and the `CHECK` is the enforced copy of
    -- that bound (255 is generous: the two parts above sum to 202).
    CONSTRAINT tracked_products_normalized_name_length_chk CHECK (CHAR_LENGTH(normalized_name) BETWEEN 1 AND 255),
    -- The service writes a lower-case SHA-256 hex digest and nothing else.
    CONSTRAINT tracked_products_config_hash_chk            CHECK (config_hash ~ '^[0-9a-f]{64}$'),
    -- A controlled value in the database, not only in the service, so that no
    -- code path can write a status the API does not publish.
    CONSTRAINT tracked_products_status_chk                 CHECK (status IN ('active', 'paused'))
);

-- One tracking identity per owner: the same model with the same configuration
-- cannot be tracked twice. Two concurrent inserts still collide on the index,
-- so the `409` is a database guarantee, not a check-then-insert race.
CREATE UNIQUE INDEX tracked_products_user_id_identity_unique_idx
    ON tracked_products (user_id, normalized_name, config_hash);
-- The product list, newest first. Volumes per owner are small (a human tracks
-- a handful of laptops), so one list index serves every listing shape.
CREATE INDEX tracked_products_user_id_created_at_idx
    ON tracked_products (user_id, created_at DESC);
-- Not a second key: `id` is already unique. PostgreSQL requires a unique
-- index on exactly the columns a composite foreign key references, and
-- `product_listings`, `monitoring_rules` and `price_alerts` all reference
-- `(id, user_id)`, which is what makes a cross-owner child unrepresentable.
CREATE UNIQUE INDEX tracked_products_id_user_id_unique_idx
    ON tracked_products (id, user_id);

-- ---------------------------------------------------------------------------
-- product_listings
-- ---------------------------------------------------------------------------

CREATE TABLE product_listings (
    id         UUID PRIMARY KEY,
    user_id    UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- Part of the composite key below; the direct key to the product is not
    -- enough, because it would let a listing name another owner's product.
    product_id UUID NOT NULL,
    -- Where the price is collected, 1 to 100 characters. A label the owner
    -- types, not a controlled value: there is no enum and no CHECK on the
    -- set, only the length.
    retailer   TEXT NOT NULL,
    -- The canonical listing URL. The uniqueness rule below makes it the
    -- listing's identity per owner, which is why it is not patchable: moving
    -- a listing to a new URL is a new listing.
    url        TEXT NOT NULL,
    -- The retailer's own product identifier (an ASIN, a SKU), for a workflow
    -- that wants to reconcile its source data. Optional, bounded, nullable.
    external_id TEXT,
    -- ISO 4217. `CHAR(3)` rather than `TEXT` so that an over-long code is a
    -- length error at the type and not only in the constraint below.
    currency   CHAR(3) NOT NULL,
    -- The availability the most recent collection reported. Recorded as data
    -- (ADR-0012: "failures are data") rather than inferred from the absence
    -- of observations, so a listing that stopped being purchasable is
    -- distinguishable from one that stopped being collected.
    availability TEXT NOT NULL DEFAULT 'unknown',
    -- The state of the listing itself: `valid` (the default) when the URL is
    -- believed to still describe the product, `stale` when the page changed
    -- and the mapping is doubted, `rejected` when it is known wrong. A
    -- workflow marks the listing instead of re-reporting a doubtful price as
    -- current (ADR-0012 §Decision 6).
    validation_state TEXT NOT NULL DEFAULT 'valid',
    -- The `observed_at` of the newest observation on this listing, maintained
    -- by the observation write in the same transaction. Derivable from
    -- `price_observations`, but it is the one fact every listing read wants
    -- and a stored column keeps that read off the observation table.
    last_observed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- The strongest constraint on this table: a listing's owner must be its
    -- product's owner, so a cross-owner listing cannot be inserted by any
    -- code path, not even one that forgets the service's pre-check. The
    -- cascade deletes the listings in the same statement as the product.
    CONSTRAINT product_listings_product_fk FOREIGN KEY (product_id, user_id)
        REFERENCES tracked_products (id, user_id) ON DELETE CASCADE,
    CONSTRAINT product_listings_retailer_length_chk CHECK (CHAR_LENGTH(retailer) BETWEEN 1 AND 100),
    -- A client-supplied reference, at most 2 048 characters, and the scheme is
    -- part of the contract: only `http` and `https` are accepted, because a
    -- `javascript:`/`file:`/`data:` scheme has no meaning a client could
    -- follow and the API would never return one. Reachability is the
    -- workflow's business, not the database's.
    CONSTRAINT product_listings_url_chk CHECK (
        CHAR_LENGTH(url) BETWEEN 1 AND 2048 AND url ~* '^https?://'
    ),
    CONSTRAINT product_listings_external_id_length_chk CHECK (
        external_id IS NULL OR CHAR_LENGTH(external_id) BETWEEN 1 AND 200
    ),
    CONSTRAINT product_listings_currency_chk CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT product_listings_availability_chk CHECK (
        availability IN ('unknown', 'available', 'unavailable')
    ),
    CONSTRAINT product_listings_validation_state_chk CHECK (
        validation_state IN ('valid', 'stale', 'rejected')
    )
);

-- One listing per URL per owner: the same retailer page tracked twice would
-- fork the price history. Two concurrent inserts collide on the index, so the
-- `409` is a database guarantee.
CREATE UNIQUE INDEX product_listings_user_id_url_unique_idx
    ON product_listings (user_id, url);
-- The listing reads of one product, and the lookup that the composite key's
-- cascade performs when a product is deleted.
CREATE INDEX product_listings_product_id_idx ON product_listings (product_id);
-- The referenced key for `price_observations`, for the same reason as
-- `tracked_products_id_user_id_unique_idx`.
CREATE UNIQUE INDEX product_listings_id_user_id_unique_idx
    ON product_listings (id, user_id);

-- ---------------------------------------------------------------------------
-- automation_runs
-- ---------------------------------------------------------------------------

-- Declared before `price_observations`, which references it.
--
-- One row per execution of the external workflow that touches ATOM. It is the
-- audit trail of what the automation did (ADR-0012 §Decision 6: "failures are
-- data"): the owner can see every run, including the ones that collected
-- nothing.
CREATE TABLE automation_runs (
    id         UUID PRIMARY KEY,
    user_id    UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- When the run started and when it finished. `started_at` is written by
    -- the create and never changes; `finished_at` is written by the one
    -- guarded finish.
    started_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ,
    -- `running` is the state a create produces; the finish replaces it with
    -- one of the three terminal outcomes and is refused for a run that has
    -- already finished (the guard is in the `UPDATE`, so two concurrent
    -- finishes cannot both win).
    outcome     TEXT NOT NULL DEFAULT 'running',
    -- A bounded summary of what went wrong, for `partial` and `failed` runs.
    -- Bounded because it is operator-authored prose, not a log.
    error_summary TEXT,
    -- The counts the run reported, as plain integer columns rather than a
    -- JSON document: three columns keep every value typed, defaulted and
    -- queryable, and a `JSONB` bag would be extensible in exactly the way
    -- this record does not want to be — an uncounted key would be a count
    -- the owner cannot see.
    observations INTEGER NOT NULL DEFAULT 0,
    alerts       INTEGER NOT NULL DEFAULT 0,
    failures     INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT automation_runs_outcome_chk CHECK (outcome IN ('running', 'ok', 'partial', 'failed')),
    CONSTRAINT automation_runs_error_summary_length_chk CHECK (
        error_summary IS NULL OR CHAR_LENGTH(error_summary) BETWEEN 1 AND 1000
    ),
    -- A run is finished exactly when it has an outcome that is not `running`,
    -- and a finish cannot predate its own start.
    CONSTRAINT automation_runs_lifecycle_chk CHECK (
        (outcome = 'running' AND finished_at IS NULL)
        OR (outcome <> 'running' AND finished_at IS NOT NULL AND finished_at >= started_at)
    )
);

-- The run history, newest first (ADR-0012 §Decision: "index (user_id,
-- started_at DESC)").
CREATE INDEX automation_runs_user_id_started_at_idx
    ON automation_runs (user_id, started_at DESC);
-- The referenced key for `price_observations`.
CREATE UNIQUE INDEX automation_runs_id_user_id_unique_idx
    ON automation_runs (id, user_id);

-- ---------------------------------------------------------------------------
-- price_observations
-- ---------------------------------------------------------------------------

CREATE TABLE price_observations (
    id          UUID PRIMARY KEY,
    -- Both keys are required. The direct `users` key states the ownership;
    -- the composite key below states that the ownership agrees with the
    -- listing's, which the direct key alone cannot say.
    user_id     UUID NOT NULL,
    listing_id  UUID NOT NULL,
    -- Money is `NUMERIC(19,4)` at rest and a string with four decimals on the
    -- wire (ADR-0007). Never a float.
    amount      NUMERIC(19,4) NOT NULL,
    -- The currency the collected price was quoted in. It is recorded per
    -- observation rather than inherited from the listing so that a retailer
    -- changing its currency mid-history is recorded as what happened; the
    -- rule evaluation below compares currencies explicitly and never
    -- converts.
    currency    CHAR(3) NOT NULL,
    -- When the source was observed. Supplied by the workflow (the collected
    -- instant), not defaulted to the write instant: a retried workflow writes
    -- the observation it originally took, and the history must show the time
    -- the price was true, not the time the write landed.
    observed_at TIMESTAMPTZ NOT NULL,
    -- The client-supplied idempotency identity (ADR-0012 §Decision 5): a
    -- hash of product, listing, observed price and collected-at, computed by
    -- the workflow. The unique index below makes a retried run a no-op that
    -- returns the existing row, so a retry can never double-write.
    fingerprint TEXT NOT NULL,
    -- The availability reported with this observation, at the moment of
    -- collection. Availability at the listing level is the latest of these.
    availability TEXT NOT NULL DEFAULT 'unknown',
    -- The run that collected this observation, when the workflow says. The
    -- composite key makes an observation claim a run of another owner
    -- unrepresentable. `NO ACTION`, not `CASCADE`, and the difference is the
    -- same one `0005` documents for `job_applications_company_fk`: it refuses
    -- a direct `DELETE FROM automation_runs` while observations remain,
    -- while still letting `DELETE FROM users` succeed, because the account's
    -- cascade removes the runs and the observations in the same statement.
    -- There is deliberately no delete endpoint for a run: it is audit data.
    run_id      UUID,
    -- There is deliberately no `updated_at`. An observation records what was
    -- observed, and nothing about it is corrected in place; a duplicate is
    -- absorbed, never rewritten. `created_at` stays the row's write instant.
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT price_observations_user_fk FOREIGN KEY (user_id)
        REFERENCES users (id) ON DELETE CASCADE,
    -- The strongest constraint on this table: an observation's owner must be
    -- its listing's owner. The cascade deletes the observations in the same
    -- statement as the listing.
    CONSTRAINT price_observations_listing_fk FOREIGN KEY (listing_id, user_id)
        REFERENCES product_listings (id, user_id) ON DELETE CASCADE,
    CONSTRAINT price_observations_run_fk FOREIGN KEY (run_id, user_id)
        REFERENCES automation_runs (id, user_id) ON DELETE NO ACTION,
    -- Upper case is part of the contract, not a formatting preference.
    CONSTRAINT price_observations_currency_chk CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT price_observations_amount_chk CHECK (amount >= 0),
    CONSTRAINT price_observations_fingerprint_length_chk CHECK (
        CHAR_LENGTH(fingerprint) BETWEEN 1 AND 200
    ),
    CONSTRAINT price_observations_availability_chk CHECK (
        availability IN ('unknown', 'available', 'unavailable')
    )
);

-- The idempotency target. A retried workflow run re-sends the same
-- fingerprint for the same listing of the same owner and is absorbed by this
-- index rather than recorded twice (ADR-0012 §Decision 5). The insert uses
-- `ON CONFLICT ... DO NOTHING` against exactly this index.
CREATE UNIQUE INDEX price_observations_user_id_listing_fingerprint_unique_idx
    ON price_observations (user_id, listing_id, fingerprint);
-- The history read: one listing's observations, newest first. The query adds
-- `created_at DESC, id DESC` as the total-order tiebreakers; the index
-- supplies the leading order, so only ties at the same instant are sorted.
CREATE INDEX price_observations_user_id_listing_observed_at_idx
    ON price_observations (user_id, listing_id, observed_at DESC);
-- The referenced key for `price_alerts`.
CREATE UNIQUE INDEX price_observations_id_user_id_unique_idx
    ON price_observations (id, user_id);

-- ---------------------------------------------------------------------------
-- monitoring_rules
-- ---------------------------------------------------------------------------

CREATE TABLE monitoring_rules (
    id         UUID PRIMARY KEY,
    user_id    UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- Part of the composite key below, for the same reason as on
    -- `product_listings`.
    product_id UUID NOT NULL,
    -- The target price: at or below it, in the target currency, the rule has
    -- been reached.
    target_amount   NUMERIC(19,4) NOT NULL,
    target_currency CHAR(3) NOT NULL,
    -- The alternative trigger: a drop of at least this many percent against
    -- the previous observation of the same listing. Optional, 1..99, and
    -- `NULL` is a real state ("this rule does not watch percentages") rather
    -- than a missing value — which is why it is `NULL`-able and cleared with
    -- an explicit `null` on the patch.
    percent_drop    INTEGER,
    -- The switch the patch toggles. A paused product keeps its rule; only an
    -- enabled rule is evaluated when an observation arrives.
    enabled         BOOLEAN NOT NULL DEFAULT TRUE,
    -- How the owner wants to be told. The value set is a single member for
    -- now: the inbox is the one channel ATOM owns. The `CHECK` keeps the
    -- column honest until a second channel exists, and adding one is a
    -- migration, not a silent string.
    notify_via      TEXT NOT NULL DEFAULT 'inbox',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- The strongest constraint on this table: a rule's owner must be its
    -- product's owner.
    CONSTRAINT monitoring_rules_product_fk FOREIGN KEY (product_id, user_id)
        REFERENCES tracked_products (id, user_id) ON DELETE CASCADE,
    CONSTRAINT monitoring_rules_target_amount_chk CHECK (target_amount >= 0),
    CONSTRAINT monitoring_rules_target_currency_chk CHECK (target_currency ~ '^[A-Z]{3}$'),
    CONSTRAINT monitoring_rules_percent_drop_chk CHECK (
        percent_drop IS NULL OR percent_drop BETWEEN 1 AND 99
    ),
    CONSTRAINT monitoring_rules_notify_via_chk CHECK (notify_via IN ('inbox'))
);

-- One rule per product (ADR-0012 §Decision: "one rule per product"). A second
-- rule would make "the rule that fired" ambiguous, so the shape is refused
-- rather than resolved.
CREATE UNIQUE INDEX monitoring_rules_user_id_product_unique_idx
    ON monitoring_rules (user_id, product_id);
-- The referenced key for `price_alerts`.
CREATE UNIQUE INDEX monitoring_rules_id_user_id_unique_idx
    ON monitoring_rules (id, user_id);

-- ---------------------------------------------------------------------------
-- price_alerts
-- ---------------------------------------------------------------------------

CREATE TABLE price_alerts (
    id         UUID PRIMARY KEY,
    user_id    UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- Both references are composite, exactly like every other child in this
    -- migration: an alert that names another owner's rule or observation is
    -- unrepresentable.
    rule_id        UUID NOT NULL,
    observation_id UUID NOT NULL,
    -- When the rule fired. Written once, by the evaluation, and never
    -- changed.
    triggered_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- The delivery state the notification channel reports. `pending` until
    -- the channel says otherwise; the switch is one-way in the product for
    -- now and carried as a controlled value so a second state has a shape to
    -- land in.
    notification_status TEXT NOT NULL DEFAULT 'pending',
    -- When the owner read the alert. `NULL` until the patch marks it, which
    -- is what the unread filter matches.
    read_at        TIMESTAMPTZ,
    -- The strongest constraints on this table: an alert's rule and its
    -- observation must both belong to the alert's owner, and the cascade
    -- deletes the alerts in the same statement as the rule or the
    -- observation they came from.
    CONSTRAINT price_alerts_rule_fk FOREIGN KEY (rule_id, user_id)
        REFERENCES monitoring_rules (id, user_id) ON DELETE CASCADE,
    CONSTRAINT price_alerts_observation_fk FOREIGN KEY (observation_id, user_id)
        REFERENCES price_observations (id, user_id) ON DELETE CASCADE,
    -- Duplicate alerts are unrepresentable: one rule firing on one
    -- observation is one alert, whatever happens concurrently (ADR-0012
    -- §Decision 6). The evaluation inserts with
    -- `ON CONFLICT (rule_id, observation_id) DO NOTHING`.
    CONSTRAINT price_alerts_rule_observation_unique UNIQUE (rule_id, observation_id),
    -- An alert cannot be read before it fired.
    CONSTRAINT price_alerts_read_after_triggered_chk CHECK (
        read_at IS NULL OR read_at >= triggered_at
    ),
    CONSTRAINT price_alerts_notification_status_chk CHECK (
        notification_status IN ('pending', 'delivered')
    )
);

-- The alert list, newest first. The unread filter (`read_at IS NULL`) filters
-- this index rather than needing its own.
CREATE INDEX price_alerts_user_id_triggered_at_idx
    ON price_alerts (user_id, triggered_at DESC);
