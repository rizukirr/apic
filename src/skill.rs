//! `apic docs` and `apic skill`: the reference docs and the agent skill built
//! into the binary, so both always match the installed version.

use crate::output::{self, CliError, ErrorKind};
use apic_core::config::find_apic_dir;
use serde_json::json;
use std::fs;
use std::io::ErrorKind as IoKind;
use std::path::{Path, PathBuf};

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

/// Skill folders written under the project root by `apic skill --install`.
/// Codex, Antigravity, Cursor and pi read `.agents/skills`, and Claude Code
/// and Cursor read `.claude/skills`.
const PROJECT_DIRS: &[&str] = &[".agents/skills/apic", ".claude/skills/apic"];

/// Skill folders written under the home directory with `--global`.
/// Antigravity reads `~/.gemini/config/skills` and Hermes `~/.hermes/skills`.
const GLOBAL_DIRS: &[&str] = &[
    ".agents/skills/apic",
    ".claude/skills/apic",
    ".gemini/config/skills/apic",
    ".hermes/skills/apic",
];

/// What an install did to one SKILL.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Created,
    Updated,
    Unchanged,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Created => "created",
            Status::Updated => "updated",
            Status::Unchanged => "unchanged",
        }
    }
}

/// One installed SKILL.md: its path relative to the base, its full path and
/// what happened to it.
#[derive(Debug)]
struct Installed {
    rel: String,
    path: PathBuf,
    status: Status,
}

/// Writes SKILL.md into each of `dirs` under `base`, in order. A missing file
/// is created, a different one overwritten, an equal one left alone. Stops at
/// the first failure, keeping the files already written.
fn install(base: &Path, dirs: &[&str]) -> Result<Vec<Installed>, CliError> {
    let mut done = Vec::new();
    for dir in dirs {
        let folder = base.join(dir);
        fs::create_dir_all(&folder)
            .map_err(|err| format!("Failed to create {}: {err}", folder.display()))?;
        let path = folder.join("SKILL.md");
        let status = match fs::read_to_string(&path) {
            Ok(text) if text == SKILL => Status::Unchanged,
            Ok(_) => Status::Updated,
            Err(err) if err.kind() == IoKind::NotFound => Status::Created,
            Err(_) => Status::Updated,
        };
        if status != Status::Unchanged {
            fs::write(&path, SKILL)
                .map_err(|err| format!("Failed to write {}: {err}", path.display()))?;
        }
        done.push(Installed {
            rel: format!("{dir}/SKILL.md"),
            path,
            status,
        });
    }
    Ok(done)
}

/// Prints the skill, or with `install` writes it into the project's skill
/// folders, or the home directory's with `global`.
pub(crate) fn skill_cmd(install_it: bool, global: bool) -> Result<(), CliError> {
    if !install_it {
        if output::json() {
            output::emit(&json!({"content": SKILL}));
        } else {
            print!("{SKILL}");
        }
        return Ok(());
    }
    let (base, dirs) = if global {
        let home = std::env::home_dir()
            .filter(|home| !home.as_os_str().is_empty())
            .ok_or_else(|| CliError::new(ErrorKind::Error, "No home directory found"))?;
        (home, GLOBAL_DIRS)
    } else {
        let base = match find_apic_dir() {
            Some(apic) => apic.parent().map(Path::to_path_buf).unwrap_or(apic),
            None => std::env::current_dir()
                .map_err(|err| format!("Failed to read the current directory: {err}"))?,
        };
        (base, PROJECT_DIRS)
    };
    let done = install(&base, dirs)?;
    if output::json() {
        let installed: Vec<_> = done
            .iter()
            .map(|entry| json!({"path": entry.path.display().to_string(), "status": entry.status.as_str()}))
            .collect();
        output::emit(&json!({"installed": installed}));
    } else {
        for entry in &done {
            println!("{:<10} {}", entry.status.as_str(), entry.rel);
        }
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

    #[test]
    fn install_reports_created_unchanged_then_updated() {
        let base = std::env::temp_dir().join(format!("apic_skill_install_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let statuses = |base: &Path| {
            install(base, PROJECT_DIRS)
                .unwrap()
                .into_iter()
                .map(|entry| entry.status)
                .collect::<Vec<_>>()
        };
        assert!(statuses(&base).iter().all(|s| *s == Status::Created));
        assert!(statuses(&base).iter().all(|s| *s == Status::Unchanged));

        let edited = base.join(PROJECT_DIRS[0]).join("SKILL.md");
        fs::write(&edited, "hand edit").unwrap();
        let third = statuses(&base);
        assert_eq!(third[0], Status::Updated);
        assert!(third[1..].iter().all(|s| *s == Status::Unchanged));
        assert_eq!(fs::read_to_string(&edited).unwrap(), SKILL);
        let _ = fs::remove_dir_all(&base);
    }
}
