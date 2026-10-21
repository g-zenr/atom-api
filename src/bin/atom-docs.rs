//! `atom-docs` — the repository consistency checker.
//!
//! A small, dependency-free checker that validates the documentation tree and
//! the configuration surface. It is a development tool, not part of the API
//! surface, but it ships with the crate so that every contributor can run it
//! without installing anything else.
//!
//! ```text
//! cargo run --bin atom-docs            # check the docs/ tree and config/
//! cargo run --bin atom-docs --strict   # treat warnings as failures
//! ```
//!
//! Checks performed:
//!
//! 1. Every required document exists.
//! 2. Every relative Markdown link resolves to a file that exists.
//! 3. Every feature status uses one of the seven defined status words.
//! 4. Every ADR has a status and a unique identifier.
//! 5. Every change-log identifier is unique.
//! 6. Every feature directory has the five required documents.
//! 7. The configuration directory has its required files.
//! 8. `.env.example` and the configuration loader agree about the environment
//!    variables, in both directions.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The status vocabulary defined in `AGENTS.md`.
const STATUSES: [&str; 7] = [
    "Proposed",
    "Accepted",
    "In progress",
    "Implemented",
    "Verified",
    "Deprecated",
    "Blocked",
];

/// Documents that must exist.
const REQUIRED_FILES: [&str; 8] = [
    "docs/README.md",
    "docs/project-truth.md",
    "docs/roadmap.md",
    "docs/architecture/README.md",
    "docs/decisions/README.md",
    "docs/features/README.md",
    "docs/api/README.md",
    "docs/database/README.md",
];

/// Documents that must exist for every feature.
const FEATURE_FILES: [&str; 5] = [
    "specification.md",
    "architecture.md",
    "api.md",
    "testing.md",
    "status.md",
];

/// The features that must have a complete document set.
const FEATURES: [&str; 4] = ["authentication", "notes", "projects", "tasks"];

/// The files that must exist in the configuration directory.
const CONFIG_FILES: [&str; 7] = [
    "config/README.md",
    "config/app.toml",
    "config/api.toml",
    "config/auth.toml",
    "config/database.toml",
    "config/messages.toml",
    "config/environments/development.toml",
];

/// The file that lists the environment variables.
const ENV_EXAMPLE: &str = ".env.example";

/// The file that reads the environment variables.
const CONFIG_LOADER: &str = "src/config/loader.rs";

/// The result of the run.
struct Report {
    errors: Vec<String>,
    warnings: Vec<String>,
}

impl Report {
    fn error(&mut self, message: impl Into<String>) {
        self.errors.push(message.into());
    }

    fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }
}

fn main() -> ExitCode {
    let strict = std::env::args().any(|argument| argument == "--strict");

    let root = match repository_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("atom-docs: {error}");
            return ExitCode::from(2);
        }
    };

    let mut report = Report {
        errors: Vec::new(),
        warnings: Vec::new(),
    };

    check_required_files(&root, &mut report);
    check_links(&root, &mut report);
    check_feature_statuses(&root, &mut report);
    check_adrs(&root, &mut report);
    check_change_log(&root, &mut report);
    check_feature_directories(&root, &mut report);
    check_configuration(&root, &mut report);
    check_environment_variables(&root, &mut report);

    for warning in &report.warnings {
        println!("WARN  {warning}");
    }
    for error in &report.errors {
        println!("ERROR {error}");
    }

    if report.errors.is_empty() && (!strict || report.warnings.is_empty()) {
        println!("atom-docs: OK ({} warning(s))", report.warnings.len());
        ExitCode::SUCCESS
    } else {
        println!(
            "atom-docs: FAILED ({} error(s), {} warning(s))",
            report.errors.len(),
            report.warnings.len()
        );
        ExitCode::FAILURE
    }
}

/// Locates the repository root by walking up from the current directory until
/// a `docs/` directory is found.
fn repository_root() -> Result<PathBuf, String> {
    let mut current = std::env::current_dir().map_err(|error| error.to_string())?;

    loop {
        if current.join("docs").is_dir() {
            return Ok(current);
        }
        if !current.pop() {
            return Err("could not find a repository root containing docs/".to_string());
        }
    }
}

/// Every Markdown file under `docs/`, plus the repository root documents.
fn markdown_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_markdown(&root.join("docs"), &mut files);
    for name in ["AGENTS.md", "README.md", "CONTRIBUTING.md"] {
        let path = root.join(name);
        if path.is_file() {
            files.push(path);
        }
    }
    files
}

fn collect_markdown(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_markdown(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "md") {
            files.push(path);
        }
    }
}

fn check_required_files(root: &Path, report: &mut Report) {
    for required in REQUIRED_FILES {
        let path = root.join(required);
        if !path.is_file() {
            report.error(format!("missing required document: {required}"));
        }
    }
}

/// Extracts the targets of inline Markdown links and reference-style links.
fn link_targets(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() {
        // Inline link: [label](target)
        if bytes[index] == b'[' {
            if let Some(close) = text[index..].find("](") {
                let after = index + close + 2;
                if let Some(end) = text[after..].find(')') {
                    let target = text[after..after + end].trim();
                    targets.push(target.to_string());
                    index = after + end + 1;
                    continue;
                }
            }
        }

        // Reference link: [label]: target
        if bytes[index] == b'[' {
            if let Some(close) = text[index..].find("]:") {
                let after = index + close + 2;
                let line_end = text[after..]
                    .find('\n')
                    .map(|offset| offset + after)
                    .unwrap_or(text.len());
                let target = text[after..line_end].trim();
                if !target.is_empty() {
                    targets.push(target.to_string());
                }
                index = line_end;
                continue;
            }
        }

        index += 1;
    }

    targets
}

