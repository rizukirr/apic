//! `apic` — API contract tooling.
//!
//! Binary entry point. The command-line interface and all subcommand handling
//! live in [`cli`]; everything below is wiring for the supporting modules.

mod cli;
mod config;
mod file;
mod fuzzy;
mod json;
mod render;

/// Delegates to [`cli::run`], which parses the command line and dispatches to
/// the matching subcommand handler.
fn main() {
    cli::run();
}
