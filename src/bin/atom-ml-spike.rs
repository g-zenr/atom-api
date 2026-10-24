//! D4.M1 spike — measures a local Burn `flex` forward pass on this machine.
//!
//! This is a bounded, reversible experiment. It is **not** a product binary:
//! it is not reachable from the HTTP API and it contains no endpoint, no
//! migration and no service. It answers one question:
//!
//! > Can this machine run a sentence-embedding forward pass in-process, in pure
//! > Rust, with Burn v0.22 on the `flex` (CPU) backend, fast enough and small
//! > enough that an endpoint may depend on it?
//!
//! Run it with:
//!
//! ```text
//! cargo run --release --bin atom-ml-spike --features ml
//! ```
//!
//! Set `ATOM_ML_MODEL` to a SafeTensors file to measure real weights. With no
//! file the binary builds the same network from generated weights and says so:
//! the timings are then a measurement of the plumbing only.
//!
//! The reporting requirements are those of the run order: model construction
//! time, first-inference time, steady-state per-text latency, peak RSS, the
//! vector's dimension and L2 norm, bit-for-bit determinism, and proof that the
//! `Disabled` path cannot panic and that `Deterministic` needs no file.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use atom_api::ml::{BurnEmbedder, Deterministic, Disabled, Embedder, l2_norm};

/// The dimension the network is built to; `ml.dimension` defaults to this.
const DIMENSION: usize = 384;

/// The model file the machine happens to have, if `ATOM_ML_MODEL` is unset.
const DEFAULT_MODEL: &str = r"C:\Users\Renz\.cache\huggingface\hub\models--sentence-transformers--all-MiniLM-L6-v2\snapshots\c9745ed1d9f207416be6d2e6f8de32d1f16199bf\model.safetensors";

fn main() -> std::process::ExitCode {
    println!("ATOM D4.M1 spike — Burn 0.22 / flex / CPU");
    println!("dimension: {DIMENSION}");
    println!();

    let Some(embedder) = build_embedder() else {
        return std::process::ExitCode::FAILURE;
    };

    if !measure_forward_pass(&embedder) {
        return std::process::ExitCode::FAILURE;
    }

    if !check_determinism(&embedder) {
        return std::process::ExitCode::FAILURE;
    }

    // The two paths that must work without a model file.
    check_disabled_is_not_a_panic();
    check_deterministic_needs_no_file();

    println!();
    println!("spike complete");
    std::process::ExitCode::SUCCESS
}

/// Load the real file when there is one, otherwise build synthetic weights.
fn build_embedder() -> Option<BurnEmbedder> {
    let configured = std::env::var("ATOM_ML_MODEL").ok().map(PathBuf::from);
    let candidate = configured.unwrap_or_else(|| PathBuf::from(DEFAULT_MODEL));

    if !candidate.is_file() {
        println!("model file: none at {}", candidate.display());
        println!("weights:    synthetic — these numbers measure the plumbing only");
        let started = Instant::now();
        let embedder = BurnEmbedder::synthetic(DIMENSION, 0x5EED);
        let elapsed = started.elapsed();
        println!(
            "construction: {} ms (synthetic, vocab {})",
            elapsed.as_millis(),
            embedder.vocab_size()
        );
        println!("peak RSS after construction: {}", rss_label());
        println!();
        return Some(embedder);
    }

    println!("model file: {}", candidate.display());
    let started = Instant::now();
    let embedder = match BurnEmbedder::from_safetensors(&candidate, DIMENSION) {
        Ok(embedder) => embedder,
        Err(error) => {
            // A file that exists but cannot be loaded is a finding, not
            // something to paper over with generated weights.
            println!("FAILED to load the model: {error}");
            return None;
        }
    };
    let elapsed = started.elapsed();

    println!(
        "weights:    {} (vocab {})",
        embedder.weight_source().label(),
        embedder.vocab_size()
    );
    println!(
        "construction: {} ms ({:.3} s)",
        elapsed.as_millis(),
        elapsed.as_secs_f64()
    );
    println!("peak RSS after construction: {}", rss_label());
    println!();
    Some(embedder)
}

