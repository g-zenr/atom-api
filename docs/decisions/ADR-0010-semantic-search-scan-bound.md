# ADR-0010 — Semantic search keeps its corpus-bound scan, and the cost is accepted up to a named volume

> **Status:** Accepted
> **Date:** 2026-10-25
> **Owner:** architect

## Context

`ML-R05` asks for three search modes. Two of them are bounded by
configuration and one is not, and until the Phase 2 performance chain nobody had
measured the third.

**The statement.** `semantic` mode and the match path read their vectors through
`NOTE_VECTORS` (`src/ml/repository.rs:81`):

```sql
SELECT e.source_id, e.vector
FROM embeddings e
JOIN notes n ON n.id = e.source_id AND n.user_id = e.user_id
WHERE e.user_id = $1
  AND e.source_type = 'note'
  AND e.model_id = $2
  AND n.is_archived = FALSE
ORDER BY e.source_id
```

It has no `LIMIT`. The ranking is done in Rust by `rank`
(`src/ml/service.rs:561`), which sorts the whole result with a total order —
score, then note id — so **the SQL `ORDER BY` does not decide the response**.
`pagination.total` counts every scored row above the similarity floor, so the
statement cannot be narrowed to a page either: an exact `total` needs every
vector.

**The measurement** ([`../database/performance.md`](../database/performance.md)
§3.5, §4, §6), at `dim = 384` — the model's real dimension — over 20 000 rows
split between two owners, `work_mem = 4 MB`:

| Property | Measured value |
| -------- | -------------- |
| Plan | `Gather Merge` → `Sort` → `Hash Join` → **`Parallel Seq Scan on embeddings`** |
| Rows returned | 8 100 live note vectors for the caller |
| Buffers | 5 387–6 135 shared (the **whole 39 MB table**), 983–1 573 temp |
| Duration | **177–292 ms** |
| Sort | `external merge`, 7 864 kB to disk |
| Index chosen? | `embeddings_user_id_source_type_model_id_idx` at 3 000 note vectors per owner (bitmap), **not** at 4 000 |

Every figure is warm-cache, single-session, on one machine
([`performance.md`](../database/performance.md) §2.1 and §9). They are the right
order of magnitude and they are not a capacity plan.

Three facts follow, and each one matters to the decision:

1. **The plan is linear in the table, not in the owner's vectors.** The index is
   a 160 kB key list; the `vector` column is not in it, so an index scan would
   fetch 9 000 rows from a 39 MB heap at random. Above ~4 000 note vectors per
   owner the planner prefers one sequential pass over the whole table, including
   the other owner's rows. In a two-owner corpus those two quantities differ by
   a factor of two; in a many-owner deployment they diverge without bound.
   Nothing is *returned* to the wrong owner — the `user_id` predicate is in the
   statement and the control query in `performance.md` §5.2 returns 0 rows in
   2 buffers — but the work is table-sized.
2. **The first cliff is the sort, not the scan.** `ORDER BY e.source_id` covers
   8 100 rows of ~1.5 kB, about 12 MB against a 4 MB `work_mem`, so it spills.
   Across the spill point — between 2 500 and 3 000 vectors per owner — the same
   statement goes from **14–16 ms to 75–123 ms**. That is a 5× jump caused by a
   server setting, on a sort whose result is discarded.
3. **The memory the request holds is linear in the owner's vectors.** Every row
   is materialised in Rust: 384 `f32` is 1 536 bytes, so 8 100 vectors is
   ~12 MB per concurrent `semantic` request, and it grows by 1.5 MB per 1 000
   vectors.

**Why this is being decided now.** [`ADR-0008`](ADR-0008-local-pure-rust-ml.md)
§4 accepted that `semantic` scores every embedding the owner has, and its
"Superseded by" field says a review is due "when the corpus outgrows the bounded
re-rank". `performance.md` §7 finding 1 left the question open with two named
options — accept, or consider a covering index or a dedicated ordering column.
[`schema.md`](../database/schema.md) §11.6 had measured the same statement at
`dim = 2` as a 6–13 ms bitmap scan and its cost claim does not survive the real
dimension; that is a documentation defect corrected in the same change as this
ADR.

**What the product already offers.** `mode` defaults to `hybrid`, which is
bounded by `ml.candidate_limit` and that setting is itself capped at
`MAX_ML_CANDIDATE_LIMIT = 1 000`
([`ADR-0008`](ADR-0008-local-pure-rust-ml.md) §12.5). A client that needs a
predictable cost has a bounded path today, at the default configuration, without
anyone changing a thing. `semantic` is the mode a caller opts into when it wants
exactness over the whole corpus, and it says so in its own contract.

