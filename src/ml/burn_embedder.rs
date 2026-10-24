//! Burn on the `flex` CPU backend — the spike's measurement subject.
//!
//! **Scope.** This is the smallest network that exercises the code path an
//! endpoint would use: an embedding lookup over a 30522×384 table, a mean-pool
//! over the sequence, and a 384→384 projection with a bias. When a real
//! SafeTensors file is supplied, the lookup table and the projection use that
//! file's **real** `embeddings.word_embeddings.weight` and `pooler.dense.*`
//! tensors. It is **not** the MiniLM encoder: the six transformer layers and
//! the WordPiece tokenizer are later work, so the latency measured here is a
//! **lower bound** on a real sentence-embedding forward pass, and the vectors
//! carry no semantic quality.
//!
//! [`ADR-0008`](../../docs/decisions/ADR-0008-local-pure-rust-ml.md) §1 fixes
//! the framework and the backend: Burn v0.22, `flex`, weights from a local
//! file, no network, no second runtime.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use burn::prelude::*;

use super::embedder::{Embedder, EmbedderStatus, MlError};

/// The identifier stored with every vector this implementation produces.
pub const MODEL_ID: &str = "burn-minilm-l6-v2-spike-v1";

/// The vocabulary size of `all-MiniLM-L6-v2`.
const MINILM_VOCAB_SIZE: usize = 30522;

/// Where the vectors came from. The report must state this, because it is the
/// difference between measuring the plumbing and measuring quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeightSource {
    /// Tensors read from a real SafeTensors file on disk.
    SafeTensors,
    /// The file was absent and the tensors are a synthetic stand-in.
    Synthetic,
}

impl WeightSource {
    /// A short description for the report.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::SafeTensors => "real SafeTensors file",
            Self::Synthetic => "synthetic (generated in-process)",
        }
    }
}

/// A Burn `flex` embedder holding a token table and a projection.
pub struct BurnEmbedder {
    device: Device,
    word_embeddings: Tensor<2>,
    projection_weight: Tensor<2>,
    projection_bias: Tensor<1>,
    vocab_size: usize,
    dimension: usize,
    source: WeightSource,
}

impl BurnEmbedder {
    /// Load a SafeTensors file and build the network.
    ///
    /// Only three tensors are materialised, so peak memory is bounded by them
    /// rather than by the size of the whole file.
    ///
    /// # Errors
    ///
    /// Returns [`MlError::Model`] when the file cannot be read, is not a
    /// SafeTensors file, or lacks a tensor of the expected name or shape.
    pub fn from_safetensors(path: &Path, dimension: usize) -> Result<Self, MlError> {
        let mut file = File::open(path).map_err(|error| MlError::Model {
            message: format!("cannot open {}: {error}", path.display()),
        })?;

        let (tensors, data_start) = read_header(&mut file)?;

        let word_info = tensors
            .get("embeddings.word_embeddings.weight")
            .ok_or_else(|| MlError::Model {
                message: "missing tensor embeddings.word_embeddings.weight".to_string(),
            })?;
        if word_info.shape.len() != 2 || word_info.shape[1] != dimension {
            return Err(MlError::Model {
                message: format!(
                    "embeddings.word_embeddings.weight has shape {:?}, expected [vocab, {dimension}]",
                    word_info.shape
                ),
            });
        }
        let vocab_size = word_info.shape[0];

        let proj_info = tensors
            .get("pooler.dense.weight")
            .ok_or_else(|| MlError::Model {
                message: "missing tensor pooler.dense.weight".to_string(),
            })?;
        if proj_info.shape != vec![dimension, dimension] {
            return Err(MlError::Model {
                message: format!(
                    "pooler.dense.weight has shape {:?}, expected [{dimension}, {dimension}]",
                    proj_info.shape
                ),
            });
        }

        let bias_info = tensors
            .get("pooler.dense.bias")
            .ok_or_else(|| MlError::Model {
                message: "missing tensor pooler.dense.bias".to_string(),
            })?;

        let word_values = read_f32(&mut file, data_start, word_info)?;
        let proj_values = read_f32(&mut file, data_start, proj_info)?;
        let bias_values = read_f32(&mut file, data_start, bias_info)?;

        let device = Device::flex();

        Ok(Self {
            word_embeddings: Tensor::from_data(
                TensorData::new(word_values, Shape::new([vocab_size, dimension])),
                &device,
            ),
            projection_weight: Tensor::from_data(
                TensorData::new(proj_values, Shape::new([dimension, dimension])),
                &device,
            ),
            projection_bias: Tensor::from_data(
                TensorData::new(bias_values, Shape::new([dimension])),
                &device,
            ),
            device,
            vocab_size,
            dimension,
            source: WeightSource::SafeTensors,
        })
    }

