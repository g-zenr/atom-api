//! Public contracts for the machine-learning endpoints.
//!
//! The shapes are [`docs/features/ml/api.md`](../../docs/features/ml/api.md)
//! §2–§5. Three properties are deliberately visible in the types rather than
//! left to the handler:
//!
//! * `score` is a **number**, never a string: it is a similarity, not money.
//!   It is `null` exactly in lexical mode, which is a scored mode's absence.
//! * `degraded_reason` is `null` exactly when `degraded` is `false`. The two
//!   cannot be set independently, because both are built by the service from
//!   one value.
//! * `MatchMode` has no `lexical` variant, so a lexical match is refused by the
//!   deserialiser (`422`) rather than by a branch a later change could forget.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::notes::dto::NoteResponse;
use crate::pagination::Pagination;

/// The longest accepted search query, in characters.
///
/// A query is not stored, so it has no database constraint to mirror. The bound
/// is the one the notes search uses, so the two endpoints cannot disagree about
/// what a search term may be.
pub const MAX_QUERY_LENGTH: usize = crate::helpers::MAX_SEARCH_LENGTH;

/// The number of ranked notes a match returns when the client does not say.
pub const DEFAULT_MATCH_LIMIT: i64 = 10;

/// The largest `limit` a match accepts.
pub const MAX_MATCH_LIMIT: i64 = 50;

/// The three search modes.
///
/// `hybrid` is the default: it is the mode the requirement describes, and the
/// one a client that does not choose gets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    /// PostgreSQL full-text match. No model.
    Lexical,
    /// Cosine similarity against the caller's embeddings.
    Semantic,
    /// A bounded lexical candidate set re-ranked by cosine similarity.
    #[default]
    Hybrid,
}

/// The modes a match accepts.
///
/// `lexical` is absent on purpose: a match without a model has no meaning, so
/// the request is refused rather than answered with a ranking that ranks
/// nothing ([`docs/features/ml/api.md`](../../docs/features/ml/api.md) §4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    /// Cosine similarity between the application and the caller's notes.
    Semantic,
    /// The same ranking; the name records what the client asked for.
    #[default]
    Hybrid,
}

/// Why a scored response came back lexical.
///
/// It is a closed enum with exactly three values: adding a fourth is a contract
/// change in `api.md`, `conventions.md` §17 and the OpenAPI schema together
/// (`docs/features/ml/architecture.md` §12.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DegradedReason {
    /// `ml.enabled` is `false`.
    MlDisabled,
    /// The weights, the vocabulary or the inference itself could not be used.
    ModelMissing,
    /// One inference exceeded `ml.timeout_seconds`.
    InferenceTimeout,
}

impl DegradedReason {
    /// The published value, which is also the stable client code.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MlDisabled => super::embedder::REASON_ML_DISABLED,
            Self::ModelMissing => super::embedder::REASON_MODEL_MISSING,
            Self::InferenceTimeout => "inference_timeout",
        }
    }

    /// Maps an [`super::embedder::EmbedderStatus`] reason onto the published
    /// enum.
    ///
    /// The `Disabled` status carries the same vocabulary, so this is a
    /// translation from the embedder's string to the contract's enum. An
    /// unknown value maps to `ModelMissing` rather than failing: the embedder
    /// being unusable is exactly what that code means, and a degraded answer
    /// must never become a `500` because of a wording change.
    #[must_use]
    pub fn from_reason(reason: &str) -> Self {
        match reason {
            super::embedder::REASON_ML_DISABLED => Self::MlDisabled,
            "inference_timeout" => Self::InferenceTimeout,
            _ => Self::ModelMissing,
        }
    }
}

/// The body of `POST /api/v1/notes/search`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SearchRequest {
    /// The text to search for. Trimmed, 1 to 200 characters.
    #[schema(example = "container orchestration")]
    pub query: String,
    /// The mode to apply. Defaults to `hybrid`.
    #[serde(default)]
    pub mode: SearchMode,
}

/// The body of `POST /api/v1/job-applications/{application_id}/match`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct MatchRequest {
    /// The mode to apply. Defaults to `hybrid`.
    #[serde(default)]
    pub mode: MatchMode,
    /// How many ranked notes to return. Defaults to 10, minimum 1, maximum 50.
    #[serde(default)]
    pub limit: Option<i64>,
}

/// One ranked note in a search response.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SearchHit {
    /// The note, in the shape the notes feature already publishes.
    pub note: NoteResponse,
    /// The cosine similarity in `[0,1]`, rounded to four decimals. `null` in
    /// lexical mode.
    pub score: Option<f64>,
}

