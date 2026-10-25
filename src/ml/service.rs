//! The machine-learning use cases: search in three modes, match, reindex.
//!
//! The degradation rule is the reason this module exists:
//!
//! > A model that is absent, disabled or slow must never turn a working
//! > endpoint into a server error: a search answers `200` with lexical results,
//! > a match answers `409`, and a reindex answers `409`.
//!
//! ([`docs/features/ml/architecture.md`](../../docs/features/ml/architecture.md)
//! §12.6.)
//!
//! Everything that maps onto that rule lives here, not in a handler: the
//! handlers extract and serialise, and they contain no branch about models.
//!
//! # CPU bound
//!
//! `flex` is a CPU backend, so `Embedder::embed` is blocking work. Every call
//! goes through [`MlService::embed`], which moves it to `spawn_blocking` and
//! bounds it with `ml.timeout_seconds`. A timeout degrades a search and answers
//! `408` for a match, because a match has no lexical fallback.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use uuid::Uuid;

use crate::config::messages::Messages;
use crate::config::settings::{MlSettings, PaginationSettings};
use crate::error::{AppError, AppResult};
use crate::helpers::required_text;
use crate::job_applications::models::JobApplication;
use crate::job_applications::repository::JobRepository;
use crate::ml::dto::{
    DEFAULT_MATCH_LIMIT, DegradedReason, MAX_MATCH_LIMIT, MAX_QUERY_LENGTH, MatchHit, MatchRequest,
    MatchResponse, ReindexResponse, SearchHit, SearchMode, SearchRequest, SearchResponse,
};
use crate::ml::embedder::{Embedder, EmbedderStatus, MlError};
use crate::ml::repository::{MlRepository, SourceType, StoredVector};
use crate::notes::dto::NoteResponse;
use crate::notes::models::Note;
use crate::notes::repository::{NoteFilters, NoteRepository};
use crate::pagination::{Pagination, PaginationQuery};

/// The number of decimals a published score carries.
///
/// `api.md` §2.3 publishes four decimals. The value is a similarity, not money,
/// but the rounding is what makes two runs of the same query comparable in a
/// client that prints the number.
const SCORE_SCALE: f64 = 10_000.0;

/// The machine-learning use cases.
#[derive(Clone)]
pub struct MlService {
    repository: MlRepository,
    notes: NoteRepository,
    jobs: JobRepository,
    embedder: Arc<dyn Embedder>,
    messages: Arc<Messages>,
    settings: MlSettings,
    pagination: PaginationSettings,
}

