//! Reads a `.skema` file and prints the syntax tree and messages.
//!
//! ```bash
//! cargo run --example dump_ast -- examples/myexample.skema
//! ```
//!
//! # What this is good for
//!
//! When building a parser, the most important question is always: *What
//! did it actually understand?* This program answers that in a second and
//! is thereby more useful than any debugger.
//!
//! # Along the way: the architecture rule in action
//!
//! `std::fs::read_to_string` appears here — nowhere in the `skema-syntax`
//! crate does it appear. That's exactly the intent: the library receives a
//! `&str` and knows nothing about files. Only that way can it later be
//! compiled to WASM, where there's no filesystem.
//!
//! An `examples/` target is its own binary and is therefore allowed to do
//! what the library isn't.

use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: cargo run --example dump_ast -- <file.skema>");
        return ExitCode::FAILURE;
    };

    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let (parsed, messages) = skema_syntax::parse(&source);

    println!("── Syntax tree {}", "─".repeat(50));
    print!("{}", skema_syntax::dump(&parsed.syntax()));

    if messages.is_empty() {
        println!("\n✓ no messages");
        return ExitCode::SUCCESS;
    }

    println!("\n── Messages {}", "─".repeat(51));
    for m in &messages {
        // Line/column is only computed here — the core only has byte
        // offsets (see `span.rs`). This is the only place in the program
        // where the conversion happens.
        let (line, column) = line_and_column(&source, m.span.start as usize);
        println!("{path}:{line}:{column}: {:?}: {}", m.severity, m.message);
    }
    ExitCode::FAILURE
}

/// Byte offset → 1-based line and column.
///
/// The column counts **characters**, not bytes — otherwise the pointer
/// would land in the wrong place next to non-ASCII characters. (A
/// language server would need UTF-16 units here; a third counting scheme,
/// same principle.)
fn line_and_column(src: &str, offset: usize) -> (usize, usize) {
    let before = src.get(..offset).unwrap_or(src);
    let line = before.matches('\n').count() + 1;
    let column = match before.rfind('\n') {
        Some(i) => before[i + 1..].chars().count() + 1,
        None => before.chars().count() + 1,
    };
    (line, column)
}
