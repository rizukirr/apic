//! Variable substitution for contracts.
//!
//! `$name`, `${name}` and `{{name}}` are filled from a [`Vars`] map, and `$$`
//! writes a literal `$`. A name is `[A-Za-z_][A-Za-z0-9_]*`. Only defined names
//! are replaced: anything else (`$set`, `$5`, a lone `$`) stays as written, so
//! text that merely contains a dollar sign is never mangled. `{name}` path
//! params (single braces) are filled separately by [`fill_path_params`].

use std::collections::BTreeMap;

/// Variable values by name.
pub type Vars = BTreeMap<String, String>;

fn is_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_name(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric()
}

/// Byte length of the name at the start of `s`, or 0 when `s` does not start
/// with one.
fn name_len(s: &str) -> usize {
    match s.chars().next() {
        Some(c) if is_start(c) => {}
        _ => return 0,
    }
    s.char_indices()
        .find(|&(_, c)| !is_name(c))
        .map_or(s.len(), |(i, _)| i)
}

/// Whether `s` is exactly one name.
fn is_whole_name(s: &str) -> bool {
    !s.is_empty() && name_len(s) == s.len()
}

/// A variable token at the start of `tail`: its byte length and the name it
/// refers to, or `None` for `$$`.
fn token(tail: &str) -> Option<(usize, Option<&str>)> {
    if tail.starts_with("$$") {
        return Some((2, None));
    }
    if let Some(rest) = tail.strip_prefix("${") {
        let n = name_len(rest);
        if n > 0 && rest[n..].starts_with('}') {
            return Some((n + 3, Some(&rest[..n])));
        }
        return None;
    }
    if let Some(rest) = tail.strip_prefix('$') {
        let n = name_len(rest);
        return (n > 0).then(|| (n + 1, Some(&rest[..n])));
    }
    if let Some(rest) = tail.strip_prefix("{{") {
        let end = rest.find("}}")?;
        let inner = rest[..end].trim();
        return is_whole_name(inner).then(|| (end + 4, Some(inner)));
    }
    None
}

