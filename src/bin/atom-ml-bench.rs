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
//! * the `ML-R09` quality evaluation — fifteen query/expected-text pairs planted
//!   as embeddings of the expected texts, the observed rank and cosine of every
//!   expected text, the pass count, the top-1 cosine of each query against a
//!   fixed threshold ladder, and a negative control that no planted text
//!   answers. It reports the numbers; it does not tune a threshold to them.
//!
//! A query the encoder gets wrong is printed as `MISS` and counted as a miss.
//! That is a finding about the model, and suppressing it by choosing easier
//! pairs would make the table worthless.
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

/// The model path when `ATOM_ML_MODEL` is unset. There is deliberately no
/// machine-specific default: this benchmark refuses to run without the real
/// weights, so the fallback is a bare relative name and the error tells the
/// operator to set the variable (the documented invocation in
/// `docs/features/ml/status.md` §5.1a sets it explicitly).
const DEFAULT_MODEL: &str = "model.safetensors";

/// One evaluation pair: the query a person types, and the note it must find.
///
/// The query is a **paraphrase**, not a fragment of the text: a pair whose query
/// shares its wording with the note would be answered by any lexical overlap
/// and would measure nothing. The pairs are ordinary workspace notes, chosen
/// because they are what the product stores, not because they are easy.
struct Pair {
    query: &'static str,
    expected: &'static str,
}

/// The fifteen `ML-R09` pairs, planted as the embeddings of their expected
/// texts.
const EVALUATION: [Pair; 15] = [
    Pair {
        query: "How do I claim back my travel expenses?",
        expected: "The Berlin conference trip was reimbursed in full once I submitted the receipts.",
    },
    Pair {
        query: "When do I need to pay my taxes?",
        expected: "The quarterly estimated tax payment is due on the fifteenth of the month.",
    },
    Pair {
        query: "What did the infrastructure team ask in the interview?",
        expected: "Interview notes from the platform infrastructure team: they asked about Kubernetes and incident response.",
    },
    Pair {
        query: "Which furniture should I buy for working from home?",
        expected: "We need to buy a standing desk and a monitor arm for the home office.",
    },
    Pair {
        query: "Ideas for improving search in my notes app",
        expected: "Design ideas for the personal workspace search interface: three modes and a relevance score.",
    },
    Pair {
        query: "How do I safely deploy a canary release?",
        expected: "Canary release checklist: roll out to one percent of traffic, watch the error rate, then promote.",
    },
    Pair {
        query: "Who is on call this weekend?",
        expected: "On-call rotation: the platform team takes the week and the data team takes the weekend.",
    },
    Pair {
        query: "How should I write a postmortem for an outage?",
        expected: "Incident postmortem template: timeline, root cause, and the action items that follow.",
    },
    Pair {
        query: "How do I negotiate a higher salary offer?",
        expected: "Salary negotiation: the recruiter's first offer is below the band, so ask for the range.",
    },
    Pair {
        query: "What equipment do I need to work from home comfortably?",
        expected: "Remote work setup: a monitor, a proper chair, and a separate room for calls.",
    },
    Pair {
        query: "How do I change a table without downtime?",
        expected: "Database migration plan: add the column as nullable, backfill it, then enforce the constraint.",
    },
    Pair {
        query: "How do I stop clients from overloading the API?",
        expected: "API rate limiting: a token bucket per key with a burst of twenty requests.",
    },
    Pair {
        query: "Which metrics should the monitoring dashboard show?",
        expected: "Observability dashboard: latency, error rate and saturation on one screen.",
    },
    Pair {
        query: "What refactoring work is still outstanding?",
        expected: "Technical debt backlog: the checkout module needs a rewrite before the next release.",
    },
    Pair {
        query: "What evidence do I need for a promotion?",
        expected: "Promotion case: evidence of scope, impact, and the last two review cycles.",
    },
];

/// The fixed threshold ladder the run reports every query against.
///
/// These are candidates, not a tuning grid: the point of the table is to show
/// which of them the data separates, so a number can be chosen from evidence
/// rather than invented. The ladder is not adjusted after a run.
const CANDIDATE_THRESHOLDS: [f64; 7] = [0.20, 0.25, 0.30, 0.35, 0.40, 0.45, 0.50];

/// A query unrelated to every planted text: the control that decides whether a
/// threshold is a threshold or a formality.
const NEGATIVE_CONTROL: &str = "the mating call of a humpback whale";

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
    evaluate_retrieval(&embedder)?;

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

