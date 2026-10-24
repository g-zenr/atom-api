# ATOM database — Phase 2 query performance

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-25
> **Owner:** maintained by the `database` agent

This document records what the Phase 2 list, search and aggregate statements
cost at realistic volume, with the plans that were actually produced. It exists
because [`schema.md`](schema.md) §11.6 measured the `embeddings` access paths at
`dim = 2` and said, honestly, that "the buffer and row counts are the finding,
not the byte size of the array". This is the measurement that completes that
qualification, and it covers the reimbursements and job-application statements
that had never been measured at all.

**This chain measures. It does not optimise.** No index, migration or statement
was changed to produce any number here, and none should be changed on the
strength of this document alone: §7 lists what a reviewer should decide.

---

## 1. Scope

The statements are the ones the API issues, taken from the repository
constants, not paraphrased. Every one is owner-scoped in its own `WHERE`
clause.

| # | Repository constant | File |
| - | ------------------- | ---- |
| 1 | `PAGE_WITHOUT_STATUS` | `src/reimbursements/repository.rs` |
| 2 | `PAGE_WITH_STATUS` | `src/reimbursements/repository.rs` |
| 3 | `COUNT_WITHOUT_STATUS` | `src/reimbursements/repository.rs` |
| 4 | `COUNT_WITH_STATUS` | `src/reimbursements/repository.rs` |
| 5 | `RECOMPUTE_TOTAL` | `src/reimbursements/repository.rs` |
| 6 | `PAGE_COMPANIES` | `src/job_applications/repository.rs` |
| 7 | `COUNT_COMPANIES` | `src/job_applications/repository.rs` |
| 8 | `PAGE_APPLICATIONS` | `src/job_applications/repository.rs` |
| 9 | `COUNT_APPLICATIONS` | `src/job_applications/repository.rs` |
| 10 | `PAGE_EVENTS` | `src/job_applications/repository.rs` |
| 11 | `COUNT_EVENTS`, `EVENTS_EXISTS_WITH_TOTAL` | `src/job_applications/repository.rs` |
| 12 | `NOTE_VECTORS` (the semantic search) | `src/ml/repository.rs` |
| 13 | `CURRENT_SOURCES` | `src/ml/repository.rs` |

Not measured: the write paths other than the item `SUM`, the Phase 1
statements, and the hybrid re-rank in Rust (which is not SQL). §9 lists the
gaps.

---

## 2. Method

### 2.1 The database

A scratch database `atom_perf_d5h3` was created empty, and `0001`-`0006` were
applied in order. Because `sqlx-cli` is not installed on this machine the
migrations were applied with `psql -f` in filename order; they are plain SQL and
produce the same schema, and the schema was verified by counting the eleven
tables the migrations create. The database was dropped afterwards (§8).

`sqlx` prepares statements and binds parameters, so the API sees a **custom
plan** for the first executions and possibly a generic one later. The statements
below were therefore run two ways where it matters:

- with each `$n` replaced by its bound value, which is exactly what a custom
  plan sees; and
- through `PREPARE`/`EXECUTE`, to check whether the plan cache would ever hand
  the API the generic plan (§5.6).

Server and settings, recorded because they decide several of the plans:

| Setting | Value |
| ------- | ----- |
| `server_version` | PostgreSQL 18.1 (x86_64-windows) |
| `shared_buffers` | 128 MB |
| `work_mem` | 4 MB |
| `effective_cache_size` | 4 GB |
| `max_parallel_workers_per_gather` | 2 |
| `random_page_cost` / `seq_page_cost` | 4 / 1 |
| `plan_cache_mode` | `auto` |

Every measurement is `EXPLAIN (ANALYZE, BUFFERS)`. Each statement ran **three
times**, except the ones marked "(single run)" in §3, where one run was enough
to establish the plan shape; the durations are given as a range because a single
run is noise, and the first run of a statement whose pages were cold is
identified where it is quoted. All numbers come from the commands in §8.

### 2.2 The corpus