impl std::fmt::Debug for MlService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MlService")
            .field("model_id", &self.embedder.model_id())
            .field("dimension", &self.embedder.dimension())
            .field("status", &self.embedder.status())
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl MlService {
    /// Builds the service.
    ///
    /// The embedder is passed in rather than constructed here, because the
    /// choice is made **once**, in `AppState::new`, and never re-evaluated per
    /// request ([`crate::ml::embedder::select`]).
    pub fn new(
        repository: MlRepository,
        notes: NoteRepository,
        jobs: JobRepository,
        embedder: Arc<dyn Embedder>,
        messages: Arc<Messages>,
        settings: MlSettings,
        pagination: PaginationSettings,
    ) -> Self {
        Self {
            repository,
            notes,
            jobs,
            embedder,
            messages,
            settings,
            pagination,
        }
    }

    /// The embedder's identifier, for the reindex response and for a log line.
    #[must_use]
    pub fn model_id(&self) -> &str {
        self.embedder.model_id()
    }

    /// Whether the embedder can serve a call.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.embedder.status().is_ready()
    }

    // ---------------------------------------------------------------- search

    /// Searches the caller's notes in one of three modes.
    ///
    /// `lexical` never reads the embedder. `semantic` and `hybrid` degrade to
    /// the same lexical answer — with `200` — when the model is unavailable,
    /// unusable or too slow to answer within the budget.
    pub async fn search(
        &self,
        owner: Uuid,
        request: SearchRequest,
        pagination: PaginationQuery,
    ) -> AppResult<SearchResponse> {
        let query = required_text(
            "query",
            &request.query,
            MAX_QUERY_LENGTH,
            &self.messages.validation.too_long,
            &self.messages,
        )?;

        let limit = pagination.effective_limit(&self.pagination);
        let offset = pagination.effective_offset();
        let requested = request.mode;

        match requested {
            SearchMode::Lexical => {
                self.lexical(owner, &query, limit, offset, requested, None)
                    .await
            }
            SearchMode::Semantic => match self.embed_query(&query).await {
                Ok(vector) => {
                    self.semantic(owner, &vector, limit, offset, requested)
                        .await
                }
                Err(reason) => {
                    self.lexical(owner, &query, limit, offset, requested, Some(reason))
                        .await
                }
            },
            SearchMode::Hybrid => match self.embed_query(&query).await {
                Ok(vector) => {
                    self.hybrid(owner, &query, &vector, limit, offset, requested)
                        .await
                }
                Err(reason) => {
                    self.lexical(owner, &query, limit, offset, requested, Some(reason))
                        .await
                }
            },
        }
    }

    /// The lexical answer, and the answer every degraded search returns.
    ///
    /// It delegates to the **existing** notes query, so a change to the notes
    /// listing cannot make this endpoint disagree with it.
    async fn lexical(
        &self,
        owner: Uuid,
        query: &str,
        limit: i64,
        offset: i64,
        requested: SearchMode,
        degraded_reason: Option<DegradedReason>,
    ) -> AppResult<SearchResponse> {
        let filters = NoteFilters {
            search: Some(query.to_string()),
            archived: Some(false),
        };

        let (notes, total) = self
            .notes
            .list_owned(owner, &filters, limit, offset)
            .await?;

        let items = notes
            .into_iter()
            .map(|note| SearchHit {
                note: NoteResponse::from(note),
                score: None,
            })
            .collect();

        Ok(SearchResponse {
            requested_mode: requested,
            mode: SearchMode::Lexical,
            degraded: degraded_reason.is_some(),
            degraded_reason,
            items,
            pagination: Pagination::new(limit, offset, total),
        })
    }

    /// Ranks every note the caller has a vector for.
    ///
    /// The scan is linear in the caller's embedding count. At a personal corpus
    /// size that is milliseconds; it is deliberately not an indexed
    /// nearest-neighbour query
    /// ([`ADR-0008`](../../docs/decisions/ADR-0008-local-pure-rust-ml.md) §4).
    ///
    /// A row whose similarity is zero is not returned: see [`is_match`]. The
    /// filter runs before `total` is taken, so the envelope counts exactly the
    /// hits the client can receive.
    async fn semantic(
        &self,
        owner: Uuid,
        query: &[f32],
        limit: i64,
        offset: i64,
        requested: SearchMode,
    ) -> AppResult<SearchResponse> {
        let stored = self
            .repository
            .note_vectors(owner, self.embedder.model_id())
            .await?;

        let mut scored = rank(&stored, query, self.embedder.dimension());
        scored.retain(|hit| is_match(hit.score));
        let total = count(&scored);
        let page = slice(scored, limit, offset);

        self.page(
            owner,
            PageWindow {
                limit,
                offset,
                total,
                requested,
                applied: SearchMode::Semantic,
            },
            page,
        )
        .await
    }

    /// Re-ranks a bounded lexical candidate set.
    ///
    /// The candidate set is at most `ml.candidate_limit` rows, so memory and CPU
    /// are bounded by configuration rather than by the corpus. A candidate
    /// without an embedding is **not** returned: a score of zero would bury a
    /// note that has simply never been embedded, and a `null` score in a mode
    /// defined by its scores would make the item shape two-shaped.
    ///
    /// The same rule then drops a candidate that *has* an embedding where the
    /// two vectors are orthogonal or opposed — the score is exactly zero, which
    /// is the absence of a similarity rather than a weak one ([`is_match`]).
    /// The lexical candidate set is only the pool to re-rank; it is not itself
    /// evidence of a semantic match.
    async fn hybrid(
        &self,
        owner: Uuid,
        query_text: &str,
        query: &[f32],
        limit: i64,
        offset: i64,
        requested: SearchMode,
    ) -> AppResult<SearchResponse> {
        let candidates = self
            .notes
            .lexical_candidates(owner, query_text, self.settings.candidate_limit)
            .await?;

        let stored = self
            .repository
            .vectors_for_sources(
                owner,
                SourceType::Note,
                self.embedder.model_id(),
                &candidates,
            )
            .await?;

        let mut scored = rank(&stored, query, self.embedder.dimension());
        scored.retain(|hit| is_match(hit.score));
        let total = count(&scored);
        let page = slice(scored, limit, offset);

        self.page(
            owner,
            PageWindow {
                limit,
                offset,
                total,
                requested,
                applied: SearchMode::Hybrid,
            },
            page,
        )
        .await
    }

    /// Builds the response for one page of ranked notes.
    async fn page(
        &self,
        owner: Uuid,
        window: PageWindow,
        page: Vec<ScoredNote>,
    ) -> AppResult<SearchResponse> {
        let ids: Vec<Uuid> = page.iter().map(|hit| hit.id).collect();
        let notes = self.notes.find_many_owned(owner, &ids).await?;
        let mut by_id: HashMap<Uuid, Note> =
            notes.into_iter().map(|note| (note.id, note)).collect();

        let mut items = Vec::with_capacity(page.len());
        for hit in page {
            // A note that vanished between the two statements drops out of the
            // page rather than arriving as an empty shape. The window is one
            // index lookup wide, and `total` is the size of the scored set.
            if let Some(note) = by_id.remove(&hit.id) {
                items.push(SearchHit {
                    note: NoteResponse::from(note),
                    score: Some(round_score(hit.score)),
                });
            }
        }

        Ok(SearchResponse {
            requested_mode: window.requested,
            mode: window.applied,
            degraded: false,
            degraded_reason: None,
            items,
            pagination: Pagination::new(window.limit, window.offset, window.total),
        })
    }

    // ----------------------------------------------------------------- match

    /// Ranks the caller's notes against one job application.
    ///
    /// A match has no lexical fallback, so an unavailable model is a `409` and
    /// an inference that overruns the budget is a `408`: nothing honest can be
    /// returned in either case.
    ///
    /// The same score floor `semantic` and `hybrid` apply applies here: a note
    /// whose similarity is at or below zero is **not** a match ([`is_match`]),
    /// exactly as in a search. Otherwise an application the caller's corpus
    /// shares no direction with would be answered with a `0.0` hit, which is
    /// the one shape this endpoint must not publish. The filter runs before the
    /// `limit`, so a dropped note does not consume one of the requested slots,
    /// and `matches: []` with `200` is the answer when nothing qualifies — a
    /// match has no envelope, and an empty list is well-formed.
    pub async fn match_application(
        &self,
        owner: Uuid,
        application_id: Uuid,
        request: MatchRequest,
    ) -> AppResult<MatchResponse> {
        let limit = validate_match_limit(request.limit, &self.messages)?;

        // The lookup runs before the embedder check, so another account's
        // application is a `404` and never a `409` that would confirm it
        // exists.
        let application = self
            .jobs
            .find_application_owned(application_id, owner)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(self.messages.resources.application_not_found.clone())
            })?;

        if let EmbedderStatus::Disabled { reason } = self.embedder.status() {
            tracing::debug!(reason, "refusing a match: the embedder is disabled");
            return Err(AppError::Conflict(
                self.messages.conflicts.ml_unavailable.clone(),
            ));
        }

        // One embed for the application, not one per note.
        let vector = self
            .embed(application_text(&application))
            .await
            .map_err(|fault| fault.into_error(&self.messages))?;

        let stored = self
            .repository
            .note_vectors(owner, self.embedder.model_id())
            .await?;

        let scored = ranked_matches(rank(&stored, &vector, self.embedder.dimension()), limit);

        let ids: Vec<Uuid> = scored.iter().map(|hit| hit.id).collect();
        let notes = self.notes.find_many_owned(owner, &ids).await?;
        let mut titles: HashMap<Uuid, String> = notes
            .into_iter()
            .map(|note| (note.id, note.title))
            .collect();

        let matches = scored
            .into_iter()
            .filter_map(|hit| {
                titles.remove(&hit.id).map(|title| MatchHit {
                    note_id: hit.id,
                    title,
                    score: round_score(hit.score),
                })
            })
            .collect();

        Ok(MatchResponse {
            application_id,
            mode: request.mode,
            matches,
        })
    }

    // --------------------------------------------------------------- reindex

    /// Rebuilds the caller's embeddings. Idempotent.
    ///
    /// Per-row, not one giant transaction: a reindex of a large corpus in one
    /// transaction would hold locks and grow the WAL for no benefit, and an
    /// interrupted run is simply run again because every upsert is idempotent.
    /// The orphan sweep runs last, so a row whose source was deleted during the
    /// run is removed rather than left behind.
    pub async fn reindex(&self, owner: Uuid) -> AppResult<ReindexResponse> {
        if let EmbedderStatus::Disabled { reason } = self.embedder.status() {
            tracing::debug!(reason, "refusing a reindex: the embedder is disabled");
            return Err(AppError::Conflict(
                self.messages.conflicts.ml_unavailable.clone(),
            ));
        }

        let model_id = self.embedder.model_id().to_string();
        let dimension = self.embedder.dimension();
        let stored_dimension = i32::try_from(dimension).unwrap_or(i32::MAX);

        let existing = self
            .repository
            .current_sources(owner, &model_id, stored_dimension)
            .await?;

        let mut current: HashSet<(SourceType, Uuid)> = HashSet::with_capacity(existing.len());
        for (source_type, source_id) in existing {
            if let Some(known) = SourceType::parse(&source_type) {
                current.insert((known, source_id));
            }
        }

        let mut indexed = 0_i64;
        let mut skipped = 0_i64;

        for note in self.notes.embeddable_owned(owner).await? {
            if current.contains(&(SourceType::Note, note.id)) {
                skipped += 1;
                continue;
            }

            let vector = self
                .embed(note_text(&note))
                .await
                .map_err(|fault| fault.into_error(&self.messages))?;

            if self
                .repository
                .upsert_note(owner, note.id, &model_id, &vector)
                .await?
            {
                indexed += 1;
            }
        }

        for application in self.jobs.embeddable_owned(owner).await? {
            if current.contains(&(SourceType::Application, application.id)) {
                skipped += 1;
                continue;
            }

            let vector = self
                .embed(application_text(&application))
                .await
                .map_err(|fault| fault.into_error(&self.messages))?;

            if self
                .repository
                .upsert_application(owner, application.id, &model_id, &vector)
                .await?
            {
                indexed += 1;
            }
        }

        let deleted = self.repository.delete_orphans(owner).await?;

        Ok(ReindexResponse {
            model_id,
            dimension,
            indexed,
            skipped,
            deleted,
        })
    }

    // ------------------------------------------------------------- inference

    /// Embeds a query, or reports the published reason it could not.
    async fn embed_query(&self, query: &str) -> Result<Vec<f32>, DegradedReason> {
        if let EmbedderStatus::Disabled { reason } = self.embedder.status() {
            tracing::debug!(%reason, "the embedder is disabled; the search degrades");
            return Err(DegradedReason::from_reason(&reason));
        }

        match self.embed(query.to_string()).await {
            Ok(vector) => Ok(vector),
            Err(fault) => {
                let reason = fault.reason();
                tracing::warn!(
                    reason = reason.as_str(),
                    "the query could not be embedded; the search degrades to lexical results"
                );
                Err(reason)
            }
        }
    }

    /// Runs one inference off the async runtime and bounds it.
    async fn embed(&self, text: String) -> Result<Vec<f32>, EmbedFault> {
        let embedder = Arc::clone(&self.embedder);

        // `spawn_blocking` moves the CPU-bound call off a Tokio worker; the
        // timeout bounds how long the request waits for it. A task that cannot
        // be cancelled keeps running after the deadline, which is recorded as a
        // limitation rather than hidden: the work is one inference and the
        // request has already returned.
        let handle = tokio::task::spawn_blocking(move || embedder.embed(&text));

        match tokio::time::timeout(self.settings.timeout, handle).await {
            Ok(Ok(Ok(vector))) => Ok(vector),
            Ok(Ok(Err(error))) => Err(EmbedFault::Failed(error)),
            Ok(Err(join)) => Err(EmbedFault::Failed(MlError::Model {
                message: format!("the inference task did not complete: {join}"),
            })),
            Err(_) => Err(EmbedFault::Timeout),
        }
    }
}

