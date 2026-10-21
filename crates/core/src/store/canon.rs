//! Content ids for the store's non-model entities, computed exactly as
//! sim.pflow.xyz's modelstore computes them: Go's `encoding/json` bytes of
//! the value decoded into `any` (sorted keys, numbers as float64, Go's
//! string escaping), sha256, the first 24 hex characters. So a map or a
//! collection stored here has the id sim would give it.
//!
//! Models are addressed differently: by their pflow.xyz CID
//! ([`crate::cid::seal`]), byte-identical to pflow.xyz.

use serde_json::Value;
use sha2::{Digest, Sha256};

/// `v` as Go's `json.Marshal` writes a value decoded into `any`.
pub fn go_json(v: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, v);
    out
}

fn write_value(out: &mut String, v: &Value) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&go_float(n.as_f64().unwrap_or(0.0))),
        Value::String(s) => write_string(out, s),
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(out, x);
            }
            out.push(']');
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort(); // byte order, as Go sorts map keys
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(out, k);
                out.push(':');
                write_value(out, &m[k]);
            }
            out.push('}');
        }
    }
}

/// Go's float64 encoding: shortest round-trip digits, `%f` style unless the
/// magnitude is below 1e-6 or at least 1e21, then `%e` with a two-digit
/// minimum positive exponent (`1e+21`) and no leading zero on a negative
/// one (`1e-7`).
fn go_float(f: f64) -> String {
    let abs = f.abs();
    if abs != 0.0 && !(1e-6..1e21).contains(&abs) {
        let s = format!("{f:e}");
        let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
        let exp = match exp.strip_prefix('-') {
            Some(e) => format!("-{e}"),
            None if exp.len() == 1 => format!("+0{exp}"),
            None => format!("+{exp}"),
        };
        return format!("{mant}e{exp}");
    }
    format!("{f}")
}

/// Go's string escaping (`encoding/json`, Go ≥ 1.22): the short escapes,
/// `\u00XX` for other control characters, and the HTML-safe escapes of
/// `<`, `>`, `&`, U+2028 and U+2029.
fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '<' | '>' | '&' | '\u{2028}' | '\u{2029}' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// The first 24 hex characters of sha256(`data`): every non-model id.
pub fn sha24(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut s = String::with_capacity(24);
    for b in &digest[..12] {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// The content id of a value: [`sha24`] of its [`go_json`] bytes.
pub fn content_id(v: &Value) -> String {
    sha24(go_json(v).as_bytes())
}

/// Is `s` shaped like a store content id (24 lowercase hex characters)?
pub fn is_sha24(s: &str) -> bool {
    s.len() == 24 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Is `s` shaped like a pflow.xyz model CID (`z` + base58btc)?
pub fn is_model_cid(s: &str) -> bool {
    const ALPHABET: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    s.len() >= 40 && s.starts_with('z') && s[1..].chars().all(|c| ALPHABET.contains(c))
}

/// Milliseconds since the Unix epoch; 0 on the web build, which has no
/// clock `std` can read (`SystemTime::now` panics on wasm32-unknown-unknown).
pub fn now_ms() -> u64 {
    if cfg!(target_arch = "wasm32") {
        return 0;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Go's zero `time.Time` in JSON: what tool-recorded edges carry, so their
/// id is a function of their ends alone.
pub const ZERO_TIME: &str = "0001-01-01T00:00:00Z";

/// `ms` since the epoch as Go's `time.Time` JSON (RFC 3339, UTC, trailing
/// zeros of the fraction trimmed); 0 is the zero time.
pub fn rfc3339(ms: u64) -> String {
    if ms == 0 {
        return ZERO_TIME.into();
    }
    let secs = ms / 1000;
    let frac = ms % 1000;
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let (y, m, d) = civil_from_days(days);
    let mut s = format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    );
    if frac != 0 {
        let f = format!("{frac:03}");
        s.push('.');
        s.push_str(f.trim_end_matches('0'));
    }
    s.push('Z');
    s
}

/// A sort key for [`rfc3339`] strings: the fraction padded to nine digits,
/// so "…05Z" sorts before "…05.1Z".
pub fn time_key(t: &str) -> String {
    let body = t.trim_end_matches('Z');
    match body.split_once('.') {
        Some((head, frac)) => format!("{head}.{frac:0<9}"),
        None => format!("{body}.000000000"),
    }
}

/// Howard Hinnant's days-from-civil inverse.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn go_json_matches_encoding_json() {
        // Expected strings are what Go's json.Marshal writes for the same
        // values decoded into `any`.
        let v = json!({"b": 1.0, "a": [0.5, 1e21, 1e-7, 123456789, -0.0], "c": "<&>\n\u{1}\u{8}é"});
        assert_eq!(
            go_json(&v),
            r#"{"a":[0.5,1e+21,1e-7,123456789,-0],"b":1,"c":"\u003c\u0026\u003e\n\u0001\bé"}"#
        );
        assert_eq!(go_float(1.5e300), "1.5e+300");
        assert_eq!(go_float(2.5e-10), "2.5e-10");
        assert_eq!(go_float(1e20), "100000000000000000000");
    }

    #[test]
    fn sim_collection_and_map_ids() {
        // sha256 of the canonical bytes, first 24 hex: the scheme of
        // sim's canonicalCollection / canonicalMap.
        assert_eq!(content_id(&json!({"Name": "x"})), sha24(br#"{"Name":"x"}"#));
        assert!(is_sha24(&content_id(&json!({"Data": {}}))));
    }

    #[test]
    fn times() {
        assert_eq!(rfc3339(0), ZERO_TIME);
        assert_eq!(rfc3339(1_000), "1970-01-01T00:00:01Z");
        assert_eq!(rfc3339(1_792_540_800_120), "2026-10-21T00:00:00.12Z");
        assert!(time_key("2026-10-21T00:00:05Z") < time_key("2026-10-21T00:00:05.1Z"));
        assert!(time_key(ZERO_TIME) < time_key("1970-01-01T00:00:01Z"));
    }
}