## Decision

**ATOM accepts the measured cost of `semantic` search: it remains an exact scan
of the caller's stored vectors, and `embeddings` gains no covering index, no
partial index, no two-stage identifier fetch and no narrower column type for it.
The acceptance is bounded by volume rather than by configuration, and it ends
when the `embeddings` table passes 100 000 rows or any single owner passes
50 000 note vectors — whichever comes first.**

Four things are part of the decision, not commentary on it:

1. **The cost is stated as `O(rows in embeddings)`, not `O(the owner's
   vectors)`.** At the measured dimension and volume, the plan reads the table.
   The per-owner figure is a *memory* bound, not a time bound. A document that
   says "linear in the caller's vectors" is wrong above 4 000 vectors per owner
   and must not be written that way.
2. **`hybrid` stays the default and stays the bounded path.** The bounded mode
   is what makes accepting the unbounded one defensible: the product is not
   asking a client to choose between a slow search and no search.
3. **The review point and the stop line are different numbers.** *Review* at
   50 000 rows in the table or 25 000 note vectors for one owner — ~0.44–0.73 s,
   ~38 MB per concurrent request — where the projection below is checked against
   a fresh measurement. *Stop* at 100 000 rows or 50 000 vectors for one owner —
   ~0.9–1.5 s, ~77 MB per concurrent request, and a single query reading ~200 MB
   of heap. Projection method: the measured 177–292 ms at 20 000 rows, scaled
   linearly in rows, which is what the plan shape implies. These are projections
   from one measured point, not measurements, which is why the first of them is
   a check and not a decision point.
4. **The `work_mem` cliff is an operational knob, not a schema problem.** It is
   recorded in [`../architecture/deployment.md`](../architecture/deployment.md)
   §7.1 and it is the first thing an operator should change if `semantic`
   latency is unacceptable — before anyone touches an index.

**When a volume line is crossed, this ADR is superseded by one that changes the
path.** The order of preference, decided here so that the future change does not
start from zero:

1. **Remove the `ORDER BY e.source_id` from `NOTE_VECTORS`.** It buys nothing —
   `rank` re-sorts with a total order — and it is the cause of the external
   merge and of the `work_mem` cliff. The order of the saving is the measured
   difference across the spill point: the statement goes from 14–16 ms to
   75–123 ms there, and the spill is what changed. It must be measured as a
   whole statement rather than assumed, because the scan itself is unchanged.
   This is a code change in `src/ml/repository.rs`, it does **not** change the
   plan shape (the scan stays sequential), and it is therefore **not authorised
   by this ADR**: it is a separate change, and `ORDER BY source_id` in
   `VECTORS_FOR_SOURCES` (`src/ml/repository.rs:99`, bounded at
   `candidate_limit` rows) and in `CURRENT_SOURCES` (`:110`, 9 500 rows per
   reindex) has the same redundancy and is part of the same small change.
2. **If the corpus still outgrows the lines, an ADR for one of two changes**,
   chosen by measurement rather than by preference: a redefinition of
   `semantic` as a bounded mode (a public contract change), or a real vector
   index — which means `halfvec`/pgvector and reopens
   [`ADR-0008`](ADR-0008-local-pure-rust-ml.md)'s extension-privilege finding,
   or an in-process ANN index, which `ADR-0008` names as its natural next step.

## Alternatives considered

