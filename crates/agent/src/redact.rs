//! Scrubs secret values from command output before an agent sees it.
//!
//! Catches the value as is, and the common ways it gets re-encoded on the way
//! out: hex, base64 (standard and URL-safe, at any byte alignment), URL
//! encoding and JSON string escaping. This catches accidents, like a stack
//! trace printing a connection string. It doesn't stop a determined leak.

use aho_corasick::{AhoCorasick, MatchKind};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD};

/// Shorter values aren't redacted: "true" or "3000" would mangle normal
/// output, and they're rarely secrets.
pub const MIN_LEN: usize = 6;
/// Base64 fragments shorter than this are too likely to match by chance.
const MIN_ENCODED: usize = 8;

pub struct Redactor {
    matcher: Option<AhoCorasick>,
    /// The variable name behind each pattern.
    names: Vec<String>,
}

/// Where a secret turned up in some text.
#[derive(Debug, PartialEq, Eq)]
pub struct Found<'a> {
    pub start: usize,
    pub end: usize,
    pub name: &'a str,
}

impl Redactor {
    /// `secrets` are (variable name, value) pairs.
    pub fn new(secrets: &[(String, String)]) -> Redactor {
        let mut patterns: Vec<String> = Vec::new();
        let mut names = Vec::new();
        for (name, value) in secrets {
            if value.len() < MIN_LEN {
                continue;
            }
            for pattern in encodings(value) {
                if !patterns.contains(&pattern) {
                    patterns.push(pattern);
                    names.push(name.clone());
                }
            }
        }
        let matcher = (!patterns.is_empty()).then(|| {
            AhoCorasick::builder()
                .match_kind(MatchKind::LeftmostLongest)
                .build(&patterns)
                .expect("literal patterns always build")
        });
        Redactor { matcher, names }
    }

    /// Every secret in `text`, in order, without changing it.
    pub fn find<'a>(&'a self, text: &str) -> Vec<Found<'a>> {
        let Some(matcher) = &self.matcher else { return Vec::new() };
        matcher
            .find_iter(text)
            .map(|m| Found { start: m.start(), end: m.end(), name: &self.names[m.pattern().as_usize()] })
            .collect()
    }

    /// The text with every secret replaced, and how many were found.
    pub fn redact(&self, text: &str) -> (String, usize) {
        let Some(matcher) = &self.matcher else { return (text.to_string(), 0) };
        let mut count = 0;
        let mut out = String::with_capacity(text.len());
        matcher.replace_all_with(text, &mut out, |m, _, dst| {
            count += 1;
            dst.push_str(&format!("[dotsafe:{}]", self.names[m.pattern().as_usize()]));
            true
        });
        (out, count)
    }
}

/// The value and the forms it's likely to take in output.
fn encodings(value: &str) -> Vec<String> {
    let bytes = value.as_bytes();
    let mut out = vec![value.to_string()];
    out.push(bytes.iter().map(|b| format!("{b:02x}")).collect());
    out.push(bytes.iter().map(|b| format!("{b:02X}")).collect());
    // Base64 of the value starting at each byte offset within a longer
    // stream: keep only the characters that depend on the value alone.
    for engine in [&STANDARD_NO_PAD, &URL_SAFE_NO_PAD] {
        for offset in 0..3usize {
            let mut padded = vec![0u8; offset];
            padded.extend_from_slice(bytes);
            padded.extend_from_slice(&[0, 0]);
            let encoded = engine.encode(&padded);
            let start = offset.div_ceil(3) * 4;
            let end = ((offset + bytes.len()) / 3) * 4;
            if end > start && end - start >= MIN_ENCODED {
                out.push(encoded[start..end].to_string());
            }
        }
    }
    let url: String = bytes
        .iter()
        .map(|&b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b => format!("%{b:02X}"),
        })
        .collect();
    out.push(url);
    if let Ok(json) = serde_json::to_string(value) {
        out.push(json[1..json.len() - 1].to_string());
    }
    out.retain(|p| !p.is_empty());
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redactor() -> Redactor {
        Redactor::new(&[
            ("DATABASE_URL".into(), "postgres://admin:hunter2@db.internal/app".into()),
            ("API_KEY".into(), "sk_live_abc123XYZ".into()),
            ("PORT".into(), "3000".into()),
        ])
    }

    #[test]
    fn raw_values_are_scrubbed() {
        let (out, n) =
            redactor().redact("connecting to postgres://admin:hunter2@db.internal/app with sk_live_abc123XYZ");
        assert_eq!(out, "connecting to [dotsafe:DATABASE_URL] with [dotsafe:API_KEY]");
        assert_eq!(n, 2);
    }

    #[test]
    fn short_values_are_left_alone() {
        assert_eq!(redactor().redact("listening on 3000").0, "listening on 3000");
    }

    #[test]
    fn encoded_values_are_scrubbed() {
        let r = redactor();
        let key = "sk_live_abc123XYZ";
        let hex: String = key.bytes().map(|b| format!("{b:02x}")).collect();
        assert!(!r.redact(&hex).0.contains(&hex));
        for prefix in ["", "a", "ab"] {
            let text = format!("{prefix}{key}");
            for encoded in [STANDARD_NO_PAD.encode(&text), URL_SAFE_NO_PAD.encode(&text)] {
                let (out, n) = r.redact(&encoded);
                assert!(n >= 1, "base64 with prefix {prefix:?} not caught: {encoded} -> {out}");
            }
        }
        let url_encoded = "postgres%3A%2F%2Fadmin%3Ahunter2%40db.internal%2Fapp";
        assert_eq!(r.redact(url_encoded).0, "[dotsafe:DATABASE_URL]");
        let json = Redactor::new(&[("X".into(), "line\"quote\\back".into())]);
        assert_eq!(json.redact(r#"{"x":"line\"quote\\back"}"#).0, r#"{"x":"[dotsafe:X]"}"#);
    }

    #[test]
    fn finds_report_where_and_which() {
        let r = redactor();
        let text = "key=sk_live_abc123XYZ";
        assert_eq!(r.find(text), [Found { start: 4, end: text.len(), name: "API_KEY" }]);
        assert!(r.find("nothing here").is_empty());
    }

    #[test]
    fn nothing_to_redact() {
        let r = Redactor::new(&[]);
        assert_eq!(r.redact("hello"), ("hello".to_string(), 0));
    }
}