/// One scored note, before it becomes a response item.
#[derive(Debug, Clone, Copy)]
struct ScoredNote {
    id: Uuid,
    score: f64,
}

/// Scores one stored row set against `query`, then orders it totally.
///
/// A row whose length is not the embedder's dimension is **skipped**, not
/// scored and not repaired: the schema's `CHECK` and this check are the same
/// rule at two layers, and repairing a broken writer on read would hide the
/// defect the write-time invariant exists to expose (ADR-0008 §9).
///
/// The order is total — the score, then the identifier — so two pages of the
/// same query neither overlap nor skip a row. `total_cmp` is used rather than
/// `partial_cmp`: a NaN can never enter (a non-finite score is `None`), and a
/// comparison that cannot return `None` cannot silently fall back to an
/// arbitrary order.
fn rank(stored: &[StoredVector], query: &[f32], dimension: usize) -> Vec<ScoredNote> {
    let mut scored: Vec<ScoredNote> = stored
        .iter()
        .filter_map(|row| {
            if row.vector.len() != dimension {
                tracing::debug!(
                    source_id = %row.source_id,
                    stored = row.vector.len(),
                    expected = dimension,
                    "skipping an embedding whose length is not the configured dimension"
                );
                return None;
            }

            score(query, &row.vector).map(|value| ScoredNote {
                id: row.source_id,
                score: value,
            })
        })
        .collect();

    scored.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.id.cmp(&right.id))
    });

    scored
}

