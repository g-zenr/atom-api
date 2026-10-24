# Job applications — specification

> **Status:** Accepted
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature identifier:** `ATOM-FEAT-JOB-001`
> **Feature name:** Companies, job applications, their stages and their timeline

## 1. Purpose and the user problem

Looking for work means keeping track of a lot of small, similar things: which
company, which role, when it was sent, who replied, when the interview is, and
what happened last. Left in an inbox, the answer to "did I ever hear back from
them?" is a search through months of mail.

This feature keeps that record in one place: a **company**, the
**applications** made to it, each application's **stage**, and a dated
**timeline of events**. It is a planning and memory tool. It deliberately does
**not** write the application for the owner: no cover letter, no summary, no
generated message. The value is in the record and the reminder, not in
producing text.

The application can point at one **task** as its next action, so the next
thing to do lives in the same task list as everything else.

## 2. Status

**Accepted, not built.** The requirements are recorded in
[`project-truth.md`](../../project-truth.md) §18.2 as `JOB-R01`…`R09` and
agreed on 2026-10-24. No code, migration or test for this feature exists, so
the feature is **not** `Implemented` and **not** `Verified`. See
[status.md](status.md).

The model rests on assumptions **A3** and **A6** in
[`../../engineering/runs/chain/plan.md`](../../engineering/runs/chain/plan.md) §3,
which the owner did not explicitly confirm (owner decision O1). They are
recorded as **provisional defaults** in
[`../../project-truth.md`](../../project-truth.md) §18.4 and are **not**
owner-approved requirements.

## 3. Scope

In scope:

- Companies owned by the caller, with a per-owner unique name, an optional
  website and notes.
- Listing companies with a search filter and pagination.
- Creating, reading, partially updating and deleting a company.
- Applications to a company, with a role title, a stage, and optional source,
  applied-on date, job URL, description and salary range.
- Listing applications with a stage filter, a company filter, a search term
  and an applied-date range, with the shared pagination.
- Reading one application.
- Reading an application's events with the shared pagination.
- Moving an application through the seven-stage machine, refusing every other
  move.
- An optional link to one task as the next action.
- A timeline of events per application, with a kind, a timestamp and a
  summary.
- Ownership isolation: a company, application or event of another account is
  invisible, and the API answers `404`, never `403`.

## 4. Non-goals

These are deliberately **not** part of Phase 2:

- **Text generation of any kind.** No cover letter, no résumé tailoring, no
  summary of an interview, no "improve this" — `JOB-R08` and
  [`project-truth.md`](../../project-truth.md) §6.1.
- **Scraping a job board.** No OnlineJobs.ph integration, no crawler, no
  import from a third party. It stays excluded and `Proposed` in
  [`../../roadmap.md`](../../roadmap.md).
- **Sending email or calendar invitations.** The feature records that an
  interview happened; it does not schedule one.
- **A rejection reason, a rating, or a free-form stage.** The stage is one of
  seven fixed values.
- **Multiple contacts per company**, an address book, or a recruitment
  pipeline across applicants. The product is single-owner: the owner is the
  applicant.
- **Attachments.** A job URL and a description are text; no file is uploaded.
- **A stage history.** The events are the timeline; a stage change is not
  itself recorded as an event in Phase 2.
- **Analytics** — response rates, average time to interview, dashboards.
- **A client-selectable sort.** The order of every list is fixed; see
  [api.md](api.md).

## 5. Accepted requirements

The source of truth is [`project-truth.md`](../../project-truth.md) §18.2.

| ID | Requirement | Planned implementation | Planned coverage |
| -- | ----------- | ---------------------- | ---------------- |
| `JOB-R01` | A company name is unique per owner, case-insensitively. | `companies` unique index on `(user_id, lower(name))` | `a_company_name_is_unique_per_owner`, `two_owners_may_use_the_same_company_name` |
| `JOB-R02` | An application belongs to one company owned by the same user, with a role title and one of seven stages. | `job_applications` with `company_id` and a `status` `CHECK` | `an_application_can_be_created_and_read`, `an_application_cannot_target_another_users_company`, `a_create_with_a_stage_other_than_wishlist_is_refused` |
| `JOB-R03` | The salary range is optional, in one currency, with `min <= max`, and money follows `REIMB-R07`. | `salary_*` columns and a `CHECK` | `a_salary_bound_requires_a_currency`, `a_salary_minimum_above_the_maximum_is_refused`, `salary_money_round_trips_with_four_decimals` |
| `JOB-R04` | The seven-state stage machine with three terminal stages. | `JobService::update_application` and `ensure_transition` | `a_wishlist_application_can_reach_offer`, `every_forbidden_stage_change_is_refused`, `offer_rejected_and_withdrawn_are_terminal` |
| `JOB-R05` | Events belong to an application and are ordered by `occurred_at` then `id`, newest first. | `application_events`, ordered list query | `events_are_ordered_by_occurred_at`, `an_event_of_another_users_application_is_invisible` |
| `JOB-R06` | Deleting an application deletes its events; a company with applications cannot be deleted. | `ON DELETE CASCADE`, and a service check | `deleting_an_application_removes_its_events`, `a_company_with_applications_cannot_be_deleted` |
| `JOB-R07` | An application may reference one task as its next action; deleting the task clears it. | `next_action_task_id` `ON DELETE SET NULL` | `the_next_action_task_must_belong_to_the_caller`, `deleting_the_task_clears_the_next_action` |
| `JOB-R08` | No text generation and no outbound AI call. | no such route, no HTTP client in the module | `no_job_operation_generates_text` (contract-level: no generation operation) |
| `JOB-R09` | Every read and write is owner-scoped; a foreign row is `404`. | `user_id = $n` in every statement ([`ADR-0005`](../../decisions/ADR-0005-ownership-enforced-in-sql.md)) | `a_company_of_another_user_is_invisible`, `an_application_of_another_user_is_invisible`, `the_companies_listing_only_contains_the_callers_companies` |