/// The `ML-R09` quality evaluation: fifteen queries against fifteen planted
/// texts, the rank every expected text reached, and the threshold evidence.
fn evaluate_retrieval(embedder: &BurnEmbedder) -> Result<(), String> {
    println!();
    println!("ML-R09 quality — fifteen queries against the real encoder");
    println!();

    let planted: Vec<&str> = EVALUATION.iter().map(|pair| pair.expected).collect();
    let mut corpus = Vec::with_capacity(planted.len());
    for text in &planted {
        corpus.push(
            embedder
                .embed(text)
                .map_err(|error| format!("cannot embed a planted text: {error}"))?,
        );
    }

    println!(
        "corpus: {} planted texts, dimension {}",
        planted.len(),
        embedder.dimension()
    );
    println!();
    println!(
        "{:>3}  {:<40}  {:<40}  {:>5}  {:>8}  {:>8}  result",
        "#", "query", "expected text", "rank", "cosine", "top-1"
    );

    let mut passes = 0_usize;
    let mut top_scores: Vec<f64> = Vec::with_capacity(EVALUATION.len());
    let mut misses: Vec<String> = Vec::new();

    for (index, pair) in EVALUATION.iter().enumerate() {
        let query = embedder
            .embed(pair.query)
            .map_err(|error| format!("cannot embed the query {:?}: {error}", pair.query))?;
        let ranked = rank_planted(&corpus, &query);

        let Some(&(top_index, top)) = ranked.first() else {
            return Err("the ranking over the planted corpus is empty".to_string());
        };

        let expected_index = planted
            .iter()
            .position(|text| *text == pair.expected)
            .ok_or_else(|| format!("the expected text of query {} is not planted", index + 1))?;

        let Some(position) = ranked
            .iter()
            .position(|(planted_index, _)| *planted_index == expected_index)
        else {
            return Err(format!(
                "the expected text of query {} did not survive the ranking",
                index + 1
            ));
        };

        let Some(&(_, value)) = ranked.get(position) else {
            return Err("the ranked position is out of range".to_string());
        };

        let hit = position == 0 && top_index == expected_index;
        if hit {
            passes += 1;
        } else {
            misses.push(format!(
                "query {:?}: rank 1 was {:?} at {top:.4}, expected {:?} at rank {} ({value:.4})",
                pair.query,
                planted.get(top_index).copied().unwrap_or_default(),
                pair.expected,
                position + 1
            ));
        }

        top_scores.push(top);

        println!(
            "{:>3}  {:<40}  {:<40}  {:>5}  {:>8.4}  {:>8.4}  {}",
            index + 1,
            truncate(pair.query, 40),
            truncate(pair.expected, 40),
            position + 1,
            value,
            top,
            if hit { "pass" } else { "MISS" }
        );
    }

    println!();
    println!("pass: {passes}/{} at rank 1", EVALUATION.len());

    let min_positive = top_scores
        .iter()
        .copied()
        .reduce(f64::min)
        .ok_or_else(|| "no query was scored".to_string())?;
    let max_positive = top_scores
        .iter()
        .copied()
        .reduce(f64::max)
        .ok_or_else(|| "no query was scored".to_string())?;
    let mean_positive = top_scores.iter().sum::<f64>() / top_scores.len() as f64;

    println!("top-1 cosine: min {min_positive:.4}, max {max_positive:.4}, mean {mean_positive:.4}");

    // The negative control is embedded and ranked exactly like a query, so its
    // top-1 is the score a threshold has to sit above.
    let control_vector = embedder
        .embed(NEGATIVE_CONTROL)
        .map_err(|error| format!("cannot embed the negative control: {error}"))?;
    let control_ranked = rank_planted(&corpus, &control_vector);
    let Some(&(control_index, control_top)) = control_ranked.first() else {
        return Err("the negative control produced no ranking".to_string());
    };

    println!();
    println!("threshold behaviour — each query's top-1 against a fixed ladder");
    println!(
        "{:>9}  {:>13}  {:>13}  separates",
        "threshold", "queries above", "control above"
    );

    let mut supported: Option<f64> = None;
    for threshold in CANDIDATE_THRESHOLDS {
        let above = top_scores
            .iter()
            .filter(|score| **score > threshold)
            .count();
        let control_above = control_top > threshold;
        let separates = above == EVALUATION.len() && !control_above;

        if separates {
            supported = Some(threshold);
        }

        println!(
            "{:>9.2}  {:>6}/{}  {:>13}  {}",
            threshold,
            above,
            EVALUATION.len(),
            if control_above { "yes" } else { "no" },
            if separates { "yes" } else { "no" }
        );
    }

    println!();
    println!("negative control: {NEGATIVE_CONTROL:?}");
    println!(
        "  nearest planted text: {:?}",
        truncate(planted.get(control_index).copied().unwrap_or_default(), 60)
    );
    println!("  top-1 cosine: {control_top:.4}");

    if control_top > 0.0 {
        println!(
            "  the 0.0 floor does not remove it: its similarity is positive, so only a threshold excludes it"
        );
    } else {
        println!("  the 0.0 floor removes it: its similarity is at or below zero");
    }

    match supported {
        Some(threshold) => println!(
            "  against {threshold:.2}: {}",
            if control_top > threshold {
                "ABOVE the threshold — the control fails"
            } else {
                "below the threshold — the control passes"
            }
        ),
        None => println!("  no candidate threshold separates it from the positives"),
    }

    println!();
    println!(
        "separation band: control top-1 {control_top:.4} < threshold < lowest positive top-1 {min_positive:.4}"
    );
    match supported {
        Some(threshold) => println!(
            "threshold supported by this run: {threshold:.2} — all {} positives are above it and the control is below it",
            EVALUATION.len()
        ),
        None => println!(
            "threshold supported by this run: none of the candidates — no ladder value lies inside the band"
        ),
    }

    if !misses.is_empty() {
        println!();
        println!(
            "the encoder missed {} of {} queries:",
            misses.len(),
            EVALUATION.len()
        );
        for miss in &misses {
            println!("  {miss}");
        }
    }

    Ok(())
}

/// Ranks every planted vector against one query, highest cosine first.
///
/// The order is total — the score, then the planted index — so a tie is broken
/// the way the service breaks one and a reported rank is reproducible.
fn rank_planted(corpus: &[Vec<f32>], query: &[f32]) -> Vec<(usize, f64)> {
    let mut scored: Vec<(usize, f64)> = corpus
        .iter()
        .enumerate()
        .filter_map(|(index, vector)| cosine(query, vector).map(|value| (index, value)))
        .collect();

    scored.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });

    scored
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
