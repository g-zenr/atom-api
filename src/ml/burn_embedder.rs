//! The Burn encoder for `sentence-transformers/all-MiniLM-L6-v2`.
//!
//! This is the implementation [`ADR-0008`](../../docs/decisions/ADR-0008-local-pure-rust-ml.md)
//! describes: a six-layer BERT encoder in pure Rust on the `flex` CPU backend,
//! with the weights loaded from a local SafeTensors file and the vocabulary from
//! the `vocab.txt` beside it. It performs **no I/O after construction** and makes
//! no network call at any point.
//!
//! # The network
//!
//! ```text
//! token ids ─► word + position + token-type embeddings ─► LayerNorm
//!           ─► 6 × [ 12-head self-attention ─► add + LayerNorm
//!                   384→1536→384 feed-forward ─► add + LayerNorm ]
//!           ─► masked mean over the sequence ─► L2 normalise ─► 384 floats
//! ```
//!
//! Two details are easy to get wrong, and both are load-bearing:
//!
//! * **The pooling is the model's own, not `pooler.dense`.** A
//!   sentence-transformers MiniLM is mean pooling over the last hidden state,
//!   masked by the attention mask, then L2 normalisation. The Hugging Face
//!   `pooler.dense` pair is in the file and is deliberately **not loaded**; using
//!   it was the spike's error and it produces vectors with a different meaning.
//! * **The attention width is 32, not 64.** `hidden_size` 384 with 12 heads gives
//!   384 / 12 = 32. `docs/features/ml/architecture.md` §12.3 says "head size 64",
//!   which is a numeric slip in that sentence; the model's `config.json` and the
//!   scale of the weights decide, and both say 32.
//!
//! # Loading
//!
//! The hundred-odd tensors are applied with `burn_store`'s
//! [`SafetensorsStore`](burn::store::SafetensorsStore) and an explicit key
//! mapping from the Hugging Face names. The spike's hand-written header parser is
//! gone: it read three tensors, and a hundred is where a hand parser stops being
//! adequate. Every module parameter must be found in the file — a missing key is
//! a load failure, never a zeroed weight — and every file tensor that is not used
//! must be one this model legitimately does not need.
//!
//! # The frozen spike
//!
//! `src/bin/atom-ml-spike.rs` is the measurement record behind `ADR-0008` §8 and
//! is not edited. It calls [`BurnEmbedder::from_safetensors`] and
//! [`BurnEmbedder::synthetic`]; both are kept, and the second builds the same
//! encoder from seeded weights so that binary still compiles and still measures a
//! Burn forward pass. Its vectors are meaningless and the type says so through
//! [`WeightSource`].

use std::path::Path;

use burn::module::{Module, ModuleMapper, Param};
use burn::nn::Initializer;
use burn::prelude::*;
use burn::store::{ModuleSnapshot, SafetensorsStore};
use burn::tensor::activation::{gelu, softmax};

use super::embedder::{Embedder, EmbedderStatus, MlError, normalise};
use super::tokenizer::WordPieceTokenizer;

/// The identifier stored with every vector this implementation produces.
///
/// It names the encoder and the pooling this file fixes. The spike's
/// `burn-minilm-l6-v2-spike-v1` is retired and is never reused: it named an
/// unnormalised vector built from the wrong pooling, so a row written by it must
/// not be mistaken for one written here
/// ([`ADR-0008`](../../docs/decisions/ADR-0008-local-pure-rust-ml.md) §9).
pub const MODEL_ID: &str = "burn-all-minilm-l6-v2-v2";

/// The hidden width of the model.
const HIDDEN: usize = 384;

/// The number of attention heads.
const HEADS: usize = 12;

/// The width of one head: 384 / 12.
const HEAD_DIM: usize = HIDDEN / HEADS;

/// The feed-forward width.
const INTERMEDIATE: usize = 1_536;

/// The number of transformer layers.
const LAYERS: usize = 6;

/// `layer_norm_eps` from the model's `config.json`.
const LAYER_NORM_EPS: f64 = 1e-12;

/// The rows of the position table (`max_position_embeddings`).
const MAX_POSITIONS: usize = 512;

/// The rows of the token-type table (`type_vocab_size`).
const TYPE_VOCAB: usize = 2;