## 6. User-facing behaviour

### 6.1 Companies

- A name is trimmed, 1–200 characters, and unique per owner **ignoring case**:
  `Acme` and `acme` collide. The collision is a `409 conflict`.
- Two owners may each have a company of the same name.
- `website` is optional, an absolute `http`/`https` URL, at most 2048
  characters. `notes` is optional, at most 5000 characters.
- The company list is ordered `lower(name)`, then `name`, then `id`, and takes
  `search`, `limit` and `offset`.

### 6.2 Applications

- An application belongs to exactly one company, and the company must belong
  to the caller (`404` otherwise).
- `role_title` is required, trimmed, 1–200 characters.
- `status` is one of `wishlist`, `applied`, `screening`, `interview`, `offer`,
  `rejected`, `withdrawn`. **On create only `wishlist` or an omitted stage is
  accepted**; any other stage is `422`, because an application starts at the
  beginning of the machine. Later changes go through the transition table.
- `source` is optional free text, at most 64 characters (`LinkedIn`,
  `referral`, …).
- `applied_on` is an optional `YYYY-MM-DD` date.
- `job_url` is optional, an absolute `http`/`https` URL, at most 2048
  characters.
- `description` is optional, at most 10 000 characters, kept verbatim.
- The salary range is optional. If either bound is present, `salary_currency`
  is required. If both bounds are present, `min <= max`. Money is a
  four-decimal string ([`ADR-0007`](../../decisions/ADR-0007-money-representation.md)).
- `next_action_task_id` is an optional task owned by the caller (`404`
  otherwise). Deleting the task clears the link.

### 6.3 The stage machine

```text
wishlist ──► applied ──► screening ──► interview ──► offer
    │           │            │             │
    └───────────┴────────────┴─────────────┴──────► rejected
    └───────────┴────────────┴─────────────┴──────► withdrawn

offer, rejected and withdrawn are terminal: nothing leaves them.
```

| From | Allowed next stages |
| ---- | ------------------- |
| `wishlist` | `applied`, `rejected`, `withdrawn` |
| `applied` | `screening`, `rejected`, `withdrawn` |
| `screening` | `interview`, `rejected`, `withdrawn` |
| `interview` | `offer`, `rejected`, `withdrawn` |
| `offer` | none |
| `rejected` | none |
| `withdrawn` | none |

- A **skipped** stage (`wishlist → interview`) is refused with `409 conflict`.
  The machine is a sequence, and a stage says what is happening now.
- A **backwards** move (`interview → applied`) is refused with `409`.
- The stage changes through `PATCH /job-applications/{application_id}` with
  the `status` field, because the agreed endpoint list has no transition
  endpoint. The `PATCH` is transactional: if the move is refused, no other
  field in the same request is written either.
- Sending the current stage again (`applied → applied`) is accepted and
  changes nothing. It is not a transition, so it is not a `409`.

### 6.4 Events

- An event belongs to one application, and the application must belong to the
  caller.
- `kind` is one of `note`, `email`, `call`, `interview`, `offer`, `rejection`.
- `occurred_at` is required, RFC 3339, and may be in the **future** — an
  interview is scheduled before it happens.
- `summary` is required, trimmed, 1–200 characters. `details` is optional, at
  most 10 000 characters.
- The events of an application are listed **newest first**:
  `occurred_at DESC, id DESC`. The order is total and stable. This matches
  every other listing in ATOM, which is newest first; a client that wants a
  chronological timeline reverses the page.
- Updating an event changes `kind`, `occurred_at`, `summary` and `details`.
  Events have no state machine.
- Deleting an application deletes its events. There is no soft delete.

### 6.5 Validation

