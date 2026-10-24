-- ATOM Phase 2 — job applications: companies, applications and their timeline.
--
-- A company is the employer; an application is one role at that company; an
-- event is one dated entry in an application's history. All three tables carry
-- the owner, not only the parent, because every statement has to scope the row
-- by `user_id` in its `WHERE` clause without a join (ADR-0005). The composite
-- foreign keys then make "an application whose owner is not its company's
-- owner" and "an event whose owner is not its application's owner"
-- unrepresentable, rather than merely rejected by the service.
--
-- The definition of every column, constraint and index here is
-- docs/database/schema.md §11.3, §11.4 and §11.5. This file is the enforced
-- copy of that description; if the two ever disagree, the document is
-- corrected in a new change and the migration stays as it was applied.
--
-- There is no endpoint behind these tables yet (chain J1 is schema and types
-- only), so the indexes below are the access paths the feature specification
-- already fixes, not measurements of a running query.

-- The seven-stage machine, as a controlled value in the database and not only
-- in the service. The enum makes an unknown stage unrepresentable; it cannot
-- express the allowed transitions, because a CHECK sees one row and cannot see
-- the previous status. The transition table therefore lives in the service
-- (JOB-R04) and the enum is the half the database can own.
CREATE TYPE job_application_status AS ENUM (
    'wishlist', 'applied', 'screening', 'interview', 'offer', 'rejected', 'withdrawn'
);

-- An event kind and an application stage are different vocabularies that
-- happen to share two words. One type for both would let an application be
-- stored as `call`.
CREATE TYPE application_event_kind AS ENUM ('note', 'email', 'call', 'interview', 'offer', 'rejection');

CREATE TABLE companies (
    id         UUID        PRIMARY KEY,
    user_id    UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name       TEXT        NOT NULL,
    -- A client-supplied reference, at most 2 048 characters. The service
    -- validates the form; the database bounds it and refuses a scheme the API
    -- would never return.
    website    TEXT,
    notes      TEXT        NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT companies_name_length_chk CHECK (CHAR_LENGTH(name) BETWEEN 1 AND 200),
    -- Trimmed, because the uniqueness rule below compares the folded name: a
    -- trailing space would otherwise make "Acme " a second "Acme" that the
    -- unique index cannot see as a duplicate.
    CONSTRAINT companies_name_trimmed_chk CHECK (name = BTRIM(name)),
    CONSTRAINT companies_website_chk CHECK (
        website IS NULL
        OR (CHAR_LENGTH(website) <= 2048 AND website ~* '^https?://')
    ),
    CONSTRAINT companies_notes_length_chk CHECK (CHAR_LENGTH(notes) <= 5000)
);

-- Case-insensitive uniqueness per owner, as a functional unique index rather
-- than a normalized column. That is deliberately different from
-- `users_email_unique_idx`: an email address has no display form worth
-- preserving, so `0001` stores it lower-cased and `users_email_normalized_chk`
-- enforces that. A company name *is* a display form, so the stored value keeps
-- the casing the owner typed and only the comparison is folded. Two concurrent
-- inserts still collide on the index, so the `409` is a database guarantee, not
-- a check-then-insert race.
CREATE UNIQUE INDEX companies_user_id_lower_name_unique_idx
    ON companies (user_id, LOWER(name));
-- The company list: `ORDER BY LOWER(name), name, id`, the published total
-- order. The unique index above cannot serve it, because `(user_id,
-- LOWER(name))` has no `name` or `id` column to break a tie between two names
-- that differ only in case.
CREATE INDEX companies_user_id_lower_name_name_id_idx
    ON companies (user_id, LOWER(name), name, id);
-- Not a second key: `id` is already unique. PostgreSQL requires a unique index
-- on exactly the columns a composite foreign key references, and
-- `job_applications` references `(id, user_id)`, which is what makes a
-- cross-owner application unrepresentable.
CREATE UNIQUE INDEX companies_id_user_id_unique_idx
    ON companies (id, user_id);

