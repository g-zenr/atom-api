-- ATOM Phase 1 — notes.
--
-- A note is owned by exactly one user. Ownership is enforced by the `user_id`
-- foreign key plus a mandatory `user_id` predicate in every application query.

CREATE TABLE notes (
    id          UUID        PRIMARY KEY,
    user_id     UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title       TEXT        NOT NULL,
    content     TEXT        NOT NULL DEFAULT '',
    is_archived BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT notes_title_length_chk    CHECK (CHAR_LENGTH(title) BETWEEN 1 AND 200),
    CONSTRAINT notes_content_length_chk  CHECK (CHAR_LENGTH(content) <= 100000)
);

-- Primary listing path: the owner's notes, newest first.
CREATE INDEX notes_user_id_created_at_idx ON notes (user_id, created_at DESC);
-- Supports the "active vs archived" filter without a full scan.
CREATE INDEX notes_user_id_archived_idx ON notes (user_id, is_archived);

-- Full-text search over title and content, owned by the note so that search
-- results are automatically restricted to the owner.
--
-- `to_tsvector('simple', ...)` is used deliberately: ATOM notes are technical
-- and often contain identifiers and code fragments. The `simple` configuration
-- keeps stemming predictable and avoids language-specific tokenisation of
-- identifiers. See docs/architecture/data-architecture.md.
ALTER TABLE notes
    ADD COLUMN search_vector tsvector
    GENERATED ALWAYS AS (
        setweight(to_tsvector('simple', COALESCE(title, '')), 'A') ||
        setweight(to_tsvector('simple', COALESCE(content, '')), 'B')
    ) STORED;

CREATE INDEX notes_search_vector_idx ON notes USING GIN (search_vector);
