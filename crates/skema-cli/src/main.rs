//! `skema` – command-line tool for the `.skema` DSL.
//!
//! ```bash
//! skema check main.skema
//! skema export --json main.skema
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
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Check { file, json } => run_check(&file, json),
        Command::Export {
            file,
            json: _,
            locations,
        } => run_export(&file, locations),
    }
}

fn run_check(path: &Path, json: bool) -> ExitCode {
    let project = match skema_loader::load(path) {
        Ok(p) => p,
        Err(e) => return report_load_error(&e, json),
    };
    let analysis = skema_loader::analyze(&project, false);

    if json {
        if !print_json(&analysis.diagnostics) {
            return ExitCode::FAILURE;
        }
    } else if analysis.diagnostics.is_empty() {
        println!("no messages");
    } else {
        for d in &analysis.diagnostics {
            println!("{}", report::line(d));
        }
    }

    exit_code(analysis.ok())
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