/// One page of a ranked search, as the response envelope needs it.
///
/// It exists so that [`MlService::page`] takes one value instead of five: the
/// five are the response's own fields, not independent parameters.
#[derive(Debug, Clone, Copy)]
struct PageWindow {
    limit: i64,
    offset: i64,
    total: i64,
    requested: SearchMode,
    applied: SearchMode,
}

/// The number of scored notes, as the page envelope counts them.
fn count(scored: &[ScoredNote]) -> i64 {
    i64::try_from(scored.len()).unwrap_or(i64::MAX)
}

/// Takes one page out of a ranked list.
fn slice(scored: Vec<ScoredNote>, limit: i64, offset: i64) -> Vec<ScoredNote> {
    let start = usize::try_from(offset.max(0)).unwrap_or(usize::MAX);
    let size = usize::try_from(limit.max(0)).unwrap_or(0);

    scored.into_iter().skip(start).take(size).collect()
}

/// The notes a match publishes: the hits that carry a similarity, up to `limit`.
///
/// The filter precedes the cut deliberately. `limit` bounds the number of
/// **matches**, so a note that is dropped must not consume a slot another note
/// would have filled; truncating first would answer a shorter list than the
/// request allows for no reason the client can see.
fn ranked_matches(scored: Vec<ScoredNote>, limit: i64) -> Vec<ScoredNote> {
    let mut matched: Vec<ScoredNote> = scored
        .into_iter()
        .filter(|hit| is_match(hit.score))
        .collect();

    matched.truncate(usize::try_from(limit).unwrap_or(0));

    matched
}