/// The response body of `POST /api/v1/notes/search`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SearchResponse {
    /// What the client asked for.
    pub requested_mode: SearchMode,
    /// What was applied. Differs from `requested_mode` only when degraded.
    pub mode: SearchMode,
    /// `true` when a scored mode was requested and lexical results were
    /// returned.
    pub degraded: bool,
    /// Why the response is lexical. `null` exactly when `degraded` is `false`.
    pub degraded_reason: Option<DegradedReason>,
    /// The notes on this page, in `score DESC, note id ASC` order when the mode
    /// is scored, and in the notes order when it is lexical.
    pub items: Vec<SearchHit>,
    /// The shared pagination envelope.
    pub pagination: Pagination,
}

/// One ranked note in a match response.
///
/// It carries the title rather than the whole note: a match is a short ranked
/// list, and the client that asked for one already has the note bodies.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MatchHit {
    /// The note identifier.
    pub note_id: Uuid,
    /// The note title.
    pub title: String,
    /// The cosine similarity in `[0,1]`, rounded to four decimals.
    pub score: f64,
}

/// The response body of `POST /api/v1/job-applications/{application_id}/match`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MatchResponse {
    /// The application the notes were ranked against.
    pub application_id: Uuid,
    /// The mode that was applied.
    pub mode: MatchMode,
    /// The ranked notes, `score DESC, note id ASC`, bounded by the request's
    /// `limit`. Not paginated.
    pub matches: Vec<MatchHit>,
}

/// The response body of `POST /api/v1/ml/reindex`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ReindexResponse {
    /// The embedder that produced the vectors.
    pub model_id: String,
    /// The vector length it produces.
    pub dimension: usize,
    /// Rows written or replaced by this run.
    pub indexed: i64,
    /// Sources already embedded with the current model and dimension.
    pub skipped: i64,
    /// Orphaned rows removed.
    pub deleted: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_degradation_reasons_are_the_published_three() {
        assert_eq!(DegradedReason::MlDisabled.as_str(), "ml_disabled");
        assert_eq!(DegradedReason::ModelMissing.as_str(), "model_missing");
        assert_eq!(
            DegradedReason::InferenceTimeout.as_str(),
            "inference_timeout"
        );
    }

    #[test]
    fn every_reason_round_trips_through_its_string() {
        for reason in [
            DegradedReason::MlDisabled,
            DegradedReason::ModelMissing,
            DegradedReason::InferenceTimeout,
        ] {
            assert_eq!(DegradedReason::from_reason(reason.as_str()), reason);
        }
    }

    #[test]
    fn an_unknown_reason_is_reported_as_a_missing_model() {
        // A degraded answer must never become a server error because of a
        // wording change: an unusable embedder is what `model_missing` means.
        assert_eq!(
            DegradedReason::from_reason("something_else"),
            DegradedReason::ModelMissing
        );
    }

    #[test]
    fn the_search_mode_defaults_to_hybrid() {
        let request: SearchRequest =
            serde_json::from_str(r#"{"query":"anything"}"#).expect("the body parses");

        assert_eq!(request.mode, SearchMode::Hybrid);
    }

    #[test]
    fn an_unknown_search_mode_is_refused_by_the_deserialiser() {
        assert!(serde_json::from_str::<SearchRequest>(r#"{"query":"x","mode":"fuzzy"}"#).is_err());
    }

    #[test]
    fn a_match_refuses_the_lexical_mode() {
        // The enum has no `lexical` variant, so the refusal is a parse error
        // rather than a branch that a later change could forget.
        assert!(serde_json::from_str::<MatchRequest>(r#"{"mode":"lexical"}"#).is_err());
        assert_eq!(
            serde_json::from_str::<MatchRequest>(r"{}")
                .expect("an empty body applies the defaults")
                .mode,
            MatchMode::Hybrid
        );
    }

    #[test]
    fn the_degraded_fields_serialise_as_documented() {
        let response = SearchResponse {
            requested_mode: SearchMode::Hybrid,
            mode: SearchMode::Lexical,
            degraded: true,
            degraded_reason: Some(DegradedReason::MlDisabled),
            items: Vec::new(),
            pagination: Pagination::new(10, 0, 0),
        };

        let body = serde_json::to_value(&response).expect("the response serialises");

        assert_eq!(body["requested_mode"], "hybrid");
        assert_eq!(body["mode"], "lexical");
        assert_eq!(body["degraded"], true);
        assert_eq!(body["degraded_reason"], "ml_disabled");
    }
}