/// The file tensors this encoder deliberately does not load.
///
/// `position_ids` is a constant the forward pass builds itself, and the pooler is
/// the wrong pooling for this model. Anything else in the file would mean the key
/// mapping is incomplete, so it is refused rather than ignored.
const IGNORED_TENSORS: [&str; 3] = [
    "embeddings.position_ids",
    "pooler.dense.weight",
    "pooler.dense.bias",
];

/// Where the vectors came from. The report must state this, because it is the
/// difference between measuring an embedding and measuring the plumbing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeightSource {
    /// Tensors read from a real SafeTensors file on disk.
    SafeTensors,
    /// The file was absent and the tensors are a seeded stand-in.
    Synthetic,
}

impl WeightSource {
    /// A short description for a report.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::SafeTensors => "real SafeTensors file",
            Self::Synthetic => "synthetic (generated in-process)",
        }
    }
}

/// A Burn `flex` encoder holding the model and its vocabulary.
pub struct BurnEmbedder {
    model: Bert,
    tokenizer: WordPieceTokenizer,
    device: Device,
    dimension: usize,
    source: WeightSource,
}

impl BurnEmbedder {
    /// Load a SafeTensors file and the `vocab.txt` beside it.
    ///
    /// This is the constructor the frozen spike calls. The configured path —
    /// `ml.tokenizer_path` — goes through [`Self::from_artifacts`] instead.
    ///
    /// # Errors
    ///
    /// Returns [`MlError::Model`] when `dimension` is not this model's width, or
    /// when the weights, the sibling vocabulary or any module parameter cannot
    /// be loaded.
    pub fn from_safetensors(path: &Path, dimension: usize) -> Result<Self, MlError> {
        let vocabulary = path.with_file_name("vocab.txt");

        Self::from_artifacts(path, &vocabulary, dimension)
    }

    /// Load the two model artifacts and build the encoder.
    ///
    /// # Errors
    ///
    /// Returns [`MlError::Model`] when either file cannot be read, when the
    /// vocabulary lacks a special token, when the file does not contain a tensor
    /// the network needs, or when `dimension` is not 384. A failed load is a
    /// `Disabled` embedder to the caller, never a start-up failure
    /// ([`ADR-0008`](../../docs/decisions/ADR-0008-local-pure-rust-ml.md) §8.3).
    pub fn from_artifacts(
        weights: &Path,
        vocabulary: &Path,
        dimension: usize,
    ) -> Result<Self, MlError> {
        if dimension != HIDDEN {
            return Err(MlError::Model {
                message: format!(
                    "ml.dimension is {dimension}, but all-MiniLM-L6-v2 produces {HIDDEN} values"
                ),
            });
        }

        let tokenizer = WordPieceTokenizer::from_file(vocabulary)?;
        let device = Device::flex();

        // Every parameter starts lazy and is replaced by the file's value. A
        // missing key therefore fails the load rather than leaving a zero.
        let mut model = Bert::new(&device);

        let mut store = SafetensorsStore::from_file(weights)
            // Rust cannot name a field `self`: `attention.self.query` is mapped
            // to `attention.self_attention.query`, and the three LayerNorm
            // groups are mapped to a snake-case field name.
            .with_key_remapping(
                r"^encoder\.layer\.(\d+)\.attention\.self\.",
                "encoder.layer.$1.attention.self_attention.",
            )
            .with_key_remapping(
                r"^encoder\.layer\.(\d+)\.attention\.output\.LayerNorm\.",
                "encoder.layer.$1.attention.output.layer_norm.",
            )
            .with_key_remapping(
                r"^encoder\.layer\.(\d+)\.output\.LayerNorm\.",
                "encoder.layer.$1.output.layer_norm.",
            )
            .with_key_remapping(r"^embeddings\.LayerNorm\.", "embeddings.layer_norm.");

        let applied = model
            .load_from(&mut store)
            .map_err(|error| MlError::Model {
                message: format!("cannot load the SafeTensors weights: {error}"),
            })?;

        if !applied.errors.is_empty() {
            let first = &applied.errors[0];
            return Err(MlError::Model {
                message: format!(
                    "{} tensor(s) could not be applied; the first is: {first}",
                    applied.errors.len()
                ),
            });
        }

        for name in &applied.unused {
            if !IGNORED_TENSORS.contains(&name.as_str()) {
                return Err(MlError::Model {
                    message: format!(
                        "the weights contain {name}, which this encoder does not load; \
                         a key mapping is incomplete"
                    ),
                });
            }
        }

        Ok(Self {
            model,
            tokenizer,
            device,
            dimension,
            source: WeightSource::SafeTensors,
        })
    }