20 000 rows in each measured table, split 10 000 / 10 000 between two owners with
fixed identifiers, so that a statement that forgot its owner predicate would
return twice as many rows and read twice as many buffers. `VACUUM (ANALYZE)` ran
before any measurement.

| Table | Rows | Per owner | Heap | Indexes |
| ----- | ---- | --------- | ---- | ------- |
| `expense_claims` | 20 000 | 10 000 | 5 344 kB | 6 096 kB |
| `expense_items` | 20 000 | 10 000 | 2 656 kB | 1 408 kB |
| `companies` | 20 000 | 10 000 | 1 864 kB | 5 672 kB |
| `job_applications` | 20 000 | 10 000 | 3 176 kB | 5 928 kB |
| `application_events` | 20 000 | 10 000 | 2 328 kB | 1 696 kB |
| `embeddings` | 20 000 | 10 000 | **39 MB** | 2 936 kB |
| `notes` | 20 000 | 10 000 | 2 952 kB | 4 344 kB |

The shapes that matter for the plans:

- **Claims** — `created_at` distinct per row, 105 minutes apart, so the
  `created_at DESC, id DESC` order has no ties. Status distributed 37 % draft,
  22.5 % submitted, 18 % approved, 13.5 % rejected, 9 % paid; lifecycle
  timestamps consistent with `expense_claims_lifecycle_timestamps_chk`.
- **Items** — 9 000 claims with one item, one claim with **900** items, one with
  100, and 998 claims with none. `total_amount` is the true `SUM`.
- **Companies** — 10 000 per owner, names `Company 00000` … with four different
  casings, so `LOWER(name)` is exercised and the per-owner folded name stays
  unique.
- **Applications** — every stage represented (1 428-1 429 per stage per owner),
  `created_at` distinct per row.
- **Events** — 2 000 on one application, 1 000 on a second, one each on 7 000
  more. `occurred_at` repeats in **groups of four**, so the `id DESC` tiebreaker
  is genuinely needed here.
- **Embeddings** — `dim = 384`, the model's real dimension, not a toy. Per
  owner: 9 000 note vectors for `bge-small-en-v1.5`, 500 note vectors for an
  older model, 500 application vectors. Every tenth note is archived, so the
  semantic scan's `is_archived = FALSE` filter excludes 900 rows per owner.

---

## 3. The plans at 20 000 rows

| # | Statement | Plan | Buffers | Duration |
| - | --------- | ---- | ------- | -------- |
| 1 | Claims list, unfiltered, `LIMIT 20` | `Index Scan` `expense_claims_user_id_created_at_idx` + `Incremental Sort` | 4 shared | 0.08-0.28 ms |
| 2 | Claims list, `status = 'approved'`, `LIMIT 20` | `Index Scan` `expense_claims_user_id_status_created_at_idx` + `Incremental Sort` | 8 shared | 0.08-0.32 ms |
| 3 | Claims count, unfiltered | `Index Only Scan` same index, `Heap Fetches: 0` | 91 shared | 3.09-4.94 ms |
| 4 | Claims count, `status = 'approved'` | `Index Only Scan` status index, `Heap Fetches: 0` | 23 shared | 0.38-0.98 ms |
| 5 | Item `SUM`, 900-item claim | `Bitmap Index Scan` `expense_items_claim_id_idx` → `Bitmap Heap Scan` | 31 shared | 0.65-2.95 ms |
| 6 | Item `SUM`, 1-item claim | `Index Scan` `expense_items_claim_id_idx` | 3 shared | 0.98 ms (single run) |
| 7 | Company list, `LIMIT 20` | `Index Scan` `companies_user_id_lower_name_name_id_idx`, **no sort** | 4 shared | 0.08-0.34 ms |
| 8 | Company count | **`Seq Scan on companies`**, 10 000 rows removed | **228 shared** | **5.85-6.89 ms** |
| 9 | Application list, no filter | `Index Scan` `job_applications_user_id_created_at_idx` + `Incremental Sort` | 12 shared | 0.26 ms (single run) |
| 10 | Application list, `status = 'interview'` | `Index Scan` `job_applications_user_id_status_created_at_idx` + `Incremental Sort` | 9 shared | 0.12-0.29 ms |
| 11 | Application list, same query, **generic plan** | **`Seq Scan on job_applications`**, 18 571 rows removed | **392 shared** | **4.64-18.32 ms** |
| 12 | Application count, `status = 'interview'` | `Index Only Scan` status index, `Heap Fetches: 0` | 19 shared | 0.71-0.79 ms |
| 13 | Event timeline, 2 000-event application | `Index Scan` `application_events_application_id_occurred_at_idx` + `Incremental Sort` | 6 shared | 0.06-0.16 ms |
| 14 | Event count, same application | `Bitmap Index Scan` → `Bitmap Heap Scan` | 68 shared | 0.72-3.02 ms |
| 15 | `EVENTS_EXISTS_WITH_TOTAL` | `Index Only Scan` + `Bitmap Heap Scan` | 71 shared | 1.95 ms (single run) |
| 16 | **Semantic scan** (`NOTE_VECTORS`), 8 100 live of 9 000 | **`Parallel Seq Scan` + external merge sort** | **5 387-6 135 + 983-1 573 temp** | **177-292 ms** |
| 17 | Bare three-column embeddings scan, 9 000 rows | `Parallel Seq Scan` + external merge sort | 5 074 + temp | 196-263 ms |
| 18 | `CURRENT_SOURCES`, 9 500 rows | `Seq Scan` + quicksort | 5 000 shared | 23.9-34.6 ms |

