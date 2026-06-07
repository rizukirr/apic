//! `apic` — API contract tooling.
//!
//! Binary entry point. The command-line interface and all subcommand handling
//! live in [`cli`]; everything below is wiring for the supporting modules.

mod cli;
mod config;
mod file;
mod fuzzy;
mod json;
mod picker;
mod render;
mod tree;

/// Restores the default `SIGPIPE` disposition on Unix.
///
/// Rust installs `SIG_IGN` for `SIGPIPE` at startup, which turns a write to a
/// closed pipe (e.g. `apic read | head`) into a panic instead of a clean exit.
/// Resetting to `SIG_DFL` makes the process terminate quietly like a normal
/// Unix tool.
#[cfg(unix)]
fn reset_sigpipe() {
    // SAFETY: `signal` with `SIG_DFL` for `SIGPIPE` is async-signal-safe and is
    // the documented way to opt back into default pipe behavior. Called once,
    // before any output, at the very start of `main`.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}

/// Delegates to [`cli::run`], which parses the command line and dispatches to
/// the matching subcommand handler.
fn main() {
    reset_sigpipe();
    cli::run();
}
