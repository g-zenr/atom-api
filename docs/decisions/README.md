# ATOM — architecture decision records

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-27
> **Owner:** maintained by the `architect` agent

An architecture decision record (ADR) records **one** decision, the options
that were considered, and the consequences that follow. The decisions in this
directory are part of the product requirements: they are ranked with
[`../project-truth.md`](../project-truth.md) as the highest authority in the
order defined in [`../README.md`](../README.md) §1.

---

## 1. The process

1. **Raise the question.** The question is a real fork: two reasonable
   options lead to materially different structures, and the choice is
   expensive to reverse.
2. **Write the ADR.** Copy [`../templates/adr.md`](../templates/adr.md) and
   follow the template's section list exactly. Name the file
   `ADR-NNNN-short-slug.md`.
3. **Take the next number.** The number is the identifier. It is never reused
   and never renumbered, even if the ADR is later rejected or superseded.
4. **Set the status.** `Proposed` while the decision is under discussion,
   `Accepted` once it is agreed.
5. **Record it.** The decision is listed in the index below, and the change
   that implements it is recorded in
   [`../engineering/change-log.md`](../engineering/change-log.md).
6. **Implement it.** The code, the migration and the documentation change
   together, in one pull request.

### 1.1 An ADR is never rewritten

A decision that turns out to be wrong is **not** edited. It is superseded:

- the old ADR is marked `Superseded` and its **Superseded by** field names the
  new ADR;
- the new ADR names the old one in its **Supersedes** field;
- both files stay in the tree forever.

The same applies to a rejected decision: it stays, with the status `Rejected`,
and the reason in the *Alternatives considered* section. The history of what
was tried is part of the value of the directory.

### 1.2 The statuses

| Status | Meaning |
| ------ | ------- |
| `Proposed` | The decision is under discussion. Nothing in the code may be built as if it were agreed. |
| `Accepted` | The decision is agreed and is in force. |
| `Rejected` | The option was seriously considered and was not chosen. The ADR records why, and it is not replaced by a new ADR unless the decision is taken up again. |
| `Superseded` | A later ADR replaced this one. The file stays, with the pointer to its successor. |
| `Deprecated` | The decision was in force and has been deliberately dropped. The reason is recorded. |

## 2. When an ADR is required

Write an ADR when the decision:

- **materially affects the structure**, the security model, the compatibility
  promise or the long-term maintenance of the system;
- **is expensive to reverse** once it is in production;
- **replaces or narrows** an earlier accepted decision;
- **constrains every future contributor** — a layering rule, a forbidden
  pattern, a mandatory mechanism;
- **changes a public contract** in a way a client can observe.

### 2.1 When an ADR is **not** required

| Situation | Where it is recorded instead |
| --------- | --------------------------- |
| Adding an endpoint that fits the existing model | The feature documents and the change log. |
| Fixing a defect | The change log and the review log. |
| Adding a dependency | The change log, with the concrete reason, as required by [`../../AGENTS.md`](../../AGENTS.md) §4.8. |
| Tuning a default value | The configuration reference in [`../architecture/deployment.md`](../architecture/deployment.md) §1. |
| Adding a table or a column | The migration, the schema document and the change log. The *rules* for how a migration is written are already decided in [`../database/migration-policy.md`](../database/migration-policy.md). |
| Fixing a documentation error | The change log. |

The test: would a future contributor reasonably ask "why is it done this
way?" and expect an answer that is not obvious from the code? If yes, write
an ADR.

## 3. The template rules

The template is [`../templates/adr.md`](../templates/adr.md). The rules are:

1. **Keep the headings and the order.** The sections are `Status`, `Date`,
   `Context`, `Decision`, `Alternatives considered`, `Consequences and
   trade-offs`, `Security and operational implications`, `Related requirements
   and changes`, and `Supersedes`/`Superseded by`.
2. **One decision per ADR.** A decision with two halves belongs in two ADRs.
3. **State the decision in the present tense and the active voice.** "ATOM
   stores …", "Ownership is enforced in …". A reader must be able to tell
   what the system does because of the decision, without interpretation.
4. **Name every serious alternative**, and say why it was not chosen. An
   alternative that is not mentioned was probably not seriously considered; an
   alternative that is mentioned without a reason was probably not seriously
   rejected.
5. **Name the negative consequences.** A decision with no downside has not
   been examined. Every negative consequence that needs work becomes an entry
   in [`../engineering/known-issues.md`](../engineering/known-issues.md) or a
   task in the change log.
6. **Every link resolves**, and every file path, type name and endpoint in
   the ADR exists in the repository.
7. **Write "None"** in the Supersedes/Superseded by field when it does not
   apply.

The checker `cargo run --bin atom-docs` verifies that every ADR has a status
and a unique identifier, and that every relative link in the tree resolves.

## 4. The index

| Identifier | Title | Status | Date |
| ---------- | ----- | ------ | ---- |
| [ADR-0001](ADR-0001-modular-monolith.md) | ATOM is a modular monolith | Accepted | 2026-10-21 |
| [ADR-0002](ADR-0002-rust-axum-postgresql.md) | Rust, Axum and PostgreSQL | Accepted | 2026-10-21 |
| [ADR-0003](ADR-0003-auth-token-design.md) | The access and refresh token design | Accepted | 2026-10-21 |
| [ADR-0004](ADR-0004-documentation-as-project-truth.md) | The documentation tree is part of the product | Superseded | 2026-10-21 |
| [ADR-0005](ADR-0005-ownership-enforced-in-sql.md) | Ownership is enforced in SQL | Accepted | 2026-10-21 |
| [ADR-0006](ADR-0006-configuration-source-of-truth.md) | The configuration directory is the source of truth | Accepted | 2026-10-22 |
| [ADR-0007](ADR-0007-money-representation.md) | Money is `NUMERIC(19,4)` and a JSON string | Accepted | 2026-10-24 |
| [ADR-0008](ADR-0008-local-pure-rust-ml.md) | Machine learning is local and pure Rust | Accepted | 2026-10-24 |
| [ADR-0009](ADR-0009-receipt-references.md) | A receipt is a reference, not an uploaded file | Accepted | 2026-10-24 |
| [ADR-0010](ADR-0010-semantic-search-scan-bound.md) | Semantic search keeps its corpus-bound scan, accepted up to a named volume | Accepted | 2026-10-25 |
| [ADR-0011](ADR-0011-api-keys.md) | A machine client authenticates with a per-user, revocable API key | Accepted | 2026-10-27 |
| [ADR-0012](ADR-0012-automation-orchestration-scope.md) | Automation is orchestrated by an external n8n instance that reaches ATOM only through authenticated endpoints | Proposed (owner decision `O7`) | 2026-10-30 |

## 5. Related documents

- [`../project-truth.md`](../project-truth.md) — the product requirements the
  decisions serve.
- [`../architecture/README.md`](../architecture/README.md) — the architecture
  the decisions produced.
- [`../engineering/change-log.md`](../engineering/change-log.md) — the
  changes that implemented the decisions.
- [`../engineering/open-questions.md`](../engineering/open-questions.md) — the
  decisions that are still open.
- [`../templates/adr.md`](../templates/adr.md) — the template.