/// The forward pass, at the three sizes the run order fixes.
fn measure_forward_pass(embedder: &BurnEmbedder) -> bool {
    let texts = sample_texts(100);

    // First inference: the lazy initialisation, the first kernel dispatch and
    // the allocator's first growth all land here.
    let first_started = Instant::now();
    let first = match embedder.embed(&texts[0]) {
        Ok(vector) => vector,
        Err(error) => {
            println!("FAILED first inference: {error}");
            return false;
        }
    };
    let first_elapsed = first_started.elapsed();

    println!("first inference: {} ms", fmt_ms(first_elapsed));
    println!(
        "vector: dimension {} (expected {}), L2 norm {:.6}",
        first.len(),
        DIMENSION,
        l2_norm(&first)
    );
    println!();

    println!("steady state (per text, after the first call)");
    println!(
        "{:>6}  {:>12}  {:>12}  {:>12}  {:>10}",
        "n", "mean (ms)", "min (ms)", "max (ms)", "repeats"
    );

    for (count, repeats) in [(1_usize, 200_usize), (10, 40), (100, 10)] {
        let batch = &texts[..count];
        let mut per_text = Vec::with_capacity(repeats);

        for _ in 0..repeats {
            let started = Instant::now();
            for text in batch {
                if let Err(error) = embedder.embed(text) {
                    println!("FAILED inference: {error}");
                    return false;
                }
            }
            let elapsed = started.elapsed();
            per_text.push(elapsed.as_secs_f64() * 1000.0 / count as f64);
        }

        per_text.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mean = per_text.iter().sum::<f64>() / per_text.len() as f64;
        let min = per_text.first().copied().unwrap_or_default();
        let max = per_text.last().copied().unwrap_or_default();

        println!("{count:>6}  {mean:>12.4}  {min:>12.4}  {max:>12.4}  {repeats:>10}",);
    }

    println!();
    println!("peak RSS after the run: {}", rss_label());
    println!("budget: ml.timeout_seconds default is 2 s; a 100-text re-rank must fit inside it");
    true
}

/// The same input twice must give the same vector, bit for bit.
fn check_determinism(embedder: &BurnEmbedder) -> bool {
    let text = "reimbursement claim for the Berlin conference travel";
    let first = embedder.embed(text);
    let second = embedder.embed(text);

    let (Ok(first), Ok(second)) = (first, second) else {
        println!("FAILED determinism: an embed call returned an error");
        return false;
    };

    let identical = first.len() == second.len()
        && first
            .iter()
            .zip(&second)
            .all(|(a, b)| a.to_bits() == b.to_bits());

    let other = embedder.embed("a completely different sentence about job applications");
    let differs = match other {
        Ok(other) => other
            .iter()
            .zip(&first)
            .any(|(a, b)| a.to_bits() != b.to_bits()),
        Err(_) => false,
    };

    println!();
    println!(
        "determinism (Burn): identical input twice = {identical}, different input differs = {differs}"
    );

    let baseline = l2_norm(&first);
    println!("determinism L2 norm: {baseline:.6}");

    identical && differs
}

/// `Disabled` must report unavailability, never panic and never block.
fn check_disabled_is_not_a_panic() {
    let disabled = Disabled::new(DIMENSION, "ml_disabled");
    let status = disabled.status();
    let result = disabled.embed("any text at all");

    println!();
    println!(
        "disabled path: status ready = {}, embed is error = {}, reason = {}",
        status.is_ready(),
        result.is_err(),
        disabled.reason()
    );

    if let Err(error) = &result {
        println!("disabled path: error code = {}", error.code());
    }
}

/// `Deterministic` must work with no model file and be stable.
fn check_deterministic_needs_no_file() {
    let started = Instant::now();
    let deterministic = Deterministic::with_default_dimension();
    let construction = started.elapsed();

    let Ok(vector) = deterministic.embed("a note about reimbursements") else {
        println!("FAILED deterministic embed");
        return;
    };
    let Ok(again) = deterministic.embed("a note about reimbursements") else {
        println!("FAILED deterministic embed");
        return;
    };

    let stable = vector
        .iter()
        .zip(&again)
        .all(|(a, b)| a.to_bits() == b.to_bits());

    println!(
        "deterministic path: construction {} ms, dimension {}, L2 norm {:.6}, stable = {stable}, file required = false",
        construction.as_millis(),
        vector.len(),
        l2_norm(&vector)
    );
}

/// One hundred short, note-shaped texts.
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

/// Milliseconds with a fixed width, so a sub-millisecond value is readable.
fn fmt_ms(duration: Duration) -> String {
    format!("{:.4}", duration.as_secs_f64() * 1000.0)
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