/// Rounds a similarity to the published four decimals.
fn round_score(value: f64) -> f64 {
    (value * SCORE_SCALE).round() / SCORE_SCALE
}

/// Validates the `limit` of a match.
///
/// `0`, a negative value and anything above 50 are refused naming the field:
/// it is a body field, so the answer is `422`, not a clamp. A search clamps
/// because its `limit` is a query parameter with a published envelope; a match
/// has no envelope and a bounded list.
fn validate_match_limit(limit: Option<i64>, messages: &Messages) -> AppResult<i64> {
    let limit = limit.unwrap_or(DEFAULT_MATCH_LIMIT);

    if !(1..=MAX_MATCH_LIMIT).contains(&limit) {
        return Err(crate::helpers::field_error(
            "limit",
            crate::config::messages::render(
                &messages.validation.too_long,
                &[MAX_MATCH_LIMIT.to_string()],
            ),
            messages,
        ));
    }

    Ok(limit)
}

/// The cosine similarity of two vectors, in `[-1, 1]`.
///
/// Both operands are unit length by construction, so this is exactly their dot
/// product; it is computed as a dot product anyway, so the function is correct
/// for a caller that has not normalised (the unit test does not). The
/// accumulation is `f64`: `vector` is `REAL[]`, and summing 384 `f32` products
/// in `f32` would let the last bits of the score depend on the order of the
/// addition.
///
/// `None` means the two cannot be compared: different lengths, an empty vector,
/// or a result that is not finite. A caller skips such a row rather than
/// inventing a score for it.
#[must_use]
pub fn cosine(left: &[f32], right: &[f32]) -> Option<f64> {
    if left.is_empty() || left.len() != right.len() {
        return None;
    }

    let dot = left
        .iter()
        .zip(right)
        .map(|(l, r)| f64::from(*l) * f64::from(*r))
        .sum::<f64>();

    let left_norm = crate::ml::embedder::l2_norm(left);
    let right_norm = crate::ml::embedder::l2_norm(right);

    if !dot.is_finite() || left_norm <= f64::EPSILON || right_norm <= f64::EPSILON {
        return None;
    }

    let similarity = dot / (left_norm * right_norm);

    if similarity.is_finite() {
        // Clamped to the mathematical range so an `f32` rounding cannot report
        // a cosine of 1.0000001.
        Some(similarity.clamp(-1.0, 1.0))
    } else {
        None
    }
}

