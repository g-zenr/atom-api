# ADR-0004 — The documentation tree is part of the product

> **Status:** Superseded
> **Date:** 2026-10-21
> **Owner:** architect

## Context

ATOM is intended to be usable and contributable by people who have never had
access to a private conversation with the maintainer. That is an explicit
Phase 1 requirement: "documentation good enough for an open-source
contributor to use the project"
([`../project-truth.md`](../project-truth.md) §3).

The project is also built by a team of agents that hand work to each other.
Every task boundary is a place where context can be lost: a subagent that
does not read the requirements can implement the wrong behaviour, and a
maintainer who cannot see the requirements cannot tell whether a change is
correct. The requirements themselves demand that documentation and code move
together: [`../../AGENTS.md`](../../AGENTS.md) §5 makes a documentation
update part of the definition of a finished change, and §4.10 makes a
changed public contract a documentation obligation.

The risk of "documentation as a deliverable" is that it becomes a parallel
work of prose that drifts from the code. A separate wiki drifts. A pile of
comments drifts from the architecture, because a comment cannot hold a
decision, a rationale or a status. Generating the documentation from the code
drifts in the other direction: it can describe the code but not the
*requirements*, the *rationale* or the *status*, and a generated document
cannot be reviewed as a decision.

What the project needs is a small, ordered set of documents, each with a
defined authority over a defined question, and a status vocabulary that says
plainly whether something is agreed, built or merely proposed.

## Decision

**The documentation tree under `docs/` is a deliverable of the product, not
an artefact of it. It has an authority order, a status vocabulary, a change
protocol and a machine check.**

### 1. The tree

```text
docs/
├── README.md              the index and the authority order
├── project-truth.md       the canonical product requirements
├── roadmap.md             future features, all `Proposed`
├── architecture/          the current design
├── decisions/             the accepted decisions (ADRs)
├── features/              per-feature requirements, contracts and status
├── api/                   the public HTTP contract
├── database/              the schema and the migration policy
├── security/              the threat model and the secure-development rules
├── testing/               the strategy and the verification policy
├── engineering/           the historical record
└── templates/             the document templates
```

### 2. The authority order

When two sources disagree, the higher one wins for the question it is
authoritative about:

| # | Source | Authoritative for |
| - | ------ | ------------------ |
| 1 | `project-truth.md` and the accepted ADRs | The agreed product requirements and the accepted technical decisions. |
| 2 | `architecture/`, `api/`, `database/`, `security/` | The current design, the public contract, the schema and the security model. |
| 3 | `features/` | Per-feature requirements, contracts and status. |
| 4 | `src/`, `migrations/`, `tests/` | What the system actually does right now. |
| 5 | `engineering/` | The historical record. |
| 6 | `roadmap.md` and anything marked `Proposed` | Nothing, until it is accepted. |

Three consequences follow, and they are enforced by convention throughout
the tree:

- **A requirement does not prove an implementation.** `project-truth.md`
  saying that the notes feature is `Accepted` says nothing about whether the
  code exists.
- **The code does not change the requirements.** A disagreement is a
  discrepancy to record, not a licence to rewrite the requirements.
- **The historical record is not the specification.**

### 3. The status vocabulary

Every requirement and every feature carries exactly one of seven statuses,
defined identically in [`../../AGENTS.md`](../../AGENTS.md) §6 and
[`../README.md`](../README.md) §2: `Proposed`, `Accepted`, `In progress`,
`Implemented`, `Verified`, `Deprecated`, `Blocked`.

`Implemented` means "the code exists and compiles". `Verified` means
"implemented **and** covered by a test or a recorded check that passes".
A feature is never `Verified` because its code exists.

### 4. The change protocol

A substantial change follows a fixed protocol, defined in
[`../../AGENTS.md`](../../AGENTS.md) §5 and in the `atom-docs` skill:

**Before** the work: read the repository instructions, the index, the
requirements, the relevant feature specification, the related architecture
documents and ADRs, the known issues and the open questions, and the real
source. If the task conflicts with an accepted requirement or an unresolved
decision, stop and say so.

**After** the work, in the same change: update the feature status, the
architecture documents, the ADR (if a decision was taken), the change log,
the review log, the implementation status, the known issues, the OpenAPI
annotations and `docs/api/` (if the contract changed), and verify that the
documentation still matches the code.

**A code change is not complete until its documentation is updated, or the
author has explicitly explained why no documentation change was necessary.**

### 5. The machine check

`cargo run --bin atom-docs` is a checker that ships with the crate. It
verifies that:

1. every required document exists;
2. every relative Markdown link resolves to a file that exists;
3. every feature status uses one of the seven defined words;
4. every ADR has a status and a unique identifier;
5. every change-log identifier is unique;
6. every feature directory has its five required documents.

`scripts/check-docs.sh` and `scripts/check-docs.ps1` run the same check.
The checker cannot tell whether a sentence is *true*; that is the job of the
code review and of the quality gate.

## Alternatives considered