    /// Build the same encoder from seeded stand-in weights.
    ///
    /// Used only by the frozen `atom-ml-spike` when no model file is present, so
    /// the binary still measures the Burn `flex` code path. The shapes, the
    /// number of parameters and the arithmetic are the real ones; the values are
    /// not, so the vectors are meaningless and [`Self::weight_source`] says so.
    #[must_use]
    pub fn synthetic(dimension: usize, seed: u64) -> Self {
        let device = Device::flex();
        let model = Bert::new(&device).map(&mut SyntheticWeights::new(seed));

        Self {
            model,
            tokenizer: WordPieceTokenizer::synthetic(5),
            device,
            dimension,
            source: WeightSource::Synthetic,
        }
    }

    /// Where the tensors came from.
    #[must_use]
    pub fn weight_source(&self) -> WeightSource {
        self.source
    }

    /// The number of rows in the token table.
    #[must_use]
    pub fn vocab_size(&self) -> usize {
        self.tokenizer.vocabulary_size()
    }

    /// The tokenizer this encoder reads text with.
    #[must_use]
    pub fn tokenizer(&self) -> &WordPieceTokenizer {
        &self.tokenizer
    }
}

impl Embedder for BurnEmbedder {
    fn model_id(&self) -> &str {
        MODEL_ID
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn status(&self) -> EmbedderStatus {
        EmbedderStatus::Ready
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>, MlError> {
        let encoded = self.tokenizer.tokenize(text);
        let mut vector = self.model.forward(&encoded, &self.device)?;

        // The write-time invariant of ADR-0008 §9: one place normalises, so no
        // path can return an unnormalised vector.
        normalise(&mut vector, self.dimension)?;

        Ok(vector)
    }
}

// ---------------------------------------------------------------------------
// The network
// ---------------------------------------------------------------------------

/// The BERT body: embeddings, six layers, and nothing else.
///
/// Field names are the Hugging Face path segments, so the remapping above is the
/// only place the two naming conventions meet.
#[derive(Module, Debug)]
struct Bert {
    embeddings: Embeddings,
    encoder: Encoder,
}

impl Bert {
    fn new(device: &Device) -> Self {
        Self {
            embeddings: Embeddings::new(device),
            encoder: Encoder::new(device),
        }
    }

    /// Runs one sequence and returns the pooled, **unnormalised** vector.
    fn forward(
        &self,
        encoded: &super::tokenizer::TokenizedText,
        device: &Device,
    ) -> Result<Vec<f32>, MlError> {
        let sequence = encoded.ids.len();
        if sequence == 0 || sequence > MAX_POSITIONS {
            return Err(MlError::Model {
                message: format!(
                    "the tokenizer produced {sequence} tokens, which the model cannot encode"
                ),
            });
        }

        let ids: Tensor<1, Int> = Tensor::from_data(
            TensorData::new(encoded.ids.clone(), Shape::new([sequence])),
            device,
        );
        let positions: Tensor<1, Int> = Tensor::arange(0..sequence as i64, device);
        let types: Tensor<1, Int> = Tensor::zeros([sequence], device);
        let mask: Tensor<3> = Tensor::from_data(
            TensorData::new(encoded.attention_mask.clone(), Shape::new([1, sequence, 1])),
            device,
        );

        let word = self.embeddings.word_embeddings.lookup(ids);
        let position = self.embeddings.position_embeddings.lookup(positions);
        let token_type = self.embeddings.token_type_embeddings.lookup(types);

        let mut hidden = self
            .embeddings
            .layer_norm
            .forward((word + position + token_type).reshape([1, sequence, HIDDEN]));

        for layer in &self.encoder.layer {
            hidden = layer.forward(hidden);
        }

        // Masked mean pooling. The mask is all ones for a single unpadded
        // sequence, which is the only shape this embedder produces, but the
        // division is by the mask's sum rather than by `sequence` so a padded
        // batch would pool correctly instead of diluting the vector with zeros.
        let pooled = (hidden * mask.clone()).sum_dim(1) / mask.sum_dim(1);
        let values = pooled
            .reshape([HIDDEN])
            .into_data()
            .try_into_vec::<f32>()
            .map_err(|error| MlError::Model {
                message: format!("cannot read the pooled output: {error}"),
            })?;

        Ok(values)
    }
}

/// The embedding block.
#[derive(Module, Debug)]
struct Embeddings {
    word_embeddings: EmbeddingTable,
    position_embeddings: EmbeddingTable,
    token_type_embeddings: EmbeddingTable,
    layer_norm: Norm,
}

impl Embeddings {
    fn new(device: &Device) -> Self {
        Self {
            word_embeddings: EmbeddingTable::new(30_522, HIDDEN, device),
            position_embeddings: EmbeddingTable::new(MAX_POSITIONS, HIDDEN, device),
            token_type_embeddings: EmbeddingTable::new(TYPE_VOCAB, HIDDEN, device),
            layer_norm: Norm::new(HIDDEN, device),
        }
    }
}

/// A lookup table, named so that its parameter is `weight`.
#[derive(Module, Debug)]
struct EmbeddingTable {
    weight: Param<Tensor<2>>,
}

impl EmbeddingTable {
    fn new(rows: usize, columns: usize, device: &Device) -> Self {
        Self {
            weight: Initializer::Zeros.init([rows, columns], device),
        }
    }

    fn lookup(&self, ids: Tensor<1, Int>) -> Tensor<2> {
        self.weight.val().select(0, ids)
    }
}

/// One linear projection, in the file's `[out, in]` orientation.
///
/// The weights are **not** transposed at load time, which is why a
/// `Tensor<2>` and an explicit `matmul(weight.transpose())` are used rather than
/// a Burn `Linear`: the file stores `[d_output, d_input]`, and a square projection
/// loaded the wrong way round would silently compute a different function.
#[derive(Module, Debug)]
struct Projection {
    weight: Param<Tensor<2>>,
    bias: Param<Tensor<1>>,
}

impl Projection {
    fn new(d_input: usize, d_output: usize, device: &Device) -> Self {
        Self {
            weight: Initializer::Zeros.init([d_output, d_input], device),
            bias: Initializer::Zeros.init([d_output], device),
        }
    }

    fn forward(&self, input: Tensor<3>) -> Tensor<3> {
        let weight = self.weight.val();
        let [d_output, d_input] = weight.dims();
        // The file stores `[d_output, d_input]`; `y = x · Wᵀ + b`, and the
        // lifted rank makes it a batched matrix product with the sequence as
        // the batch axis.
        let weight = weight.transpose().reshape([1, d_input, d_output]);
        let bias = self.bias.val().reshape([1, 1, d_output]);

        input.matmul(weight) + bias
    }
}

/// Layer normalisation over the last dimension.
///
/// `var_mean_bias` is the biased variance PyTorch's `LayerNorm` uses (it divides
/// by N, not N−1), and the epsilon is the model's `1e-12`.
#[derive(Module, Debug)]
struct Norm {
    weight: Param<Tensor<1>>,
    bias: Param<Tensor<1>>,
}

impl Norm {
    fn new(dimension: usize, device: &Device) -> Self {
        Self {
            weight: Initializer::Ones.init([dimension], device),
            bias: Initializer::Zeros.init([dimension], device),
        }
    }

    fn forward(&self, input: Tensor<3>) -> Tensor<3> {
        let (variance, mean) = input.clone().var_mean_bias(2);
        let normalised = (input - mean) / (variance + LAYER_NORM_EPS).sqrt();

        // A `[hidden]` gain has to be lifted to the rank of the activations:
        // Burn's arithmetic is same-rank with broadcasting, not implicit
        // rank-raising.
        let weight = self.weight.val();
        let hidden = weight.dims()[0];
        let gain = weight.reshape([1, 1, hidden]);
        let shift = self.bias.val().reshape([1, 1, hidden]);

        normalised * gain + shift
    }
}

/// The stack of encoder layers.
#[derive(Module, Debug)]
struct Encoder {
    layer: Vec<BertLayer>,
}

impl Encoder {
    fn new(device: &Device) -> Self {
        Self {
            layer: (0..LAYERS).map(|_| BertLayer::new(device)).collect(),
        }
    }
}

/// One transformer layer: attention, then the feed-forward pair.
#[derive(Module, Debug)]
struct BertLayer {
    attention: Attention,
    intermediate: Intermediate,
    output: Output,
}

impl BertLayer {
    fn new(device: &Device) -> Self {
        Self {
            attention: Attention::new(device),
            intermediate: Intermediate::new(device),
            output: Output::new(device),
        }
    }

    fn forward(&self, hidden: Tensor<3>) -> Tensor<3> {
        let attended = self.attention.forward(hidden.clone());
        let hidden = self.attention.output.layer_norm.forward(hidden + attended);

        let intermediate = gelu(self.intermediate.dense.forward(hidden.clone()));
        let projected = self.output.dense.forward(intermediate);

        self.output.layer_norm.forward(hidden + projected)
    }
}

/// Multi-head self-attention and its output projection.
#[derive(Module, Debug)]
struct Attention {
    self_attention: SelfAttention,
    output: AttentionOutput,
}

impl Attention {
    fn new(device: &Device) -> Self {
        Self {
            self_attention: SelfAttention::new(device),
            output: AttentionOutput::new(device),
        }
    }

    fn forward(&self, hidden: Tensor<3>) -> Tensor<3> {
        let [batch, sequence, _] = hidden.dims();

        let query = self.self_attention.query.forward(hidden.clone());
        let key = self.self_attention.key.forward(hidden.clone());
        let value = self.self_attention.value.forward(hidden);

        let query = query
            .reshape([batch, sequence, HEADS, HEAD_DIM])
            .permute([0, 2, 1, 3]);
        let key = key
            .reshape([batch, sequence, HEADS, HEAD_DIM])
            .permute([0, 2, 1, 3]);
        let value = value
            .reshape([batch, sequence, HEADS, HEAD_DIM])
            .permute([0, 2, 1, 3]);

        // The scale is 1/sqrt(head_dim), and the transpose is over the sequence
        // and head axes, not a full `transpose()` of a four-dimensional tensor.
        let scores = query
            .matmul(key.swap_dims(2, 3))
            .div_scalar((HEAD_DIM as f64).sqrt());
        let weights = softmax(scores, 3);

        let context = weights
            .matmul(value)
            .permute([0, 2, 1, 3])
            .reshape([batch, sequence, HIDDEN]);

        self.output.dense.forward(context)
    }
}

/// The query, key and value projections of one layer.
#[derive(Module, Debug)]
struct SelfAttention {
    query: Projection,
    key: Projection,
    value: Projection,
}

impl SelfAttention {
    fn new(device: &Device) -> Self {
        Self {
            query: Projection::new(HIDDEN, HIDDEN, device),
            key: Projection::new(HIDDEN, HIDDEN, device),
            value: Projection::new(HIDDEN, HIDDEN, device),
        }
    }
}

/// The projection and normalisation after the attention block.
#[derive(Module, Debug)]
struct AttentionOutput {
    dense: Projection,
    layer_norm: Norm,
}

impl AttentionOutput {
    fn new(device: &Device) -> Self {
        Self {
            dense: Projection::new(HIDDEN, HIDDEN, device),
            layer_norm: Norm::new(HIDDEN, device),
        }
    }
}

/// The feed-forward pair, 384 → 1536.
#[derive(Module, Debug)]
struct Intermediate {
    dense: Projection,
}

impl Intermediate {
    fn new(device: &Device) -> Self {
        Self {
            dense: Projection::new(HIDDEN, INTERMEDIATE, device),
        }
    }
}

/// The feed-forward output projection and its normalisation.
#[derive(Module, Debug)]
struct Output {
    dense: Projection,
    layer_norm: Norm,
}

impl Output {
    fn new(device: &Device) -> Self {
        Self {
            dense: Projection::new(INTERMEDIATE, HIDDEN, device),
            layer_norm: Norm::new(HIDDEN, device),
        }
    }
}

// ---------------------------------------------------------------------------
// Seeded stand-in weights
// ---------------------------------------------------------------------------

/// Fills every float parameter with reproducible, small, non-zero values.
///
/// Used by [`BurnEmbedder::synthetic`] only. A zero LayerNorm gain would pool to a
/// zero vector, which has no unit form, so the stand-in is deliberately random
/// rather than zero.
struct SyntheticWeights {
    state: u64,
}

impl SyntheticWeights {
    fn new(seed: u64) -> Self {
        Self { state: seed | 1 }
    }

    /// splitmix64, so the stand-in is reproducible without an RNG crate.
    fn next(&mut self) -> f32 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;

        // 24 bits of mantissa, mapped onto [-0.05, 0.05).
        let unit = (z >> 40) as f32 / 16_777_216.0;
        (unit * 2.0 - 1.0) * 0.05
    }
}

impl ModuleMapper for SyntheticWeights {
    fn map_float<const D: usize>(&mut self, param: Param<Tensor<D>>) -> Param<Tensor<D>> {
        let shape = {
            let value = param.val();
            value.shape()
        };
        let device = param.val().device();
        let count = shape.num_elements();

        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(self.next());
        }

