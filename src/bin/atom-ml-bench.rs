//! `M3` re-measurement — the real encoder, the real tokenizer, this machine.
//!
//! The `atom-ml-spike` binary measured a lookup, a mean-pool and a projection:
//! a lower bound on the plumbing, not an embedding (ADR-0008 §8.1). This binary
//! measures what an endpoint actually pays: the six-layer BERT forward pass over
//! the real SafeTensors file and a WordPiece-tokenised text, including the
//! write-time normalisation.
//!
//! Run it with:
//!
//! ```text
//! cargo run --release --bin atom-ml-bench --features ml
//! ```
//!
//! Set `ATOM_ML_MODEL` to the SafeTensors file; the `vocab.txt` beside it is the
//! tokenizer. With no file the binary reports the absence and exits non-zero: a
//! synthetic model would measure the wrong network, and the point of this run is
//! the real one.
//!
//! It reports:
//!
//! * construction, first inference and steady state at 1, 10 and 100 texts;
//! * peak RSS, before and after the run;
//! * the output's L2 norm (it must be 1: ADR-0008 §9);
//! * bit-exact determinism for one input and a difference for another;
//! * a five-text, three-query retrieval check — the smallest evidence that the
//!   vectors mean something. It is **not** `ML-R09`'s fifteen-query evaluation.
//!
//! The numbers this prints are what `ml.timeout_seconds` and
//! `ml.candidate_limit` have to be judged against.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use atom_api::ml::burn_embedder::BurnEmbedder;
use atom_api::ml::embedder::{Embedder, l2_norm};
use atom_api::ml::service::cosine;

/// The dimension `ml.dimension` defaults to.
const DIMENSION: usize = 384;

/// The model file the development machine has, if `ATOM_ML_MODEL` is unset.
const DEFAULT_MODEL: &str = r"C:\Users\Renz\.cache\huggingface\hub\models--sentence-transformers--all-MiniLM-L6-v2\snapshots\c9745ed1d9f207416be6d2e6f8de32d1f16199bf\model.safetensors";

/// Five short note-shaped texts and the query each one should answer.
const EVALUATION: [(&str, &str); 5] = [
    (
        "The Berlin conference trip was reimbursed in full.",
        "How do I file an expense claim for travel?",
    ),
    (
        "The quarterly estimated tax payment is due next week.",
        "When do I need to pay my taxes?",
    ),
    (
        "Interview notes from the platform infrastructure team.",
        "What did the infrastructure team say in the interview?",
    ),
    (
        "We need to buy a standing desk and a monitor arm.",
        "Which desk should I buy for the home office?",
    ),
    (
        "Design ideas for the personal workspace search interface.",
        "Ideas for improving search in my notes app",
    ),
];

