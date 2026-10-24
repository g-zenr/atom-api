//! `atom-docs` — the repository consistency checker.
//!
//! A small, dependency-free checker that validates the documentation tree and
//! the configuration surface. It is a development tool, not part of the API
//! surface, but it ships with the crate so that every contributor can run it
//! without installing anything else.
//!
//! ```text
//! cargo run --bin atom-docs            # check the docs/ tree and config/
//! cargo run --bin atom-docs -- --strict   # treat warnings as failures
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
//! 9. The configuration files and the operator reference agree about the
//!    settings and about their integer and boolean defaults, in both
//!    directions.

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
const FEATURES: [&str; 7] = [
    "authentication",
    "job-applications",
    "ml",
    "notes",
    "projects",
    "reimbursements",
    "tasks",
];

/// The files that must exist in the configuration directory.
const CONFIG_FILES: [&str; 8] = [
    "config/README.md",
    "config/app.toml",
    "config/api.toml",
    "config/auth.toml",
    "config/database.toml",
    "config/messages.toml",
    "config/environments/development.toml",
    "config/environments/production.toml",
];

/// The configuration files that hold the settings, and the section of the
/// operator reference that documents them.
///
/// The message catalogue is not here: it is documented in `config/README.md`,
/// and its keys are messages, not settings.
const SETTINGS_FILES: [&str; 4] = [
    "config/app.toml",
    "config/api.toml",
    "config/auth.toml",
    "config/database.toml",
];

/// The document that is the operator's reference for the settings.
const SETTINGS_REFERENCE: &str = "docs/architecture/deployment.md";

/// The second operator reference, in the configuration directory.
///
/// It repeats the same table for a reader who starts in `config/`. A second
/// table that disagrees with the first is worse than no second table, because
/// the reader trusts whichever one they happened to open. It is compared with
/// the settings for the same reason the first one is.
const SETTINGS_REFERENCE_SECONDARY: &str = "config/README.md";

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
    check_settings_reference(&root, &mut report);

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
        if bytes[index] == b'['
            && let Some(close) = text[index..].find("](")
        {
            let after = index + close + 2;
            if let Some(end) = text[after..].find(')') {
                let target = text[after..after + end].trim();
                targets.push(target.to_string());
                index = after + end + 1;
                continue;
            }
        }

        // Reference link: [label]: target
        if bytes[index] == b'['
            && let Some(close) = text[index..].find("]:")
        {
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
        .filter(|name| {
            name.starts_with("APP_") || name.starts_with("DATABASE_") || name.starts_with("ATOM_")
        })
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

/// Checks that the configuration files and the operator reference agree about
/// the settings, in both directions.
///
/// A setting that exists in a file but in no reference table is a setting an
/// operator cannot find. A setting in the reference table that exists in no
/// file is a setting that cannot be set: writing it into a configuration file
/// is a start-up error, because every settings structure refuses unknown
/// keys.
fn check_settings_reference(root: &Path, report: &mut Report) {
    let mut values: BTreeMap<String, toml::Value> = BTreeMap::new();
    let mut comparable: BTreeMap<String, SettingValue> = BTreeMap::new();

    for file in SETTINGS_FILES {
        let path = root.join(file);
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) => {
                report.error(format!("could not read {file}: {error}"));
                continue;
            }
        };

        let table: toml::Table = match contents.parse() {
            Ok(table) => table,
            Err(error) => {
                report.error(format!("could not parse {file}: {error}"));
                continue;
            }
        };

        collect_setting_values(&table, "", &mut values, &mut comparable);
    }

    let configured: BTreeSet<String> = values.keys().cloned().collect();

    let reference = match fs::read_to_string(root.join(SETTINGS_REFERENCE)) {
        Ok(contents) => contents,
        Err(error) => {
            report.error(format!("could not read {SETTINGS_REFERENCE}: {error}"));
            return;
        }
    };

    let documented = reference_setting_keys(&reference);

    for key in configured.difference(&documented) {
        report.error(format!(
            "{SETTINGS_REFERENCE} does not document the setting {key}"
        ));
    }

    for key in documented.difference(&configured) {
        report.error(format!(
            "{SETTINGS_REFERENCE} documents the setting {key}, but no configuration file defines it"
        ));
    }

    // The second reference table is checked as well. The project keeps two
    // operator tables -- one in `docs/architecture/`, one in `config/` -- and
    // a second table that disagrees with the first is worse than no second
    // table, because the reader trusts whichever one they happened to open.
    let secondary = match fs::read_to_string(root.join(SETTINGS_REFERENCE_SECONDARY)) {
        Ok(contents) => contents,
        Err(error) => {
            report.error(format!(
                "could not read {SETTINGS_REFERENCE_SECONDARY}: {error}"
            ));
            return;
        }
    };

    let repeated = reference_setting_keys(&secondary);

    for key in configured.difference(&repeated) {
        report.error(format!(
            "{SETTINGS_REFERENCE_SECONDARY} does not document the setting {key}"
        ));
    }

    for key in repeated.difference(&configured) {
        report.error(format!(
            "{SETTINGS_REFERENCE_SECONDARY} documents the setting {key}, but no configuration file defines it"
        ));
    }

    // Names alone are not enough: `app.limits.shutdown_grace_seconds` was
    // documented as `100` in `config/README.md` while the file set it to `10`,
    // and every check above passed. The defaults are compared in both tables.
    for message in compare_setting_values(SETTINGS_REFERENCE, &comparable, &reference) {
        report.error(message);
    }

    for message in compare_setting_values(SETTINGS_REFERENCE_SECONDARY, &comparable, &secondary) {
        report.error(message);
    }
}

