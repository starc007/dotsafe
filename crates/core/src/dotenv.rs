//! A small `.env` parser, shared by `dotsafe import` and the desktop app's
//! import, and the writer behind `dotsafe export`. [`parse_any`] also reads
//! the JSON other secret managers export (Doppler, Infisical, 1Password).
//!
//! Supports `KEY=value`, `export KEY=value`, comments, single quotes (literal),
//! double quotes (with `\n`, `\t`, `\"`, `\\` escapes, and values spanning
//! lines), and trailing ` # comments` on unquoted values. Unlike some dotenv
//! libraries it never expands `$VARS`, so nothing from the current shell leaks
//! into the vault.

use crate::model::is_valid_var_name;

pub fn parse(input: &str) -> Result<Vec<(String, String)>, String> {
    let mut vars = Vec::new();
    let mut lines = input.lines().enumerate();
    while let Some((i, line)) = lines.next() {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").map(str::trim_start).unwrap_or(line);
        let lineno = i + 1;
        let (key, rest) = line
            .split_once('=')
            .ok_or_else(|| format!("line {lineno}: expected KEY=value"))?;
        let key = key.trim();
        if !is_valid_var_name(key) {
            return Err(format!("line {lineno}: invalid variable name {key:?}"));
        }
        let rest = rest.trim_start();

        let value = if let Some(body) = rest.strip_prefix('\'') {
            let end = body.find('\'').ok_or_else(|| format!("line {lineno}: unterminated '"))?;
            body[..end].to_string()
        } else if let Some(body) = rest.strip_prefix('"') {
            let mut raw = body.to_string();
            let end = loop {
                if let Some(end) = closing_quote(&raw) {
                    break end;
                }
                let (_, next) = lines.next().ok_or_else(|| format!("line {lineno}: unterminated \""))?;
                raw.push('\n');
                raw.push_str(next);
            };
            unescape(&raw[..end])
        } else {
            let value = match rest.find(" #") {
                Some(i) => &rest[..i],
                None => rest,
            };
            value.trim_end().to_string()
        };
        vars.push((key.to_string(), value));
    }
    Ok(vars)
}

/// Parses `.env` text, or the JSON other managers export, telling them apart
/// by the first character:
/// - an object of names to values (`doppler secrets download --format json`);
/// - an array of `{ key | name, value }` (`infisical export --format json`);
/// - a 1Password item (`op item get --format json`): its fields, by label.
///
/// Numbers and booleans become text. Names that aren't valid variable names
/// are refused, like in `.env` files.
pub fn parse_any(input: &str) -> Result<Vec<(String, String)>, String> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        return parse(input);
    }
    let json: serde_json::Value = serde_json::from_str(trimmed).map_err(|e| format!("not valid JSON: {e}"))?;
    let pairs: Vec<(String, &serde_json::Value)> = match &json {
        serde_json::Value::Object(map) if map.get("fields").is_some_and(|f| f.is_array()) => map["fields"]
            .as_array()
            .into_iter()
            .flatten()
            // The item's own notes field isn't a variable; nor is an empty field.
            .filter(|f| f.get("purpose").and_then(|p| p.as_str()) != Some("NOTES") && f.get("id") != Some(&"notesPlain".into()))
            .filter_map(|f| Some((f.get("label")?.as_str()?.to_string(), f.get("value")?)))
            .filter(|(label, value)| is_valid_var_name(label) && value.as_str() != Some(""))
            .collect(),
        serde_json::Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), v)).collect(),
        serde_json::Value::Array(items) => items
            .iter()
            .map(|item| {
                let key = item.get("key").or_else(|| item.get("name")).and_then(|k| k.as_str());
                match (key, item.get("value")) {
                    (Some(key), Some(value)) => Ok((key.to_string(), value)),
                    _ => Err("expected a list of { key, value } items".to_string()),
                }
            })
            .collect::<Result<_, _>>()?,
        _ => return Err("expected a JSON object or list".into()),
    };
    pairs
        .into_iter()
        .map(|(key, value)| {
            if !is_valid_var_name(&key) {
                return Err(format!("invalid variable name {key:?}"));
            }
            let value = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Null => String::new(),
                _ => return Err(format!("{key} isn't a plain value")),
            };
            Ok((key, value))
        })
        .collect()
}

