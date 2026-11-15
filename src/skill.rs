//! `apic docs` and `apic skill`: the reference docs and the agent skill built
//! into the binary, so both always match the installed version.

use crate::output::{self, CliError};
use serde_json::json;

/// The agent skill, `skills/apic/SKILL.md`.
pub(crate) const SKILL: &str = include_str!("../skills/apic/SKILL.md");

/// A reference doc that `apic docs` prints.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum Doc {
    /// Every command and flag (COMMAND.md).
    Command,
    /// Environments, variables and dry runs (SENDING.md).
    Sending,
    /// The contract file format (CONTRACT.md).
    Contract,
}

impl Doc {
    fn name(self) -> &'static str {
        match self {
            Doc::Command => "command",
            Doc::Sending => "sending",
            Doc::Contract => "contract",
        }
    }

    fn text(self) -> &'static str {
        match self {
            Doc::Command => include_str!("../COMMAND.md"),
            Doc::Sending => include_str!("../SENDING.md"),
            Doc::Contract => include_str!("../CONTRACT.md"),
        }
    }
}

/// Prints `doc` as Markdown, or `{"name", "content"}` in `--json` mode.
pub(crate) fn docs_cmd(doc: Doc) -> Result<(), CliError> {
    if output::json() {
        output::emit(&json!({"name": doc.name(), "content": doc.text()}));
    } else {
        print!("{}", doc.text());
    }
    Ok(())
}

/// Prints the skill, or `{"content"}` in `--json` mode.
pub(crate) fn skill_cmd() -> Result<(), CliError> {
    if output::json() {
        output::emit(&json!({"content": SKILL}));
    } else {
        print!("{SKILL}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_has_name_and_description() {
        let text = SKILL.replace("\r\n", "\n");
        let front = text
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
            .map(|(front, _)| front)
            .expect("SKILL.md starts with a frontmatter block");
        assert!(front.lines().any(|line| line == "name: apic"));
        let description = front
            .lines()
            .find_map(|line| line.strip_prefix("description: "))
            .expect("frontmatter has a description");
        assert!(!description.trim().is_empty());
        assert!(description.len() <= 1024);
    }
}
