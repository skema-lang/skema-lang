//! `skema` – command-line tool for the `.skema` DSL.
//!
//! ```bash
//! skema check main.skema
//! skema export --json main.skema
//! ```
//!
//! Both invocations appear exactly like this in `docs/02`. This crate is the
//! only place in the workspace that reads files: `skema-syntax` and
//! `skema-hir` only ever receive `&str` or already-parsed trees (see their
//! module documentation).

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
        /// `skema export --json main.skema` matches `docs/02` exactly.
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Check { file, json } => run_check(&file, json),
        Command::Export { file, json: _ } => run_export(&file),
    }
}

fn run_check(path: &Path, json: bool) -> ExitCode {
    let project = match skema_loader::load(path) {
        Ok(p) => p,
        Err(e) => return report_load_error(&e),
    };
    let files = project.input_files();
    let (_model, diagnostics) = skema_hir::analyze(&files);
    let reports = report::build(&project, &diagnostics);
    let has_errors = reports.iter().any(|r| r.severity == "error");

    if json {
        if !print_json(&reports) {
            return ExitCode::FAILURE;
        }
    } else if reports.is_empty() {
        println!("no messages");
    } else {
        for r in &reports {
            println!("{r}");
        }
    }

    if has_errors {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn run_export(path: &Path) -> ExitCode {
    let project = match skema_loader::load(path) {
        Ok(p) => p,
        Err(e) => return report_load_error(&e),
    };
    let files = project.input_files();
    let (model, diagnostics) = skema_hir::analyze(&files);
    let reports = report::build(&project, &diagnostics);
    let has_errors = reports.iter().any(|r| r.severity == "error");

    // Messages go to stderr so stdout stays pure JSON and can be piped.
    for r in &reports {
        eprintln!("{r}");
    }
    if !print_json(&model) {
        return ExitCode::FAILURE;
    }

    if has_errors {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
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

fn report_load_error(e: &skema_loader::LoadError) -> ExitCode {
    eprintln!("{e}");
    ExitCode::FAILURE
}