/// The published score: the cosine clamped to `[0,1]`.
///
/// The clamp is not cosmetic. A negative similarity means the two texts are
/// opposed in this embedding space, which for a ranking is "not a match"; the
/// published range is `[0,1]`, so a client can render the value as a
/// proportion.
#[must_use]
pub fn score(left: &[f32], right: &[f32]) -> Option<f64> {
    cosine(left, right).map(|value| value.clamp(0.0, 1.0))
}

/// Whether a similarity is evidence of a match at all.
///
/// A cosine at or below zero means the two vectors are orthogonal (or opposed):
/// they share no direction, so the row carries no similarity to the query. The
/// zero is not a weak score; it is the **absence** of one. Publishing it would
/// turn "nothing in this corpus resembles the query" into a confident-looking
/// best match, which is worse than an empty page with `total: 0`
/// (`docs/features/ml/testing.md` §2.7 records the `ML-R09` control that
/// exposed it).
///
/// `semantic`, `hybrid` and `match` drop these hits. `lexical` does not call
/// this and is unchanged: it has no score, and every full-text match is a real
/// hit.
///
/// The cut is exactly zero and deliberately **not** a positive threshold. A
/// cosine of `0.01` and a cosine of `0.9` come from the same arithmetic, and no
/// quality evaluation exists to say which positive value would separate a
/// relevant note from an irrelevant one — the fifteen-query fixture measures
/// plumbing, not retrieval quality. Picking a positive number would invent a
/// product rule. Zero is the one boundary the geometry itself defines: the
/// point where the two vectors stop pointing in the same direction.
fn is_match(value: f64) -> bool {
    value > 0.0
}

/// The text a note is embedded from.
///
/// The title carries the subject and the body the detail, and the notes
/// full-text index weights them the same way (`A` and `B`), so the two modes see
/// the same text.
fn note_text(note: &Note) -> String {
    format!("{}\n{}", note.title, note.content)
}

/// The text an application is embedded from.
///
/// `role_title` and `description`, exactly as
/// [`docs/features/ml/architecture.md`](../../docs/features/ml/architecture.md)
/// §4.4 fixes it. The company name, the salary and the stage are not part of
/// the meaning of the role and would add noise to a short text.
fn application_text(application: &JobApplication) -> String {
    format!("{}\n{}", application.role_title, application.description)
}

/// Why one inference did not produce a vector, in the terms the response needs.
#[derive(Debug)]
enum EmbedFault {
    /// The call exceeded `ml.timeout_seconds`.
    Timeout,
    /// The embedder returned an error.
    Failed(MlError),
}

impl EmbedFault {
    /// The published degradation reason.
    ///
    /// This is a pure function on purpose: the mapping is asserted without a
    /// clock, because forcing a real timeout end to end would need a fourth
    /// `Embedder` implementation, which `ML-R03` forbids.
    fn reason(&self) -> DegradedReason {
        match self {
            Self::Timeout => DegradedReason::InferenceTimeout,
            Self::Failed(error) => DegradedReason::from_reason(error.code()),
        }
    }

