---
name: Bug report
about: Something behaves differently from what the project documents promise
title: ""
labels: ["bug"]
assignees: ""
---

## What happened

A clear description of the actual behaviour.

## What you expected

What the documentation says should happen. If it is a product requirement,
link it — for example [`docs/project-truth.md`](../docs/project-truth.md) §9.

## How to reproduce

1.
2.
3.

Include the request and the response where possible. **Remove every token,
password and secret before pasting.** A redacted `Authorization: Bearer <redacted>`
is enough.

## Where

- Endpoint or module:
- Commit (`git rev-parse HEAD`):
- Environment: local / DEV / UAT / PROD

## Evidence

Error output, logs or test output. Again: sanitize secrets and personal data.

## Which side is wrong?

If the audit trail is relevant, note whether this looks like a **code** defect
or a **documentation** defect — they are fixed in the same pull request, but
knowing which is which helps. See
[`docs/decisions/ADR-0004-documentation-as-project-truth.md`](../docs/decisions/ADR-0004-documentation-as-project-truth.md).
