# ADR-0009 — A receipt is a reference, not an uploaded file

> **Status:** Accepted
> **Date:** 2026-10-24
> **Owner:** architect

## Context

A reimbursement claim is evidence of money the owner has already spent. Phase 2
has to let the owner attach the proof — the receipt — to the expense. The
requirement is `REIMB-R08` in
[`../project-truth.md`](../project-truth.md) §18.1, and the assumption behind
it is **A2**: "receipts are a URL/reference in Phase 2, not uploaded files"
([`../engineering/runs/chain/plan.md`](../engineering/runs/chain/plan.md) §3).
A2 is a **provisional default**; the owner did not explicitly confirm it
(owner decision O1 in
[`../engineering/runs/chain/ledger.md`](../engineering/runs/chain/ledger.md)).

An upload looks like a small feature and is not one. Accepting a file means
the API becomes a **store** as well as a **record**, and the obligations that
follow are not in the current system at all:

| What an upload needs | Why |
| -------------------- | --- |
| A storage layer | Local disk or an object store. Local disk makes the process stateful, so a restart or a second instance loses or cannot see the file; it also needs a backup and a quota. |
| Multipart or binary body handling | The API accepts JSON only, and the body limit is `APP_MAX_BODY_BYTES` (1 MiB by default) — [`../api/conventions.md`](../api/conventions.md) §9. A receipt photo is often larger. |
| Size and count limits | Otherwise one client fills the disk. The limits have to be enforced before the bytes are buffered, not after. |
| A content-type allowlist and a magic-byte check | The `Content-Type` header is client-controlled. A file served back later must not be able to be a script. |
| Serving the file back | It needs its own authenticated route, an owner check, a content disposition and a cache policy. A public URL would break the ownership rule in `AGENTS.md` §4.3. |
| Malware scanning | Untrusted bytes are stored and later served. A scanner is either a dependency, a subprocess or a hosted service; the last two collide with the Rust-only and no-hosted-service constraints. |
| Retention, erasure and backup | "Delete my account" would have to delete the files too, and a backup would have to honour the same deadline. |
| Disk-full and corruption behaviour | A full disk is a new failure mode that turns an unrelated request into a `500`. |

None of that is impossible, but all of it is a phase of its own, and none of it
is needed to answer the question the feature exists to answer: *which expense
does this receipt belong to?* A URL answers that.

The API also has no storage layer, no object-store client and no file-serving
route today. Adding one in the same phase as three new domain features would
put a new trust boundary and a new operational dependency inside the change
that is supposed to be about claims.

## Decision

**In Phase 2 a receipt is an optional URL on a line item, supplied by the
client and stored as text. The API never accepts a file body, never stores the
bytes, and never fetches the URL.**

### 1. The field

- `receipt_url` is an optional field on a **line item**, not on the claim. A
  claim has several expenses, and a receipt proves one expense; a single
  claim-level reference could not say which item it proves.
- It is at most **2048 characters**, and it must be an **absolute `http` or
  `https` URL**. A `data:`, `file:`, `javascript:` or relative value is a `422`
  naming the field.
- The value is returned verbatim in the item response. The API does not
  normalise it beyond trimming surrounding whitespace.
- It is a **pointer to the owner's own evidence**, not a value the server can
  verify. Nothing checks that the target exists.

### 2. What the API does not do

- It does not accept `multipart/form-data`, a binary body or a base64 blob.
  A body with an upload is a `400` or a `413`, never a silently ignored part.
- It does not download, preview, thumbnail or scan the target.
- It does not log the URL. A receipt URL can carry a signed query string that
  is itself a credential; it is treated as sensitive owner data.

### 3. What replaces an upload later

If the owner wants files, the change is a new ADR that supersedes this one,
and it must name the storage, the size limit, the content check, the scanning
story, the serving route, the retention rule and the deletion cascade. The
`receipt_url` field can coexist with it: a stored file would be served at a
URL, so the record keeps the same shape.

## Alternatives considered

