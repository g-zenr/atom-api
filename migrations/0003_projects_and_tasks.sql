-- ATOM Phase 1 — projects and tasks.
--
-- Phase 1 is single-owner: a project belongs to exactly one user and every
-- task belongs to exactly one project (and therefore to exactly one user).
-- The ownership predicates are always written as an explicit join or subquery
-- so that a future team-membership model can widen the predicate without
-- rewriting every handler.

CREATE TABLE projects (
    id          UUID        PRIMARY KEY,
    user_id     UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name        TEXT        NOT NULL,
    description TEXT        NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT projects_name_length_chk        CHECK (CHAR_LENGTH(name) BETWEEN 1 AND 120),
    CONSTRAINT projects_description_length_chk CHECK (CHAR_LENGTH(description) <= 5000)
);

CREATE INDEX projects_user_id_created_at_idx ON projects (user_id, created_at DESC);

-- Task status and priority are constrained by the database as well as by the
-- application so that no code path can write an out-of-contract value.
CREATE TYPE task_status AS ENUM ('open', 'in_progress', 'done');
CREATE TYPE task_priority AS ENUM ('low', 'medium', 'high', 'urgent');

CREATE TABLE tasks (
    id           UUID          PRIMARY KEY,
    project_id   UUID          NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    title        TEXT          NOT NULL,
    description  TEXT          NOT NULL DEFAULT '',
    status       task_status   NOT NULL DEFAULT 'open',
    priority     task_priority NOT NULL DEFAULT 'medium',
    -- Fractional index used for manual ordering inside a project.
    position     DOUBLE PRECISION NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    CONSTRAINT tasks_title_length_chk       CHECK (CHAR_LENGTH(title) BETWEEN 1 AND 200),
    CONSTRAINT tasks_description_length_chk CHECK (CHAR_LENGTH(description) <= 10000),
    -- A task marked `done` must carry a completion timestamp and vice versa.
    CONSTRAINT tasks_completion_consistency_chk CHECK (
        (status = 'done' AND completed_at IS NOT NULL)
        OR (status <> 'done' AND completed_at IS NULL)
    )
);

CREATE INDEX tasks_project_id_position_idx ON tasks (project_id, position, created_at);
CREATE INDEX tasks_project_id_status_idx ON tasks (project_id, status);
CREATE INDEX tasks_project_id_priority_idx ON tasks (project_id, priority);

-- Deleting a project cascades to its tasks (see the ON DELETE CASCADE above).
-- The product decision for "what happens to the tasks of a deleted project" is
-- recorded in docs/features/projects/specification.md.
