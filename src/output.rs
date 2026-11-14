//! Machine-readable output: the `--json` switch, typed errors and the exit-code
//! table shared by text and JSON mode.

use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};

static JSON: AtomicBool = AtomicBool::new(false);

/// Turns `--json` mode on or off for the rest of the run.
pub(crate) fn set_json(on: bool) {
    JSON.store(on, Ordering::Relaxed);
}

/// Whether `--json` mode is on.
pub(crate) fn json() -> bool {
    JSON.load(Ordering::Relaxed)
}

/// Prints `value` as one compact JSON document on stdout.
pub(crate) fn emit(value: &Value) {
    println!("{value}");
}

/// What kind of failure an error is. Decides the exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorKind {
    Usage,
    NotInitialized,
    Error,
    NotFound,
    Ambiguous,
    Invalid,
    Network,
    /// A live response differs from its contract. Only a run's final status, never an error document.
    Mismatch,
}

impl ErrorKind {
    /// The `kind` string in a JSON error document.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ErrorKind::Usage => "usage",
            ErrorKind::NotInitialized => "not_initialized",
            ErrorKind::Error => "error",
            ErrorKind::NotFound => "not_found",
            ErrorKind::Ambiguous => "ambiguous",
            ErrorKind::Invalid => "invalid",
            ErrorKind::Network => "network",
            ErrorKind::Mismatch => "mismatch",
        }
    }

    /// The process exit code for this kind.
    pub(crate) fn exit_code(self) -> i32 {
        match self {
            ErrorKind::Usage | ErrorKind::NotInitialized | ErrorKind::Error => 1,
            ErrorKind::NotFound | ErrorKind::Ambiguous => 2,
            ErrorKind::Invalid => 3,
            ErrorKind::Network => 4,
            ErrorKind::Mismatch => 5,
        }
    }
}

/// A failed command. An empty `message` means the command already printed
/// its own report, so only the exit code is left to apply.
#[derive(Debug)]
pub(crate) struct CliError {
    pub(crate) kind: ErrorKind,
    pub(crate) message: String,
}

impl CliError {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        CliError {
            kind,
            message: message.into(),
        }
    }

    /// A failure whose output the command already printed.
    pub(crate) fn reported(kind: ErrorKind) -> Self {
        CliError::new(kind, String::new())
    }
}

impl From<String> for CliError {
    fn from(message: String) -> Self {
        CliError::new(ErrorKind::Error, message)
    }
}

impl From<&str> for CliError {
    fn from(message: &str) -> Self {
        CliError::new(ErrorKind::Error, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_follow_the_table() {
        assert_eq!(ErrorKind::Usage.exit_code(), 1);
        assert_eq!(ErrorKind::NotInitialized.exit_code(), 1);
        assert_eq!(ErrorKind::Error.exit_code(), 1);
        assert_eq!(ErrorKind::NotFound.exit_code(), 2);
        assert_eq!(ErrorKind::Ambiguous.exit_code(), 2);
        assert_eq!(ErrorKind::Invalid.exit_code(), 3);
        assert_eq!(ErrorKind::Network.exit_code(), 4);
        assert_eq!(ErrorKind::Mismatch.exit_code(), 5);
    }
}