        Param::from_tensor(Tensor::from_data(TensorData::new(values, shape), &device))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ml::embedder::{DEFAULT_DIMENSION, UNIT_NORM_TOLERANCE, l2_norm};

    #[test]
    fn the_encoded_model_reports_the_documented_identifier() {
        let embedder = BurnEmbedder::synthetic(DEFAULT_DIMENSION, 0x5EED);

        assert_eq!(embedder.model_id(), MODEL_ID);
        assert_eq!(embedder.model_id(), "burn-all-minilm-l6-v2-v2");
        assert!(!embedder.model_id().contains("spike"));
    }

    #[test]
    fn the_encoded_model_is_ready_and_has_the_configured_dimension() {
        let embedder = BurnEmbedder::synthetic(DEFAULT_DIMENSION, 0x5EED);

        assert!(embedder.status().is_ready());
        assert_eq!(embedder.dimension(), DEFAULT_DIMENSION);
        assert_eq!(embedder.weight_source(), WeightSource::Synthetic);
    }

    #[test]
    fn the_encoder_produces_a_unit_vector() {
        let embedder = BurnEmbedder::synthetic(DEFAULT_DIMENSION, 0x5EED);

        for text in [
            "",
            "kubernetes rollout",
            "a sentence with several words in it",
            "reimbursement claim for the Berlin conference travel",
        ] {
            let vector = embedder.embed(text).expect("the synthetic encoder runs");
            let norm = l2_norm(&vector);

            assert_eq!(vector.len(), DEFAULT_DIMENSION);
            assert!(
                (1.0 - norm).abs() <= UNIT_NORM_TOLERANCE,
                "the vector for {text:?} has norm {norm}, not 1"
            );
        }
    }