| Alternative | Why it was not chosen |
| ----------- | --------------------- |
| **Accept and describe the cost as `O(the owner's vectors)`** | It is the description the code reads as, and the measurement contradicts it. Above ~4 000 note vectors per owner the plan scans the table, so the cost also grows with other owners' rows. Accepting a bound and mis-stating it is worse than accepting it accurately: the first deployment to add accounts discovers the difference. |
| **Covering index: `(user_id, source_type, model_id) INCLUDE (source_id, vector)`** | It converts a 39 MB heap scan into a 39 MB index-only scan over the same bytes, for the same buffer count, and doubles the write cost of every upsert (each row's 1.5 kB vector maintained in two places). It does not change the asymptote, and it inherits a hard limit: a btree tuple must fit in ~2 704 bytes, which a 384-dim `REAL[]` only just survives — a 768-dim model would not, so a routine model change would silently require a different design. |
| **Two-stage id-then-fetch: select `source_id` from the narrow index, then fetch vectors by id** | It is the right shape only if the second stage is bounded. `semantic` needs every row for an exact `total` and an exact top-K, so the second stage fetches all 8 100 anyway — two round trips and the same heap traffic. It would help `hybrid`, which does not need help. It becomes right only together with a contract change that makes `semantic` approximate or page-bounded, which is item 2 of the "when a line is crossed" list. |
| **Partial index, `(user_id, model_id) WHERE source_type = 'note'`** | It narrows the key list, not the heap, and the heap is what costs 5 000 buffers. The 160 kB key list is already cheap; the planner declines it because of the fetch, not the size. |
| **`halfvec`** | Halves the heap to ~20 MB and would let a real index exist. It requires the `vector` extension, which `ADR-0008` rejected because the `atom` role cannot create it on the development machine while CI's can — a migration that passes CI and fails locally. It also changes the column type, the `dim = cardinality(vector)` check and the write path, for a latency no one has reported. |
| **Store the vector as `BYTEA` instead of `REAL[]`** | Saves the array header and the per-element datum, roughly 2 % of 1.5 kB, and costs the `CHECK (dim = cardinality(vector))` that makes the row self-describing. A rounding error against a 5× cliff. |
| **Reduce `ml.dimension` or truncate the vector** | Changes what the vector means, which `ADR-0008` §9 forbids without a new `model_id`, and degrades ranking quality to buy back milliseconds. |
| **Make `semantic` bounded by `ml.candidate_limit`, i.e. make it `hybrid`** | It removes the reason the mode exists. `semantic` is the only exact mode — it finds a note that shares no lexical term with the query — and it is the mode that makes the hybrid limitation tolerable. Collapsing the two is a product change, not an optimisation, and it belongs in a specification change. |
| **An in-process HNSW index over the owner's vectors** | The correct long-term answer and `ADR-0008`'s named next step. It is an index to build, persist, invalidate on every write and rebuild after a model change — for a corpus whose measured cost is 177–292 ms and which already has a bounded alternative. Buying it now is the storage-and-complexity trade the measurement does not justify. |
| **An application-level cache of the ranked result** | It hides the cost on the happy path and leaves it on the first request after every write, while adding an invalidation problem across the reindex, the delete path and `model_id` changes. A cache whose invalidation is harder than the thing it caches is not a simplification. |

## Consequences and trade-offs

### Positive

- **No schema change, no migration, no new dependency.** The decision costs a
  documentation correction and two operational notes, and it does not put a
  39 MB index in the write path of every reindex.
- **The bound is written down with a number.** `performance.md` §7 finding 1
  asked for "accept while corpora stay small" without saying small; this ADR
  says 100 000 rows in the table, 50 000 vectors for one owner, and gives the
  projection.
- **The bounded path already ships.** `hybrid` is the default mode and is
  capped by `ml.candidate_limit`; no client is forced onto the unbounded path.
- **The stated cost is now true.** The `O(owner's vectors)` description is
  replaced by `O(rows in embeddings)` for time and `O(owner's vectors)` for
  memory, which is what the measurement shows.
- **`ADR-0005` is unaffected.** Reading a row and returning it remain different
  properties; the owner predicate is in the statement, not in the handler.

### Negative

- **Latency grows with the table, so the acceptance is conditional on the
  deployment shape.** [`project-truth.md`](../project-truth.md) fixes a
  single-owner personal workspace, where the table and one owner's vectors are
  nearly the same number and 20 000 rows is a large corpus. A deployment with
  many accounts reaches the table line while every individual corpus is small,
  and the statement degrades for all of them at once. That is the honest limit
  of this acceptance, and it is why the trigger is stated in table rows as well
  as in owner vectors.
- **The degradation is silent.** Nothing fails when the line is crossed; the
  request just takes longer, and the API exports no metrics
  ([`deployment.md`](../architecture/deployment.md) §8), so the only way to
  notice is to measure. This ADR therefore names a corpus milestone, not an
  alarm.
- **Two items of work follow, and neither is a defect.** (1) The `work_mem`
  operational note, written in `deployment.md` §7.1. (2) A re-measurement at the
  review point — 50 000 rows in the table or 25 000 note vectors for one owner —
  where the projection in this ADR is checked against reality before the stop
  line is relied on. Both are recorded here rather than in
  [`../engineering/known-issues.md`](../engineering/known-issues.md): nothing is
  broken and the trigger is a volume milestone, not a defect. If the review is
  deferred, that file is where a standing entry belongs.
- **Exactness is now a documented cost as well as a documented benefit.**
  `semantic` is the only mode that can find a note the lexical query missed, and
  it is also the only mode whose cost is not bounded by configuration. Future
  contributors will be tempted to "fix" it with an index; the alternatives table
  exists to make that expensive to do without reading this first.

## Security and operational implications

- **The scan reads other owners' rows inside the executor.** That is a
  performance property, not an access-control one: the `user_id` predicate is in
  the statement (`NOTE_VECTORS`), the join repeats it
  (`n.user_id = e.user_id`), and the control query in `performance.md` §5.2
  returns 0 rows in 2 buffers. No log line, error or response exposes a vector,
  and a foreign row is never returned. Recording this explicitly is the point:
  a future reader who sees `Rows Removed by Filter: 3667` must not conclude
  that ownership is enforced somewhere outside SQL.
- **A slow statement holds a database connection.** `semantic` occupies one
  pooled connection for 177–292 ms, against a pool of 10 by default and 20 in
  the production overlay. At the measured cost that is a rounding error at
  personal scale; it is the reason not to raise this mode's concurrency
  independently of the pool size.
- **`work_mem` is the operator's knob, and it is per role, not global.** Raising
  it server-wide multiplies memory by every sort on every connection; the
  correct scope is the API's role or database
  (`ALTER ROLE … SET work_mem = '16MB'`). The value is a property of the
  database and the host, not of the API's configuration: there is deliberately
  no `database.work_mem` setting, because a per-transaction `SET` issued by the
  API would be a planner hint wearing a configuration key, and it would make
  latency depend on connection-pool bookkeeping.
- **`work_mem` must be recorded with any benchmark.** A latency number quoted
  without it is not reproducible across the spill point, where the same
  statement differs by 5×. `performance.md` §2.1 records it for exactly this
  reason.
- **No new secret, no new trust boundary, no new outbound call.** The decision
  removes work rather than adding it.
- **A change that crosses the lines is a decision, not a tuning exercise.**
  Adding an index to `embeddings` later means a migration, a measured plan, and
  a superseding ADR; the alternatives table above is what the next author has to
  beat.

## Related requirements and changes

| Reference | Relationship |
| --------- | ------------ |
| [`../project-truth.md`](../project-truth.md) §18.3 `ML-R05` | The three search modes this decision bounds; `semantic` stays exact and corpus-bound. |
| [`ADR-0008-local-pure-rust-ml.md`](ADR-0008-local-pure-rust-ml.md) §4, §12 | The decision this ADR extends: `REAL[]` instead of pgvector, `hybrid` bounded by `ml.candidate_limit`, `semantic` bounded by the corpus. |
| [`ADR-0005-ownership-enforced-in-sql.md`](ADR-0005-ownership-enforced-in-sql.md) | The ownership predicate this scan still carries while reading rows it does not return. |
| [`../database/performance.md`](../database/performance.md) §3.5, §4, §6, §7 | The measurement, the plan, the thresholds, and finding 1, which this ADR resolves. |
| [`../database/schema.md`](../database/schema.md) §11.6 | The `embeddings` access paths; corrected in the same change where it generalised a `dim = 2` figure. |
| [`../features/ml/architecture.md`](../features/ml/architecture.md) §4.3, §7, §11 | The feature design, whose "milliseconds" claim about §4.3 and whose limitations list now carry the measured cost. |
| [`../architecture/deployment.md`](../architecture/deployment.md) §7, §7.1 | The `work_mem` cliff as an operational obligation. |
| [`../database/migration-policy.md`](../database/migration-policy.md) | How the migration would be written if a later ADR chooses an index. |

## Supersedes / Superseded by

Supersedes: none. This ADR answers the review trigger
[`ADR-0008`](ADR-0008-local-pure-rust-ml.md) records in its *Superseded by*
field — "a review of this decision is due when the corpus outgrows the bounded
re-rank" — and leaves that decision in force; it does not replace it.

Superseded by: none. A review is due when the `embeddings` table reaches
100 000 rows, when one owner reaches 50 000 note vectors, when the deployment
stops being a single-owner personal workspace, when `ml.dimension` changes, or
when a client reports that `semantic` search is too slow. The first move at that
point is the `ORDER BY` removal in the *Decision* section, measured; the second
is a new ADR for a real vector index or for a narrower `semantic` contract.