| Alternative | Why it was not chosen |
| ----------- | --------------------- |
| **A wiki only** — the requirements and the design live in a hosted wiki, the repository holds only the code. | The wiki and the code drift the moment they are edited separately, and the wiki is not reviewable in a pull request. It also puts the project's most important artefact behind an account. The project must be usable from a clone. |
| **Comments only** — the code carries its own documentation, and there is no separate tree. | A comment can explain *what* a line does. It cannot hold a decision and its rationale, an authority order, a status, or a link to the requirement it serves. The decisions would be lost, and the requirements would have no home. |
| **Documentation generated from the code only** — the OpenAPI document, the schema and the module reference are all generated. | Generation covers the parts that describe the code. It cannot produce the product requirements, the rationale for a decision, the security model or the honest statement that something is *not* implemented. Generated documents are also not reviewable prose, and a generator that is wrong is harder to notice than a sentence that is wrong. |
| **Generated API reference plus hand-written everything else, with no authority order** | The tree is needed, but without an explicit order the disagreements between documents are resolved by whoever is loudest. The order is the part that makes the tree usable by a contributor who has never read the project. |
| **A single, very large document** | It is impossible to review, impossible to keep current and impossible to hand to a subagent. The split into product, architecture, decisions, features, contracts, security, testing and record matches the way the work is actually divided. |

## Consequences and trade-offs

### Positive

- **A contributor can work correctly from a clone.** The requirements, the
  decisions and the current design are all in the repository.
- **An agent can pick up where the previous one stopped.** The status
  vocabulary, the evidence rules and the change protocol tell a reader
  exactly what is settled and what is not.
- **Disagreements have a procedure.** "The code and the requirements
  disagree" has a defined outcome — record the discrepancy — instead of
  becoming a silent rewrite.
- **The history is preserved.** The ADRs are never rewritten, and the change
  log and the review log are append-only, so the reasoning behind the system
  is recoverable.
- **The check is mechanical.** The broken-link and status problems are caught
  by a command rather than by a reader.
- **The public contract has one home.** The OpenAPI document is generated
  from the handlers, and `docs/api/` describes the conventions around it.

### Negative

- **It is a lot of writing, and it is a real cost.** Every change carries a
  documentation obligation, and the obligation is sometimes larger than the
  change. This is the price of the decision, paid deliberately.
- **The documents can be wrong.** The checker verifies existence, links and
  vocabulary; it cannot verify truth. A confident, incorrect sentence is
  worse than a missing one, and only review catches it.
- **The vocabulary is easy to misuse.** "Implemented" and "Verified" are
  adjacent and are confused more often than any other pair in the tree. The
  evidence column in
  [`../engineering/implementation-status.md`](../engineering/implementation-status.md)
  exists to make the difference visible.
- **The authority order is a convention, not a mechanism.** Nothing stops a
  rank-4 document from contradicting a rank-1 requirement; the order only
  says which one wins.
- **Two documentation defects are known and open** — a broken relative link
  in `project-truth.md` and a missing contributor document. They are recorded
  as KI-013 in
  [`../engineering/known-issues.md`](../engineering/known-issues.md).
- **The tree grows faster than the code.** Every new feature needs five
  documents. That is the point, but it is a cost.

## Security and operational implications

- **The threat model and the secure-development rules are part of the
  product.** A change that weakens a control has to change the security
  documentation in the same pull request, which makes the weakening visible
  in review.
- **The rules are checkable.** The ownership predicate, the parameterised
  SQL, the absence of `unwrap` in a request path and the CORS allowlist are
  all greppable, and [`../security/secure-development.md`](../security/secure-development.md)
  §2 says how.
- **The absence of a secret in the documentation is a rule.** `.env`,
  key material and credentials never enter the tree, and `.env.example`
  holds placeholders only.
- **The published configuration reference is the operator's contract.**
  [`../architecture/deployment.md`](../architecture/deployment.md) §1 is the
  single list of the environment variables, so a secret that is not in that
  table does not exist.
- **The historical record is a security record.** The change log and the
  review log record what was changed and what was found, which is what an
  incident investigation needs.

## Related requirements and changes

| Reference | Relationship |
| --------- | ------------ |
| [`../project-truth.md`](../project-truth.md) §3 | The documentation requirement this decision implements. |
| [`../project-truth.md`](../project-truth.md) §13 | The quality gates, of which the documentation check is one. |
| [`../../AGENTS.md`](../../AGENTS.md) §5 | The mandatory documentation protocol. |
| [`../../AGENTS.md`](../../AGENTS.md) §6 | The status vocabulary. |
| [`../README.md`](../README.md) | The index and the authority order this decision defines. |
| [`../architecture/README.md`](../architecture/README.md) | The rules for the architecture documents. |
| [`../decisions/README.md`](../decisions/README.md) | The ADR process this decision defines. |
| [`../templates/adr.md`](../templates/adr.md) | The template the ADRs follow. |
| [`../engineering/known-issues.md`](../engineering/known-issues.md) KI-013 | The two known documentation defects. |

## Supersedes / Superseded by

Supersedes: none. This is the first decision about the documentation
process.

Superseded by: ADR-0006, in the narrow sense that the claim above — that the
published configuration reference is the single list of the *environment
variables* — no longer describes the system, which now resolves its settings
from three layers. The rest of this decision stands. A review of this
decision is due when the project adopts a second, non-git artefact — a hosted
wiki or a generated site — because that would break the "usable from a clone"
property.