/// Writes variables as `.env` lines that [`parse`] (and dotenv, Docker
/// Compose and Prisma) read back unchanged. Plain values stay bare; others
/// are single-quoted so tools that expand `$VARS` leave them alone, and only
/// values with a newline or a `'` fall back to escaped double quotes.
pub fn format<'a>(vars: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut out = String::new();
    for (key, value) in vars {
        out.push_str(key);
        out.push('=');
        if value.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-./:@%+,".contains(&b)) {
            out.push_str(value);
        } else if !value.contains(['\'', '\n', '\r']) {
            out.push('\'');
            out.push_str(value);
            out.push('\'');
        } else {
            out.push('"');
            for c in value.chars() {
                match c {
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        out.push('\n');
    }
    out
}

fn closing_quote(s: &str) -> Option<usize> {
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return Some(i),
            _ => {}
        }
    }
    None
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn parses_common_forms() {
        let input = r#"
# comment
PLAIN=value
export EXPORTED=yes
SPACED = padded   # trailing comment
EMPTY=
URL=postgres://u:p@host/db?x=1#frag
SINGLE='keep $HOME and \n literal'
DOUBLE="line\nbreak \"quoted\""
"#;
        assert_eq!(
            parse(input).unwrap(),
            kv(&[
                ("PLAIN", "value"),
                ("EXPORTED", "yes"),
                ("SPACED", "padded"),
                ("EMPTY", ""),
                ("URL", "postgres://u:p@host/db?x=1#frag"),
                ("SINGLE", "keep $HOME and \\n literal"),
                ("DOUBLE", "line\nbreak \"quoted\""),
            ])
        );
    }

    #[test]
    fn parses_multiline_double_quoted_values() {
        let input = "KEY=\"-----BEGIN KEY-----\nabc\n-----END KEY-----\"\nNEXT=1\n";
        assert_eq!(
            parse(input).unwrap(),
            kv(&[("KEY", "-----BEGIN KEY-----\nabc\n-----END KEY-----"), ("NEXT", "1")])
        );
    }

    #[test]
    fn formats_values_that_parse_back_unchanged() {
        let vars = kv(&[
            ("PLAIN", "abc-123_x.y/z:8080"),
            ("EMPTY", ""),
            ("SPACES", "two words # not a comment"),
            ("DOLLAR", "pa$$word ${HOME}"),
            ("QUOTE", "it's \"here\""),
            ("PEM", "-----BEGIN-----\nab\\cd\n-----END-----"),
        ]);
        let text = format(vars.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        assert!(text.contains("PLAIN=abc-123_x.y/z:8080\n"));
        assert!(text.contains("DOLLAR='pa$$word ${HOME}'\n"));
        assert_eq!(parse(&text).unwrap(), vars);
    }

    #[test]
    fn reads_other_managers_exports() {
        let doppler = r#"{"DATABASE_URL": "postgres://x", "PORT": 3000, "DEBUG": false}"#;
        assert_eq!(parse_any(doppler).unwrap(), kv(&[("DATABASE_URL", "postgres://x"), ("DEBUG", "false"), ("PORT", "3000")]));
        let infisical = r#"[{"key": "API_KEY", "value": "sk", "type": "shared"}, {"name": "B", "value": "2"}]"#;
        assert_eq!(parse_any(infisical).unwrap(), kv(&[("API_KEY", "sk"), ("B", "2")]));
        let onepassword = r#"{"title": "api", "fields": [
            {"id": "notesPlain", "label": "notesPlain", "value": ""},
            {"label": "STRIPE_KEY", "value": "sk_live"},
            {"label": "web site", "value": "https://x"},
            {"label": "NO_VALUE"}
        ]}"#;
        assert_eq!(parse_any(onepassword).unwrap(), kv(&[("STRIPE_KEY", "sk_live")]));
        assert_eq!(parse_any("A=1\n").unwrap(), kv(&[("A", "1")]), ".env still works");
        assert!(parse_any(r#"{"bad name": "x"}"#).unwrap_err().contains("invalid variable name"));
        assert!(parse_any(r#"{"NESTED": {"a": 1}}"#).unwrap_err().contains("isn't a plain value"));
        assert!(parse_any("[1, 2]").is_err());
    }

    #[test]
    fn does_not_expand_variables() {
        assert_eq!(parse("A=$HOME\nB=${A}").unwrap(), kv(&[("A", "$HOME"), ("B", "${A}")]));
    }

    #[test]
    fn rejects_bad_lines() {
        assert!(parse("NOEQUALS").unwrap_err().contains("line 1"));
        assert!(parse("1BAD=x").unwrap_err().contains("invalid variable name"));
        assert!(parse("A=\"open").unwrap_err().contains("unterminated"));
    }
}
