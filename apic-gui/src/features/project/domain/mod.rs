//! What a picked folder means, as a pure decision. No filesystem, process,
//! thread or egui.

use std::path::PathBuf;

/// What to do with a folder the user picked to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpenDecision {
    /// It is already a project.
    Open,
    /// It is not a project and its contracts are valid: initialize it.
    Init,
    /// It is not a project and these contracts are invalid.
    Blocked(Vec<(PathBuf, String)>),
}

/// Decides from whether `.apic/config.toml` exists and the folder's
/// validation failures, the rule `finish_open` applies today.
pub(crate) fn open_decision(has_config: bool, failures: Vec<(PathBuf, String)>) -> OpenDecision {
    if has_config {
        OpenDecision::Open
    } else if failures.is_empty() {
        OpenDecision::Init
    } else {
        OpenDecision::Blocked(failures)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure() -> Vec<(PathBuf, String)> {
        vec![(PathBuf::from("bad.json"), "invalid".to_string())]
    }

    #[test]
    fn a_folder_with_a_config_opens_even_with_failures() {
        assert_eq!(open_decision(true, failure()), OpenDecision::Open);
    }

    #[test]
    fn a_folder_without_a_config_and_no_failures_is_initialized() {
        assert_eq!(open_decision(false, Vec::new()), OpenDecision::Init);
    }

    #[test]
    fn a_folder_without_a_config_and_failures_is_blocked() {
        assert_eq!(
            open_decision(false, failure()),
            OpenDecision::Blocked(failure())
        );
    }
}