Read the table this way: rows 1-7 and 9-15 are what the indexes were designed
for. Row 8 and rows 16-18 are the ones that are not, and §5 says why.

### 3.1 The claims list is an index scan

```text
 Limit
   ->  Incremental Sort  (Sort Key: created_at DESC, id DESC, Presorted Key: created_at)
         ->  Index Scan using expense_claims_user_id_created_at_idx on expense_claims
               Index Cond: (user_id = '11111111-…'::uuid)
```

`user_id` is an equality on the leading column and `created_at DESC` matches the
index direction, so the planner reads 21 rows to return 20. It is not a
sequential scan because it does not need to see row 10 001: the `LIMIT` lets an
ordered index scan stop. The cost line says the same thing —
`Limit (cost=0.69..7.23 rows=20)` against a table of 10 000 rows per owner.

The filtered shape is the same with one more key column:

```text
   ->  Incremental Sort
         ->  Index Scan using expense_claims_user_id_status_created_at_idx on expense_claims
               Index Cond: ((user_id = '11111111-…'::uuid) AND (status = 'approved'::expense_claim_status))
```

This is the payoff of the repository holding **two** statement shapes: because
`status` is a literal predicate here rather than `($2 IS NULL OR status = $2)`,
it reaches the planner as an index key. §5.6 shows what the other spelling
costs.

### 3.2 The item `SUM` is a bitmap scan over one claim

```text
 Update on expense_claims
   ->  Nested Loop
         ->  Index Scan using expense_claims_id_user_id_unique_idx on expense_claims
               Index Cond: ((id = '…'::uuid) AND (user_id = '11111111-…'::uuid))
         ->  Subquery Scan on sums
               ->  Aggregate
                     Filter: (COALESCE(sum(amount), '0'::numeric) <= 999999999999999.9999)
                     ->  Bitmap Heap Scan on expense_items
                           Recheck Cond: (claim_id = '…'::uuid)
                           Filter: (user_id = '11111111-…'::uuid)
                           Heap Blocks: exact=29
                           ->  Bitmap Index Scan on expense_items_claim_id_idx
                                 Index Cond: (claim_id = '…'::uuid)
```

900 rows come from 2 index buffers and 29 heap blocks. The `user_id` filter is
redundant for correctness in this corpus — the claim is already known to be the
caller's, and the composite key `expense_items_claim_fk` makes the item's owner
its claim's owner — but it is present, it costs nothing, and it is the predicate
that keeps the statement safe if the preceding lookup ever changes.

