# The AI retrieval roadmap — from local search toward RAG

> **Status:** Preparation document — it records what exists, what is missing,
> and the order of the work. Nothing here is implemented beyond what §1 names,
> and nothing here authorises a hosted or paid service
> **Last updated:** 2026-10-30
> **Owner:** maintained by the `architect` agent

The human's direction keeps local AI, retrieval, prompt history and
conversational memory on the roadmap. This document plans the **next layer**
of that direction against the retrieval stack that already exists and is
`Verified`, so that the next change extends it instead of rebuilding it.

---

## 1. What exists today (all `Verified`, 2026-10-29)

| Capability | Where | Boundary |
| ---------- | ----- | -------- |
| Local inference | `src/ml/burn_embedder.rs` — a six-layer BERT encoder on Burn's `flex` CPU backend, weights from a local SafeTensors file; `src/ml/tokenizer.rs` — a hand-written uncased WordPiece tokenizer | Model file supplied by the operator; no network, no second runtime (`ADR-0008`, owner decision `O6`) |
| Vector storage | `embeddings` table (`migrations/0006`), `REAL[]` with a dimension CHECK and a unique upsert target | No pgvector; a sequential scan bounded by `candidate_limit` (`ADR-0010`) |
| Lexical search | The generated `tsvector` on `notes` (`migrations/0002`) | PostgreSQL full-text match |
| Semantic + hybrid search | `src/ml/service.rs` — cosine re-rank of a bounded lexical candidate set; match ranking over applications | Degradation contract: a scored mode that cannot embed answers lexical `200` with a reason (`ML-R06`) |
| Retrieval quality | 15 fixture queries (`ML-R09`); the real model's quality recorded by a manual `atom-ml-bench` run | The single recorded limitation of the `ml` feature: no CI case loads a model |

**What this is not:** this is retrieval-augmented *search*. There is no
generation, no prompt assembly, no conversation storage — those are excluded
by `project-truth.md` §6.1 and are exactly the work §3 plans. Full RAG exists
only when retrieval is integrated with context assembly and actual generation.

## 2. Standing constraints any next layer inherits

1. **Local first, host-optional second.** Inference runs in the API process
   (`O6`, `ADR-0008`); no request may leave the machine to obtain text. A
   hosted LLM provider would need its own owner decision and a new amendment
   to §6.1.
2. **Generation is excluded today.** Adding it is a product decision first
   (the §6.1 amendment that permits local generation), then an
   `embedder`-style trait so the provider is pluggable without redesign.
3. **Retrieval stays owner-scoped.** Every new retrieval surface inherits the
   ADR-0005 predicate rule; a context assembled for a prompt may contain only
   the caller's own rows.
4. **AI actions never bypass the contract.** Anything an AI feature *does*
   (write a note, move a task) must go through the normal endpoints, with the
   normal authentication, validation, ownership and audit characteristics —
   the model is never a second authorization path.

## 3. The planned layers, in dependency order

| # | Layer | What it adds | First buildable increment | Depends on |
| - | ----- | ------------ | -------------------------- | ---------- |
| 1 | **Prompt and conversation history** | Tables for conversations and messages (owner-scoped, like every table), and CRUD endpoints. No inference involved. | A `conversations` + `messages` migration and module following the existing feature layout; prompt text is ordinary validated text. | Nothing — it is storage, and it is the foundation of everything below |
| 2 | **Searchable activity history** | The `embeddings` source-type CHECK gains new kinds (e.g. `'conversation'`), so past prompts/messages become retrievable with the existing search | One migration widening `embeddings_source_type_chk` + reindex support for the new source | Layer 1 |
| 3 | **Context assembly** | A pure-Rust module that takes a query and assembles a bounded context from retrieved chunks (existing search + future history), with token budgeting and source attribution | A `context::assemble(query, budget) -> Vec<Chunk>` with its own tests; still no generation | Layers 1–2 |
| 4 | **Local generation provider** | A `Generator` trait beside `Embedder` (local CPU first — e.g. a quantised model via Burn or a future local backend), `Disabled`/`Deterministic` implementations for tests, the same start-up selection pattern | The trait, the config surface (`ai.*` settings following the `ml.*` pattern), and the degraded contract: generation unavailable is an explicit state, never a silent failure | An owner decision permitting generation (§6.1 amendment) |
| 5 | **RAG endpoints** | An endpoint that retrieves (Layer 3), assembles, and generates an answer **with citations back to the caller's own rows**; failure and refusal modes specified like every other contract | One endpoint behind the feature layout, with the degradation table written before the code | Layers 1–4 |
| 6 | **Tool execution and memory policy** | AI-initiated actions through real endpoints with permission boundaries; retention and deletion rules for conversation history (a personal-data lifecycle, owner-controlled) | A policy document and the first tool, only after Layer 5 is `Verified` | Layer 5 |

## 4. Quality and evaluation before, not after

- The retrieval stack earned `Verified` with fixture queries and a recorded
  manual benchmark. Layers 3–5 need the same discipline: a fixed evaluation
  set (query → expected context → expected behaviour), run on the real model,
  recorded in the feature card; the existing `atom-ml-bench` pattern is the
  template.
- A generation layer additionally needs a refusal/quality eval (what it does
  when the retrieved context is empty or irrelevant) — written as acceptance
  criteria before the endpoint exists.

## 5. What this document deliberately does not do

- It does not accept any requirement: §3 is an order of work, and each layer
  becomes scope only when `project-truth.md` gains it (the §6.1 amendment for
  generation; a §19 or successor for the history features).
- It does not name a model or a crate for generation: that is Layer 4's
  decision, made with measurements, after the owner accepts generation.
- It does not treat the existing `embeddings` table as "memory": today it is a
  search index over the owner's notes and applications. Memory begins when
  conversations (Layer 1) exist and are retrievable (Layer 2).

## 6. Related

| Reference | Relationship |
| --------- | ------------ |
| [`../project-truth.md`](../project-truth.md) §6.1, §18.3 | The exclusions the later layers amend, and the verified retrieval requirements |
| [`ADR-0008-local-pure-rust-ml.md`](../decisions/ADR-0008-local-pure-rust-ml.md) | The local-inference decision the generator trait would follow |
| [`ADR-0010-semantic-search-scan-bound.md`](../decisions/ADR-0010-semantic-search-scan-bound.md) | The scan bound Layers 2–3 must respect |
| [`../features/ml/status.md`](../features/ml/status.md) | The verified state of the existing stack and its recorded limitation |
| [`ADR-0012-automation-orchestration-scope.md`](../decisions/ADR-0012-automation-orchestration-scope.md) | The pattern for introducing an owner-gated capability as a bounded proposal |
