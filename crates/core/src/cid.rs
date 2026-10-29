//! Content identifiers: pflow.xyz's "seal" ported to Rust, byte-identical to
//! `internal/seal/seal.go` (piprate/json-gold v0.7.0) and
//! `public/seal-cid.mjs` (jsonld.js) in pflow-xyz.
//!
//! The contract:
//!
//! 1. Drop the top-level `"@id"`. A sealed net's `@id` *is* its CID, so it
//!    cannot be part of its own preimage; dropping it makes sealing
//!    idempotent.
//! 2. JSON-LD → RDF with the fixed pflow.xyz context (preloaded, never
//!    fetched), an empty base IRI and safe mode off.
//! 3. Canonicalize with URDNA2015 (RDFC-1.0) to N-Quads.
//! 4. sha2-256 the N-Quads bytes.
//! 5. CIDv1 = `0x01 | varint(0x0129 dag-json) | 0x12 | 0x20 | digest`,
//!    base58btc, `z` prefix.
//!
//! # Why not a JSON-LD crate
//!
//! The general crates (`json-ld`, `sophia_jsonld`) are async, pull in a large
//! dependency tree and still would not reproduce json-gold's quirks, which
//! *are* the contract (the server assigns the stored CIDs). pflow documents
//! use one tiny context — `@vocab`, plain terms and `@list` containers, no
//! `@id` references, no scoped contexts — so expansion here is purpose-built
//! for exactly that context, and anything outside it is refused with an
//! error rather than hashed differently. Canonicalization is a direct port
//! of json-gold's URDNA2015, including its permutation order.
//!
//! # json-gold behaviours reproduced on purpose
//!
//! * Numbers are float64. A value is an `xsd:integer` when it is integral
//!   and fits an int64 (`1.0` and `1` are the same literal), otherwise an
//!   `xsd:double` in canonical form (`1.5E0`).
//! * A property whose IRI json-gold considers invalid is dropped: under the
//!   vocabulary that means a key with whitespace (`\t \n \f \r` or space) or
//!   a malformed `%` escape. The node it points to still contributes its
//!   own triples. So a place keyed `"Waiting Customers"` is detached from
//!   `places`, exactly as pflow.xyz does it.
//! * Keys shaped like keywords but not keywords (`@foo`) and `@version` are
//!   ignored.
//! * N-Quads escaping is json-gold's: only `\\`, `"`, `\n`, `\r`, `\t`.
//!
//! Absolute (non-vocabulary) IRIs used as keys or `@type` are validated by
//! an approximation of Go's `net/url` and govalidator's `IsURL`; that only
//! matters for documents that use full IRIs as property names.
//!
//! # Bounded work, no deep recursion
//!
//! URDNA2015 is superlinear on purpose: blank nodes that look alike are told
//! apart by walking their neighbours (a long `@list` is quadratic) and by
//! permuting groups of identical neighbours (factorial). A document a few
//! hundred bytes long can ask for more time than anyone has, and `seal` runs
//! on documents that were received. So the canonicaliser counts its steps
//! ([`seal_work_limit`]: twice the quadratic cost an honest long list has,
//! between [`MIN_SEAL_WORK`] and [`MAX_SEAL_WORK`]) and refuses, with
//! `[DOCUMENT_TOO_COMPLEX]`, a document that needs more; the count is the same
//! on every platform, and a document that is refused is never given a
//! different CID. The N-degree hash is
//! recursive in the specification and in json-gold; here its recursion is a
//! stack on the heap, so the depth of a long list cannot overflow the
//! thread's stack (a browser's is 1 MiB): the same steps in the same order,
//! the same hashes.
//!
//! # Go is the reference, not jsonld.js
//!
//! The server's CID is the one stored as `@id`, so where pflow.xyz's two
//! implementations disagree this follows Go. They agree on every parity
//! fixture, but `tests/fixtures/cid/edge-cases.json` shows jsonld.js
//! diverging on: keys with whitespace, `%`, `"` or `\` (it drops the
//! whole subtree, or keeps the triple, where Go drops only the link), tabs
//! in strings, integers of 2^63 and above (integers to jsonld.js up to
//! 1e21), and subnormal-range doubles.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{json, Map, Number, Value};
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// The context URL pflow documents name, served from the preloaded copy.
pub const PFLOW_CONTEXT_URL: &str = "https://pflow.xyz/schema";

const VOCAB: &str = "https://pflow.xyz/schema#";
/// Terms with `"@container": "@list"`: list order is part of the identity.
const LIST_TERMS: &[&str] = &["arcs", "token", "weight", "capacity", "initial", "parents"];
/// Plain terms. Every term maps to `VOCAB + name`, the same IRI `@vocab`
/// would give, so only the containers above change the RDF.
const PLAIN_TERMS: &[&str] = &[
    "places",
    "transitions",
    "source",
    "target",
    "inhibitTransition",
    "offset",
    "x",
    "y",
];

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

/// The pflow.xyz context, exactly as seal.go and seal-cid.mjs preload it.
/// A document may name it by URL or inline it.
pub fn pflow_context() -> Value {
    json!({
        "@vocab": VOCAB,
        "arcs": { "@id": "https://pflow.xyz/schema#arcs", "@container": "@list" },
        "places": "https://pflow.xyz/schema#places",
        "transitions": "https://pflow.xyz/schema#transitions",
        "token": { "@id": "https://pflow.xyz/schema#token", "@container": "@list" },
        "source": "https://pflow.xyz/schema#source",
        "target": "https://pflow.xyz/schema#target",
        "weight": { "@id": "https://pflow.xyz/schema#weight", "@container": "@list" },
        "inhibitTransition": "https://pflow.xyz/schema#inhibitTransition",
        "capacity": { "@id": "https://pflow.xyz/schema#capacity", "@container": "@list" },
        "initial": { "@id": "https://pflow.xyz/schema#initial", "@container": "@list" },
        "parents": { "@id": "https://pflow.xyz/schema#parents", "@container": "@list" },
        "offset": "https://pflow.xyz/schema#offset",
        "x": "https://pflow.xyz/schema#x",
        "y": "https://pflow.xyz/schema#y",
    })
}

