# ATOM — architecture

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `architect` agent

This directory describes **the ATOM Phase 1 backend as it is built**. It is the
rank-2 source of truth in the authority order of
[`../README.md`](../README.md): authoritative for the current design, the
structure and the invariants, and **not** authoritative for the product scope.
The product scope lives in [`../project-truth.md`](../project-truth.md) and in
the accepted decisions in [`../decisions/`](../decisions/README.md).

---

## 1. The documents

| Document | Contents |
| -------- | -------- |
| [`system-overview.md`](system-overview.md) | What the system is, the runtime shape, the components, the deployment topology, the stack and the known limitations. |
| [`module-boundaries.md`](module-boundaries.md) | Every module that exists, plus the three planned Phase 2 modules: responsibility, public interface, dependencies, data ownership, invariants, security boundary, tests and limitations. |
| [`request-lifecycle.md`](request-lifecycle.md) | From socket to SQL and back: middleware order, extractor order, the handler → service → repository path, error mapping and the correlation id. |
| [`data-architecture.md`](data-architecture.md) | The persistence model, the ownership model, the search design, the index strategy, the migration/model relationship and the transaction boundaries. |
| [`security-architecture.md`](security-architecture.md) | Trust boundaries, the authentication boundary, the authorization model, secret handling, rate limiting and error disclosure. |
| [`deployment.md`](deployment.md) | Configuration reference, secrets, local development, the production checklist, observability and scaling characteristics. |
| [`../../config/README.md`](../../config/README.md) | The configuration reference: the layers, the precedence, every setting, the secrets, the message catalogue and the validation rules. |

The architecture documents cross-link rather than repeat. The public HTTP
contract is described in [`../api/`](../api/README.md), the schema in
[`../database/schema.md`](../database/schema.md), the security controls in
[`../security/threat-model.md`](../security/threat-model.md) and the per-feature
behaviour in [`../features/`](../features/README.md).

**Planned Phase 2 material is marked, and the marking is not decorative.**
`module-boundaries.md`, `data-architecture.md`, `system-overview.md` and
`deployment.md` each carry one passage that describes the accepted Phase 2
modules, tables or settings. Every such passage opens by naming what does not
exist, and nothing in it is `Implemented` or `Verified`. The rule in §4 below —
a section that covers something that does not exist says so plainly — is the
rule those passages follow.

## 2. When to write an architecture document

**Create a new architecture document** when a change alters the *structure* of
the system and the structure cannot be understood from the existing ones:

- a new module with its own responsibility and its own data;
- a new cross-cutting mechanism that every request passes through (a new
  middleware layer, a new extractor, a new transaction strategy);
- a new persistence technology, a new trust boundary, or a new way of
  delivering the process;
- a change to the layering rule (`handlers` → `service` → `repository` →
  PostgreSQL).

**Update an existing architecture document** in the same change as the code,
whenever a change alters something the document already describes: a new
middleware layer, a new endpoint group, a new table, a new index, a new
configuration variable, a changed invariant.

**Do not** put product scope into an architecture document. If a change needs a
new product behaviour, the requirement belongs in
[`../project-truth.md`](../project-truth.md) and the feature specification
first.

## 3. When an ADR is required

An architecture decision record is **required** when a decision:

- materially affects the structure, the security model, the compatibility
  promise or the long-term maintenance of the system;
- is expensive to reverse once it is in production;
- replaces or narrows an earlier accepted decision;
- constrains every future contributor (a layering rule, a forbidden pattern, a
  mandatory mechanism).

An ADR is **not required** for:

- adding an endpoint that fits the existing model;
- fixing a defect or correcting a mistake;
- adding a dependency — that is recorded with its reason in the change log;
- tuning a default value — the configuration reference is updated instead.

The process, the statuses and the template are described in
[`../decisions/README.md`](../decisions/README.md).

## 4. Desired design versus implemented design

This distinction matters more in an architecture document than anywhere else,
because a reader cannot run the code from the page.

- **Architecture documents describe what exists.** Every claim about behaviour
  must be checkable in `src/`, `migrations/` or `tests/`.
- When a section has to cover something that does not exist yet, it says so
  plainly and marks it as **not implemented**. A heading such as *Not
  implemented* is used for that, never a sentence in the future tense that
  could be mistaken for a description.
- The status words are the ones defined in [`../../AGENTS.md`](../../AGENTS.md)
  §6 and repeated in [`../README.md`](../README.md) §2: `Proposed`,
  `Accepted`, `In progress`, `Implemented`, `Verified`, `Deprecated`,
  `Blocked`. An architecture document is `Accepted` as a document; it never
  claims that a component is `Verified` on the strength of code existing.
- The evidence for each component's state lives in
  [`../engineering/implementation-status.md`](../engineering/implementation-status.md).
  This directory links to it rather than repeating it, so the two cannot drift
  silently.
- **A roadmap item is not architecture.** Anything that is only `Proposed`
  belongs in [`../roadmap.md`](../roadmap.md).

## 5. House rules

- **Relative links only.** Never an absolute path, never a line number.
- **Real names.** Every file path, type name, table name and endpoint in these
  documents exists in the repository. If a name is not in the source, it is not
  written here.
- **No secrets.** No real token, key, password or connection string appears in
  a documentation file, and no absolute machine path or personal name.
- **One fact, one home.** A rule that is stated in
  [`../api/error-contract.md`](../api/error-contract.md) is referenced from
  here, not restated.
- **The checker must pass.** `cargo run --bin atom-docs` verifies that the
  documents exist, that the relative links resolve and that the ADR
  identifiers are unique.

## 6. Related documents

- [`../project-truth.md`](../project-truth.md) — the agreed product
  requirements.
- [`../README.md`](../README.md) — the documentation index and the authority
  order.
- [`../decisions/README.md`](../decisions/README.md) — the accepted decisions.
- [`../engineering/known-issues.md`](../engineering/known-issues.md) — the
  confirmed limitations.
- [`../engineering/open-questions.md`](../engineering/open-questions.md) — the
  unresolved decisions.
- [`../../AGENTS.md`](../../AGENTS.md) — the repository conventions.
