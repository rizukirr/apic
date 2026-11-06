//! `apic send`: fire a contract's request and print the reply.

use crate::cli::{Resolved, resolve_one};
use crate::render::{highlight_json_line, sanitize, use_color};
use apic_core::config::find_apic_dir;
use apic_core::file::read_file;
use apic_core::http;
use apic_core::json::JsonContent;
use apic_core::vars::{Vars, substitute};
use crossterm::style::Stylize;
use std::io::{IsTerminal, Write};
use std::time::Duration;

/// Parsed `apic send` arguments.
pub(crate) struct SendArgs {
    /// The contract query.
    pub(crate) find: String,
    /// The environment name.
    pub(crate) env: Option<String>,
    /// Raw `NAME=VALUE` pairs.
    pub(crate) vars: Vec<String>,
    /// Print status and headers before the body.
    pub(crate) include: bool,
    /// Timeout in seconds.
    pub(crate) timeout: u64,
}

/// Parses `NAME=VALUE` pairs into `vars`, later ones winning.
fn apply_var_flags(pairs: &[String], vars: &mut Vars) -> Result<(), String> {
    for pair in pairs {
        let (name, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("--var expects NAME=VALUE, got '{}'", sanitize(pair)))?;
        vars.insert(name.trim().to_string(), value.to_string());
    }
    Ok(())
}

/// Undefined variable names used in the URL or header values.
fn undefined_in_url_and_headers(contract: &JsonContent, vars: &Vars) -> Vec<String> {
    let mut names = substitute(&contract.url, vars).1;
    for h in &contract.headers {
        for name in substitute(&h.value, vars).1 {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// Writes the reply body: pretty, highlighted JSON on a terminal, raw bytes
/// otherwise.
fn print_body(body: &[u8]) -> Result<(), String> {
    let mut out = std::io::stdout().lock();
    let io = |err: std::io::Error| err.to_string();
    if !std::io::stdout().is_terminal() {
        return out.write_all(body).map_err(io);
    }
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(body)
        && let Ok(pretty) = serde_json::to_string_pretty(&value)
    {
        let color = use_color();
        for line in pretty.lines() {
            let line = if color {
                highlight_json_line(line)
            } else {
                line.to_string()
            };
            writeln!(out, "{line}").map_err(io)?;
        }
        return Ok(());
    }
    // Untrusted bytes on a terminal: strip control characters per line.
    for line in String::from_utf8_lossy(body).lines() {
        writeln!(out, "{}", sanitize(line)).map_err(io)?;
    }
    Ok(())
}

/// Handles `apic send`.
pub(crate) fn send_cmd(args: &SendArgs) -> Result<(), String> {
    let path = match resolve_one(&args.find)? {
        Resolved::Path(path) => path,
        Resolved::NotFound => {
            return Err(format!("No contract matches '{}'", sanitize(&args.find)));
        }
    };
    let apic_dir = find_apic_dir().ok_or("Not in an apic project (run `apic init`)")?;
    let project_root = apic_dir.parent().unwrap_or(&apic_dir).to_path_buf();
    apic_core::env::ensure_gitignore(&apic_dir)?;

    let content =
        read_file(&path).map_err(|err| format!("Failed to read {}: {}", path.display(), err))?;
    let contract: JsonContent = serde_json::from_str(&content)
        .map_err(|err| format!("{} is not a valid contract: {}", path.display(), err))?;

    let mut vars = match &args.env {
        Some(name) => apic_core::env::load(&apic_dir, name)?,
        None => Vars::new(),
    };
    apply_var_flags(&args.vars, &mut vars)?;

    for name in undefined_in_url_and_headers(&contract, &vars) {
        eprintln!("Warning: ${name} is not defined, sent as written");
    }

    let reply = http::send(
        &contract,
        &vars,
        &project_root,
        Duration::from_secs(args.timeout),
    )?;

    let status_line = format!("HTTP {} {}", reply.status, reply.reason);
    let summary = format!("{status_line} · {} ms", reply.elapsed.as_millis());
    let summary = if std::io::stderr().is_terminal()
        && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
    {
        match reply.status {
            200..=299 => summary.green().to_string(),
            300..=399 => summary.yellow().to_string(),
            _ => summary.red().to_string(),
        }
    } else {
        summary
    };
    eprintln!("{summary}");

    if args.include {
        let mut out = std::io::stdout().lock();
        let io = |err: std::io::Error| err.to_string();
        writeln!(out, "{status_line}").map_err(io)?;
        for (name, value) in &reply.headers {
            writeln!(out, "{}: {}", sanitize(name), sanitize(value)).map_err(io)?;
        }
        writeln!(out).map_err(io)?;
    }
    print_body(&reply.body)
}