/// The document's CID (`z…`, base58btc CIDv1 dag-json sha2-256). Any
/// top-level `@id` is ignored, so `seal(d) == seal(d with @id = seal(d))`.
pub fn seal(doc: &Value) -> Result<String> {
    SEALS.with(|n| n.set(n.get() + 1));
    Ok(cid_of_nquads(&nquads(doc)?))
}

thread_local! {
    static SEALS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// How many times [`seal`] has run on this thread, finished or refused. Sealing is the cost of
/// identity, so a test pins the commands that must not pay for it (the autosave, a receipt's
/// document id) by counting, not by timing.
pub fn seals_on_this_thread() -> u64 {
    SEALS.with(std::cell::Cell::get)
}

/// The canonical (URDNA2015) N-Quads `seal` hashes, for debugging a CID
/// mismatch against pflow.xyz's stored `canonical/{cid}.nq`.
pub fn nquads(doc: &Value) -> Result<String> {
    canonicalize(&to_rdf(doc)?)
}

/// CIDv1 (dag-json, sha2-256, base58btc) of canonical N-Quads.
pub fn cid_of_nquads(nquads: &str) -> String {
    let digest = Sha256::digest(nquads.as_bytes());
    let mut bytes = Vec::with_capacity(5 + digest.len());
    bytes.push(0x01); // CIDv1
    bytes.extend_from_slice(&[0xa9, 0x02]); // varint(0x0129) dag-json
    bytes.push(0x12); // sha2-256
    bytes.push(digest.len() as u8);
    bytes.extend_from_slice(&digest);
    format!("z{}", base58btc(&bytes))
}

fn base58btc(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    // Little-endian base-58 digits.
    let mut digits: Vec<u8> = Vec::new();
    for &b in bytes {
        let mut carry = b as u32;
        for d in digits.iter_mut() {
            carry += (*d as u32) << 8;
            *d = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }
    let zeros = bytes.iter().take_while(|&&b| b == 0).count();
    let mut out = String::with_capacity(zeros + digits.len());
    out.extend(std::iter::repeat_n('1', zeros));
    out.extend(digits.iter().rev().map(|&d| ALPHABET[d as usize] as char));
    out
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

fn sha256_hex(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
    }
    hex(&h.finalize())
}

// ---------------------------------------------------------------------------
// JSON-LD → RDF for the pflow context
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Term {
    Iri(String),
    /// A blank node, by index.
    Blank(usize),
    Literal(String, &'static str),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Quad {
    s: Term,
    p: String,
    o: Term,
}

fn unsupported(what: impl std::fmt::Display) -> Error {
    Error::Invalid(format!(
        "cannot seal: {what} (only the pflow.xyz context's plain terms, @type and @list containers are supported)"
    ))
}

fn to_rdf(doc: &Value) -> Result<Vec<Quad>> {
    let obj = doc
        .as_object()
        .ok_or_else(|| Error::Invalid("cannot seal: the document is not a JSON object".into()))?;
    check_context(obj.get("@context"))?;
    let mut gen = Gen::default();
    gen.node(obj, true)?;
    // The node map json-gold builds holds each value once per subject and
    // predicate, so repeated literals collapse to one triple.
    let mut seen = HashSet::new();
    gen.quads.retain(|q| seen.insert(q.clone()));
    Ok(gen.quads)
}

fn check_context(ctx: Option<&Value>) -> Result<()> {
    let ok = |v: &Value| match v {
        Value::String(s) => s == PFLOW_CONTEXT_URL,
        Value::Object(_) => *v == pflow_context(),
        _ => false,
    };
    match ctx {
        Some(Value::Array(a)) if a.len() == 1 && ok(&a[0]) => Ok(()),
        Some(v) if ok(v) => Ok(()),
        Some(v) => Err(unsupported(format!(
            "@context {v} is not {PFLOW_CONTEXT_URL:?}"
        ))),
        None => Err(unsupported(format!(
            "no @context (expected {PFLOW_CONTEXT_URL:?})"
        ))),
    }
}

#[derive(Default)]
struct Gen {
    quads: Vec<Quad>,
    next: usize,
}

impl Gen {
    fn blank(&mut self) -> Term {
        let t = Term::Blank(self.next);
        self.next += 1;
        t
    }

    fn push(&mut self, s: &Term, p: &str, o: Term) {
        self.quads.push(Quad {
            s: s.clone(),
            p: p.to_string(),
            o,
        });
    }

    /// A node object; every node is a blank node (no `@id` references).
    fn node(&mut self, obj: &Map<String, Value>, top: bool) -> Result<Term> {
        let subject = self.blank();
        for (key, value) in obj {
            match key.as_str() {
                "@context" if top => {}
                // seal() drops the top-level @id before this runs.
                "@id" if top => {}
                "@version" => {}
                "@type" => self.types(&subject, value)?,
                k if is_keyword(k) => return Err(unsupported(format!("keyword {k:?}"))),
                k if is_keyword_like(k) => {}
                // json-gold fails on an empty key.
                "" => return Err(unsupported("an empty key")),
                k => {
                    let prop = expand_iri(k).filter(|p| emits_property(p));
                    if LIST_TERMS.contains(&k) {
                        self.list(&subject, prop.as_deref(), value)?;
                    } else {
                        self.set(&subject, prop.as_deref(), value)?;
                    }
                }
            }
        }
        Ok(subject)
    }

    fn types(&mut self, subject: &Term, value: &Value) -> Result<()> {
        let one = |v: &Value| -> Result<String> {
            v.as_str()
                .map(str::to_string)
                .ok_or_else(|| unsupported(format!("@type value {v}")))
        };
        let types = match value {
            Value::Array(a) => a.iter().map(one).collect::<Result<Vec<_>>>()?,
            v => vec![one(v)?],
        };
        for t in types {
            if t.starts_with("_:") || is_keyword(&t) {
                return Err(unsupported(format!("@type {t:?}")));
            }
            if let Some(iri) = expand_iri(&t) {
                if go_valid_iri(&iri) {
                    self.push(subject, RDF_TYPE, Term::Iri(iri));
                }
            }
        }
        Ok(())
    }

    /// A value, or set of values, of a plain property. `prop` is `None`
    /// when json-gold drops the property: nested nodes still get triples.
    fn set(&mut self, subject: &Term, prop: Option<&str>, value: &Value) -> Result<()> {
        match value {
            Value::Null => {}
            Value::Array(items) => {
                for item in items {
                    self.set(subject, prop, item)?;
                }
            }
            v => {
                let o = self.object(v)?;
                if let Some(p) = prop {
                    self.push(subject, p, o);
                }
            }
        }
        Ok(())
    }

    /// A `@list` container: an rdf:first/rdf:rest chain, or rdf:nil.
    fn list(&mut self, subject: &Term, prop: Option<&str>, value: &Value) -> Result<()> {
        let items: Vec<&Value> = match value {
            Value::Null => return Ok(()),
            Value::Array(a) => a.iter().filter(|v| !v.is_null()).collect(),
            v => vec![v],
        };
        let mut objects = Vec::with_capacity(items.len());
        for item in items {
            if item.is_array() {
                return Err(unsupported("a list of lists"));
            }
            objects.push(self.object(item)?);
        }
        let Some(p) = prop else { return Ok(()) };
        if objects.is_empty() {
            self.push(subject, p, Term::Iri(RDF_NIL.into()));
            return Ok(());
        }
        let head = self.blank();
        self.push(subject, p, head.clone());
        let n = objects.len();
        let mut cell = head;
        for (i, o) in objects.into_iter().enumerate() {
            self.push(&cell, RDF_FIRST, o);
            if i + 1 == n {
                self.push(&cell, RDF_REST, Term::Iri(RDF_NIL.into()));
            } else {
                let next = self.blank();
                self.push(&cell, RDF_REST, next.clone());
                cell = next;
            }
        }
        Ok(())
    }

    fn object(&mut self, v: &Value) -> Result<Term> {
        Ok(match v {
            Value::Bool(b) => Term::Literal(b.to_string(), XSD_BOOLEAN),
            Value::Number(n) => number_literal(n),
            Value::String(s) => Term::Literal(s.clone(), XSD_STRING),
            Value::Object(o) => self.node(o, false)?,
            Value::Null | Value::Array(_) => unreachable!("handled by the caller"),
        })
    }
}

/// json-gold decodes every number to float64; integral values that fit an
/// int64 are `xsd:integer`, the rest `xsd:double` (`%1.15E`, trimmed).
fn number_literal(n: &Number) -> Term {
    let f = n.as_f64().unwrap_or(f64::NAN);
    // 2^63: Go's float64(int64(f)) == f holds exactly in [-2^63, 2^63).
    const TWO_63: f64 = 9_223_372_036_854_775_808.0;
    if f.trunc() == f && (-TWO_63..TWO_63).contains(&f) {
        Term::Literal((f as i64).to_string(), XSD_INTEGER)
    } else {
        Term::Literal(canonical_double(f), XSD_DOUBLE)
    }
}

fn canonical_double(f: f64) -> String {
    let s = format!("{f:.15E}");
    let (mantissa, exp) = s.split_once('E').expect("exponent");
    let mut m = mantissa.trim_end_matches('0').to_string();
    if m.ends_with('.') {
        m.push('0');
    }
    let e: i32 = exp.parse().expect("exponent");
    format!("{m}E{e}")
}

/// json-gold's keyword list.
fn is_keyword(k: &str) -> bool {
    matches!(
        k,
        "@base"
            | "@container"
            | "@context"
            | "@default"
            | "@direction"
            | "@embed"
            | "@explicit"
            | "@json"
            | "@id"
            | "@included"
            | "@index"
            | "@first"
            | "@graph"
            | "@import"
            | "@language"
            | "@list"
            | "@nest"
            | "@none"
            | "@omitDefault"
            | "@prefix"
            | "@preserve"
            | "@propagate"
            | "@protected"
            | "@requireAll"
            | "@reverse"
            | "@set"
            | "@type"
            | "@value"
            | "@version"
            | "@vocab"
    )
}

/// `^@[a-zA-Z]+$`: reserved for future keywords, ignored.
fn is_keyword_like(k: &str) -> bool {
    k.len() > 1 && k.starts_with('@') && k[1..].bytes().all(|b| b.is_ascii_alphabetic())
}

/// json-gold's `ExpandIri(value, relative=true, vocab=true)` under the
/// pflow context.
fn expand_iri(v: &str) -> Option<String> {
    if is_keyword_like(v) {
        return None;
    }
    if LIST_TERMS.contains(&v) || PLAIN_TERMS.contains(&v) {
        return Some(format!("{VOCAB}{v}"));
    }
    if let Some(i) = v.find(':') {
        if i > 0 {
            let (prefix, suffix) = (&v[..i], &v[i + 1..]);
            // No term of the pflow context is a prefix.
            if prefix == "_" || suffix.starts_with("//") || go_is_absolute_iri(v) {
                return Some(v.to_string());
            }
        }
    }
    Some(format!("{VOCAB}{v}"))
}

/// GraphToRDF's property filter plus the quad validity check.
fn emits_property(p: &str) -> bool {
    !p.starts_with("_:") && go_is_absolute_iri(p) && go_valid_iri(p)
}

fn go_is_absolute_iri(v: &str) -> bool {
    v.starts_with("_:") || go_url_parse(v).is_some_and(|u| !u.scheme.is_empty())
}

/// json-gold's `validIRI`: http(s) IRIs must pass govalidator's `IsURL`.
fn go_valid_iri(v: &str) -> bool {
    if !(v.starts_with("http://") || v.starts_with("https://")) {
        return true;
    }
    if v.len() <= 3 || v.chars().count() >= 2083 || v.starts_with('.') {
        return false;
    }
    let Some(u) = go_url_parse(v) else {
        return false;
    };
    if u.host.starts_with('.') || (u.host.is_empty() && !u.path.is_empty() && !u.path.contains('.'))
    {
        return false;
    }
    // govalidator's URL regex: under a well-formed scheme and host it
    // accepts any path/fragment without RE2 `\s` (`[\t\n\f\r ]`).
    !u.host.is_empty()
        && !v
            .bytes()
            .any(|b| matches!(b, b'\t' | b'\n' | 0x0c | b'\r' | b' '))
}

struct GoUrl {
    scheme: String,
    host: String,
    path: String,
}

/// Go's `net/url.Parse`, as far as it decides success, scheme, host and
/// path. Authority validation is approximate.
fn go_url_parse(raw: &str) -> Option<GoUrl> {
    let (u, frag) = match raw.split_once('#') {
        Some((u, f)) => (u, Some(f)),
        None => (raw, None),
    };
    if u.bytes().any(|b| b < 0x20 || b == 0x7f) {
        return None;
    }
    if let Some(f) = frag {
        if !pct_ok(f) {
            return None;
        }
    }
    if u == "*" {
        return Some(GoUrl {
            scheme: String::new(),
            host: String::new(),
            path: "*".into(),
        });
    }
    let (scheme, rest) = go_get_scheme(u)?;
    let rest = if rest.ends_with('?') && rest.matches('?').count() == 1 {
        &rest[..rest.len() - 1]
    } else {
        rest.split_once('?').map_or(rest, |(a, _)| a)
    };
    let mut url = GoUrl {
        scheme: scheme.to_ascii_lowercase(),
        host: String::new(),
        path: String::new(),
    };
    if !rest.starts_with('/') {
        if !scheme.is_empty() {
            return Some(url); // opaque
        }
        if rest.split('/').next().unwrap_or("").contains(':') {
            return None;
        }
    }
    let mut path = rest;
    if (!scheme.is_empty() || !rest.starts_with("///")) && rest.starts_with("//") {
        let after = &rest[2..];
        let (authority, p) = match after.find('/') {
            Some(i) => (&after[..i], &after[i..]),
            None => (after, ""),
        };
        url.host = go_parse_authority(authority)?;
        path = p;
    }
    if !pct_ok(path) {
        return None;
    }
    url.path = path.to_string();
    Some(url)
}

fn go_get_scheme(raw: &str) -> Option<(&str, &str)> {
    for (i, c) in raw.bytes().enumerate() {
        match c {
            b'a'..=b'z' | b'A'..=b'Z' => {}
            b'0'..=b'9' | b'+' | b'-' | b'.' => {
                if i == 0 {
                    return Some(("", raw));
                }
            }
            b':' => {
                if i == 0 {
                    return None; // "missing protocol scheme"
                }
                return Some((&raw[..i], &raw[i + 1..]));
            }
            _ => return Some(("", raw)),
        }
    }
    Some(("", raw))
}

fn go_parse_authority(authority: &str) -> Option<String> {
    let (userinfo, host) = match authority.rfind('@') {
        Some(i) => (Some(&authority[..i]), &authority[i + 1..]),
        None => (None, authority),
    };
    if let Some(ui) = userinfo {
        let ok = ui
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b >= 0x80 || b"-._:~!$&'()*+,;=%@".contains(&b));
        if !ok || !pct_ok(ui) {
            return None;
        }
    }
    if !host.starts_with('[') {
        if let Some(i) = host.rfind(':') {
            if !host[i + 1..].bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
        }
    }
    let ok = host
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b >= 0x80 || b"-_.~!$&'()*+,;=:[]<>\"%".contains(&b));
    (ok && pct_ok(host)).then(|| host.to_string())
}

/// Every `%` is followed by two hex digits.
fn pct_ok(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if i + 2 >= b.len() || !b[i + 1].is_ascii_hexdigit() || !b[i + 2].is_ascii_hexdigit() {
                return false;
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// URDNA2015, ported from json-gold's api_normalize.go
// ---------------------------------------------------------------------------
//
// Blank nodes are indices. Which of several automorphic choices a run
// picks depends on label order, but the canonical output does not (that is
// the algorithm's guarantee, and why json-gold, jsonld.js and this port
// agree although each labels the input differently).

/// json-gold's N-Quads string escaping.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

/// One quad in N-Quads, blank nodes written by `blank`.
fn quad_nq(q: &Quad, blank: &impl Fn(usize) -> String) -> String {
    let term = |t: &Term, out: &mut String| match t {
        Term::Iri(i) => {
            out.push('<');
            out.push_str(&escape(i));
            out.push('>');
        }
        Term::Blank(b) => out.push_str(&blank(*b)),
        Term::Literal(v, dt) => {
            out.push('"');
            out.push_str(&escape(v));
            out.push('"');
            if *dt != XSD_STRING {
                out.push_str("^^<");
                out.push_str(&escape(dt));
                out.push('>');
            }
        }
    };
    let mut out = String::new();
    term(&q.s, &mut out);
    out.push_str(" <");
    out.push_str(&escape(&q.p));
    out.push_str("> ");
    term(&q.o, &mut out);
    out.push_str(" .\n");
    out
}

const UNISSUED: u32 = u32::MAX;

/// An identifier issuer: blank node → issue number, and the issue order.
#[derive(Clone)]
struct Issuer {
    issued: Vec<u32>,
    order: Vec<usize>,
}

impl Issuer {
    fn new(nodes: usize) -> Self {
        Self {
            issued: vec![UNISSUED; nodes],
            order: Vec::new(),
        }
    }

    fn get(&self, id: usize) -> Option<u32> {
        let n = self.issued[id];
        (n != UNISSUED).then_some(n)
    }

    fn issue(&mut self, id: usize) -> u32 {
        if let Some(n) = self.get(id) {
            return n;
        }
        let n = self.order.len() as u32;
        self.issued[id] = n;
        self.order.push(id);
        n
    }
}

struct Canon<'a> {
    quads: &'a [Quad],
    /// Blank node → indices of the quads it occurs in (once per position).
    bnode_quads: Vec<Vec<usize>>,
    first_degree: Vec<Option<String>>,
    /// The canonical issuer (`_:c14n`).
    canonical: Issuer,
    /// Steps taken so far, and the most that are allowed ([`seal_work_limit`]).
    work: u64,
    limit: u64,
}

/// The fewest and the most steps URDNA2015 may take for one document before
/// `seal` refuses it with `[DOCUMENT_TOO_COMPLEX]`. A step is one N-degree
/// hash call (counted with the quads it reads) or one permutation tried
/// (counted with its length). The algorithm is superlinear by design: blank
/// nodes that look alike (siblings with the same content, the cells of a long
/// `@list`) are told apart by walking and permuting their neighbours, so a
/// few hundred bytes of identical siblings cost factorial time and a long
/// list costs quadratic time. The byte cap on a document bounds its size, not
/// this; the count does, and it is the same on every platform, so a document
/// seals or is refused alike everywhere.
///
/// What an honest document needs is about its blank nodes squared (a net's
/// arcs are one long list): about 0.8 to 0.95 times the square of the
/// blank-node count, rising with size, for the nets of this repository
/// (tic-tac-toe 0.4 million steps for 701 nodes, connect-3 2.9 million for
/// 1796, connect-4 37 million for 6260). So the limit is twice that,
/// [`seal_work_limit`], and a hostile document pays for its own size:
/// a small one (identical siblings are few bytes and few nodes) gets the
/// floor, a few seconds in a debug build and a fraction of that in a release
/// one, whatever it hides; a large one gets at most the ceiling.
pub const MIN_SEAL_WORK: u64 = 2_000_000;
pub const MAX_SEAL_WORK: u64 = 80_000_000;

/// The steps a document of `blank_nodes` blank nodes may take to seal.
pub fn seal_work_limit(blank_nodes: usize) -> u64 {
    let n = blank_nodes as u64;
    n.saturating_mul(n)
        .saturating_mul(2)
        .clamp(MIN_SEAL_WORK, MAX_SEAL_WORK)
}

thread_local! {
    static WORK_LIMIT: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// Replaces [`seal_work_limit`] on this thread (`None` restores it), so a test can show the
/// refusal of a document that is honest but not small without spending the real budget on it.
pub fn limit_work_on_this_thread(limit: Option<u64>) {
    WORK_LIMIT.with(|l| l.set(limit));
}

/// The deepest chain of N-degree calls (the length of a list, give or take):
/// each pending call holds a few hundred bytes on the heap, not the stack.
const MAX_SEAL_DEPTH: usize = 20_000;

fn too_complex(limit: u64) -> Error {
    Error::Invalid(format!(
        "[DOCUMENT_TOO_COMPLEX] cannot seal: canonicalising this document takes more than {limit} steps \
         (many identical sibling objects, or a very long list, make URDNA2015 superlinear); \
         remove the repeated structure or split the document"
    ))
}

/// Whether `e` is `seal`'s refusal of a document it would take too long to
/// canonicalise (`[DOCUMENT_TOO_COMPLEX]`), which a caller that stores or
/// loads "as given" should pass on, not retry as another kind of document.
pub fn is_too_complex(e: &Error) -> bool {
    matches!(e, Error::Invalid(m) if m.starts_with("[DOCUMENT_TOO_COMPLEX] "))
}

fn canonicalize(quads: &[Quad]) -> Result<String> {
    canonicalize_within(quads, None)
}

/// `limit` of `None` is the rule: this thread's override, else [`seal_work_limit`] of the nodes.
fn canonicalize_within(quads: &[Quad], limit: Option<u64>) -> Result<String> {
    let nodes = quads
        .iter()
        .flat_map(|q| [&q.s, &q.o])
        .filter_map(|t| match t {
            Term::Blank(b) => Some(b + 1),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let mut bnode_quads = vec![Vec::new(); nodes];
    for (i, q) in quads.iter().enumerate() {
        for t in [&q.s, &q.o] {
            if let Term::Blank(b) = t {
                bnode_quads[*b].push(i);
            }
        }
    }
    let mut c = Canon {
        quads,
        first_degree: vec![None; nodes],
        canonical: Issuer::new(nodes),
        bnode_quads,
        work: 0,
        limit: limit
            .or_else(|| WORK_LIMIT.with(std::cell::Cell::get))
            .unwrap_or_else(|| seal_work_limit(nodes)),
    };

    // Issue canonical ids to nodes with a unique first-degree hash, in hash
    // order, until none is left.
    let mut non_normalized: Vec<usize> = (0..nodes)
        .filter(|&b| !c.bnode_quads[b].is_empty())
        .collect();
    let mut hash_to_bnodes: BTreeMap<String, Vec<usize>>;
    loop {
        hash_to_bnodes = BTreeMap::new();
        for &id in &non_normalized {
            let h = c.hash_first_degree(id);
            hash_to_bnodes.entry(h).or_default().push(id);
        }
        let mut simple = false;
        hash_to_bnodes.retain(|_, ids| {
            if ids.len() > 1 {
                return true;
            }
            c.canonical.issue(ids[0]);
            simple = true;
            false
        });
        non_normalized.retain(|&id| c.canonical.get(id).is_none());
        if !simple {
            break;
        }
    }

    // The rest, by N-degree hash within each first-degree group.
    for ids in hash_to_bnodes.values() {
        let mut hash_paths: BTreeMap<String, Vec<Issuer>> = BTreeMap::new();
        for &id in ids {
            if c.canonical.get(id).is_some() {
                continue;
            }
            // A fresh issuer is a vector of every node: that is work too.
            c.spend(1 + nodes as u64 / 64)?;
            let mut issuer = Issuer::new(nodes);
            issuer.issue(id);
            let (h, result) = c.hash_n_degree(id, issuer)?;
            hash_paths.entry(h).or_default().push(result);
        }
        for issuers in hash_paths.values() {
            for result in issuers {
                for &existing in &result.order {
                    c.canonical.issue(existing);
                }
            }
        }
    }

    let label = |b: usize| {
        format!(
            "_:c14n{}",
            c.canonical.get(b).expect("every blank node is labelled")
        )
    };
    let mut lines: Vec<String> = quads.iter().map(|q| quad_nq(q, &label)).collect();
    lines.sort();
    Ok(lines.concat())
}

/// One pending call of the N-degree hash. The hash is recursive in the
/// specification (depth = the length of a list); here the recursion is a
/// stack of these on the heap, so a long list cannot overflow the thread's
/// stack (the browser's is 1 MiB), and [`seal_work_limit`] bounds the time.
struct Frame {
    /// The issuer this call returns: the chosen one after each related group.
    issuer: Issuer,
    groups: std::vec::IntoIter<(String, Vec<usize>)>,
    md: Sha256,
    group: Option<Group>,
}

/// One related-hash group of a [`Frame`]: the permutations of its nodes.
struct Group {
    perms: Permutator,
    /// One related node: one permutation, always chosen, no copy of the issuer.
    single: bool,
    chosen_path: String,
    chosen_issuer: Option<Issuer>,
    attempt: Option<Attempt>,
}

/// One permutation being tried: its path so far, and the related nodes it
/// still has to hash (the calls that were recursive).
struct Attempt {
    issuer: Issuer,
    path: String,
    todo: std::vec::IntoIter<usize>,
    /// The identifier issued to the related node being hashed.
    issued: u32,
}

impl Canon<'_> {
    fn spend(&mut self, steps: u64) -> Result<()> {
        self.work = self.work.saturating_add(steps);
        if self.work > self.limit {
            return Err(too_complex(self.limit));
        }
        Ok(())
    }

    fn hash_first_degree(&mut self, id: usize) -> String {
        if let Some(h) = &self.first_degree[id] {
            return h.clone();
        }
        let mark = |b: usize| {
            if b == id {
                "_:a".to_string()
            } else {
                "_:z".to_string()
            }
        };
        let mut nquads: Vec<String> = self.bnode_quads[id]
            .iter()
            .map(|&i| quad_nq(&self.quads[i], &mark))
            .collect();
        nquads.sort();
        let refs: Vec<&str> = nquads.iter().map(String::as_str).collect();
        let h = sha256_hex(&refs);
        self.first_degree[id] = Some(h.clone());
        h
    }

    fn hash_related(&mut self, related: usize, quad: usize, issuer: &Issuer, pos: &str) -> String {
        let id = if let Some(c) = self.canonical.get(related) {
            format!("_:c14n{c}")
        } else if let Some(t) = issuer.get(related) {
            format!("_:b{t}")
        } else {
            self.hash_first_degree(related)
        };
        let pred = format!("<{}>", self.quads[quad].p);
        sha256_hex(&[pos, &pred, &id])
    }

    /// The start of an N-degree call: the related blank nodes of `id`,
    /// grouped by the hash of how they relate to it.
    fn enter(&mut self, id: usize, issuer: Issuer) -> Result<Frame> {
        self.spend(1 + self.bnode_quads[id].len() as u64)?;
        let quads = self.quads;
        let mut hash_to_related: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for qi in self.bnode_quads[id].clone() {
            let q = &quads[qi];
            for (t, pos) in [(&q.s, "s"), (&q.o, "o")] {
                if let Term::Blank(b) = *t {
                    if b != id {
                        let h = self.hash_related(b, qi, &issuer, pos);
                        hash_to_related.entry(h).or_default().push(b);
                    }
                }
            }
        }
        Ok(Frame {
            issuer,
            groups: hash_to_related.into_iter().collect::<Vec<_>>().into_iter(),
            md: Sha256::new(),
            group: None,
        })
    }

    /// json-gold's `hashNDegreeQuads`, with its recursion turned into a stack
    /// of [`Frame`]s: same steps in the same order, so the same hash.
    fn hash_n_degree(&mut self, id: usize, issuer: Issuer) -> Result<(String, Issuer)> {
        let longer = |path: &str, chosen: &str| {
            !chosen.is_empty() && path.len() >= chosen.len() && path > chosen
        };
        let mut stack = vec![self.enter(id, issuer)?];
        // What the call that just finished returns to its caller.
        let mut returned: Option<(String, Issuer)> = None;
        loop {
            let Frame {
                issuer,
                groups,
                md,
                group,
            } = stack.last_mut().expect("a frame while the loop runs");

            if let Some((result_hash, result_issuer)) = returned.take() {
                // Back in the caller, inside its attempt: the recursion's tail.
                let g = group.as_mut().expect("a caller is inside a group");
                let at = g.attempt.as_mut().expect("a caller is inside an attempt");
                at.path
                    .push_str(&format!("_:b{}<{result_hash}>", at.issued));
                at.issuer = result_issuer;
                if longer(&at.path, &g.chosen_path) {
                    g.attempt = None;
                }
            }

            let Some(g) = group.as_mut() else {
                // Between groups: the next related-hash group, or this call is done.
                match groups.next() {
                    Some((related_hash, blank_nodes)) => {
                        md.update(related_hash.as_bytes());
                        *group = Some(Group {
                            single: blank_nodes.len() == 1,
                            perms: Permutator::new(blank_nodes),
                            chosen_path: String::new(),
                            chosen_issuer: None,
                            attempt: None,
                        });
                    }
                    None => {
                        let done = stack.pop().expect("the frame just used");
                        let result = (hex(&done.md.finalize()), done.issuer);
                        if stack.is_empty() {
                            return Ok(result);
                        }
                        returned = Some(result);
                    }
                }
                continue;
            };

            if let Some(at) = g.attempt.as_mut() {
                if let Some(related) = at.todo.next() {
                    // Issued when the attempt was built; the call keeps it.
                    at.issued = at.issuer.issue(related);
                    let handed = std::mem::replace(&mut at.issuer, Issuer::new(0));
                    if stack.len() >= MAX_SEAL_DEPTH {
                        return Err(too_complex(self.limit));
                    }
                    let child = self.enter(related, handed)?;
                    stack.push(child);
                } else {
                    let at = g.attempt.take().expect("the attempt just read");
                    if g.chosen_path.is_empty() || at.path < g.chosen_path {
                        g.chosen_path = at.path;
                        g.chosen_issuer = Some(at.issuer);
                    }
                }
                continue;
            }

            // No attempt in progress: the group's next permutation.
            let Some(permutation) = g.perms.next_permutation() else {
                md.update(g.chosen_path.as_bytes());
                if let Some(chosen) = g.chosen_issuer.take() {
                    *issuer = chosen;
                }
                *group = None;
                continue;
            };
            self.spend(1 + permutation.len() as u64)?;
            // With a single related node there is one permutation, always
            // chosen: work on `issuer` itself instead of a copy.
            let mut issuer_copy = if g.single {
                std::mem::replace(issuer, Issuer::new(0))
            } else {
                issuer.clone()
            };
            let mut path = String::new();
            let mut recursion = Vec::new();
            let mut skip = false;
            for &related in &permutation {
                if let Some(c) = self.canonical.get(related) {
                    path.push_str(&format!("_:c14n{c}"));
                } else {
                    if issuer_copy.get(related).is_none() {
                        recursion.push(related);
                    }
                    path.push_str(&format!("_:b{}", issuer_copy.issue(related)));
                }
                if longer(&path, &g.chosen_path) {
                    skip = true;
                    break;
                }
            }
            if !skip {
                g.attempt = Some(Attempt {
                    issuer: issuer_copy,
                    path,
                    todo: recursion.into_iter(),
                    issued: 0,
                });
            }
        }
    }
}

/// json-gold's Steinhaus–Johnson–Trotter permutator, starting from the
/// sorted list.
struct Permutator {
    list: Vec<usize>,
    /// Direction of each element (looking left?), by element.
    left: HashMap<usize, bool>,
    done: bool,
}

impl Permutator {
    fn new(mut list: Vec<usize>) -> Self {
        list.sort_unstable();
        let left = list.iter().map(|&e| (e, true)).collect();
        Self {
            list,
            left,
            done: false,
        }
    }

    fn next_permutation(&mut self) -> Option<Vec<usize>> {
        if self.done {
            return None;
        }
        let rval = self.list.clone();
        let n = self.list.len();
        // The largest mobile element: greater than the one it looks at.
        let mut k: Option<(usize, usize)> = None;
        for (i, &e) in self.list.iter().enumerate() {
            let left = self.left[&e];
            let mobile = (left && i > 0 && e > self.list[i - 1])
                || (!left && i + 1 < n && e > self.list[i + 1]);
            if mobile && k.is_none_or(|(k, _)| e > k) {
                k = Some((e, i));
            }
        }
        match k {
            None => self.done = true,
            Some((k, pos)) => {
                let swap = if self.left[&k] { pos - 1 } else { pos + 1 };
                self.list.swap(pos, swap);
                for e in &self.list {
                    if *e > k {
                        let l = self.left.get_mut(e).expect("tracked");
                        *l = !*l;
                    }
                }
            }
        }
        Some(rval)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base58_matches_reference() {
        assert_eq!(base58btc(b"hello world"), "StV1DL6CwTryKyV");
        assert_eq!(base58btc(&[0, 0, 1]), "112");
        assert_eq!(base58btc(&[]), "");
    }

    #[test]
    fn doubles_are_canonical() {
        assert_eq!(canonical_double(1.5), "1.5E0");
        assert_eq!(canonical_double(-0.000125), "-1.25E-4");
        assert_eq!(canonical_double(1e300), "1.0E300");
        assert_eq!(canonical_double(0.1), "1.0E-1");
    }

    #[test]
    fn integral_floats_are_integers() {
        let lit = |v: Value| match number_literal(v.as_number().unwrap()) {
            Term::Literal(s, dt) => (s, dt),
            _ => unreachable!(),
        };
        assert_eq!(lit(json!(1.0)), ("1".into(), XSD_INTEGER));
        assert_eq!(lit(json!(-0.0)), ("0".into(), XSD_INTEGER));
        assert_eq!(lit(json!(1e19)), ("1.0E19".into(), XSD_DOUBLE));
        assert_eq!(lit(json!(2.5)), ("2.5E0".into(), XSD_DOUBLE));
    }

    #[test]
    fn permutator_order_is_sjt() {
        let mut p = Permutator::new(vec![3, 1, 2]);
        let mut all = vec![];
        while let Some(v) = p.next_permutation() {
            all.push(v);
        }
        let want: [[usize; 3]; 6] = [
            [1, 2, 3],
            [1, 3, 2],
            [3, 1, 2],
            [3, 2, 1],
            [2, 3, 1],
            [2, 1, 3],
        ];
        assert_eq!(all, want);
    }

    #[test]
    fn refuses_foreign_contexts() {
        let err = seal(&json!({"@context": "https://schema.org", "a": 1})).unwrap_err();
        assert!(err.to_string().contains("@context"), "{err}");
        assert!(seal(&json!({"a": 1})).is_err());
        let inline = json!({"@context": pflow_context(), "x": 1});
        let by_url = json!({"@context": PFLOW_CONTEXT_URL, "x": 1});
        assert_eq!(seal(&inline).unwrap(), seal(&by_url).unwrap());
    }

    #[test]
    fn ignores_top_level_id() {
        let a = json!({"@context": PFLOW_CONTEXT_URL, "@type": "PetriNet", "token": ["t"]});
        let mut b = a.clone();
        b["@id"] = json!(seal(&a).unwrap());
        assert_eq!(seal(&a).unwrap(), seal(&b).unwrap());
    }

    /// An RDF graph of `n` identical empty objects under each of two
    /// identical parents: the canonicaliser permutes the `n` of each, so the
    /// work is factorial in `n`.
    fn identical_siblings(n: usize) -> Value {
        let kids: Vec<Value> = (0..n).map(|_| json!({})).collect();
        json!({
            "@context": PFLOW_CONTEXT_URL,
            "@type": "PetriNet",
            "junk": [{"k": kids.clone()}, {"k": kids}],
        })
    }

    #[test]
    fn the_work_is_counted_and_a_small_budget_refuses() {
        let doc = identical_siblings(7);
        let quads = to_rdf(&doc).unwrap();
        let whole = canonicalize(&quads).unwrap();
        // Enough budget gives the same answer as the default, and a small one
        // refuses with the code, not with a different canonical form.
        assert_eq!(
            canonicalize_within(&quads, Some(10_000_000)).unwrap(),
            whole
        );
        let err = canonicalize_within(&quads, Some(100_000))
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("[DOCUMENT_TOO_COMPLEX] "), "{err}");
        // Eight siblings (40320 permutations of each group) are refused by
        // the default budget: seconds, not the half a minute they cost before.
        let err = seal(&identical_siblings(8)).unwrap_err().to_string();
        assert!(err.starts_with("[DOCUMENT_TOO_COMPLEX] "), "{err}");
    }

    #[test]
    fn a_long_list_is_refused_not_overflowed() {
        // 3000 elements: the cells of an `@list` look alike, so the hash walks
        // the chain to its end from each of them. Recursing on the call stack
        // aborted the process (this thread has 2 MiB) at a depth of about a
        // thousand, before the work, which is quadratic, was counted; on the
        // heap the walk reaches the bottom and is only slow. (The refusal is
        // asked of a small budget: the real one is spent in seconds.)
        let items: Vec<Value> = (0..3000)
            .map(|i| json!({"source": format!("p{i}")}))
            .collect();
        let doc = json!({"@context": PFLOW_CONTEXT_URL, "@type": "PetriNet", "arcs": items});
        let quads = to_rdf(&doc).unwrap();
        let err = canonicalize_within(&quads, Some(2_000_000))
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("[DOCUMENT_TOO_COMPLEX] "), "{err}");
        limit_work_on_this_thread(Some(2_000_000));
        let err = seal(&doc).unwrap_err().to_string();
        limit_work_on_this_thread(None);
        assert!(err.starts_with("[DOCUMENT_TOO_COMPLEX] "), "{err}");
    }

    #[test]
    fn the_limit_grows_with_the_document_between_a_floor_and_a_ceiling() {
        assert_eq!(seal_work_limit(0), MIN_SEAL_WORK);
        assert_eq!(
            seal_work_limit(19),
            MIN_SEAL_WORK,
            "a small hostile document gets the floor"
        );
        assert_eq!(seal_work_limit(10_000_000), MAX_SEAL_WORK);
        // What the nets of examples/ cost to seal, measured (blank nodes, steps): they must fit,
        // each with room to spare.
        for (nodes, needs) in [(701, 398_145), (1796, 2_887_626), (6260, 37_342_916)] {
            assert!(
                seal_work_limit(nodes) >= 2 * needs,
                "{nodes} nodes need {needs}, may take {}",
                seal_work_limit(nodes)
            );
        }
    }

    #[test]
    fn a_list_within_the_budget_still_seals_to_the_same_cid() {
        // 40 elements: the iterative walk and a second run agree, and the CID
        // does not depend on the budget (parity with pflow.xyz is tests/cid.rs).
        let items: Vec<Value> = (0..40)
            .map(|i| json!({"source": format!("p{i}")}))
            .collect();
        let doc = json!({"@context": PFLOW_CONTEXT_URL, "@type": "PetriNet", "arcs": items});
        let quads = to_rdf(&doc).unwrap();
        assert_eq!(
            canonicalize_within(&quads, Some(2_000_000)).unwrap(),
            canonicalize_within(&quads, Some(20_000_000)).unwrap()
        );
        assert!(seal(&doc).unwrap().starts_with('z'));
    }
}