| Alternative | Why it was not chosen |
| ----------- | --------------------- |
| **Multipart upload to local disk** | The simplest upload, and it needs no service. It was not chosen because it makes the API stateful: a second instance cannot see the file, a container restart loses it, and the backup and quota obligations are exactly the ones the project has not designed. It also needs the content checks and the serving route, which is most of the work an object store would do. |
| **An S3-compatible object store** | The right answer at scale: durable, cheap, and it moves the serving problem off the API. It was not chosen because it is a hosted or paid dependency (excluded by [`../project-truth.md`](../project-truth.md) §8), it adds credentials and a new trust boundary, and it makes every developer and every test need the store. |
| **Base64 in the JSON body** | No new storage layer, and it reuses the existing extractor. It was not chosen because base64 inflates the payload by a third, the 1 MiB body limit makes a normal photo impossible, the bytes land in the `claims` row and every backup, and the database is then the blob store. |
| **A separate `claim_receipts` table with many references per item** | It would support a receipt that covers several expenses, or several photos of one receipt. It was not chosen because no requirement asks for it, and one optional URL per item is enough to answer "which expense is this". A later change can add the table without breaking the field. |
| **No receipt field at all** | Zero risk and zero work. It was not chosen because the feature would lose its link to the evidence, which is the point of a reimbursement tool. |
| **Accept any string as the reference, unvalidated** | It would support a paper receipt in a drawer ("folder 3, envelope 2"). It was not chosen because an unvalidated free-text field that is rendered as a link is a stored-XSS vector in the future frontend, and because the scheme check is cheap. A non-URL reference can be added later as a second, clearly named field. |
| **Let the client upload to a third party and send the link anyway** | That is what happens in practice with a URL, and it is compatible with this decision: the owner may store the file wherever they like. The API's choice is not to be that place. |

## Consequences and trade-offs

### Positive

- **No new trust boundary.** No untrusted bytes are stored, served or scanned
  by the API.
- **The process stays stateless.** A restart, a second instance or a container
  replacement loses nothing.
- **No storage cost, no backup change, no retention problem.** Deleting the
  claim deletes the reference and nothing else.
- **The body limit is unchanged.** A receipt does not need a larger
  `APP_MAX_BODY_BYTES`, so the denial-of-service surface of
  [`../security/threat-model.md`](../security/threat-model.md) T-09 does not
  grow.
- **The field survives a later upload feature.** A stored file is served at a
  URL, so the shape does not have to change.

### Negative

- **Link rot.** The target can be deleted, moved or made private, and the API
  cannot tell. A claim can point at nothing.
- **No proof of purchase.** The server holds a claim and a link, not the
  evidence. If the owner's storage is lost, the record is not evidence.
- **The URL may carry a credential.** A signed link is a bearer credential for
  that file; storing it in the database and returning it to the owner is a
  disclosure surface the API cannot control. This is why it is never logged.
- **The URL is not verified.** A typo produces a `404` only when the owner
  clicks it, not when it is saved.
- **One receipt per item.** An item with two pages of a receipt has to
  concatenate them or pick one. A future `claim_receipts` table fixes it.
- **A client could store something that is not a receipt.** Nothing stops a
  `https://example.test/not-a-receipt` value. The check is syntactic on
  purpose; content validation would need the bytes.

## Security and operational implications

- **Server-side request forgery is avoided by construction.** The API never
  fetches the URL, so a URL pointing at `http://127.0.0.1:5432` or at a
  cloud metadata endpoint is inert. This is the main reason the rule "never
  fetch the target" is part of the decision rather than an implementation
  detail.
- **Stored XSS is prevented at the scheme.** Only `http` and `https` are
  accepted, so a `javascript:` URL cannot be stored and later rendered as a
  link by a future frontend.
- **The value is owner data and is treated as sensitive.** It is scoped by
  `user_id` like every other column, it is never written to a log line, and it
  is never returned to another account (a foreign item is `404`).
- **No new configuration and no new dependency.** There is no storage path, no
  bucket, no credential and no scanner to operate.
- **The operator's backup already covers it**, because it is a column in a
  table that is already backed up.
- **The failure mode is a broken link, not a failed request.** That is the
  right failure for an optional reference.

## Related requirements and changes

| Reference | Relationship |
| --------- | ------------ |
| [`../project-truth.md`](../project-truth.md) §18.1 `REIMB-R01`, `REIMB-R08` | The requirements this decision serves. |
| [`../project-truth.md`](../project-truth.md) §18.4 | Records A2 as a provisional default. |
| [`../features/reimbursements/api.md`](../features/reimbursements/api.md) | The item contract that carries `receipt_url`. |
| [`../features/reimbursements/architecture.md`](../features/reimbursements/architecture.md) | Where the value is validated and why it is never fetched. |
| [`../api/conventions.md`](../api/conventions.md) §9 | The body limit that an upload would have had to change. |
| [`../security/threat-model.md`](../security/threat-model.md) | The trust boundaries and T-09 that an upload would have widened. |
| [`ADR-0005-ownership-enforced-in-sql.md`](ADR-0005-ownership-enforced-in-sql.md) | The ownership rule a file-serving route would have had to honour. |
| [`../engineering/change-log.md`](../engineering/change-log.md) | The change that implements it. |

## Supersedes / Superseded by

Supersedes: none. No earlier decision covered receipts.

Superseded by: none. A review of this decision is due when the owner asks for
stored files, or when provisional default A2 is revised.