    #[test]
    fn the_encoder_is_deterministic_and_input_dependent() {
        let embedder = BurnEmbedder::synthetic(DEFAULT_DIMENSION, 0x5EED);

        let first = embedder.embed("a note about reimbursements").expect("runs");
        let second = embedder.embed("a note about reimbursements").expect("runs");
        let other = embedder.embed("an unrelated sentence").expect("runs");

        assert_eq!(first, second, "the same input must produce the same vector");
        assert_ne!(
            first, other,
            "a different input must produce a different vector"
        );
    }

    #[test]
    fn a_dimension_other_than_the_models_is_refused() {
        // The model is fixed at 384; `ml.dimension` cannot make it produce 256.
        // This is the load path, so it must fail rather than silently truncate.
        let error = BurnEmbedder::from_artifacts(
            Path::new("does-not-exist.safetensors"),
            Path::new("does-not-exist-vocab.txt"),
            256,
        )
        .err()
        .expect("a dimension the model cannot produce must be refused");

        assert!(matches!(error, MlError::Model { .. }));
    }

    #[test]
    fn a_missing_vocabulary_is_a_load_failure() {
        let error = BurnEmbedder::from_artifacts(
            Path::new("does-not-exist.safetensors"),
            Path::new("does-not-exist-vocab.txt"),
            DEFAULT_DIMENSION,
        )
        .err()
        .expect("a missing vocabulary must fail the load");

        assert!(matches!(error, MlError::Model { .. }));
    }
}
