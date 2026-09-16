//! `skema` – command-line tool for the `.skema` DSL.
//!
//! ```bash
//! skema check main.skema
//! skema export --json main.skema
//! skema diff old.skema new.skema
//! ```
//!
//! The work happens in `skema-loader`: it loads the import graph and returns
//! `skema-api`'s model and diagnostics, the same types the language server
//! and the browser editor answer with. This crate adds the process around
//! it — argument parsing, stdout/stderr, exit code.

mod report;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use skema_loader::ProjectAnalysis;

#[derive(Parser)]
#[command(name = "skema", version, about = "Checks and exports .skema projects")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Loads the root file along with all its `import`s and reports every
    /// syntax and semantic error.
    Check {
        file: PathBuf,
        /// Print messages as a JSON array instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Exports the resolved model (tables, enums, groups) as JSON.
    Export {
        file: PathBuf,
        /// Currently the only format – accepted as a flag so that
        /// `skema export --json project.skema` matches `docs/02` exactly.
        #[arg(long)]
        json: bool,
        /// Add a `location` (file, line, column) to every entity in the
        /// model. Off by default: it roughly doubles the output, and a
        /// consumer that only wants the structure never reads it.
        #[arg(long)]
        locations: bool,
    },
    /// Structurally diffs the resolved models of two projects, each loaded
    /// and checked independently (not git refs – see `README.md`).
    Diff {
        old: PathBuf,
        new: PathBuf,
        /// Print the diff as JSON instead of one line per change.
        #[arg(long)]
        json: bool,
        /// Exit non-zero when the diff contains a breaking change.
        #[arg(long)]
        fail_on_breaking: bool,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Check { file, json } => run_check(&file, json),
        Command::Export {
            file,
            json: _,
            locations,
        } => run_export(&file, locations),
        Command::Diff {
            old,
            new,
            json,
            fail_on_breaking,
        } => run_diff(&old, &new, json, fail_on_breaking),
    }
}

fn run_check(path: &Path, json: bool) -> ExitCode {
    let project = match skema_loader::load(path) {
        Ok(p) => p,
        Err(e) => return report_load_error(&e, json),
    };
    let analysis = skema_loader::analyze(&project, false);
    if !report_diagnostics(&analysis, json) {
        return ExitCode::FAILURE;
    }
    exit_code(analysis.ok())
}

/// The `check` output: a JSON array, or one line per diagnostic (`no
/// messages` when there are none). Shared with `diff`, which reports a side
/// this way whenever it fails to check cleanly on its own. Returns `false`
/// on a JSON serialization failure, the same signal `print_json` gives.
fn report_diagnostics(analysis: &ProjectAnalysis, json: bool) -> bool {
    if json {
        return print_json(&analysis.diagnostics);
    }
    if analysis.diagnostics.is_empty() {
        println!("no messages");
    } else {
        for d in &analysis.diagnostics {
            println!("{}", report::line(d));
        }
    }
    true
}

fn run_export(path: &Path, locations: bool) -> ExitCode {
    let project = match skema_loader::load(path) {
        Ok(p) => p,
        Err(e) => return report_load_error(&e, false),
    };
    let analysis = skema_loader::analyze(&project, locations);

    // Messages go to stderr so stdout stays nothing but the model and can be
    // redirected into a file (`skema export --json x.skema > model.json`,
    // `docs/02`).
    for d in &analysis.diagnostics {
        eprintln!("{}", report::line(d));
    }
    if !print_json(&analysis.model) {
        return ExitCode::FAILURE;
    }

    exit_code(analysis.ok())
}

fn run_diff(old: &Path, new: &Path, json: bool, fail_on_breaking: bool) -> ExitCode {
    let old_analysis = match load_checked(old, json) {
        Ok(a) => a,
        Err(code) => return code,
    };
    let new_analysis = match load_checked(new, json) {
        Ok(a) => a,
        Err(code) => return code,
    };

    let diff = skema_api::diff_models(&old_analysis.model, &new_analysis.model);

    if json {
        if !print_json(&diff) {
            return ExitCode::FAILURE;
        }
    } else if diff.tables.is_empty() && diff.enums.is_empty() {
        println!("no differences");
    } else {
        for t in &diff.tables {
            println!("{}", report::table_diff_line(t));
        }
        for e in &diff.enums {
            println!("{}", report::enum_diff_line(e));
        }
    }

    if fail_on_breaking && diff.has_breaking_changes {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Loads and checks one side of a `diff`, exactly like `run_check` – a model
/// that doesn't check cleanly on its own isn't meaningful to diff. `Err`
/// already carries the exit code, with the load error or diagnostics
/// reported on the way.
fn load_checked(path: &Path, json: bool) -> Result<ProjectAnalysis, ExitCode> {
    let project = skema_loader::load(path).map_err(|e| report_load_error(&e, json))?;
    let analysis = skema_loader::analyze(&project, false);
    if !analysis.ok() {
        report_diagnostics(&analysis, json);
        return Err(ExitCode::FAILURE);
    }
    Ok(analysis)
}

fn print_json(value: &impl serde::Serialize) -> bool {
    match serde_json::to_string_pretty(value) {
        Ok(s) => {
            println!("{s}");
            true
        }
        Err(e) => {
            eprintln!("JSON error: {e}");
            false
        }
    }
}

/// An unreadable root file is reported as the same `Diagnostic` as every
/// other message, so `--json` stays a single, uniformly shaped array rather
/// than "the list, unless the very first step failed".
fn report_load_error(e: &skema_loader::LoadError, json: bool) -> ExitCode {
    let diagnostic = skema_loader::load_error_diagnostic(e);
    if json {
        print_json(&[diagnostic]);
    } else {
        eprintln!("{}", report::line(&diagnostic));
    }
    ExitCode::FAILURE
}

fn exit_code(ok: bool) -> ExitCode {
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
