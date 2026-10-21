# ADR-NNNN: `<decision in one sentence>`

> **Status:** `<Proposed | Accepted | Superseded | Deprecated>`
> **Date:** `<YYYY-MM-DD>`
> **Owner:** `<role>`

An architecture decision record. It records **one** decision, the options that
were considered, and the consequences that follow. It is never rewritten: a
decision that is replaced gets a new ADR, and this one is marked
`Superseded`.

The filename is the identifier plus a short slug, for example
`ADR-0006-short-slug.md`. An identifier is never reused and never deleted.

The section list below is the standard one. Keep the headings and the order.

---

## Context

*The situation that forces a decision. What is true today, what constrains the
options, and what the cost of doing nothing is. Quote the requirement, the
ADR or the known issue that raises the question. Facts with file references,
not adjectives.*

## Decision

*The decision, in the present tense and in the active voice: "ATOM stores …",
"Ownership is enforced in …". State it so that a reader can tell, without
interpretation, what the system does because of it. A decision with two halves
belongs in two ADRs.*

## Alternatives considered

*Every serious alternative, including the one that was rejected and the reason
for the rejection. An alternative that is not mentioned here was probably not
seriously considered; an alternative that is mentioned without a reason was
probably not seriously rejected.*

| Alternative | Why it was not chosen |
| ----------- | --------------------- |

## Consequences and trade-offs

*What the decision makes easier and what it makes harder. The costs are the
interesting half: name them, and name the work they create. A decision with no
downside has not been examined.*

### Positive

*What improves.*

### Negative

*What gets worse, and what has to be done about it. Every negative consequence
that needs work becomes an entry in
[`../engineering/known-issues.md`](../engineering/known-issues.md) or a task in
the change log.*

## Security and operational implications

*What the decision means for the security boundary and for running the system:
the trust boundaries it touches, the controls it adds or removes, the failure
modes it introduces, the secret handling it implies, the performance or
capacity effect, and what an operator has to do differently. If the decision
has no security or operational effect, say so and say why.*

## Related requirements and changes

*The requirements this decision serves, the ADRs it builds on, and the changes
that implement it. Every link resolves.*

| Reference | Relationship |
| --------- | ------------ |
| [`../project-truth.md`](../project-truth.md) §4.x | The requirement this serves. |
| `ADR-NNNN-short-slug.md` | The decision this builds on. Replace with the real file name. |
| [`../engineering/change-log.md`](../engineering/change-log.md) | The change that implements it. |

## Supersedes / Superseded by

*`Supersedes:` ADR-NNNN, if this decision replaces an earlier one.*
*`Superseded by:` ADR-NNNN, if a later decision replaces this one.*

*If this decision has never been replaced, write: `Superseded by: none. A
review of this decision is due when the requirement it serves changes.`*