fn check_links(root: &Path, report: &mut Report) {
    for file in markdown_files(root) {
        let Ok(text) = fs::read_to_string(&file) else {
            report.error(format!("could not read {}", display(root, &file)));
            continue;
        };

        for target in link_targets(&text) {
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
                || target.starts_with('#')
                || target.starts_with('/')
            {
                continue;
            }

            // Drop an optional anchor and title.
            let path_part = target
                .split('#')
                .next()
                .unwrap_or(target.as_str())
                .split_whitespace()
                .next()
                .unwrap_or(target.as_str());

            if path_part.is_empty() {
                continue;
            }

            let resolved = file
                .parent()
                .map(|parent| parent.join(path_part))
                .unwrap_or_else(|| PathBuf::from(path_part));

            if !resolved.exists() {
                report.error(format!("broken link in {}: {target}", display(root, &file)));
            }
        }
    }
}

/// Extracts the feature status from a `status.md` document.
fn feature_status(text: &str) -> Option<&str> {
    for line in text.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("**Status:**") else {
            continue;
        };
        return Some(rest.trim().trim_matches('`').trim());
    }
    None
}

fn check_feature_statuses(root: &Path, report: &mut Report) {
    for feature in FEATURES {
        let path = root.join("docs/features").join(feature).join("status.md");
        let Ok(text) = fs::read_to_string(&path) else {
            report.error(format!(
                "missing feature status document: docs/features/{feature}/status.md"
            ));
            continue;
        };

        match feature_status(&text) {
            None => report.error(format!(
                "docs/features/{feature}/status.md has no '**Status:**' line"
            )),
            Some(status) => {
                if STATUSES.contains(&status) {
                    // fine
                } else {
                    report.error(format!(
                        "docs/features/{feature}/status.md uses the unknown status '{status}'. Allowed: {}",
                        STATUSES.join(", ")
                    ));
                }
            }
        }
    }
}

fn check_adrs(root: &Path, report: &mut Report) {
    let directory = root.join("docs/decisions");
    let Ok(entries) = fs::read_dir(&directory) else {
        report.error("missing directory: docs/decisions".to_string());
        return;
    };

    let mut identifiers: BTreeMap<String, PathBuf> = BTreeMap::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().is_none_or(|extension| extension != "md") {
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if name == "README.md" {
            continue;
        }

        let Some(identifier) = name.strip_suffix(".md") else {
            continue;
        };

        if identifiers
            .insert(identifier.to_string(), path.clone())
            .is_some()
        {
            report.error(format!("duplicate ADR identifier: {identifier}"));
        }

        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };

        if !text.contains("Status:") {
            report.error(format!("ADR {identifier} has no 'Status:' line"));
        }
    }

    if identifiers.is_empty() {
        report.warn("docs/decisions contains no ADR yet".to_string());
    }
}

fn check_change_log(root: &Path, report: &mut Report) {
    let path = root.join("docs/engineering/change-log.md");
    let Ok(text) = fs::read_to_string(&path) else {
        report.error("missing document: docs/engineering/change-log.md".to_string());
        return;
    };

    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("### ") {
            let identifier = rest.split_whitespace().next().unwrap_or_default();
            if identifier.starts_with("ATOM-CHANGE-") && !seen.insert(identifier) {
                report.error(format!("duplicate change identifier: {identifier}"));
            }
        }
    }
}

fn check_feature_directories(root: &Path, report: &mut Report) {
    for feature in FEATURES {
        for required in FEATURE_FILES {
            let path = root.join("docs/features").join(feature).join(required);
            if !path.is_file() {
                report.error(format!(
                    "missing feature document: docs/features/{feature}/{required}"
                ));
            }
        }
    }
}

/// Renders a path relative to the repository root for readable messages.
fn display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Checks that the configuration directory has its required files.
fn check_configuration(root: &Path, report: &mut Report) {
    for required in CONFIG_FILES {
        if !root.join(required).is_file() {
            report.error(format!("missing configuration file: {required}"));
        }
    }
}

/// Checks that `.env.example` and the configuration loader agree about the
/// environment variables, in both directions.
///
/// A variable that the code reads but the template does not document is a
/// variable a developer cannot discover. A variable the template documents but
/// the code does not read is a setting that does nothing.
fn check_environment_variables(root: &Path, report: &mut Report) {
    let example = match fs::read_to_string(root.join(ENV_EXAMPLE)) {
        Ok(contents) => contents,
        Err(error) => {
            report.error(format!("could not read {ENV_EXAMPLE}: {error}"));
            return;
        }
    };

    let loader = match fs::read_to_string(root.join(CONFIG_LOADER)) {
        Ok(contents) => contents,
        Err(error) => {
            report.error(format!("could not read {CONFIG_LOADER}: {error}"));
            return;
        }
    };

    let documented: BTreeSet<&str> = example
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (name, _) = line.split_once('=')?;
            let name = name.trim();
            if name.is_empty()
                || !name
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            {
                return None;
            }
            Some(name)
        })
        .collect();

    let read: BTreeSet<&str> = loader
        .lines()
        .filter_map(|line| {
            let start = line.find('"')?;
            let rest = &line[start + 1..];
            let end = rest.find('"')?;
            Some(&rest[..end])
        })
        .filter(|name| name.starts_with("APP_") || name.starts_with("DATABASE_"))
        .collect();

    for name in read.difference(&documented) {
        report.error(format!(
            "the configuration reads {name}, but {ENV_EXAMPLE} does not document it"
        ));
    }

    for name in documented.difference(&read) {
        // `TEST_DATABASE_URL` is read by the test harness, not by the loader.
        if *name == "TEST_DATABASE_URL" {
            continue;
        }
        report.error(format!(
            "{ENV_EXAMPLE} documents {name}, but the configuration does not read it"
        ));
    }
}