| Field | Rule | Result |
| ----- | ---- | ------ |
| company `name` | Trimmed, 1–200 characters, unique per owner ignoring case. | `422` for the shape, `409` for the duplicate. |
| company `website` | Absolute `http`/`https`, 1–2048 characters. | `422`. |
| company `notes` | At most 5000 characters. | `422`. |
| `role_title` | Trimmed, 1–200 characters. | `422`. |
| `status` | One of the seven values (body) / the transition table. | On create, only `wishlist` or omitted is accepted; any other value is `422`. On update, an unknown value is `422` and a refused move is `409`. |
| `source` | At most 64 characters; a value that is blank after trimming is refused. | `422`. |
| `applied_on` | `YYYY-MM-DD`, a real date. | `422`. |
| `job_url` | Absolute `http`/`https`, 1–2048 characters. | `422`. |
| `description` | At most 10 000 characters. | `422`. |
| `salary_min`, `salary_max` | Four-decimal money strings, `>= 0`; `min <= max`. | `422` naming the field. |
| `salary_currency` | Three ASCII letters; required when a bound is present, and refused when neither bound is. | `422`. |
| `next_action_task_id` | UUID of a task owned by the caller. | `404` if foreign, `422` if not a UUID. |
| event `kind` | One of the six values. | `422`. |
| event `occurred_at` | RFC 3339 with a timezone. | `422`. |
| event `summary` | Trimmed, 1–200 characters. | `422`. |
| event `details` | At most 10 000 characters. | `422`. |
| any id in the path | UUID. | `400`. |

## 7. Dependencies on other features

| Direction | Relationship |
| --------- | ------------ |
| Job applications → Authentication | Every endpoint requires the `AuthUser` context; its `user_id` is the ownership predicate. |
| Job applications → Users | `companies.user_id`, `job_applications.user_id` and `application_events.user_id` are non-null foreign keys to `users (id) ON DELETE CASCADE`. |
| Job applications → Tasks | **Optional.** `next_action_task_id` is nullable, `ON DELETE SET NULL`. A foreign task is `404`. |
| Job applications → Projects | None directly. The linked task belongs to a project, but the application does not. |
| Job applications → Notes | Only through the ML feature, which may embed an application's text. There is no note link. |
| Job applications → ML | `application` is one of the two `source_type` values; `POST /job-applications/{id}/match` ranks the owner's notes against it. |

## 8. Acceptance criteria

A change to this feature is complete when all of the following hold.

- [ ] The three quality gates pass with a disposable `TEST_DATABASE_URL`, and
      `atom-docs` reports no error.
- [ ] Every requirement in §5 has a named test in [testing.md](testing.md),
      and the test fails if the rule is removed.
- [ ] Every allowed stage edge returns `200` and every other pair returns
      `409 conflict`, asserted one case per pair.
- [ ] A company name collision within one owner is `409`; the same name for
      two owners is accepted.
- [ ] Deleting an application removes its events; deleting a company with
      applications is `409`.
- [ ] Every statement that touches a company, an application or an event
      carries `user_id = $n`, and a foreign row is `404`.
- [ ] The companies listing is `lower(name)`, then `name`, then `id`; the
      applications listing is `created_at DESC`; and the events listing is
      `occurred_at DESC`; each is asserted with rows inserted out of order.
- [ ] The pagination clamp and the filters are asserted on all three list
      endpoints.
- [ ] Every path and method is in `/api/v1/openapi.json`, asserted by the
      contract test.
- [ ] The migration applies from an empty database, and
      [`schema.md`](../../database/schema.md) describes it.
- [ ] The seeder creates companies and applications in every stage, with
      events, in the same chain.
- [ ] [status.md](status.md) reflects the evidence, and nothing is marked
      `Verified` without a recorded passing run.

## 9. Known limitations

- **The stage is changed through `PATCH`.** The agreed endpoint list has no
  transition route, so a stage move and a field edit share one request. A
  refused move therefore refuses the whole request. This is stated in
  [api.md](api.md) for each status.
- **No stage history.** A timeline event can record that an interview
  happened, but the service does not write a "status changed" event. The
  stage's previous values are lost.
- **No reopened application.** `rejected` and `withdrawn` are terminal, so a
  company that reopens a role needs a new application.
- **No rejection reason**, rating, or notes per stage.
- **A company cannot be deleted while it has applications.** The client must
  delete or move them first. This protects the history from a cascade.
- **No text generation**, by design. A client that wants a cover letter must
  produce it elsewhere.
- **Salary is informational.** There is no conversion, no comparison against
  a market rate and no total compensation.
- **A single next-action task.** An application with three follow-ups needs
  three tasks in the task list, and the link points at one.

## 10. Changelog references

Substantial changes are recorded in
[`change-log.md`](../../engineering/change-log.md). The feature identifier is
stable; the entries are appended and never rewritten.

- The Phase 2 product intake that accepted
  `JOB-R01`…`R09`. The change-log identifier is assigned when the intake is
  committed; no identifier is claimed here, because the log is the authority
  for it.
