-- ATOM Phase 2 — local machine learning: the embedding index.
--
-- One row per source, model and owner: the vector an embedder produced for a
-- note or a job application, stored beside the model identifier that gives the
-- vector its meaning. The table carries the owner in its own column, not only
-- through the source, because every statement has to scope the row by
-- `user_id` in its `WHERE` clause without a join (ADR-0005, ML-R08).
--
-- The column is `REAL[]` rather than pgvector: `CREATE EXTENSION vector` is
-- denied to the application role on the development machine while CI's role
-- would be allowed to create it, so a migration that required the extension
-- would pass CI and fail locally. The hybrid search re-ranks a bounded
-- candidate set in Rust instead, over the built-in array type.
--
-- The definition of every column, constraint and index here is
-- docs/database/schema.md §11.6. This file is the enforced copy of that
-- description; if the two ever disagree, the document is corrected in a new
-- change and the migration stays as it was applied.
--
-- `source_id` is **polymorphic** (`note` or `application`) and therefore carries
-- **no foreign key**: PostgreSQL cannot express a reference to one of two
-- tables from one column. The cost is that the database cannot refuse an
-- embedding whose source does not exist. The replacement, fixed by ADR-0008 §3:
--
--   * the source row and its embedding are written in the same statement, and
--     the source delete purges its own embeddings in that same statement — a
--     data-modifying CTE — so both halves commit or neither does;
--   * the owner-scoped reindex sweep deletes the embeddings whose source is
--     gone, as the backstop for a delete that bypassed the service (ML-R07);
--   * the write path checks the source with `WHERE id = $ AND user_id = $1`
--     before inserting, and every read filters `user_id` first, so a row that
--     somehow pointed across owners is never returned to the caller.
--
-- A trigger is deliberately not added: no rule of this database lives in a
-- trigger, and a hidden delete is harder to audit than the statement that owns
-- it. The trade-off is recorded in ADR-0008 and in schema.md §11.6.

CREATE TABLE embeddings (
    id          UUID        PRIMARY KEY,
    user_id     UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- `TEXT` with a CHECK rather than an enum type, unlike the other controlled
    -- values of Phase 2 (§11.7). It is a two-value discriminator the query
    -- builder matches on before it chooses a static statement, and the ML
    -- storage design fixes it as text. The CHECK still makes any other value
    -- unrepresentable, which is the guarantee that matters.
    source_type TEXT        NOT NULL,
    -- The id of the note or the application. No foreign key: see the header.
    source_id   UUID        NOT NULL,
    -- The embedder that produced the vector, including its version. Changing
    -- what a vector means is a new `model_id`, never a silent rewrite of the
    -- stored rows.
    model_id    TEXT        NOT NULL,
    -- The declared dimension, stored rather than inferred, so a reader can
    -- reject a candidate set from another model without inspecting every array,
    -- and so the check below is a column comparison rather than an array length
    -- computed per row.
    dim         INTEGER     NOT NULL,
    -- Unit length by construction (ADR-0008 §9), so a cosine score is the dot
    -- product of two stored values. That convention lives in `Embedder::embed`,
    -- not in this table: `dim` records the length, not the normalisation.
    vector      REAL[]      NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Set by the upsert's `DO UPDATE` when a reindex replaces the vector for the
    -- same source and model.
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embeddings_source_type_chk     CHECK (source_type IN ('note', 'application')),
    CONSTRAINT embeddings_model_id_length_chk CHECK (CHAR_LENGTH(model_id) BETWEEN 1 AND 200),
    -- `cardinality` counts the elements and returns 0 for an empty array, so an
    -- empty vector fails for every positive `dim`. `array_length(vector, 1)`
    -- would return NULL instead, and a CHECK passes when its expression is
    -- NULL; `cardinality` is the function that makes the rule total.
    CONSTRAINT embeddings_dimension_chk       CHECK (dim > 0 AND dim = cardinality(vector))
);

-- The conflict target of the re-embed upsert (ML-R04, ML-R07), the
-- delete-by-source lookup, and the predicate every owner-scoped statement
-- starts with. `user_id` leads, so a statement that names another owner cannot
-- read this owner's rows through it, and two concurrent inserts for one source
-- cannot both succeed: the duplicate is a unique violation, not a
-- check-then-insert race.
CREATE UNIQUE INDEX embeddings_user_id_source_model_unique_idx
    ON embeddings (user_id, source_type, source_id, model_id);

-- The search pipeline filters the caller's rows by source type and model id
-- before it reads any vector (ML-R05). A different access path from the unique
-- index, which cannot seek on `model_id` before `source_id`.
CREATE INDEX embeddings_user_id_source_type_model_id_idx
    ON embeddings (user_id, source_type, model_id);