The planner estimates 450 rows and finds 900. This claim is an outlier and the
column statistics describe the distribution of the 9 002 claims that have items,
not this one. The estimate is off by 2x and the plan is still correct; it is
worth knowing before anyone "fixes" the estimate.

### 3.3 The company list needs no sort

```text
 Limit
   ->  Index Scan using companies_user_id_lower_name_name_id_idx on companies
         Index Cond: (user_id = '11111111-…'::uuid)
```

`ORDER BY LOWER(name), name, id` is provided by the index exactly as written:
there is no `Sort` or `Incremental Sort` node in the plan at all. 4 buffers,
0.08-0.34 ms.

### 3.4 The event timeline sorts only its tie groups

```text
 Limit
   ->  Incremental Sort  (Sort Key: occurred_at DESC, id DESC, Presorted Key: occurred_at)
         ->  Index Scan using application_events_application_id_occurred_at_idx on application_events
               Index Cond: (application_id = '…'::uuid)
               Filter: (user_id = '11111111-…'::uuid)
```

`application_events_application_id_occurred_at_idx` is `(application_id,
occurred_at DESC)` and does **not** contain `user_id`, so the owner is a filter
rather than an index key. That is harmless here: an application belongs to one
owner, so the filter can only ever see that owner's rows. The rule of `AGENTS.md`
§4, item 3 is still satisfied — the predicate is in the `WHERE`, and a foreign
`application_id` matches nothing.

The 2 000-event application is estimated at 1 000 rows — again an outlier the
statistics average over, and again without consequence for the plan.

### 3.5 The semantic scan is not an index scan

This is the finding of the document.

```text
 Gather Merge
   Buffers: shared hit=6135, temp read=983 written=984
   ->  Sort  (Sort Key: e.source_id, Sort Method: external merge  Disk: 7864kB)
         ->  Hash Join  (Hash Cond: (e.source_id = n.id))
               ->  Parallel Seq Scan on embeddings e
                     Filter: ((user_id = '11111111-…'::uuid) AND (source_type = 'note'::text) AND (model_id = 'bge-small-en-v1.5'::text))
                     Rows Removed by Filter: 3667
                     Buffers: shared hit=5000
               ->  Hash
                     ->  Bitmap Heap Scan on notes n
                           Recheck Cond: ((user_id = '11111111-…'::uuid) AND (NOT is_archived))
                           ->  Bitmap Index Scan on notes_user_id_archived_idx
```

Three separate things are happening, and each has a cause:

1. **The scan is sequential.** `embeddings_user_id_source_type_model_id_idx` is
   only 160 kB and could seek the 9 000 matching keys, but the `vector` column
   is not in it, so an index scan would then perform 9 000 random heap fetches
   into a 39 MB heap. At this selectivity (45 % of the table qualifies) the cost
   model prefers one sequential pass. `Rows Removed by Filter: 3667` is what each
   parallel process read and discarded — every row that is *not* a note vector
   for this model, owner B's included.
2. **The sort spills to disk.** `ORDER BY e.source_id` covers 8 100 rows of
   ~1.5 kB each — about 12 MB against a 4 MB `work_mem` — so the sort is
   `external merge` with 983-1 573 temp buffers read and written.
3. **It costs 177-292 ms.** The plan spends the time in two places: 5 000 heap
   buffers read by the parallel scan, and up to 12.6 MB of temporary file for the
   sort. The join node itself reports 3-87 ms; the remainder is the scan and the
   external merge. The first run (292 ms) was cold; the two warm runs were the
   lower end.

The same statement on the same data at the dimension §11.6 used is a different
plan. §6 puts the two side by side.

---

## 4. Where the plan flips

Each volume is the whole corpus shrunk to *n* rows per owner by deleting a
deterministic suffix, then `VACUUM (ANALYZE)`. `VACUUM` rather than only
`ANALYZE` matters: without it the deleted tuples stay in the indexes and a
bitmap index scan reports dead entries as rows examined. The first version of
this sweep omitted it and produced a nonsense 3.5 ms at 5 000 rows, which is why
it was rerun.