/// Compares the integer and boolean defaults of the configuration files with
/// the defaults documented in one reference table.
///
/// The comparison is deliberately narrow, because a false failure in a gate is
/// worse than a check that is narrower than it could be:
///
/// - Only values [`SettingValue`] can model are compared. A string, a float or
///   an array has no single documented spelling, so it is never collected.
/// - A value cell that is not a plain integer or a plain `true`/`false` is
///   skipped, not reported: several documented defaults are prose, such as
///   `` `1048576` (1 MiB) `` or `` `900` (15 min) ``.
/// - A key that appears on more than one row of the same document is skipped
///   for value in that document. The repeated rows document another layer with
///   another layer's value -- `app.limits.request_timeout_seconds` is `30` in
///   the base file and `60` in the development overlay -- and both rows are
///   correct. The key is still compared in a document where it is unique.
fn compare_setting_values(
    document: &str,
    configured: &BTreeMap<String, SettingValue>,
    reference: &str,
) -> Vec<String> {
    let rows = reference_setting_rows(reference);
    let mut errors = Vec::new();

    for (key, value) in configured {
        let Some(cells) = rows.get(key) else {
            continue;
        };

        if cells.len() > 1 {
            continue;
        }

        let Some(expected) = cells[0] else {
            continue;
        };

        if expected == *value {
            continue;
        }

        errors.push(format!(
            "{document} documents the setting {key} as {}, but the configuration file value is {}",
            expected.render(),
            value.render()
        ));
    }

    errors
}

/// A setting value, from a configuration file or from a reference table.
///
/// Only the two shapes the reference tables write unambiguously are modelled;
/// everything else is prose and is never compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingValue {
    Integer(i64),
    Boolean(bool),
}

impl SettingValue {
    /// Reads the comparable form of a TOML leaf, if it has one.
    fn from_toml(value: &toml::Value) -> Option<Self> {
        match value {
            toml::Value::Integer(number) => Some(Self::Integer(*number)),
            toml::Value::Boolean(flag) => Some(Self::Boolean(*flag)),
            _ => None,
        }
    }

    /// Reads a documented default from a table cell.
    ///
    /// The cell must be a plain integer or a plain `true`/`false`, optionally
    /// wrapped in the backticks the tables use for values. Anything else -- a
    /// size with a unit, a duration with a comment, a sentence -- is prose and
    /// yields `None`, so the comparison skips it instead of reporting it.
    fn from_cell(cell: &str) -> Option<Self> {
        let trimmed = cell.trim();
        let text = trimmed
            .strip_prefix('`')
            .and_then(|rest| rest.strip_suffix('`'))
            .unwrap_or(trimmed);

        if let Ok(number) = text.parse::<i64>() {
            return Some(Self::Integer(number));
        }

        match text {
            "true" => Some(Self::Boolean(true)),
            "false" => Some(Self::Boolean(false)),
            _ => None,
        }
    }

    /// Renders the value for an error message.
    fn render(self) -> String {
        match self {
            Self::Integer(number) => number.to_string(),
            Self::Boolean(flag) => flag.to_string(),
        }
    }
}

/// Collects the leaf key paths of a TOML table and their values.
///
/// The raw value is kept for the key-set comparison, which must see every leaf.
/// The comparable subset is collected in the same walk, so that the value
/// comparison can never be handed a value it is unable to judge.
fn collect_setting_values(
    table: &toml::Table,
    prefix: &str,
    values: &mut BTreeMap<String, toml::Value>,
    comparable: &mut BTreeMap<String, SettingValue>,
) {
    for (name, value) in table {
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}.{name}")
        };

        match value {
            toml::Value::Table(inner) => {
                collect_setting_values(inner, &path, values, comparable);
            }
            _ => {
                if let Some(setting) = SettingValue::from_toml(value) {
                    comparable.insert(path.clone(), setting);
                }
                values.insert(path, value.clone());
            }
        }
    }
}

