//! `apic send`: fire a contract's request and print the reply.

use crate::cli::resolve_one;
use crate::output::{CliError, ErrorKind};
use crate::render::{highlight_json_line, sanitize, use_color};
use apic_core::config::find_apic_dir;
use apic_core::file::read_file;
use apic_core::http;
use apic_core::json::JsonContent;
use apic_core::vars::{Vars, substitute};
use crossterm::style::Stylize;
use std::io::{IsTerminal, Write};
use std::path::Path;
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
pub(crate) fn apply_var_flags(pairs: &[String], vars: &mut Vars) -> Result<(), String> {
    for pair in pairs {
        let (name, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("--var expects NAME=VALUE, got '{}'", sanitize(pair)))?;
        vars.insert(name.trim().to_string(), value.to_string());
    }
    Ok(())
}

/// Reads and parses the contract at `path`. A file that does not parse is an
/// `invalid` error.
pub(crate) fn load_contract(path: &Path) -> Result<JsonContent, CliError> {
    let content =
        read_file(path).map_err(|err| format!("Failed to read {}: {}", path.display(), err))?;
    serde_json::from_str(&content).map_err(|err| {
        CliError::new(
            ErrorKind::Invalid,
            format!("{} is not a valid contract: {}", path.display(), err),
        )
    })
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

/// Standard base64 with padding, for binary bodies in `--json` output.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The `send --json` document for `reply`. A JSON body is embedded as a value,
/// a UTF-8 body as a string, anything else as base64.
fn reply_json(reply: &http::Reply) -> serde_json::Value {
    let (body, encoding) = match serde_json::from_slice::<serde_json::Value>(&reply.body) {
        Ok(value) => (value, "json"),
        Err(_) => match std::str::from_utf8(&reply.body) {
            Ok(text) => (serde_json::Value::String(text.to_string()), "text"),
            Err(_) => (serde_json::Value::String(base64(&reply.body)), "base64"),
        },
    };
    let headers: Vec<serde_json::Value> = reply
        .headers
        .iter()
        .map(|(name, value)| serde_json::json!({"name": name, "value": value}))
        .collect();
    serde_json::json!({
        "status": reply.status,
        "reason": reply.reason,
        "elapsed_ms": reply.elapsed.as_millis() as u64,
        "headers": headers,
        "body": body,
        "body_encoding": encoding,
    })
}

/// Handles `apic send`.
pub(crate) fn send_cmd(args: &SendArgs) -> Result<(), CliError> {
    let path = resolve_one(&args.find)?;
    let apic_dir = find_apic_dir().ok_or("Not in an apic project (run `apic init`)")?;
    let project_root = apic_dir.parent().unwrap_or(&apic_dir).to_path_buf();
    apic_core::env::ensure_gitignore(&apic_dir)?;

    let contract = load_contract(&path)?;

    let mut vars = match &args.env {
        Some(name) => apic_core::env::load(&apic_dir, name)?,
        None => Vars::new(),
    };
    apply_var_flags(&args.vars, &mut vars)?;

    for name in undefined_in_url_and_headers(&contract, &vars) {
        eprintln!("Warning: ${name} is not defined, sent as written");
    }

    let prepared = http::prepare(&contract, &vars, &project_root)?;
    let reply = http::execute(&prepared, Duration::from_secs(args.timeout))
        .map_err(|err| CliError::new(ErrorKind::Network, err))?;
    if crate::output::json() {
        crate::output::emit(&reply_json(&reply));
        return Ok(());
    }

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
    Ok(print_body(&reply.body)?)
}

/// Handles `apic env`.
pub(crate) fn env_cmd() -> Result<(), CliError> {
    let apic_dir = find_apic_dir().ok_or("Not in an apic project (run `apic init`)")?;
    let envs = apic_core::env::list(&apic_dir);
    if crate::output::json() {
        let rows: Vec<serde_json::Value> = envs
            .iter()
            .map(|e| serde_json::json!({"name": e.name, "has_local": e.has_local}))
            .collect();
        crate::output::emit(&serde_json::json!({"environments": rows}));
        return Ok(());
    }
    if envs.is_empty() {
        println!("No environments. Create .apic/env/<name>.toml");
        return Ok(());
    }
    for e in envs {
        if e.has_local {
            println!("{} (+local)", sanitize(&e.name));
        } else {
            println!("{}", sanitize(&e.name));
        }
    }
    Ok(())
}

/// Handles `apic vars`: with `convert`, rewrites `{{name}}` to `$name` in
/// every contract and template, printing each file and count, and writes only
/// with `write`.
pub(crate) fn vars_cmd(convert: bool, write: bool) -> Result<(), CliError> {
    if !convert {
        return Err("Nothing to do: pass --convert".into());
    }
    let apic_dir = find_apic_dir().ok_or("Not in an apic project (run `apic init`)")?;
    let root = apic_dir.parent().unwrap_or(&apic_dir).to_path_buf();
    let mut files = crate::cli::list(true)?.unwrap_or_default();
    files.extend(apic_core::template::list_templates(&apic_dir));

    let json = crate::output::json();
    let mut rows = Vec::new();
    let mut total = 0;
    for path in files {
        let content = read_file(&path)
            .map_err(|err| format!("Failed to read {}: {}", path.display(), err))?;
        let (converted, count) = apic_core::vars::convert_braces(&content);
        if count == 0 {
            continue;
        }
        total += count;
        let shown = apic_core::file::relative_slash(&path, &root);
        if !json {
            println!("{}: {count}", sanitize(&shown));
        }
        rows.push(serde_json::json!({"path": shown, "count": count}));
        if write {
            std::fs::write(&path, converted)
                .map_err(|err| format!("Failed to write {}: {}", path.display(), err))?;
        }
    }
    if json {
        crate::output::emit(&serde_json::json!({"files": rows, "total": total, "written": write}));
    } else if total == 0 {
        println!("No {{{{name}}}} variables found");
    } else if !write {
        println!("Dry run: pass --write to apply");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::base64;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