    /// The error a match or a reindex answers with.
    ///
    /// Neither has a lexical fallback, so neither degrades: a timeout is `408`
    /// and an unusable model is `409`. The message never names the underlying
    /// fault — the cause is logged once, at start-up, or once per request at
    /// `debug`.
    fn into_error(self, messages: &Messages) -> AppError {
        match self {
            Self::Timeout => AppError::Timeout(messages.general.timeout.clone()),
            Self::Failed(_) => AppError::Conflict(messages.conflicts.ml_unavailable.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ml::embedder::{DEFAULT_DIMENSION, normalise};

    /// A unit vector built from a plain vector, so a test never exercises a
    /// shape the product cannot produce.
    fn unit(values: &[f32]) -> Vec<f32> {
        let mut vector = values.to_vec();
        normalise(&mut vector, values.len()).expect("the vector has a unit form");
        vector
    }

    #[test]
    fn cosine_similarity_is_symmetric_and_bounded() {
        let a = unit(&[1.0, 0.0, 0.0]);
        let b = unit(&[0.0, 1.0, 0.0]);
        let opposite = unit(&[-1.0, 0.0, 0.0]);

        // Identical vectors score 1.
        assert!((cosine(&a, &a).expect("comparable") - 1.0).abs() < 1e-9);
        // Orthogonal vectors score 0.
        assert!(cosine(&a, &b).expect("comparable").abs() < 1e-9);
        // Opposite vectors score -1.
        assert!((cosine(&a, &opposite).expect("comparable") + 1.0).abs() < 1e-9);

        // Symmetry, on a pair that is neither axis.
        let c = unit(&[1.0, 2.0, 3.0]);
        let d = unit(&[3.0, -2.0, 0.5]);
        assert!(
            (cosine(&c, &d).expect("comparable") - cosine(&d, &c).expect("comparable")).abs()
                < 1e-12
        );
    }

    #[test]
    fn a_published_score_is_clamped_to_the_unit_interval() {
        let a = unit(&[1.0, 0.0]);
        let opposite = unit(&[-1.0, 0.0]);

        // `REAL[]` holds single-precision values, so the assertion is a
        // tolerance, never an exact equality.
        assert!((score(&a, &a).expect("comparable") - 1.0).abs() < 1e-9);
        assert_eq!(score(&a, &opposite), Some(0.0));
    }

    #[test]
    fn a_similarity_at_or_below_zero_is_not_a_match() {
        // Orthogonal and opposed vectors carry no similarity: they must be
        // dropped rather than published as a `0.0` hit. `-0.0` is included
        // because `f64` distinguishes it from `0.0` by bit pattern only.
        assert!(!is_match(0.0));
        assert!(!is_match(-0.0));
        assert!(!is_match(-1.0));

        // Any positive similarity is a match. The cut is zero, not a positive
        // threshold nobody measured.
        assert!(is_match(f64::MIN_POSITIVE));
        assert!(is_match(0.0001));
        assert!(is_match(1.0));
    }

    #[test]
    fn a_match_drops_a_zero_scored_note_without_consuming_the_limit() {
        // The defect this closes: a note whose similarity is zero was published
        // as a match with `score: 0.0`, which is a confident-looking answer
        // that says the two texts share nothing.
        let scored = vec![
            ScoredNote {
                id: Uuid::from_u128(1),
                score: 0.4,
            },
            ScoredNote {
                id: Uuid::from_u128(2),
                score: 0.0,
            },
            ScoredNote {
                id: Uuid::from_u128(3),
                score: -0.0,
            },
            ScoredNote {
                id: Uuid::from_u128(4),
                score: 0.2,
            },
            ScoredNote {
                id: Uuid::from_u128(5),
                score: 0.1,
            },
        ];

        let published = ranked_matches(scored, 2);

        assert_eq!(
            published.len(),
            2,
            "the limit bounds the matches published, not the rows considered"
        );
        assert!(
            published.iter().all(|hit| hit.score > 0.0),
            "a zero-scored note must never be a match"
        );
        assert_eq!(published[0].id, Uuid::from_u128(1));
        assert_eq!(
            published[1].id,
            Uuid::from_u128(4),
            "the dropped notes must not push a positive note out of the limit"
        );
    }

    #[test]
    fn a_match_with_nothing_above_zero_publishes_an_empty_list() {
        // `matches: []` with `200` is the honest answer: a match has no
        // envelope to carry a `total`, and an empty list is well-formed.
        let scored = vec![
            ScoredNote {
                id: Uuid::from_u128(1),
                score: 0.0,
            },
            ScoredNote {
                id: Uuid::from_u128(2),
                score: -0.0,
            },
        ];

        assert!(ranked_matches(scored, 10).is_empty());
        assert!(ranked_matches(Vec::new(), 10).is_empty());
    }

    #[test]
    fn the_published_total_counts_only_hits_that_score_above_zero() {
        // The filter runs before `count`, so the envelope cannot claim a hit
        // the page does not hold.
        let scored = vec![
            ScoredNote {
                id: Uuid::from_u128(1),
                score: 0.4,
            },
            ScoredNote {
                id: Uuid::from_u128(2),
                score: 0.0,
            },
            ScoredNote {
                id: Uuid::from_u128(3),
                score: 0.05,
            },
        ];

        let published: Vec<ScoredNote> = scored
            .into_iter()
            .filter(|hit| is_match(hit.score))
            .collect();

        assert_eq!(count(&published), 2);
        assert!(
            published.iter().all(|hit| hit.score > 0.0),
            "a zero-scored hit must not survive the filter"
        );
    }

    #[test]
    fn vectors_of_different_lengths_are_not_comparable() {
        assert_eq!(cosine(&[1.0, 0.0], &[1.0, 0.0, 0.0]), None);
        assert_eq!(cosine(&[], &[]), None);
    }

    #[test]
    fn a_zero_vector_is_not_comparable() {
        // The write path refuses to store one, so this is a defensive branch:
        // a score for a degenerate row would be a division by zero.
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 0.0]), None);
    }