fn main() -> ExitCode {
    println!("ATOM M3 bench — Burn 0.22 / flex / CPU / six-layer MiniLM");
    println!();

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            println!("FAILED: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let model = std::env::var("ATOM_ML_MODEL")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_MODEL));

    if !model.is_file() {
        return Err(format!(
            "no model at {} (set ATOM_ML_MODEL); this benchmark needs the real weights",
            model.display()
        ));
    }

    let vocabulary = model.with_file_name("vocab.txt");
    if !vocabulary.is_file() {
        return Err(format!(
            "no vocabulary at {}; the WordPiece tokenizer needs vocab.txt beside the weights",
            vocabulary.display()
        ));
    }

    println!("model:      {}", model.display());
    println!("vocabulary: {} tokens", count_lines(&vocabulary));
    println!("peak RSS before construction: {}", rss_label());

    let started = Instant::now();
    let embedder = BurnEmbedder::from_artifacts(&model, &vocabulary, DIMENSION)
        .map_err(|error| format!("cannot load the encoder: {error}"))?;
    let construction = started.elapsed();

    println!(
        "construction: {:.1} ms ({:.3} s)",
        construction.as_secs_f64() * 1000.0,
        construction.as_secs_f64()
    );
    println!("peak RSS after construction: {}", rss_label());
    println!(
        "model id: {} — dimension {}, vocabulary {}",
        embedder.model_id(),
        embedder.dimension(),
        embedder.vocab_size()
    );
    println!();

    measure_forward_pass(&embedder)?;
    check_determinism(&embedder)?;
    check_quality(&embedder)?;

    println!();
    println!("peak RSS at the end: {}", rss_label());
    println!("budget: ml.timeout_seconds default is 2 s; compare it with the 100-text row");

    Ok(())
}

fn measure_forward_pass(embedder: &BurnEmbedder) -> Result<(), String> {
    let texts = sample_texts(100);

    let started = Instant::now();
    let first = embedder
        .embed(&texts[0])
        .map_err(|error| format!("the first inference failed: {error}"))?;
    let first_elapsed = started.elapsed();

    println!(
        "first inference: {:.3} ms",
        first_elapsed.as_secs_f64() * 1000.0
    );
    println!(
        "vector: dimension {} (expected {}), L2 norm {:.6} (expected 1)",
        first.len(),
        DIMENSION,
        l2_norm(&first)
    );
    println!();

    println!("steady state (per text, after the first call)");
    println!(
        "{:>6}  {:>12}  {:>12}  {:>12}  {:>12}",
        "n", "mean (ms)", "min (ms)", "max (ms)", "total (ms)"
    );

    for (count, repeats) in [(1_usize, 200_usize), (10, 40), (100, 10)] {
        let batch = &texts[..count];
        let mut per_text = Vec::with_capacity(repeats);
        let mut total = Duration::ZERO;

        for _ in 0..repeats {
            let started = Instant::now();
            for text in batch {
                embedder
                    .embed(text)
                    .map_err(|error| format!("an inference failed: {error}"))?;
            }
            let elapsed = started.elapsed();
            total += elapsed;
            per_text.push(elapsed.as_secs_f64() * 1000.0 / count as f64);
        }

        per_text.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mean = per_text.iter().sum::<f64>() / per_text.len() as f64;
        let min = per_text.first().copied().unwrap_or_default();
        let max = per_text.last().copied().unwrap_or_default();
        let one_run = total.as_secs_f64() * 1000.0 / repeats as f64;

        println!("{count:>6}  {mean:>12.4}  {min:>12.4}  {max:>12.4}  {one_run:>12.4}");
    }

    // The worst case one request can put on the encoder: a full-length
    // sequence. `ml.timeout_seconds` has to cover this, not the short texts
    // above, because attention cost grows with the square of the sequence.
    let mut long = sample_texts(40).join(" ");
    long.push_str(" the architecture review discussion and the documentation sweep");
    let tokens = embedder.tokenizer().tokenize(&long).ids.len();

    let mut samples = Vec::with_capacity(20);
    for _ in 0..20 {
        let started = Instant::now();
        let vector = embedder
            .embed(&long)
            .map_err(|error| format!("the worst-case inference failed: {error}"))?;
        samples.push(started.elapsed().as_secs_f64() * 1000.0);

        if (1.0 - l2_norm(&vector)).abs() > 1e-5 {
            return Err("the worst-case vector is not unit length".to_string());
        }
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    println!();
    println!(
        "worst-case sequence: {tokens} tokens, mean {:.3} ms, min {:.3}, max {:.3}",
        samples.iter().sum::<f64>() / samples.len() as f64,
        samples.first().copied().unwrap_or_default(),
        samples.last().copied().unwrap_or_default()
    );

    Ok(())
}

fn check_determinism(embedder: &BurnEmbedder) -> Result<(), String> {
    let text = "reimbursement claim for the Berlin conference travel";
    let first = embedder.embed(text).map_err(|error| error.to_string())?;
    let second = embedder.embed(text).map_err(|error| error.to_string())?;
    let other = embedder
        .embed("a completely different sentence about job applications")
        .map_err(|error| error.to_string())?;

    let identical = first
        .iter()
        .zip(&second)
        .all(|(left, right)| left.to_bits() == right.to_bits());
    let differs = first
        .iter()
        .zip(&other)
        .any(|(left, right)| left.to_bits() != right.to_bits());

    println!();
    println!(
        "determinism: identical input twice = {identical}, different input differs = {differs}"
    );

    if identical && differs {
        Ok(())
    } else {
        Err("the encoder is not deterministic, or ignores its input".to_string())
    }
}

/// The smallest quality evidence: embed five texts and three queries, and check
/// whether each query's nearest neighbour is the text a person would expect.
fn check_quality(embedder: &BurnEmbedder) -> Result<(), String> {
    println!();
    println!("quality: five texts, each query's nearest neighbour");

    let texts: Vec<&str> = EVALUATION.iter().map(|(text, _)| *text).collect();
    let mut vectors = Vec::with_capacity(texts.len());
    for text in &texts {
        vectors.push(embedder.embed(text).map_err(|error| error.to_string())?);
    }

    let mut matched = 0;
    let mut total = 0;

    for (index, (_, query)) in EVALUATION.iter().enumerate() {
        let query_vector = embedder.embed(query).map_err(|error| error.to_string())?;

        let mut ranked: Vec<(f64, &str)> = vectors
            .iter()
            .zip(&texts)
            .map(|(vector, text)| {
                cosine(&query_vector, vector)
                    .map(|score| (score, *text))
                    .unwrap_or((f64::NEG_INFINITY, *text))
            })
            .collect();
        ranked.sort_by(|left, right| right.0.total_cmp(&left.0));

        let (score, nearest) = ranked[0];
        let expected = EVALUATION
            .get(index)
            .map(|(text, _)| *text)
            .unwrap_or_default();
        let hit = nearest == expected;
        if hit {
            matched += 1;
        }
        total += 1;

        println!(
            "query {:?}\n  nearest: {:?} (cosine {score:.4})\n  expected: {:?} — {}",
            query,
            truncate(nearest, 60),
            truncate(expected, 60),
            if hit { "match" } else { "MISS" }
        );
    }

    println!();
    println!("quality: {matched}/{total} queries returned the expected nearest text");

    Ok(())
}

/// One hundred short, note-shaped texts, the same shape the spike used so the
/// two tables are comparable.
fn sample_texts(count: usize) -> Vec<String> {
    const SUBJECTS: [&str; 10] = [
        "reimbursement claim for the Berlin conference travel",
        "quarterly estimated tax payment reminder",
        "notes from the interview with the platform team",
        "invoice for the standing desk and the monitor arm",
        "ideas for the personal workspace search interface",
        "application to the infrastructure role at the startup",
        "task list for the documentation sweep",
        "receipt for the train ticket to the client site",
        "summary of the architecture review discussion",
        "draft of the seeder specification",
    ];

    (0..count)
        .map(|index| {
            let subject = SUBJECTS[index % SUBJECTS.len()];
            format!("{subject} — entry {}", index + 1)
        })
        .collect()
}

fn count_lines(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .map(|text| text.lines().count())
        .unwrap_or(0)
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_string()
    } else {
        let mut shortened: String = text.chars().take(limit).collect();
        shortened.push('…');
        shortened
    }
}

fn rss_label() -> String {
    match peak_rss_bytes() {
        Some(bytes) => format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0)),
        None => "unavailable on this platform".to_string(),
    }
}

#[cfg(windows)]
fn peak_rss_bytes() -> Option<u64> {
    use std::mem::size_of;

    #[repr(C)]
    #[derive(Default)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(
            process: isize,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    let mut counters = ProcessMemoryCounters {
        cb: u32::try_from(size_of::<ProcessMemoryCounters>()).ok()?,
        ..Default::default()
    };

    // SAFETY: both calls take a valid handle to this process and a pointer to a
    // correctly sized, zeroed structure that outlives the call.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    if ok == 0 {
        None
    } else {
        Some(counters.peak_working_set_size as u64)
    }
}

#[cfg(not(windows))]
fn peak_rss_bytes() -> Option<u64> {
    None
}