"Index" means an ordered index scan that stops at the `LIMIT`; "bitmap" means a
bitmap index scan plus heap scan.

| Rows per owner | Claims list | Claims by status | Company list | Company count | Applications by stage | Semantic scan |
| -------------- | ----------- | ---------------- | ------------ | ------------- | --------------------- | ------------- |
| 20 | seq | bitmap | seq | seq | seq | bitmap, 14 buf |
| 200 | index | index | seq | seq | seq | bitmap, 111 buf |
| 1 000 | index | index | index + `Incremental Sort` | **seq**, 23 buf | index | bitmap, 540 buf |
| 2 500 | index | index | index + `Incremental Sort` | **seq** | index | bitmap, sort **in memory** 3 559 kB |
| 3 000 | index | index | index + `Incremental Sort` | **seq** | index | bitmap, sort **to disk** 4 200 kB |
| 4 000 | index | index | index + `Incremental Sort` | **seq** | index | **seq** |
| 5 000 | index | index | index + `Incremental Sort` | **seq**, 114 buf | index | **seq**, 5 182 buf |
| 10 000 | index | index | index, no sort | **seq**, 228 buf | index | **parallel seq**, 6 135 buf |

Four thresholds fall out of it:

- **The list indexes are chosen from about 200 rows per owner for claims, and
  about 1 000 for companies and applications.** Below those points a sequential
  scan is genuinely cheaper, and the planner is right to say so. At 20 rows per
  owner the claims list was a seq scan over a relation the sweep had already
  bloated, so its buffer count there is not comparable; the plan choice is the
  correct one for the size either way.
- **The semantic scan stops using its index between 3 000 and 4 000 note
  vectors per owner.** At 3 000 the bitmap scan is still chosen; at 4 000 it is
  not.
- **The semantic sort spills to disk between 2 500 and 3 000 vectors per
  owner** (3 559 kB in memory at 2 500, 4 200 kB to disk at 3 000). This is the
  first cliff, and it is a `work_mem` cliff, not an index one. The duration goes
  from 14-16 ms to 75-123 ms across it.
- **The company count is a sequential scan at every volume tested**, from 20 rows
  per owner to 10 000. It never chooses an index.

At 20 000 rows the semantic scan reads 5 387-6 135 shared buffers for 8 100
rows; at 1 000 rows per owner it read 540 for 900 rows. The cost is linear in
the *table*, not in the caller's result.

---

## 5. What the numbers say

### 5.1 Is any list a sequential scan it should not be?

Two statements are:

- **`COUNT_COMPANIES`**, at every volume. The plan removes exactly 10 000 rows —
  the other owner's companies — and reads all 228 heap pages. The reason is
  visible in the index sizes: the narrowest index on `companies` that begins
  with `user_id` is `companies_user_id_lower_name_unique_idx` at 1 368 kB, and
  the wide one is 2 384 kB. A count has no `LIMIT` and no `ORDER BY`, so an
  index-only scan would still read 10 000 wide index entries; 228 sequential
  pages win. Compare the claims count, which *is* an index-only scan with
  `Heap Fetches: 0` over a 1 432 kB index. The planner is not wrong; the schema
  simply has no narrow index on `companies` for a count to use.
- **`NOTE_VECTORS`** from 4 000 note vectors per owner upward, for the reason
  §3.5 gives.

Everything else — the event timeline, all the item and event aggregates, the
claims lists from 200 rows per owner, and the company and application lists from
1 000 — is an index or bitmap scan at every volume at which it was measured.

### 5.2 Does any query read all owners' rows?

Three do, physically, at 20 000 rows:

| Statement | Evidence |
| --------- | -------- |
| `COUNT_COMPANIES` | `Seq Scan on companies … Rows Removed by Filter: 10000`, 228 buffers = the whole table |
| `NOTE_VECTORS` at ≥ 4 000/owner | `Parallel Seq Scan on embeddings e … Rows Removed by Filter: 3667`; at 20 000 rows, 5 000 buffers = the whole 39 MB table |
| `PAGE_APPLICATIONS` under a generic plan | `Seq Scan on job_applications … Rows Removed by Filter: 18571`, 392 buffers |