CREATE TABLE job_applications (
    id                  UUID                   PRIMARY KEY,
    user_id             UUID                   NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- Part of the composite key below; the direct key to the company is not
    -- enough, because it would let an application name another owner's company.
    company_id          UUID                   NOT NULL,
    role_title          TEXT                   NOT NULL,
    -- A label the owner types, not a controlled value: there is no enum and no
    -- CHECK on the set, only the trim and the length.
    source              TEXT,
    status              job_application_status NOT NULL DEFAULT 'wishlist',
    -- A calendar day, not an instant: storing it as TIMESTAMPTZ would invent a
    -- time zone the user never supplied and move the day for half the world.
    applied_on          DATE,
    -- Money is `NUMERIC(19,4)` at rest and a string with four decimals on the
    -- wire (ADR-0007). Salary is optional, so all three columns are nullable;
    -- the currency-required constraint below is what keeps them consistent.
    salary_min          NUMERIC(19,4),
    salary_max          NUMERIC(19,4),
    -- ISO 4217. `CHAR(3)` rather than `TEXT` so that an over-long code is a
    -- length error at the type and not only in the constraint below.
    salary_currency     CHAR(3),
    job_url             TEXT,
    description         TEXT                   NOT NULL DEFAULT '',
    next_action_task_id UUID,
    created_at          TIMESTAMPTZ            NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ            NOT NULL DEFAULT NOW(),
    -- `NO ACTION`, not `RESTRICT`, and the difference is the whole reason this
    -- is written out rather than inline. Both refuse a direct
    -- `DELETE FROM companies` while applications remain, which is the `409` of
    -- JOB-R06. Only `NO ACTION` defers the check to the end of the statement,
    -- and that is what lets `DELETE FROM users` succeed: the account's cascade
    -- removes its applications and its companies in the same statement, so by
    -- the time the check runs there is nothing left to object to. `RESTRICT`
    -- fires as soon as the company row goes, before the applications are gone,
    -- and would make deleting an account impossible without deleting its rows
    -- by hand first.
    CONSTRAINT job_applications_company_fk FOREIGN KEY (company_id, user_id)
        REFERENCES companies (id, user_id) ON DELETE NO ACTION,
    -- The next action is an optional link into Phase 1's tasks, cleared when
    -- the task is deleted: the application is the record being tracked, the
    -- task is the reminder attached to it. This key cannot be owner-consistent
    -- in SQL: `tasks` has no `user_id` (§6 of schema.md), ownership is inherited
    -- through `projects`, so a composite key has no matching unique index to
    -- reference. The service checks `tasks -> projects -> user_id` before
    -- writing and returns `404` for a foreign task; the test is the evidence.
    CONSTRAINT job_applications_next_action_task_fk FOREIGN KEY (next_action_task_id)
        REFERENCES tasks (id) ON DELETE SET NULL,
    CONSTRAINT job_applications_role_title_length_chk CHECK (CHAR_LENGTH(role_title) BETWEEN 1 AND 200),
    CONSTRAINT job_applications_source_chk CHECK (
        source IS NULL
        OR (source = BTRIM(source) AND CHAR_LENGTH(source) BETWEEN 1 AND 64)
    ),
    CONSTRAINT job_applications_description_length_chk CHECK (CHAR_LENGTH(description) <= 10000),
    CONSTRAINT job_applications_job_url_chk CHECK (
        job_url IS NULL
        OR (CHAR_LENGTH(job_url) <= 2048 AND job_url ~* '^https?://')
    ),
    CONSTRAINT job_applications_salary_non_negative_chk CHECK (
        (salary_min IS NULL OR salary_min >= 0)
        AND (salary_max IS NULL OR salary_max >= 0)
    ),
    -- An inverted range is a data-entry bug that a report would silently
    -- average into nonsense, so the row is refused instead of stored.
    CONSTRAINT job_applications_salary_range_chk CHECK (
        salary_min IS NULL OR salary_max IS NULL OR salary_min <= salary_max
    ),
    -- Either all three salary columns are null, or a currency is present. A
    -- bound with no currency is a number whose meaning is unknown, which is
    -- worse than no number at all (JOB-R03).
    CONSTRAINT job_applications_salary_currency_required_chk CHECK (
        (salary_min IS NULL AND salary_max IS NULL AND salary_currency IS NULL)
        OR ((salary_min IS NOT NULL OR salary_max IS NOT NULL) AND salary_currency IS NOT NULL)
    ),
    -- Upper case is part of the contract, not a formatting preference: the wire
    -- form is `USD`, so a lower-case code is a bug that would surface only when
    -- the client compares strings.
    CONSTRAINT job_applications_salary_currency_chk CHECK (
        salary_currency IS NULL OR salary_currency ~ '^[A-Z]{3}$'
    )
);