/// Extracts the setting keys and their documented values from a Markdown
/// table.
///
/// Only a backticked, dotted, lower-case name in the first cell is taken, and
/// only one of the four settings sections. Anything else in a table cell is
/// prose. A value cell that is not a plain integer or a plain `true`/`false`
/// becomes `None`, which the comparison skips.
///
/// A key maps to **every** row that mentions it, because a repeated key means
/// the document describes another layer and cannot be compared by value; see
/// [`compare_setting_values`].
fn reference_setting_rows(contents: &str) -> BTreeMap<String, Vec<Option<SettingValue>>> {
    let mut rows: BTreeMap<String, Vec<Option<SettingValue>>> = BTreeMap::new();

    for line in contents.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }

        let mut cells = line.split('|');
        let Some(cell) = cells.nth(1) else {
            continue;
        };
        let cell = cell.trim();

        if !cell.starts_with('`') || !cell.ends_with('`') || cell.len() < 3 {
            continue;
        }
        let key = &cell[1..cell.len() - 1];

        if !key.contains('.') {
            continue;
        }
        if !key.split('.').all(|part| {
            !part.is_empty()
                && part.chars().all(|character| {
                    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
                })
        }) {
            continue;
        }
        if !(key.starts_with("app.")
            || key.starts_with("api.")
            || key.starts_with("auth.")
            || key.starts_with("database."))
        {
            continue;
        }

        let value_cell = cells.next().unwrap_or_default();
        rows.entry(key.to_string())
            .or_default()
            .push(SettingValue::from_cell(value_cell));
    }

    rows
}

/// Extracts the setting keys named in the rows of a Markdown table.
///
/// The keys are what the name comparison in [`check_settings_reference`]
/// needs; the values live next to them in [`reference_setting_rows`].
fn reference_setting_keys(contents: &str) -> BTreeSet<String> {
    reference_setting_rows(contents).into_keys().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses a TOML snippet the way the checker parses a configuration file.
    fn configuration(source: &str) -> BTreeMap<String, SettingValue> {
        let table: toml::Table = source.parse().expect("the test snippet is valid TOML");
        let mut values = BTreeMap::new();
        let mut comparable = BTreeMap::new();
        collect_setting_values(&table, "", &mut values, &mut comparable);
        comparable
    }

    #[test]
    fn a_numeric_default_that_differs_from_the_file_is_reported() {
        let configured = configuration("[app.limits]\nshutdown_grace_seconds = 10\n");
        let reference = "| Key | Default | Environment variable |\n\
                         | --- | ------- | -------------------- |\n\
                         | `app.limits.shutdown_grace_seconds` | `100` | `APP_SHUTDOWN_GRACE_SECONDS` |\n";

        let errors = compare_setting_values(SETTINGS_REFERENCE_SECONDARY, &configured, reference);

        let expected = format!(
            "{SETTINGS_REFERENCE_SECONDARY} documents the setting \
             app.limits.shutdown_grace_seconds as 100, but the configuration file value is 10"
        );
        assert_eq!(errors, vec![expected]);
    }

    #[test]
    fn a_numeric_default_that_matches_is_not_reported() {
        let configured = configuration("[app.limits]\nshutdown_grace_seconds = 10\n");
        let reference = "| Key | Default | Environment variable |\n\
                         | --- | ------- | -------------------- |\n\
                         | `app.limits.shutdown_grace_seconds` | `10` | `APP_SHUTDOWN_GRACE_SECONDS` |\n";

        let errors = compare_setting_values(SETTINGS_REFERENCE_SECONDARY, &configured, reference);

        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[test]
    fn a_boolean_default_that_differs_is_reported() {
        let configured = configuration("[api]\ndocs_enabled = false\n");
        let reference = "| Key | Default | Environment variable |\n\
                         | --- | ------- | -------------------- |\n\
                         | `api.docs_enabled` | `true` | `APP_API_DOCS_ENABLED` |\n";

        let errors = compare_setting_values(SETTINGS_REFERENCE, &configured, reference);

        let expected = format!(
            "{SETTINGS_REFERENCE} documents the setting api.docs_enabled as true, \
             but the configuration file value is false"
        );
        assert_eq!(errors, vec![expected]);
    }
}