None of them *returns* another owner's row. Ownership is enforced by the
`user_id` predicate in every case, and the control queries prove it:

```text
-- owner B counting owner A's claim id
 Index Only Scan using expense_claims_id_user_id_unique_idx on expense_claims
   Index Cond: ((id = 'e4306665-…'::uuid) AND (user_id = '22222222-…'::uuid))
   Heap Fetches: 0
```

returns 0 rows in 2 buffers, 0.058 ms. The distinction this document draws is
between *reading* a row and *returning* it: the first is a performance property,
the second is a correctness one, and only the second is a defect.

### 5.3 Is the `Incremental Sort` over the `id` tiebreak measurable?

It depends entirely on how many rows share the leading key, and the corpus could
answer both cases.

**With distinct timestamps it is negligible.** The claims list sorts one group
of about twenty rows:

```text
 Incremental Sort  (Sort Key: created_at DESC, id DESC, Presorted Key: created_at)
   Full-sort Groups: 1  Sort Method: quicksort  Average Memory: 27kB  Peak Memory: 27kB
```

27 kB, no extra shared buffer, and the statement is dominated by the index scan
(0.08-0.28 ms end to end).

**With a large group it is measurable.** Setting the 5 000 newest claims of one
owner to a single `created_at` — which is what a bulk import or a transaction
that inserts many rows under one `NOW()` produces — changes the same statement
to:

```text
 Limit  (actual time=4.412..4.418 rows=20.00 loops=1)
   Buffers: shared hit=143
   ->  Incremental Sort  (Sort Key: created_at DESC, id DESC, Presorted Key: created_at)
         Pre-sorted Groups: 1  Sort Method: top-N heapsort
         ->  Index Scan using expense_claims_user_id_created_at_idx
               (actual time=0.032..2.244 rows=5001.00 loops=1)
               Buffers: shared hit=143
```

3.8-4.7 ms and 143 buffers, against 0.08-0.28 ms and 4 buffers — an order of
magnitude or more, and at least thirteen times the slowest baseline run. The
cost is not the sort itself (top-N heapsort over 20 rows) but reading the whole
5 000-row tie group from the heap before the first 20 can be emitted. So:
**negligible when the leading key is distinct, measurable when it is not**, and
the API's own `NOW()` default is what makes ties likely in bulk-written data.

The company list has the same clause and **cannot** tie: `LOWER(name)` is unique
per owner, enforced by `companies_user_id_lower_name_unique_idx`. At 10 000 rows
the planner uses the wide index and there is no sort node; at 5 000 it uses the
narrow unique index plus an `Incremental Sort` whose groups are all one row.
Both are correct orders. The practical consequence is that `name, id` in the
`ORDER BY` never break a tie for one owner, so the wide index's extra columns
buy the removal of a sort that has nothing to sort. That is an observation for
§7, not a change made here.

### 5.4 The company count scales with the table, not the owner

5.85-6.89 ms at 20 000 rows, 228 buffers, and every one of those buffers belongs
to a page the statement had to read to find 10 000 of 20 000 rows. The number is
small today. It is worth knowing that it is `O(companies)` and not
`O(the caller's companies)`, because the other counts in the same features are
the latter.

### 5.5 The event timeline's owner filter is not an index key

`application_events_application_id_occurred_at_idx` is `(application_id,
occurred_at DESC)`, so `user_id` is a filter. That is a deliberate consequence
of the schema: an event's `application_id` already determines its owner through
`application_events_application_fk`, so adding `user_id` to the index would
duplicate a fact. The measured cost is 6 buffers for a 2 000-event application,
so the filter is not a problem. It is recorded here because a reader comparing
the two listing indexes will notice the asymmetry and should not "fix" it.

### 5.6 The stage filter disappears from a generic plan

`PAGE_APPLICATIONS` is one statement for every filter combination, so its stage
predicate is written as:

```sql
AND ($2::job_application_status IS NULL OR status = $2::job_application_status)
```

With the parameter bound, the planner folds `'interview' IS NULL` to false and
the statement is an index scan (9 buffers, 0.12-0.29 ms). With the parameter
unresolved, it cannot:

```text
 Limit  (cost=992.02..992.03)
   ->  Sort  (Sort Method: top-N heapsort  Memory: 35kB)
         ->  Seq Scan on job_applications  (cost=0.00..992.01 rows=1)
               Filter: ((($2 IS NULL) OR (status = $2)) AND … AND (user_id = $1) AND …)
               Rows Removed by Filter: 18571
```

392 buffers and 4.64-18.32 ms, against 9 buffers and 0.12-0.29 ms — a regression
of one to two orders of magnitude on the same rows. This is the mistake the
claims repository deliberately avoids by holding two statement shapes (see
§3.1).

**It is not what the API does today.** With `plan_cache_mode = auto` and eight
executions of the prepared statement, PostgreSQL kept choosing the custom plan:

| Prepared statement | `generic_plans` | `custom_plans` |
| ------------------ | --------------- | -------------- |
| `PAGE_APPLICATIONS` shape | 0 | 8 |
| `PAGE_WITHOUT_STATUS` shape | 0 | 8 |

The generic plan is much more expensive, so `auto` never settles on it. The
hazard is real but latent: it needs `plan_cache_mode = force_generic_plan`, or a
future data distribution in which the generic estimate stops looking expensive,
to become the plan the API runs.

---

## 6. What this changes about `schema.md` §11.6

§11.6 recorded four statement shapes as index scans on 40 000 rows of `dim = 2`.
The row counts and the buffer counts there are still correct. The plan shape is
**not** what these statements do at the dimension the model actually produces,
because the heap is an order of magnitude larger:

| | §11.6 (`dim = 2`) | This document (`dim = 384`) |
| - | ----------------- | --------------------------- |
| Rows | 40 000, one owner | 20 000, two owners |
| Heap | small | 39 MB |
| Semantic scan | `Bitmap Index Scan` on `embeddings_user_id_source_type_model_id_idx` | `Parallel Seq Scan` + external merge sort |
| Duration | 6.3-12.8 ms | 177-292 ms |

§11.6 already said that "the buffer and row counts are the finding, not the byte
size of the array". This document makes the size part of the finding: the
`REAL[]` column is what makes the heap 39 MB for 20 000 rows, and a heap that
size is why the index stops being chosen at 4 000 vectors per owner.

Nothing in §11.6's *design* claim is contradicted. The scan is still a linear
scan of the owner's rows rather than an indexed nearest-neighbour query, which
is what ADR-0008 accepted. What changes is the cost and the plan shape at
realistic dimensions.

---

## 7. Findings for review

None of these was acted on in this chain. Each is either a decision to accept or
the subject of a separate change.

1. **The semantic scan degrades from an index scan to a full-table scan plus a
   disk sort at realistic dimensions.** 177-292 ms for 8 100 vectors at
   `dim = 384`, and it reads the other owner's rows. The index stops being
   chosen at 4 000 note vectors per owner. *Decision:* accept while corpora stay
   small, or open a change that considers a covering index or a dedicated
   ordering column. Do not treat the 6-13 ms in §11.6 as the current cost.
2. **The sort, not the scan, is the first cliff.** `work_mem = 4 MB` holds about
   2 700 vectors of ~1.5 kB; beyond that the `ORDER BY source_id` spills and the
   statement goes from 14-16 ms to 75-123 ms. This is a deployment setting, not
   a schema one, and it is cheap to test before touching an index.
3. **`PAGE_APPLICATIONS` has the shape the claims repository rejected.** A
   generic plan turns it into a 392-buffer sequential scan. It does not happen
   under `plan_cache_mode = auto` today (0 generic of 8), so this is a latent
   risk to record rather than a live defect.