-- The board view: the owner's applications in one stage, newest first.
CREATE INDEX job_applications_user_id_status_created_at_idx
    ON job_applications (user_id, status, created_at DESC);
-- The unfiltered list.
CREATE INDEX job_applications_user_id_created_at_idx
    ON job_applications (user_id, created_at DESC);
-- The company's applications, and the lookup the `NO ACTION` check performs
-- when a company is deleted.
CREATE INDEX job_applications_company_id_idx ON job_applications (company_id);
-- The lookup the `ON DELETE SET NULL` maintenance performs when a task is
-- deleted. Partial because most rows carry no next action.
CREATE INDEX job_applications_next_action_task_id_idx
    ON job_applications (next_action_task_id) WHERE next_action_task_id IS NOT NULL;
-- The referenced key for `application_events`, for the same reason as
-- `companies_id_user_id_unique_idx`.
CREATE UNIQUE INDEX job_applications_id_user_id_unique_idx
    ON job_applications (id, user_id);

CREATE TABLE application_events (
    id             UUID                   PRIMARY KEY,
    -- Both keys are required. The direct `users` key states the ownership; the
    -- composite key below states that the ownership agrees with the
    -- application's, which the direct key alone cannot say.
    user_id        UUID                   NOT NULL,
    application_id UUID                   NOT NULL,
    kind           application_event_kind NOT NULL,
    -- Required, and a future value is accepted on purpose: an interview can be
    -- scheduled, so "occurred" means "belongs at this point on the timeline",
    -- not "is in the past".
    occurred_at    TIMESTAMPTZ            NOT NULL,
    summary        TEXT                   NOT NULL,
    details        TEXT                   NOT NULL DEFAULT '',
    -- There is deliberately no `updated_at`. A timeline entry records what the
    -- owner says happened, and `kind`, `occurred_at`, `summary` and `details`
    -- are corrected in place (decision A-D1-1); `created_at` stays the row's
    -- creation instant and `occurred_at` is the event's own time. A second
    -- timestamp would invite a reader to treat the row as mutable state with a
    -- last-write time, which is not what it is.
    created_at     TIMESTAMPTZ            NOT NULL DEFAULT NOW(),
    CONSTRAINT application_events_user_fk FOREIGN KEY (user_id)
        REFERENCES users (id) ON DELETE CASCADE,
    -- The strongest constraint on this table: an event's owner must be its
    -- application's owner, so a cross-owner event cannot be inserted by any
    -- code path, not even one that forgets the service's pre-check. The cascade
    -- deletes the events in the same statement as the application.
    CONSTRAINT application_events_application_fk FOREIGN KEY (application_id, user_id)
        REFERENCES job_applications (id, user_id) ON DELETE CASCADE,
    CONSTRAINT application_events_summary_length_chk CHECK (CHAR_LENGTH(summary) BETWEEN 1 AND 200),
    CONSTRAINT application_events_details_length_chk CHECK (CHAR_LENGTH(details) <= 10000)
);

-- The timeline, newest first. The query adds `id DESC` as the total-order
-- tiebreaker JOB-R05 requires; the index supplies the leading order, so only
-- ties between two events at the same instant are sorted.
CREATE INDEX application_events_application_id_occurred_at_idx
    ON application_events (application_id, occurred_at DESC);
