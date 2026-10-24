-- ATOM Phase 2 — reimbursements: expense claims and their line items.
--
-- A claim is the aggregate root: it owns the currency and the stored total,
-- and its items are denominated in it. Both tables carry the owner, not only
-- the claim, because the ownership predicate has to be expressible in the
-- `WHERE` clause of every statement without a join (ADR-0005).
--
-- The definition of every column, constraint and index here is
-- docs/database/schema.md §11.1 and §11.2. This file is the enforced copy of
-- that description; if the two ever disagree, the document is corrected in a
-- new change and the migration stays as it was applied.

-- The five-state lifecycle. A controlled value in the database, not only in
-- the service, so that no code path can write a status the API does not
-- publish.
CREATE TYPE expense_claim_status AS ENUM ('draft', 'submitted', 'approved', 'rejected', 'paid');

CREATE TABLE expense_claims (
    id           UUID                 PRIMARY KEY,
    user_id      UUID                 NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title        TEXT                 NOT NULL,
    description  TEXT                 NOT NULL DEFAULT '',
    -- ISO 4217. `CHAR(3)` rather than `TEXT` so that an over-long code is a
    -- length error at the type and not only in the constraint below.
    currency     CHAR(3)              NOT NULL,
    -- Money is `NUMERIC(19,4)` at rest and a string with four decimals on the
    -- wire (ADR-0007). Never a float.
    total_amount NUMERIC(19,4)        NOT NULL DEFAULT 0,
    status       expense_claim_status NOT NULL DEFAULT 'draft',
    -- Optional link. `SET NULL` keeps the claim when its project is deleted:
    -- the claim is money the user is owed, the project is an organising label,
    -- so losing the label must not lose the record (REIMB-R09).
    project_id   UUID                 REFERENCES projects (id) ON DELETE SET NULL,
    submitted_at TIMESTAMPTZ,
    decided_at   TIMESTAMPTZ,
    paid_at      TIMESTAMPTZ,
    created_at   TIMESTAMPTZ          NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ          NOT NULL DEFAULT NOW(),
    CONSTRAINT expense_claims_title_length_chk       CHECK (CHAR_LENGTH(title) BETWEEN 1 AND 200),
    CONSTRAINT expense_claims_description_length_chk CHECK (CHAR_LENGTH(description) <= 5000),
    -- Upper case is part of the contract, not a formatting preference: the
    -- wire form of a currency is `USD`, so a lower-case code is a bug that
    -- would surface only when the client compares strings.
    CONSTRAINT expense_claims_currency_chk           CHECK (currency ~ '^[A-Z]{3}$'),
    -- A claim's total is maintained by the service from SUM(amount) in the
    -- same transaction as every item write (REIMB-R03); a row CHECK cannot
    -- see other rows, so the equality itself is not enforced here. What the
    -- database does enforce is the sign.
    CONSTRAINT expense_claims_total_amount_chk       CHECK (total_amount >= 0),
    -- The three stamps are monotonic: a decision cannot predate the
    -- submission and a payment cannot predate the decision, which the
    -- lifecycle arms alone do not say.
    CONSTRAINT expense_claims_timestamp_order_chk CHECK (
        (decided_at IS NULL OR (submitted_at IS NOT NULL AND decided_at >= submitted_at))
        AND (paid_at IS NULL OR (decided_at IS NOT NULL AND paid_at >= decided_at))
    ),
    -- The direct analogue of tasks_completion_consistency_chk: it makes "a
    -- submitted claim without a submission date" and "a paid claim without a
    -- decision" impossible to store, whatever writes the row. A rejected
    -- claim that is reopened clears both stamps, which is why the `draft` arm
    -- requires all three to be null.
    CONSTRAINT expense_claims_lifecycle_timestamps_chk CHECK (
        (status = 'draft'
            AND submitted_at IS NULL AND decided_at IS NULL AND paid_at IS NULL)
        OR (status = 'submitted'
            AND submitted_at IS NOT NULL AND decided_at IS NULL AND paid_at IS NULL)
        OR (status IN ('approved', 'rejected')
            AND submitted_at IS NOT NULL AND decided_at IS NOT NULL AND paid_at IS NULL)
        OR (status = 'paid'
            AND submitted_at IS NOT NULL AND decided_at IS NOT NULL AND paid_at IS NOT NULL)
    )
);

-- The filtered list: the owner's claims in one status, newest first.
CREATE INDEX expense_claims_user_id_status_created_at_idx
    ON expense_claims (user_id, status, created_at DESC);
-- The unfiltered list.
CREATE INDEX expense_claims_user_id_created_at_idx
    ON expense_claims (user_id, created_at DESC);
-- The claims attached to one project, and the lookup that `ON DELETE SET
-- NULL` performs when a project is deleted. Partial because most claims carry
-- no project.
CREATE INDEX expense_claims_project_id_idx
    ON expense_claims (project_id) WHERE project_id IS NOT NULL;
-- Not a second key: `id` is already unique. PostgreSQL requires a unique
-- index on exactly the columns a composite foreign key references, and
-- `expense_items` references `(id, user_id)`, which is what makes a
-- cross-owner item unrepresentable.
CREATE UNIQUE INDEX expense_claims_id_user_id_unique_idx
    ON expense_claims (id, user_id);

CREATE TABLE expense_items (
    id          UUID          PRIMARY KEY,
    -- Both keys are required. The direct `users` key states the ownership;
    -- the composite key below states that the ownership agrees with the
    -- claim's, which the direct key alone cannot say.
    user_id     UUID          NOT NULL,
    claim_id    UUID          NOT NULL,
    description TEXT          NOT NULL,
    amount      NUMERIC(19,4) NOT NULL,
    -- A calendar day, not an instant: storing it as TIMESTAMPTZ would invent a
    -- time zone the user never supplied and move the day for half the world.
    incurred_on DATE          NOT NULL,
    category    TEXT,
    receipt_url TEXT,
    created_at  TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    CONSTRAINT expense_items_user_fk FOREIGN KEY (user_id)
        REFERENCES users (id) ON DELETE CASCADE,
    -- The strongest constraint on this table: the item's owner must be the
    -- claim's owner, so a cross-owner item cannot be inserted by any code
    -- path, not even one that forgets the service's pre-check. The cascade
    -- deletes the items in the same statement as the claim.
    CONSTRAINT expense_items_claim_fk FOREIGN KEY (claim_id, user_id)
        REFERENCES expense_claims (id, user_id) ON DELETE CASCADE,
    CONSTRAINT expense_items_description_length_chk CHECK (CHAR_LENGTH(description) BETWEEN 1 AND 500),
    CONSTRAINT expense_items_amount_chk             CHECK (amount >= 0),
    CONSTRAINT expense_items_category_chk           CHECK (
        category IS NULL
        OR (category = LOWER(BTRIM(category)) AND CHAR_LENGTH(category) BETWEEN 1 AND 64)
    ),
    -- The receipt is a client-supplied reference, not an upload (REIMB-R08),
    -- so the database bounds its length and refuses a scheme the API would
    -- never return, while leaving reachability to the client.
    CONSTRAINT expense_items_receipt_url_chk        CHECK (
        receipt_url IS NULL
        OR (CHAR_LENGTH(receipt_url) BETWEEN 1 AND 2048 AND receipt_url ~* '^https?://')
    )
);

-- The claim detail read, the SUM(amount) that maintains the total, and the
-- leading column of the composite key's cascade lookup.
CREATE INDEX expense_items_claim_id_idx ON expense_items (claim_id);