4. **`COUNT_COMPANIES` is a sequential scan at every volume** because no narrow
   `user_id`-leading index on `companies` exists to scan-only. 5.85-6.89 ms at
   20 000 rows, and linear in the table.
5. **`companies_user_id_lower_name_name_id_idx` removes a sort that cannot
   sort**, and costs 2 384 kB against the 1 368 kB unique index that supplies
   the same leading order. It is chosen at 10 000 rows per owner and not at
   5 000. Worth a look in a change that is allowed to touch indexes; not worth
   one on its own.

---

## 8. Reproducing

The loader and measurement files were scratch scripts, not repository files;
the corpus rules in §2.2 are the specification for regenerating them. The
command set that produced every number above:

```powershell
# Connection strings. The scratch database only; never atom_test, never atom_dev.
$env:PGURL   = 'postgres://atom:atom@127.0.0.1:5432/postgres'
$env:SCRATCH = 'postgres://atom:atom@127.0.0.1:5432/atom_perf_d5h3'

# 1. A scratch database.
psql $env:PGURL -c 'DROP DATABASE IF EXISTS atom_perf_d5h3;'
psql $env:PGURL -c 'CREATE DATABASE atom_perf_d5h3;'

# 2. Every migration, in order. (sqlx-cli was not installed; the files are
#    plain SQL and psql applies them identically.)
Get-ChildItem migrations/*.sql | Sort-Object Name | ForEach-Object {
    psql $env:SCRATCH -v ON_ERROR_STOP=1 -q -f $_.FullName
}

# 3. The corpus, then the statistics the planner reads.
psql $env:SCRATCH -f load.sql

# 4. EXPLAIN (ANALYZE, BUFFERS), three runs each.
psql $env:SCRATCH -f m1.sql   # claims list, filtered list, counts, item SUM
psql $env:SCRATCH -f m2.sql   # company list and count, application list, events
psql $env:SCRATCH -f m3.sql   # embeddings, and the prepared/generic comparison
psql $env:SCRATCH -f m4.sql   # the count statements
psql $env:SCRATCH -f tie.sql  # the 5 000-row created_at tie

# 5. The threshold sweep. Each volume reloads the corpus first, because
#    sweep.sql only ever deletes down: 5000, 4000, 3000, 2500, 2000, 1000,
#    200, 20 rows per owner.
psql $env:SCRATCH -f load.sql
psql $env:SCRATCH -v n=5000 -f sweep.sql

# 6. Drop it.
psql $env:PGURL -c 'DROP DATABASE atom_perf_d5h3;'
```

---

## 9. What was not measured

- **Cold-cache latency.** Every number is warm, apart from the first run of each
  statement, which is identified where it is quoted. A restart before each
  measurement would separate I/O from CPU, and it was not done.
- **Concurrency.** One session. Two writers on one claim serialise on
  `LOCK_CLAIM` and the effect on the item `SUM` is a locking question, not a
  plan question; the transaction boundary is already documented in
  [`../architecture/reimbursements-crud-design.md`](../architecture/reimbursements-crud-design.md).
- **The write paths other than the item `SUM`** — inserts, lifecycle
  transitions, the guarded application `UPDATE`. Their plans are single-row
  index seeks by primary key and were not the question this chain asked.
- **The hybrid re-rank in Rust.** It is not SQL and its cost is CPU, not
  buffers.
- **`VECTORS_FOR_SOURCES` with a real candidate list.** It was measured in
  §11.6 as an index scan (4 buffers); at 20 000 rows it is a different question
  because the candidate array is supplied by the lexical query. Left for a
  chain that can produce the real candidate set.

---

## 10. Related documents

- [`schema.md`](schema.md) — the tables, constraints, indexes and §11.6, whose
  measurement this document completes.
- [`migration-policy.md`](migration-policy.md) — how a migration would be
  written if a finding here needs an index.
- [`../architecture/reimbursements-crud-design.md`](../architecture/reimbursements-crud-design.md)
  — why the two claim listing shapes exist.
- [`../architecture/data-architecture.md`](../architecture/data-architecture.md)
  — the ownership model the plans rely on.