/// Fills `$name`, `${name}` and `{{name}}` in `text` from `vars`, and writes
/// `$$` as `$`. Returns the filled text and the undefined names seen, each once
/// in first-seen order. Undefined names stay literal.
pub fn substitute(text: &str, vars: &Vars) -> (String, Vec<String>) {
    let mut out = String::with_capacity(text.len());
    let mut undefined: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find(['$', '{']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        match token(tail) {
            Some((len, None)) => {
                out.push('$');
                rest = &tail[len..];
            }
            Some((len, Some(name))) => {
                match vars.get(name) {
                    Some(value) => out.push_str(value),
                    None => {
                        out.push_str(&tail[..len]);
                        if !undefined.iter().any(|u| u == name) {
                            undefined.push(name.to_string());
                        }
                    }
                }
                rest = &tail[len..];
            }
            None => {
                // `$` and `{` are one byte each.
                out.push_str(&tail[..1]);
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    (out, undefined)
}

/// Rebuilds `url`, passing the name of every `{name}` path param (outside
/// `{{...}}`) to `f` and writing its return value, or the token unchanged when
/// `f` returns `None`.
fn map_path_params(url: &str, mut f: impl FnMut(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(url.len());
    let mut rest = url;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if let Some(inner) = tail.strip_prefix("{{") {
            let len = inner.find("}}").map_or(tail.len(), |end| end + 4);
            out.push_str(&tail[..len]);
            rest = &tail[len..];
            continue;
        }
        let body = &tail[1..];
        let n = name_len(body);
        if n > 0 && body[n..].starts_with('}') {
            let name = &body[..n];
            match f(name) {
                Some(value) => out.push_str(&value),
                None => out.push_str(&tail[..n + 2]),
            }
            rest = &tail[n + 2..];
        } else {
            out.push('{');
            rest = body;
        }
    }
    out.push_str(rest);
    out
}

/// Fills `{name}` path params in `url` from `vars`. Undefined ones stay.
pub fn fill_path_params(url: &str, vars: &Vars) -> String {
    map_path_params(url, |name| vars.get(name).cloned())
}

/// The names of the `{name}` path params still in `url`, each once.
pub fn path_params(url: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    map_path_params(url, |name| {
        if !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
        None
    });
    names
}

/// One variable reference found by [`tokens`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Byte range of the whole reference in the text, delimiters included.
    pub range: std::ops::Range<usize>,
    /// The name the reference points at.
    pub name: String,
}

/// Finds every variable reference in `text`, in order: `$name`, `${name}`,
/// `{{name}}` and `{name}` path params. `$$` is an escaped dollar, not a
/// reference. A `{{` that does not hold exactly one name is not a reference,
/// and neither of its two braces starts a path param.
///
/// # Arguments
///
/// * `text` - a URL or a value, as written in the contract
///
/// # Returns
///
/// Each reference's byte range and name. A highlighter colours the ranges,
/// so values are never needed.
///
/// # Examples
///
/// ```
/// use apic_core::vars::tokens;
/// let found = tokens("$host/u/{id}?k={{ key }}&p=$$5");
/// let names: Vec<&str> = found.iter().map(|t| t.name.as_str()).collect();
/// assert_eq!(names, ["host", "id", "key"]);
/// assert_eq!(found[1].range, 8..12);
/// ```
pub fn tokens(text: &str) -> Vec<Token> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(i) = text[at..].find(['$', '{']) {
        let start = at + i;
        let tail = &text[start..];
        let len = match token(tail) {
            Some((len, None)) => len,
            Some((len, Some(name))) => {
                found.push(Token {
                    range: start..start + len,
                    name: name.to_string(),
                });
                len
            }
            None if tail.starts_with("{{") => 2,
            None => match tail.strip_prefix('{') {
                Some(body) => {
                    let n = name_len(body);
                    if n > 0 && body[n..].starts_with('}') {
                        found.push(Token {
                            range: start..start + n + 2,
                            name: body[..n].to_string(),
                        });
                        n + 2
                    } else {
                        1
                    }
                }
                // `$` and `{` are one byte each.
                None => 1,
            },
        };
        at = start + len;
    }
    found
}

/// Rewrites every `{{name}}` in `text` to `$name`, or to `${name}` when a name
/// character follows. Returns the new text and how many were rewritten.
pub fn convert_braces(text: &str) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut count = 0;
    let mut rest = text;
    while let Some(i) = rest.find("{{") {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        match token(tail) {
            Some((len, Some(name))) => {
                if tail[len..].chars().next().is_some_and(is_name) {
                    out.push_str(&format!("${{{name}}}"));
                } else {
                    out.push('$');
                    out.push_str(name);
                }
                count += 1;
                rest = &tail[len..];
            }
            _ => {
                out.push('{');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    (out, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vars {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn fills_dollar_braced_and_double_brace_forms() {
        let v = vars(&[("host", "api.test"), ("id", "7")]);
        let (out, undef) = substitute("https://$host/u/${id}x/{{ id }}", &v);
        assert_eq!(out, "https://api.test/u/7x/7");
        assert!(undef.is_empty());
    }

    #[test]
    fn undefined_names_stay_literal_and_are_reported_once() {
        let (out, undef) = substitute("$a $a ${b} {{c}}", &Vars::new());
        assert_eq!(out, "$a $a ${b} {{c}}");
        assert_eq!(undef, vec!["a", "b", "c"]);
    }

    #[test]
    fn double_dollar_and_non_names_are_literal() {
        let v = vars(&[("a", "X")]);
        let (out, undef) = substitute("$$a costs $5, $ alone, ${ a}, {{a b}}", &v);
        assert_eq!(out, "$a costs $5, $ alone, ${ a}, {{a b}}");
        assert!(undef.is_empty());
    }

    #[test]
    fn name_stops_at_first_non_name_char() {
        let v = vars(&[("a", "X")]);
        assert_eq!(substitute("$a-b $a.c", &v).0, "X-b X.c");
    }

    #[test]
    fn path_params_fill_and_list_but_skip_double_braces() {
        let v = vars(&[("id", "5")]);
        let url = "https://h/{{base}}/u/{id}/p/{pid}";
        assert_eq!(fill_path_params(url, &v), "https://h/{{base}}/u/5/p/{pid}");
        assert_eq!(path_params(&fill_path_params(url, &v)), vec!["pid"]);
    }

    #[test]
    fn convert_braces_rewrites_and_counts() {
        let (out, n) = convert_braces("{{baseUrl}}/a {{ tok }}x {{not a name}} {x}");
        assert_eq!(out, "$baseUrl/a ${tok}x {{not a name}} {x}");
        assert_eq!(n, 2);
    }

    #[test]
    fn tokens_finds_every_form_with_its_range() {
        let text = "$a ${b} {{ c }} {d}";
        let pairs: Vec<(&str, String)> = tokens(text)
            .into_iter()
            .map(|t| (&text[t.range], t.name))
            .collect();
        assert_eq!(
            pairs,
            [
                ("$a", "a".to_string()),
                ("${b}", "b".to_string()),
                ("{{ c }}", "c".to_string()),
                ("{d}", "d".to_string()),
            ]
        );
    }

    #[test]
    fn tokens_skips_escapes_and_non_names() {
        assert!(tokens("$$a costs $5, ${ a}, {{a b}}, {1}, { x}").is_empty());
    }

    #[test]
    fn tokens_names_agree_with_substitute() {
        let text = "$host/u/{id}/{{ tok }}?q=${q}&p={pid}";
        let names: Vec<String> = tokens(text).into_iter().map(|t| t.name).collect();
        assert_eq!(names, ["host", "id", "tok", "q", "pid"]);
        assert_eq!(substitute(text, &Vars::new()).1, ["host", "tok", "q"]);
    }
}