    /// Build the same network with generated weights.
    ///
    /// Used when no model file is present, so the binary still measures the
    /// Burn `flex` code path. The vectors are meaningless, and the source says
    /// so.
    #[must_use]
    pub fn synthetic(dimension: usize, seed: u64) -> Self {
        let device = Device::flex();
        let vocab_size = MINILM_VOCAB_SIZE;

        let mut state = seed | 1;
        let mut next = || {
            // splitmix64, so the stand-in is reproducible without an RNG crate.
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            (z ^ (z >> 31)) as f32 / u64::MAX as f32
        };

        let scale = 0.05;
        let mut word_values = Vec::with_capacity(vocab_size * dimension);
        for _ in 0..vocab_size * dimension {
            word_values.push((next() * 2.0 - 1.0) * scale);
        }
        let mut proj_values = Vec::with_capacity(dimension * dimension);
        for _ in 0..dimension * dimension {
            proj_values.push((next() * 2.0 - 1.0) * 0.5);
        }
        let mut bias_values = Vec::with_capacity(dimension);
        for _ in 0..dimension {
            bias_values.push((next() * 2.0 - 1.0) * 0.05);
        }

        Self {
            word_embeddings: Tensor::from_data(
                TensorData::new(word_values, Shape::new([vocab_size, dimension])),
                &device,
            ),
            projection_weight: Tensor::from_data(
                TensorData::new(proj_values, Shape::new([dimension, dimension])),
                &device,
            ),
            projection_bias: Tensor::from_data(
                TensorData::new(bias_values, Shape::new([dimension])),
                &device,
            ),
            device,
            vocab_size,
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
        self.vocab_size
    }

    /// Map a text to token ids.
    ///
    /// This is **not** WordPiece. Each whitespace token is hashed into
    /// `[0, vocab_size)` with FNV-1a, which is enough to exercise the lookup
    /// and produces a stable id for the same token. A real tokenizer is part
    /// of the work this spike does not do.
    fn token_ids(&self, text: &str) -> Vec<i32> {
        let mut ids: Vec<i32> = Vec::new();

        for word in text.split_whitespace() {
            let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
            for byte in word.as_bytes() {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
            }
            let id = (hash % self.vocab_size as u64) as i32;
            ids.push(id);
        }

        if ids.is_empty() {
            // Never hand an empty sequence to the pooling op.
            ids.push(0);
        }

        ids
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
        let ids = self.token_ids(text);
        let sequence = ids.len();

        let indices: Tensor<1, Int> =
            Tensor::from_data(TensorData::new(ids, Shape::new([sequence])), &self.device);

        // Gather the rows: [sequence, dim] -> mean over the sequence -> [1, dim]
        let embedded = self.word_embeddings.clone().select(0, indices);
        let pooled = embedded.mean_dim(0).reshape([1, self.dimension]);

        // x @ W^T + b, the orientation a torch `Linear` stores.
        let projected = pooled.matmul(self.projection_weight.clone().transpose())
            + self.projection_bias.clone().reshape([1, self.dimension]);

        let values = projected
            .into_data()
            .try_into_vec::<f32>()
            .map_err(|error| MlError::Model {
                message: format!("cannot read the output tensor: {error}"),
            })?;

        if values.len() != self.dimension {
            return Err(MlError::Model {
                message: format!(
                    "the model produced {} values, expected {}",
                    values.len(),
                    self.dimension
                ),
            });
        }

        Ok(values)
    }
}

/// One tensor's location inside a SafeTensors file.
struct TensorInfo {
    shape: Vec<usize>,
    start: usize,
    end: usize,
}

/// Parse the JSON header, returning each tensor's location and the offset at
/// which the data section begins.
fn read_header(file: &mut File) -> Result<(HashMap<String, TensorInfo>, u64), MlError> {
    let mut length_bytes = [0u8; 8];
    file.read_exact(&mut length_bytes)
        .map_err(|error| MlError::Model {
            message: format!("cannot read the SafeTensors header length: {error}"),
        })?;
    let header_len = u64::from_le_bytes(length_bytes);

    // The header is JSON; a corrupt or hostile value must not allocate blindly.
    if header_len == 0 || header_len > 100 * 1024 * 1024 {
        return Err(MlError::Model {
            message: format!("implausible SafeTensors header length: {header_len}"),
        });
    }
    let mut buffer = vec![0u8; header_len as usize];
    file.read_exact(&mut buffer)
        .map_err(|error| MlError::Model {
            message: format!("cannot read the SafeTensors header: {error}"),
        })?;

    let json: serde_json::Value =
        serde_json::from_slice(&buffer).map_err(|error| MlError::Model {
            message: format!("the SafeTensors header is not JSON: {error}"),
        })?;

    let object = json.as_object().ok_or_else(|| MlError::Model {
        message: "the SafeTensors header is not an object".to_string(),
    })?;

    let mut tensors = HashMap::new();
    for (name, value) in object {
        if name == "__metadata__" {
            continue;
        }

        let dtype = value
            .get("dtype")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if dtype != "F32" {
            // The file is rejected at the tensor we need rather than silently
            // reinterpreted; `read_tensor` reports the name.
            continue;
        }

        let shape = value
            .get("shape")
            .and_then(serde_json::Value::as_array)
            .map(|dims| {
                dims.iter()
                    .filter_map(serde_json::Value::as_u64)
                    .map(|dim| dim as usize)
                    .collect::<Vec<usize>>()
            })
            .unwrap_or_default();

        let offsets = value
            .get("data_offsets")
            .and_then(serde_json::Value::as_array)
            .map(|range| {
                range
                    .iter()
                    .filter_map(serde_json::Value::as_u64)
                    .map(|offset| offset as usize)
                    .collect::<Vec<usize>>()
            })
            .unwrap_or_default();

        if offsets.len() != 2 {
            continue;
        }

        tensors.insert(
            name.clone(),
            TensorInfo {
                shape,
                start: offsets[0],
                end: offsets[1],
            },
        );
    }

    Ok((tensors, 8 + header_len))
}

/// Read one tensor as `f32`, skipping everything else in the file.
fn read_f32(file: &mut File, data_start: u64, info: &TensorInfo) -> Result<Vec<f32>, MlError> {
    let byte_len = info.end.saturating_sub(info.start);
    if !byte_len.is_multiple_of(4) {
        return Err(MlError::Model {
            message: format!("tensor byte length {byte_len} is not a multiple of four"),
        });
    }

    file.seek(SeekFrom::Start(data_start + info.start as u64))
        .map_err(|error| MlError::Model {
            message: format!("cannot seek to the tensor: {error}"),
        })?;

    let mut buffer = vec![0u8; byte_len];
    file.read_exact(&mut buffer)
        .map_err(|error| MlError::Model {
            message: format!("cannot read the tensor: {error}"),
        })?;

    let mut values = Vec::with_capacity(byte_len / 4);
    for chunk in buffer.as_chunks::<4>().0 {
        values.push(f32::from_le_bytes(*chunk));
    }

    Ok(values)
}