    #[test]
    fn a_vector_of_the_wrong_dimension_is_skipped() {
        let good = unit(&[1.0_f32; DEFAULT_DIMENSION]);
        let mut short = unit(&[1.0_f32; 4]);

        // The row of the wrong length is a stored row, not the query: it must
        // not panic and must not contribute a score.
        let rows = vec![
            StoredVector {
                source_id: Uuid::nil(),
                vector: good.clone(),
            },
            StoredVector {
                source_id: Uuid::from_u128(u128::MAX),
                vector: std::mem::take(&mut short),
            },
        ];

        let scored = rank(&rows, &good, DEFAULT_DIMENSION);

        assert_eq!(scored.len(), 1, "the short row must be skipped");
        assert_eq!(scored[0].id, Uuid::nil());
    }

    #[test]
    fn the_rank_is_total_and_orders_by_score_then_identifier() {
        let low = Uuid::from_u128(1);
        let high = Uuid::from_u128(2);

        let rows = vec![
            StoredVector {
                source_id: high,
                vector: unit(&[1.0, 0.0]),
            },
            StoredVector {
                source_id: low,
                vector: unit(&[1.0, 0.0]),
            },
        ];

        let scored = rank(&rows, &unit(&[1.0, 0.0]), 2);

        // Equal scores, so the identifier breaks the tie.
        assert_eq!(
            scored.iter().map(|hit| hit.id).collect::<Vec<_>>(),
            vec![low, high]
        );
        assert!(scored[0].score >= scored[1].score);
    }

    #[test]
    fn a_timeout_maps_to_the_inference_timeout_degradation() {
        // The mapping is asserted without a clock: the error variant in, the
        // published reason out.
        assert_eq!(
            EmbedFault::Timeout.reason(),
            DegradedReason::InferenceTimeout
        );
        assert_eq!(
            EmbedFault::Failed(MlError::Unavailable {
                reason: crate::ml::embedder::REASON_ML_DISABLED.to_string()
            })
            .reason(),
            DegradedReason::MlDisabled
        );
        assert_eq!(
            EmbedFault::Failed(MlError::Model {
                message: "unusable".to_string()
            })
            .reason(),
            DegradedReason::ModelMissing
        );

        // And the two endpoints that cannot degrade answer differently.
        let messages = crate::config::messages::test_messages();
        assert_eq!(
            EmbedFault::Timeout.into_error(&messages).status(),
            axum::http::StatusCode::REQUEST_TIMEOUT
        );
        assert_eq!(
            EmbedFault::Failed(MlError::Model {
                message: "unusable".to_string()
            })
            .into_error(&messages)
            .status(),
            axum::http::StatusCode::CONFLICT
        );
    }

    #[test]
    fn a_score_is_rounded_to_four_decimals() {
        assert_eq!(round_score(0.812_345_67), 0.8123);
        assert_eq!(round_score(0.999_99), 1.0);
        assert_eq!(round_score(0.0), 0.0);
    }

    #[test]
    fn a_match_limit_outside_the_published_range_is_refused() {
        let messages = crate::config::messages::test_messages();

        assert_eq!(
            validate_match_limit(None, &messages).unwrap(),
            DEFAULT_MATCH_LIMIT
        );
        assert_eq!(validate_match_limit(Some(1), &messages).unwrap(), 1);
        assert_eq!(
            validate_match_limit(Some(MAX_MATCH_LIMIT), &messages).unwrap(),
            MAX_MATCH_LIMIT
        );

        for refused in [0, -1, MAX_MATCH_LIMIT + 1] {
            let error = validate_match_limit(Some(refused), &messages)
                .expect_err("a limit outside 1..=50 must be refused");
            assert_eq!(error.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(error.body().error.details[0].field, "limit");
        }
    }

    #[test]
    fn the_slice_is_bounded_and_an_offset_past_the_end_is_an_empty_page() {
        let scored: Vec<ScoredNote> = (0..5)
            .map(|index| ScoredNote {
                id: Uuid::from_u128(index),
                score: 1.0,
            })
            .collect();

        assert_eq!(slice(scored.clone(), 2, 1).len(), 2);
        assert!(slice(scored.clone(), 10, 50).is_empty());
        assert_eq!(slice(scored, 10, 0).len(), 5);
    }
}
