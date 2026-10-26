//! Provenance: signed run receipts and local witness checking.
//!
//! **Receipts** mirror sim.pflow.xyz's `pkg/receipt`: a seeded scenario run
//! is certified by an Ed25519 signature over the canonical JSON of
//! (model CID, the rates sealed into it, scenario as run, result hash,
//! engine, revision, issue time, key id, public key). Anyone holding a
//! receipt can check two independent things: the signature (this key
//! reported this result), and the replay (running the certified scenario on
//! the model reproduces the result hash). The canonical encoding is Go's
//! `encoding/json` over a sorted-key round trip, as `receipt.canonical`
//! produces it, so the bytes signed here are the bytes sim would sign for
//! the same receipt.
//!
//! The model CID is sealed over the document as `file.save` writes it,
//! session rates included, so it moves whenever a rate does. The receipt
//! records those rates (`modelRates`), and a verifier rebuilds the model
//! from the bare net under them. For one issued with session rates before
//! the field, they are rebuilt from the rates its certified scenario ran
//! with, unless the scenario overrode one; then it is found only where
//! those rates are again. The replay itself
//! reads no document rate: the certified scenario carries every one.
//!
//! The signing key lives at `<data_dir>/keys/receipt-ed25519.json` (created
//! on first use, mode 0600). Without a data directory (the web build) the
//! key is ephemeral: generated per process, and the receipt says so.
//!
//! **Witness checking** is the local half of `sim_check_witness`: a reach or
//! verify trace is replayed through the metamodel firing rule
//! ([`crate::sim::Compiled`]), a reach "no" certificate is re-checked as a
//! P-invariant that separates from and to (sim's `reachcheck.CheckInvariant`,
//! conditions 2-7), and an explicit place/transition map is checked as an
//! isomorphism or an induced embedding (sim's `pkg/morphism`). Like sim's
//! checkers, nothing here searches: every check is a pass over the net and
//! the witness, and none of it calls the code that found the witness.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use pflow_metamodel::{ArcType, Model};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::artifact::TraceStep;
use crate::doc::Document;
use crate::sim::{self, Compiled, Marking};
use crate::stochastic::{self as st, Net, Scenario, ScenarioRun};
use crate::{Error, Result};

/// The pflow-rs revision this build is pinned to (root `Cargo.toml`; a
/// test keeps the two equal).
pub const PFLOW_RS_REV: &str = "70caba8c4df701ebb866164601d211be60965bfe";

/// Where the receipt key lives, under the data directory.
pub const KEY_FILE: &str = "keys/receipt-ed25519.json";

// ================================================================ canonical

/// Go's `encoding/json` encoding of `v` after a sorted-key round trip:
/// object keys in byte order, numbers through float64 (integers beyond
/// 2^53 keep their digits), `<`, `>`, `&`, U+2028 and U+2029 escaped. This is
/// sim's `receipt.canonical`.
pub fn canonical(v: &Value) -> String {
    let mut out = String::new();
    write_canonical(v, &mut out);
    out
}

fn write_canonical(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&go_number(n)),
        Value::String(s) => go_string(s, out),
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(x, out);
            }
            out.push(']');
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                go_string(k, out);
                out.push(':');
                write_canonical(&m[k], out);
            }
            out.push('}');
        }
    }
}

const MAX_EXACT_INT: u64 = 1 << 53;

fn go_number(n: &serde_json::Number) -> String {
    if let Some(i) = n.as_i64() {
        if i.unsigned_abs() > MAX_EXACT_INT {
            return i.to_string();
        }
    } else if let Some(u) = n.as_u64() {
        if u > MAX_EXACT_INT {
            return u.to_string();
        }
    }
    go_float(n.as_f64().unwrap_or(0.0))
}

/// `strconv.AppendFloat(f, 'f' or 'e', -1, 64)` as encoding/json picks it.
fn go_float(f: f64) -> String {
    let abs = f.abs();
    if abs != 0.0 && !(1e-6..1e21).contains(&abs) {
        let s = format!("{f:e}");
        let (m, e) = s.split_once('e').expect("exponent form");
        let e: i32 = e.parse().expect("exponent");
        if e < 0 {
            format!("{m}e-{}", -e)
        } else {
            format!("{m}e+{e}")
        }
    } else {
        format!("{f}")
    }
}

fn go_string(s: &str, out: &mut String) {
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

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// sim's `HashResult`: `sha256:` + hex over the canonical JSON.
pub fn hash_result(v: &Value) -> String {
    format!("sha256:{}", sha256_hex(canonical(v).as_bytes()))
}

pub fn b64(bytes: &[u8]) -> String {
    B64.encode(bytes)
}

pub fn unb64(s: &str, what: &str) -> Result<Vec<u8>> {
    B64.decode(s.trim())
        .map_err(|e| Error::BadArgs(format!("{what} is not base64: {e}")))
}

/// 32 bytes from the operating system (the browser's crypto on the web).
pub fn random_seed() -> Result<[u8; 32]> {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed)
        .map_err(|e| Error::Io(format!("no randomness available: {e}")))?;
    Ok(seed)
}

/// Seconds since the Unix epoch (works on the web too).
pub fn unix_now() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// RFC 3339 UTC, second precision (Go's `time.RFC3339` of a UTC time).
pub fn rfc3339(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

// ===================================================================== keys

/// The receipt signing key.
pub struct ReceiptKey {
    signing: SigningKey,
    /// The key file, or `None` for an ephemeral (per-process) key.
    pub path: Option<PathBuf>,
    /// Created by this call (first use).
    pub created: bool,
}

impl ReceiptKey {
    pub fn from_seed(seed: [u8; 32], path: Option<PathBuf>) -> Self {
        Self {
            signing: SigningKey::from_bytes(&seed),
            path,
            created: false,
        }
    }

    pub fn ephemeral(&self) -> bool {
        self.path.is_none()
    }

    pub fn public_bytes(&self) -> [u8; 32] {
        self.signing.verifying_key().to_bytes()
    }

    /// sim's `KeyID`: the first 8 bytes of sha256(public key), hex.
    pub fn key_id(&self) -> String {
        key_id(&self.public_bytes())
    }

    pub fn public_key(&self) -> String {
        b64(&self.public_bytes())
    }

    pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
        self.signing.sign(msg).to_bytes().to_vec()
    }

    /// What `provenance.key` shows: never the secret.
    pub fn describe(&self) -> Value {
        let mut out = json!({
            "algorithm": "Ed25519",
            "keyId": self.key_id(),
            "publicKey": self.public_key(),
            "ephemeral": self.ephemeral(),
            "store": match &self.path {
                Some(p) => json!(p),
                None => json!("ephemeral (in memory: no data directory, e.g. the web build); receipts stay verifiable against their embedded key, but this key dies with the process"),
            },
            "created": self.created,
        });
        if let Some(mode) = self.path.as_deref().and_then(file_mode) {
            out["permissions"] = json!(format!("{mode:04o}"));
            if mode & 0o077 != 0 {
                out["warning"] = json!("the key file is readable by other users; chmod 600 it");
            }
        }
        out
    }
}

pub fn key_id(public: &[u8]) -> String {
    hex(&Sha256::digest(public)[..8])
}

#[cfg(unix)]
fn file_mode(p: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .ok()
        .map(|m| m.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn file_mode(_: &Path) -> Option<u32> {
    None
}

#[derive(Serialize, Deserialize)]
struct KeyFile {
    algorithm: String,
    /// The 32-byte Ed25519 seed, base64.
    seed: String,
    #[serde(rename = "publicKey")]
    public_key: String,
    #[serde(rename = "keyId")]
    key_id: String,
    #[serde(default)]
    created: String,
}

/// The key at `<dir>/keys/receipt-ed25519.json`, created (mode 0600, in a
/// 0700 directory) when absent.
pub fn load_or_create_key(dir: &Path) -> Result<ReceiptKey> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = dir.join(KEY_FILE);
    if path.exists() {
        return load_key(&path);
    }
    let parent = path.parent().expect("has a parent");
    std::fs::create_dir_all(parent).map_err(|e| Error::Io(format!("{}: {e}", parent.display())))?;
    restrict_dir(parent);
    let seed = random_seed()?;
    let mut key = ReceiptKey::from_seed(seed, Some(path.clone()));
    let file = KeyFile {
        algorithm: "Ed25519".into(),
        seed: b64(&seed),
        public_key: key.public_key(),
        key_id: key.key_id(),
        created: rfc3339(unix_now()),
    };
    let text = serde_json::to_string_pretty(&file).expect("serializable");
    match write_private(&path, text.as_bytes()) {
        Ok(()) => {
            key.created = true;
            Ok(key)
        }
        // Another process created it first: use theirs.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => load_key(&path),
        Err(e) => Err(Error::Io(format!("{}: {e}", path.display()))),
    }
}

fn load_key(path: &Path) -> Result<ReceiptKey> {
    let text =
        std::fs::read_to_string(path).map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
    let f: KeyFile = serde_json::from_str(&text)
        .map_err(|e| Error::Parse(format!("{}: {e}", path.display())))?;
    let seed: [u8; 32] = unb64(&f.seed, "the key file's seed")?
        .try_into()
        .map_err(|_| Error::Invalid(format!("{}: the seed is not 32 bytes", path.display())))?;
    let key = ReceiptKey::from_seed(seed, Some(path.to_path_buf()));
    if key.public_key() != f.public_key {
        return Err(Error::Invalid(format!(
            "{}: the stored public key does not match the seed; the file is corrupt",
            path.display()
        )));
    }
    Ok(key)
}

/// Creates `path` holding `bytes`, readable by the owner only, atomically:
/// the content goes to a temporary file first and is linked into place, so
/// another process never reads a half-written key, and an existing file is
/// never replaced (`AlreadyExists`).
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    let linked = std::fs::hard_link(&tmp, path);
    let _ = std::fs::remove_file(&tmp);
    linked
}

#[cfg(unix)]
fn restrict_dir(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn restrict_dir(_: &Path) {}

static EPHEMERAL: OnceLock<[u8; 32]> = OnceLock::new();

/// The signing key: the data directory's (created on first use), else an
/// ephemeral key for this process.
pub fn receipt_key() -> Result<ReceiptKey> {
    match crate::outbound::data_dir() {
        Some(dir) => load_or_create_key(&dir),
        None => {
            if EPHEMERAL.get().is_none() {
                let seed = random_seed()?;
                let _ = EPHEMERAL.set(seed);
            }
            Ok(ReceiptKey::from_seed(
                *EPHEMERAL.get().expect("just set"),
                None,
            ))
        }
    }
}

/// The local key's id if one exists, without creating it.
pub fn local_key_id() -> Option<String> {
    match crate::outbound::data_dir() {
        Some(dir) => {
            let path = dir.join(KEY_FILE);
            path.exists()
                .then(|| load_key(&path).ok().map(|k| k.key_id()))
                .flatten()
        }
        None => EPHEMERAL
            .get()
            .map(|s| ReceiptKey::from_seed(*s, None).key_id()),
    }
}

// ================================================================= receipts

/// A run receipt. Everything except `signature` is covered by it. The
/// fields are sim's `receipt.Receipt` as it was before receipts recorded
/// inputs, plus `modelRates`, `versions` and `keyEphemeral`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    /// The CID of the document the scenario ran against, as `file.save`
    /// writes it (`file.cid`): session rates included.
    pub model: String,
    /// The session rates sealed into `model` (transition id → rate, as
    /// `file::with_rates` writes them), stored canonical: the bare net under
    /// these rates is `model` again after the session rates move. Absent
    /// when there were none, so such a receipt keeps the shape (and signing
    /// bytes) it had before this field.
    #[serde(rename = "modelRates", default, skip_serializing_if = "Map::is_empty")]
    pub model_rates: Map<String, Value>,
    /// The scenario as run, every default filled in, the full start marking
    /// and every rate written out: replaying it needs nothing else.
    pub scenario: Value,
    /// `sha256:` over the canonical JSON of the result.
    #[serde(rename = "resultHash")]
    pub result_hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub engine: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub revision: String,
    #[serde(rename = "issuedAt")]
    pub issued_at: String,
    /// pflow-native and pflow-rs versions, and how the result was digested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub versions: Option<Value>,
    /// Signed under a per-process key (no data directory).
    #[serde(rename = "keyEphemeral", default, skip_serializing_if = "is_false")]
    pub key_ephemeral: bool,
    #[serde(rename = "keyId")]
    pub key_id: String,
    #[serde(rename = "publicKey")]
    pub public_key: String,
    /// Ed25519 over the canonical JSON of the receipt with this field "",
    /// base64.
    #[serde(default)]
    pub signature: String,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// The revision string receipts carry (sim's is its Cloud Run revision).
pub fn revision() -> String {
    format!(
        "pflow-native/{} pflow-rs/{}",
        env!("CARGO_PKG_VERSION"),
        &PFLOW_RS_REV[..12]
    )
}

impl Receipt {
    /// The bytes the signature covers (sim's `signingBytes`: the receipt
    /// with `signature` set to "", canonical).
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut unsigned = self.clone();
        unsigned.signature = String::new();
        canonical(&serde_json::to_value(&unsigned).expect("serializable")).into_bytes()
    }

    /// Checks the signature against the embedded public key; the key id it
    /// verified under, or why not.
    pub fn verify_signature(&self) -> std::result::Result<String, String> {
        let pub_bytes: [u8; 32] = B64
            .decode(&self.public_key)
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or("receipt carries no valid public key")?;
        let vk = VerifyingKey::from_bytes(&pub_bytes)
            .map_err(|_| "receipt carries no valid public key".to_string())?;
        let sig: [u8; 64] = B64
            .decode(&self.signature)
            .map_err(|e| format!("signature is not base64: {e}"))?
            .try_into()
            .map_err(|_| "signature is not 64 bytes".to_string())?;
        vk.verify(&self.signing_bytes(), &Signature::from_bytes(&sig))
            .map_err(|_| {
                "signature does not verify: the receipt was altered or was not signed by the key it names"
                    .to_string()
            })?;
        Ok(key_id(&pub_bytes))
    }

    /// `modelRates` as `file::with_rates` takes them, or None if one is not
    /// a number. As f64, so `with_rates` writes the numbers it wrote at
    /// issue (a canonical `3` is the session's 3.0).
    pub fn rates_at_issue(&self) -> Option<BTreeMap<String, f64>> {
        self.model_rates
            .iter()
            .map(|(t, r)| Some((t.clone(), r.as_f64()?)))
            .collect()
    }
}

/// Signs a receipt for a run (sim's `Signer.Issue`). `model_rates` are the
/// session rates sealed into `model`, recorded as `modelRates`.
#[allow(clippy::too_many_arguments)]
pub fn issue(
    key: &ReceiptKey,
    model: &str,
    model_rates: &BTreeMap<String, f64>,
    scenario: Value,
    result_hash: String,
    engine: &str,
    versions: Value,
    issued_at: u64,
) -> Receipt {
    // Stored canonical, as sim stores it (sorted keys, numbers as Go
    // writes them).
    let stored = |v: Value| serde_json::from_str(&canonical(&v)).unwrap_or(v);
    let mut r = Receipt {
        model: model.to_string(),
        model_rates: model_rates
            .iter()
            .map(|(t, r)| (t.clone(), stored(json!(r))))
            .collect(),
        scenario: stored(scenario),
        result_hash,
        engine: engine.to_string(),
        revision: revision(),
        issued_at: rfc3339(issued_at),
        versions: Some(versions),
        key_ephemeral: key.ephemeral(),
        key_id: key.key_id(),
        public_key: key.public_key(),
        signature: String::new(),
    };
    r.signature = b64(&key.sign(&r.signing_bytes()));
    r
}

/// A receipt from JSON: the receipt object, a string holding one, or the
/// `{result, receipt}` object `sim.receipt` returns.
pub fn parse_receipt(v: &Value) -> Result<Receipt> {
    let v = st::json_arg(v, "receipt")?;
    let inner = match v.get("receipt") {
        Some(r) if v.get("signature").is_none() => r.clone(),
        _ => v,
    };
    serde_json::from_value(inner).map_err(|e| Error::BadArgs(format!("not a receipt: {e}")))
}

/// A scenario with everything a replay needs written in: sim.scenario's
/// defaults, the full start marking (the session marking under the
/// scenario's overrides, every token place by display name) and every
/// transition's rate (scenario over session, absent = 1). Replaying it
/// does not depend on the canvas marking or the session rates.
pub fn certify_scenario(
    doc: &Document,
    mk: &Marking,
    rates: &BTreeMap<String, f64>,
    scenario: &Value,
) -> Result<Value> {
    let net = Net::new(doc)?;
    let sc = st::parse_scenario(scenario, &net)?;
    let mut start = net.model_marking(mk);
    for (k, v) in &sc.marking {
        start.insert(k.clone(), *v);
    }
    let marking: Map<String, Value> = net
        .places
        .iter()
        .zip(&net.display)
        .map(|(id, d)| (d.clone(), json!(start.get(id).copied().unwrap_or(0))))
        .collect();
    let all_rates: Map<String, Value> = doc
        .transitions
        .iter()
        .map(|t| {
            let r = sc
                .rates
                .get(&t.id)
                .or_else(|| rates.get(&t.id))
                .copied()
                .unwrap_or(1.0);
            (t.id.clone(), json!(r))
        })
        .collect();
    let mut out = json!({
        "marking": marking,
        "rates": all_rates,
        "hours": sc.hours,
        "samples": sc.samples,
        "realizations": sc.realizations,
        "seed": sc.seed,
        "summary": sc.summary,
        "engine": sc.engine.name(),
    });
    if let Some(n) = &sc.name {
        out["name"] = json!(n);
    }
    if !sc.schedule.is_empty() {
        out["schedule"] = serde_json::to_value(&sc.schedule).expect("serializable");
    }
    Ok(out)
}

/// A certified scenario's run, independent of any session state.
pub struct CertifiedRun {
    pub scenario: Scenario,
    pub run: ScenarioRun,
    /// The result as returned (and hashed).
    pub result: Value,
    pub result_hash: String,
    /// How `result_hash` was computed.
    pub digest: &'static str,
}

/// How a result is digested: SSA results are byte-exact; ODE results are
/// rounded first. Before pflow-rs fcb38ac the ODE summed derivatives in hash
/// order, so the last bits varied between runs (FEATURES.md §D 7). It is
/// bit-reproducible now, but the rule stays: receipts already issued name
/// it, and other platforms' floating point can still differ in the last bits.
pub const DIGEST_EXACT: &str = "sha256 over the canonical JSON of the result";
pub const DIGEST_ODE: &str =
    "sha256 over the canonical JSON of the result, every number rounded to 9 significant digits (pflow-rs ODE sums are not bit-reproducible, FEATURES.md §D 7)";

/// Runs a certified scenario from the document's declared marking with no
/// session rates (the certified scenario overrides both entirely).
pub fn run_certified(doc: &Document, scenario: &Value) -> Result<CertifiedRun> {
    let net = Net::new(doc)?;
    let sc = st::parse_scenario(scenario, &net)?;
    let mk = sim::initial_marking(doc);
    let run = st::run_scenario(doc, &net, &mk, &BTreeMap::new(), &sc)?;
    let name = sc.name.clone().unwrap_or_else(|| "scenario".into());
    let mut result = run.summary(&name, &sc);
    if !sc.summary {
        let e = &run.series;
        let mean: Map<String, Value> = e
            .names
            .iter()
            .zip(&e.mean)
            .map(|(n, v)| (n.clone(), json!(v)))
            .collect();
        let mut series = json!({ "t": e.t, "mean": mean });
        if let Some(sd) = &e.sd {
            let sd: Map<String, Value> = e
                .names
                .iter()
                .zip(sd)
                .map(|(n, v)| (n.clone(), json!(v)))
                .collect();
            series["sd"] = Value::Object(sd);
        }
        result["series"] = series;
    }
    let (result_hash, digest) = if sc.engine == st::Engine::Ode {
        (hash_result(&round_numbers(&result, 9)), DIGEST_ODE)
    } else {
        (hash_result(&result), DIGEST_EXACT)
    };
    Ok(CertifiedRun {
        scenario: sc,
        run,
        result,
        result_hash,
        digest,
    })
}

/// Every non-integer number rounded to `sig` significant digits.
pub fn round_numbers(v: &Value, sig: i32) -> Value {
    match v {
        Value::Number(n) if n.is_f64() => {
            let f = n.as_f64().unwrap_or(0.0);
            if f == 0.0 || !f.is_finite() {
                return v.clone();
            }
            let mag = f.abs().log10().floor() as i32;
            let scale = 10f64.powi(sig - 1 - mag);
            let r = (f * scale).round() / scale;
            // Through the decimal string, so 0.1 + noise and 0.1 meet.
            let r: f64 = format!("{r:.*e}", (sig - 1) as usize).parse().unwrap_or(r);
            json!(r)
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| round_numbers(x, sig)).collect()),
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, x)| (k.clone(), round_numbers(x, sig)))
                .collect(),
        ),
        other => other.clone(),
    }
}

pub fn versions(digest: &str) -> Value {
    json!({
        "pflow-native": env!("CARGO_PKG_VERSION"),
        "pflow-rs": PFLOW_RS_REV,
        "resultDigest": digest,
    })
}

/// The document's CID, as `file.cid` reports it.
pub fn model_cid(doc: &Document) -> Result<String> {
    crate::cid::seal(&doc.to_value())
}

// ========================================================= witness checks

pub const VALID: &str = "valid";
pub const INVALID: &str = "invalid";
pub const UNCHECKABLE: &str = "uncheckable";

/// A checker's answer: sim's {verdict, reason}, plus every failure found.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Verdict {
    pub verdict: String,
    pub reason: String,
    pub failures: Vec<String>,
}

impl Verdict {
    pub fn valid(reason: impl Into<String>) -> Self {
        Self {
            verdict: VALID.into(),
            reason: reason.into(),
            failures: vec![],
        }
    }

    pub fn invalid(failures: Vec<String>) -> Self {
        Self {
            verdict: INVALID.into(),
            reason: failures.first().cloned().unwrap_or_default(),
            failures,
        }
    }

    pub fn uncheckable(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            verdict: UNCHECKABLE.into(),
            reason: reason.clone(),
            failures: vec![],
        }
    }
}

/// Failures reported per check; the verdict needs only the first.
const MAX_FAILURES: usize = 20;

/// sim's per-place and total caps on a reach question's markings.
pub const MAX_MARKING: i64 = 1_000_000;
pub const MAX_TOTAL_TOKENS: i64 = 4_000_000;
/// sim's cap on an invariant coefficient.
pub const MAX_COEFFICIENT: i64 = 2_147_483_647;
/// Steps a trace replay walks (sim's `MaxTraceSteps`, its state budget).
pub const MAX_TRACE_STEPS: usize = 100_000;

/// A document marking from a trace or answer, checked against the net:
/// every place known, counts whole, non-negative and capped, one per color.
fn doc_marking(
    doc: &Document,
    v: &BTreeMap<String, Vec<i64>>,
    what: &str,
) -> std::result::Result<Marking, String> {
    let n = doc.colors();
    let mut out = sim::initial_marking(doc);
    for c in out.values_mut() {
        c.iter_mut().for_each(|x| *x = 0);
    }
    let mut total = 0i64;
    for (p, counts) in v {
        if doc.place(p).is_none() {
            return Err(format!(
                "{what} names {p:?}, which is not a place of the net"
            ));
        }
        if counts.len() > n {
            return Err(format!(
                "{what} gives {p:?} {} colors; the net has {n}",
                counts.len()
            ));
        }
        for &x in counts {
            if x < 0 {
                return Err(format!(
                    "{what} gives {p:?} a negative count ({x}); a coverability ω is not a marking"
                ));
            }
            if x > MAX_MARKING {
                return Err(format!(
                    "{what} gives {p:?} {x} tokens, over the {MAX_MARKING} cap"
                ));
            }
            total += x;
        }
        let mut c = counts.clone();
        c.resize(n, 0);
        out.insert(p.clone(), c);
    }
    if total > MAX_TOTAL_TOKENS {
        return Err(format!(
            "{what} totals {total} tokens, over the {MAX_TOTAL_TOKENS} cap"
        ));
    }
    Ok(out)
}

fn same_marking(a: &Marking, b: &Marking) -> Option<String> {
    let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    for k in keys {
        let x = a.get(k).cloned().unwrap_or_default();
        let y = b.get(k).cloned().unwrap_or_default();
        let n = x.len().max(y.len());
        let (mut x, mut y) = (x, y);
        x.resize(n, 0);
        y.resize(n, 0);
        if x != y {
            return Some(format!("{k} is {y:?} there, but replaying gives {x:?}"));
        }
    }
    None
}

/// Replays a trace (step 0 = the start marking when its transition is "";
/// otherwise the trace starts from the declared initial marking) through
/// the metamodel firing rule, checking each claimed marking. With `to`,
/// the last marking must also match those constraints (model place index,
/// count over `net`).
pub fn check_trace(
    doc: &Document,
    steps: &[TraceStep],
    to: Option<(&Net, &[(String, i64)])>,
) -> Verdict {
    if steps.is_empty() {
        return Verdict::invalid(vec!["the trace has no steps".into()]);
    }
    if steps.len() > MAX_TRACE_STEPS {
        return Verdict::uncheckable(format!(
            "the trace has {} steps, over the {MAX_TRACE_STEPS} a search can produce",
            steps.len()
        ));
    }
    let compiled = match Compiled::new(doc) {
        Ok(c) => c,
        Err(e) => return Verdict::uncheckable(format!("the net does not compile: {e}")),
    };
    let (mut cur, firings, assumed) = if steps[0].transition.is_empty() {
        match doc_marking(doc, &steps[0].marking, "step 0") {
            Ok(m) => (m, &steps[1..], false),
            Err(e) => return Verdict::invalid(vec![e]),
        }
    } else {
        (sim::initial_marking(doc), steps, true)
    };
    let offset = steps.len() - firings.len();
    let mut failures = vec![];
    for (k, st) in firings.iter().enumerate() {
        let i = k + offset;
        let claimed = match doc_marking(doc, &st.marking, &format!("step {i}")) {
            Ok(m) => m,
            Err(e) => {
                failures.push(e);
                break;
            }
        };
        if doc.transition(&st.transition).is_none() {
            failures.push(format!(
                "step {i} names {:?}, which is not a transition of the net",
                st.transition
            ));
            break;
        }
        match compiled.fire(&st.transition, &cur) {
            Ok(next) => {
                if let Some(diff) = same_marking(&next, &claimed) {
                    failures.push(format!(
                        "step {i} ({}): the trace claims a marking the firing does not produce: {diff}",
                        st.transition
                    ));
                }
            }
            Err(e) => failures.push(format!(
                "step {i}: {} cannot fire at the marking before it: {e}",
                st.transition
            )),
        }
        if failures.len() >= MAX_FAILURES {
            break;
        }
        // Continue from the claimed marking, so every bad step is named.
        cur = claimed;
    }
    if let Some((net, to)) = to {
        let last = net.model_marking(&cur);
        for (id, n) in to {
            let got = last.get(id).copied().unwrap_or(0);
            if got != *n {
                failures.push(format!(
                    "the trace ends with {} at {got}, but to asks for {n}",
                    net.display_of(id)
                ));
            }
        }
    }
    if !failures.is_empty() {
        return Verdict::invalid(failures);
    }
    let start = if assumed {
        "from the declared initial marking (the trace names no start)"
    } else {
        "from its start marking"
    };
    let ends = if to.is_some() { " and ends on to" } else { "" };
    Verdict::valid(format!(
        "the {}-step trace replays {start} through the metamodel firing rule, every claimed marking matches{ends}",
        firings.len()
    ))
}

/// Resolves a `{place: n}` key (a model id or display name) to a model
/// token place.
fn token_place(net: &Net, key: &str) -> std::result::Result<String, String> {
    net.resolve_place(key)
        .map_err(|_| format!("{key:?} is not a token place of the net"))
}

/// Resolves a reach answer's `to` (`{display name: n}`, or a document id
/// with a per-color list) to model place constraints.
fn resolve_to(
    net: &Net,
    doc: &Document,
    to: &Value,
) -> std::result::Result<Vec<(String, i64)>, String> {
    let obj = to.as_object().ok_or("to is not an object")?;
    if obj.is_empty() {
        return Err("to names no place".into());
    }
    let mut out = vec![];
    for (k, v) in obj {
        match v {
            Value::Array(a) => {
                if doc.place(k).is_none() {
                    return Err(format!("to names {k:?}, which is not a place of the net"));
                }
                for (c, x) in a.iter().enumerate() {
                    if x.is_null() {
                        continue;
                    }
                    let n = whole(x, &format!("to's {k}[{c}]"))?;
                    let id = net
                        .places
                        .iter()
                        .find(|id| net.base(id) == (k.clone(), c))
                        .ok_or_else(|| format!("place {k:?} has no color {c}"))?;
                    out.push((id.clone(), n));
                }
            }
            x => out.push((token_place(net, k)?, whole(x, &format!("to's {k}"))?)),
        }
    }
    for (id, n) in &out {
        if *n < 0 || *n > MAX_MARKING {
            return Err(format!(
                "to gives {} {n} tokens, outside 0..{MAX_MARKING}",
                net.display_of(id)
            ));
        }
    }
    Ok(out)
}

fn whole(v: &Value, what: &str) -> std::result::Result<i64, String> {
    v.as_f64()
        .filter(|x| x.is_finite() && x.fract() == 0.0)
        .map(|x| x as i64)
        .ok_or_else(|| format!("{what} must be a whole number, got {v}"))
}

/// Checks a reach answer as `analysis.reach` returns it (sim's
/// `reachcheck.Check`, against the open document).
pub fn check_reach_answer(doc: &Document, a: &Value) -> Verdict {
    let def = a.get("definition").and_then(Value::as_str).unwrap_or("");
    if def != "reach-v1" {
        return Verdict::uncheckable(format!("checked under reach-v1; this answer names {def:?}"));
    }
    let net = match Net::new(doc) {
        Ok(n) => n,
        Err(e) => return Verdict::uncheckable(format!("the net does not compile: {e}")),
    };
    let from: BTreeMap<String, Vec<i64>> = match serde_json::from_value(a["from"].clone()) {
        Ok(f) => f,
        Err(e) => return Verdict::invalid(vec![format!("from is not a marking: {e}")]),
    };
    for p in &doc.places {
        if !from.contains_key(&p.id) {
            return Verdict::invalid(vec![format!(
                "from is missing place {:?}; a reach answer lists every place",
                p.id
            )]);
        }
    }
    let from = match doc_marking(doc, &from, "from") {
        Ok(m) => m,
        Err(e) => return Verdict::invalid(vec![e]),
    };
    let to = match resolve_to(&net, doc, &a["to"]) {
        Ok(t) => t,
        Err(e) => return Verdict::invalid(vec![e]),
    };
    let answer = a.get("answer").and_then(Value::as_str).unwrap_or("");
    let method = a.get("method").and_then(Value::as_str).unwrap_or("");
    let w = &a["witness"];
    let kind = w.get("kind").and_then(Value::as_str).unwrap_or("");
    match (answer, method, kind) {
        ("yes", "trace", "trace") => {
            let ids: Vec<String> = match serde_json::from_value(w["trace"]["steps"].clone()) {
                Ok(s) => s,
                Err(e) => return Verdict::invalid(vec![format!("the trace's steps: {e}")]),
            };
            let compiled = match Compiled::new(doc) {
                Ok(c) => c,
                Err(e) => return Verdict::uncheckable(format!("the net does not compile: {e}")),
            };
            let mut mk = from.clone();
            let mut steps = vec![TraceStep {
                transition: String::new(),
                marking: mk.clone(),
            }];
            for (i, t) in ids.iter().enumerate() {
                match compiled.fire(t, &mk) {
                    Ok(next) => mk = next,
                    Err(e) => {
                        return Verdict::invalid(vec![format!("step {}: {t}: {e}", i + 1)])
                    }
                }
                steps.push(TraceStep {
                    transition: t.clone(),
                    marking: mk.clone(),
                });
            }
            let mut v = check_trace(doc, &steps, Some((&net, &to)));
            if v.verdict == VALID {
                let reached: BTreeMap<String, Vec<i64>> =
                    serde_json::from_value(w["trace"]["reached"].clone()).unwrap_or_default();
                match doc_marking(doc, &reached, "reached") {
                    Ok(r) => {
                        if let Some(d) = same_marking(&mk, &r) {
                            v = Verdict::invalid(vec![format!("reached disagrees with the trace's end: {d}")]);
                        }
                    }
                    Err(e) => v = Verdict::invalid(vec![e]),
                }
            }
            v
        }
        ("no", "invariant", "invariant") => check_invariant(&net, &from, &to, &w["invariant"]),
        ("no", "exhaustive", "exhaustive") => Verdict::uncheckable(
            "an exhaustive search has no certificate smaller than the search itself; re-run analysis.reach, which is deterministic",
        ),
        ("unknown", _, _) => Verdict::uncheckable("an unknown answer claims nothing to check"),
        _ => Verdict::invalid(vec![format!(
            "answer {answer:?} with a {kind:?} witness is not a reach answer"
        )]),
    }
}

/// sim's `reachcheck.CheckInvariant`: `y` (coefficients by display name or
/// model id) is a P-invariant of the net (yᵀC = 0 over consuming and
/// producing arcs), weighs free places ≥ 0 with no double-consuming
/// transition on them, its stated sums are right, its free list is right,
/// and it separates from and to.
pub fn check_invariant(net: &Net, from: &Marking, to: &[(String, i64)], w: &Value) -> Verdict {
    let coeffs = match w.get("coefficients").and_then(Value::as_object) {
        Some(c) if !c.is_empty() => c,
        _ => {
            return Verdict::invalid(vec![
                "y has no coefficients; an invariant witness lists its nonzero entries".into(),
            ])
        }
    };
    let mut y: BTreeMap<String, i128> = BTreeMap::new();
    for (k, c) in coeffs {
        let id = match token_place(net, k) {
            Ok(id) => id,
            Err(e) => return Verdict::invalid(vec![format!("y has a coefficient on {e}")]),
        };
        let c = match c.as_i64() {
            Some(c) => c,
            None => {
                return Verdict::invalid(vec![format!(
                    "y's coefficient on {k:?} is not an integer"
                )])
            }
        };
        if c == 0 {
            return Verdict::invalid(vec![format!(
                "y gives {k:?} a zero coefficient; list nonzero entries only"
            )]);
        }
        if c.abs() > MAX_COEFFICIENT {
            return Verdict::invalid(vec![format!(
                "y gives {k:?} coefficient {c}, over the {MAX_COEFFICIENT} cap"
            )]);
        }
        y.insert(id, c as i128);
    }
    let yy = |p: &str| y.get(p).copied().unwrap_or(0);
    let m = &net.model;
    let mut failures = vec![];
    // 2. yᵀC = 0.
    for t in &m.transitions {
        let mut d: i128 = 0;
        for a in m.outputs(&t.id) {
            d += yy(&a.place) * a.weight as i128;
        }
        for a in m.inputs(&t.id) {
            d -= yy(&a.place) * a.weight as i128;
        }
        if d != 0 {
            failures.push(format!(
                "y is not a P-invariant: firing {:?} changes the weighted sum by {d}",
                t.id
            ));
        }
    }
    if !failures.is_empty() {
        return Verdict::invalid(failures);
    }
    // 3, 4: the free support.
    let free: Vec<&String> = y
        .keys()
        .filter(|p| !to.iter().any(|(q, _)| q == *p))
        .collect();
    for p in &free {
        if y[*p] < 0 {
            return Verdict::invalid(vec![format!(
                "y weighs free place {} by {}; a place to leaves free can hold any count, so a negative weight on it bounds nothing",
                net.display_of(p),
                y[*p]
            )]);
        }
        for t in &m.transitions {
            let n = m.inputs(&t.id).iter().filter(|a| a.place == **p).count();
            if n >= 2 {
                return Verdict::invalid(vec![format!(
                    "{:?} has {n} consuming arcs from free place {}; the shared rule tests each alone and consumes their sum, so it can go negative and bounds nothing",
                    t.id,
                    net.display_of(p)
                )]);
            }
        }
    }
    // 5. The stated sums.
    let from_m = net.model_marking(from);
    let at_from: i128 = y
        .iter()
        .map(|(p, c)| c * from_m.get(p).copied().unwrap_or(0) as i128)
        .sum();
    let at_target: i128 = to.iter().map(|(p, n)| yy(p) * *n as i128).sum();
    let stated = |k: &str| w.get(k).and_then(Value::as_i64).map(i128::from);
    if stated("atFrom") != Some(at_from) {
        failures.push(format!(
            "atFrom is {at_from}, not the stated {}",
            w["atFrom"]
        ));
    }
    if stated("atTarget") != Some(at_target) {
        failures.push(format!(
            "atTarget is {at_target}, not the stated {}",
            w["atTarget"]
        ));
    }
    // 6. Free, as stated (display names or model ids).
    let stated_free: BTreeSet<String> = w
        .get("free")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(|s| token_place(net, s).unwrap_or_else(|_| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let actual_free: BTreeSet<String> = free.iter().map(|p| (*p).clone()).collect();
    if stated_free != actual_free {
        failures.push(format!(
            "free lists {:?}; the support places to leaves free are {:?}",
            stated_free
                .iter()
                .map(|p| net.display_of(p))
                .collect::<Vec<_>>(),
            actual_free
                .iter()
                .map(|p| net.display_of(p))
                .collect::<Vec<_>>()
        ));
    }
    if !failures.is_empty() {
        return Verdict::invalid(failures);
    }
    // 7. Separation.
    if free.is_empty() {
        if at_target == at_from {
            return Verdict::invalid(vec![format!(
                "atTarget equals atFrom ({at_from}), so y does not separate from and to"
            )]);
        }
        return Verdict::valid(format!(
            "y is a P-invariant of the net and separates from ({at_from}) from to ({at_target})"
        ));
    }
    if at_target <= at_from {
        return Verdict::invalid(vec![format!(
            "atTarget {at_target} does not exceed atFrom {at_from}; with free places weighted ≥ 0, y separates from and to only when atTarget > atFrom"
        )]);
    }
    Verdict::valid(format!(
        "y is a P-invariant of the net and separates from ({at_from}) from to (at least {at_target}, before counting the free places {})",
        free.iter().map(|p| net.display_of(p)).collect::<Vec<_>>().join(", ")
    ))
}

// ============================================================== morphisms

/// sim's definitions: isomorphicTo checks `coloured-net-v1`, subnetOf
/// `induced-incidence-v1`.
pub const DEF_ISOMORPHISM: &str = "coloured-net-v1";
pub const DEF_EMBEDDING: &str = "induced-incidence-v1";

/// A claimed map from a subject net onto (or into) an object net.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Mapping {
    #[serde(default)]
    pub places: BTreeMap<String, String>,
    #[serde(default)]
    pub transitions: BTreeMap<String, String>,
}

/// sim's `ArcLabel`: "<consume|read|inhibitor>/<weight>/<k|nk>".
fn arc_label(a: &pflow_metamodel::Arc) -> String {
    let kind = match a.typ {
        ArcType::Normal => "consume",
        ArcType::Read => "read",
        ArcType::Inhibitor => "inhibitor",
    };
    let kin = if a.is_kinetic() { "k" } else { "nk" };
    format!("{kind}/{}/{kin}", a.effective_weight())
}

struct Indexed<'a> {
    m: &'a Model,
    places: BTreeMap<&'a str, &'a pflow_metamodel::Place>,
    transitions: BTreeSet<&'a str>,
    pairs: BTreeMap<(String, String), Vec<String>>,
}

fn index(m: &Model) -> Indexed<'_> {
    let mut pairs: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for a in &m.arcs {
        pairs
            .entry((a.from.clone(), a.to.clone()))
            .or_default()
            .push(arc_label(a));
    }
    for v in pairs.values_mut() {
        v.sort();
    }
    Indexed {
        m,
        places: m.places.iter().map(|p| (p.id.as_str(), p)).collect(),
        transitions: m.transitions.iter().map(|t| t.id.as_str()).collect(),
        pairs,
    }
}

/// Malformed nets are uncheckable, not refuted (sim's `Wellformed`).
fn wellformed(side: &str, m: &Model) -> std::result::Result<(), String> {
    let mut seen = BTreeSet::new();
    for id in m
        .places
        .iter()
        .map(|p| &p.id)
        .chain(m.transitions.iter().map(|t| &t.id))
    {
        if id.is_empty() {
            return Err(format!(
                "model {side} is not a well-formed net: a node has no id"
            ));
        }
        if !seen.insert(id) {
            return Err(format!(
                "model {side} is not a well-formed net: {id:?} is declared twice"
            ));
        }
    }
    Ok(())
}

fn refine_tags(p: &pflow_metamodel::Place) -> BTreeMap<String, String> {
    p.tags
        .iter()
        .flatten()
        .filter(|(k, _)| k.starts_with("refine."))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Checks `mp` under `definition` from `a` (subject) to `b` (object):
/// sim's `morphism.Check` for coloured-net-v1 (isomorphism: a bijection
/// preserving every arc's direction, type, weight and kinetic flag, and
/// each place's sort, initial marking, capacity and refine.* tags) and
/// induced-incidence-v1 (an injective, sort-preserving map whose image
/// carries exactly the subject's arcs).
pub fn check_mapping(definition: &str, a: &Model, b: &Model, mp: &Mapping) -> Verdict {
    if definition != DEF_ISOMORPHISM && definition != DEF_EMBEDDING {
        return Verdict::uncheckable(format!(
            "definition {definition:?} is not one this checks ({DEF_ISOMORPHISM}, {DEF_EMBEDDING}); preservesInvariant and refines witnesses are checked remotely (sim_check_witness)"
        ));
    }
    if let Err(e) = wellformed("subject", a).and_then(|_| wellformed("object", b)) {
        return Verdict::uncheckable(e);
    }
    let (na, nb) = (index(a), index(b));
    let fail = |s: String| Verdict::invalid(vec![s]);
    // Shape: total, names only subject nodes, sort-preserving, injective.
    for id in na.places.keys() {
        if !mp.places.contains_key(*id) {
            return fail(format!("subject place {id:?} has no image; the map must cover every place and transition of the subject"));
        }
    }
    for id in &na.transitions {
        if !mp.transitions.contains_key(*id) {
            return fail(format!("subject transition {id:?} has no image; the map must cover every place and transition of the subject"));
        }
    }
    for id in mp.places.keys() {
        if !na.places.contains_key(id.as_str()) {
            return fail(format!(
                "the map names {id:?} as a subject place, and the subject has no place {id:?}"
            ));
        }
    }
    for id in mp.transitions.keys() {
        if !na.transitions.contains(id.as_str()) {
            return fail(format!("the map names {id:?} as a subject transition, and the subject has no transition {id:?}"));
        }
    }
    let sort = |p: &pflow_metamodel::Place| if p.is_token() { "token" } else { "data" };
    for (id, p) in &na.places {
        let img = &mp.places[*id];
        let Some(q) = nb.places.get(img.as_str()) else {
            return fail(format!(
                "subject place {id:?} maps to {img:?}, which is not a place of the object"
            ));
        };
        if sort(p) != sort(q) {
            return fail(format!(
                "subject place {id:?} is a {} place and its image {img:?} is a {} place",
                sort(p),
                sort(q)
            ));
        }
    }
    for id in &na.transitions {
        let img = &mp.transitions[*id];
        if !nb.transitions.contains(img.as_str()) {
            return fail(format!("subject transition {id:?} maps to {img:?}, which is not a transition of the object"));
        }
    }
    let mut inv: HashMap<&str, &str> = HashMap::new();
    for (id, img) in mp.places.iter().chain(&mp.transitions) {
        if let Some(prev) = inv.insert(img.as_str(), id.as_str()) {
            return fail(format!(
                "subject nodes {prev:?} and {id:?} both map to object node {img:?}; the map must be injective"
            ));
        }
    }
    if definition == DEF_ISOMORPHISM {
        for id in nb.places.keys().chain(nb.transitions.iter()) {
            if !inv.contains_key(id) {
                return fail(format!(
                    "object node {id:?} is the image of no subject node; {DEF_ISOMORPHISM} needs a bijection"
                ));
            }
        }
        for (id, p) in &na.places {
            let img = &mp.places[*id];
            let q = nb.places[img.as_str()];
            if p.initial != q.initial {
                return fail(format!(
                    "subject place {id:?} starts with {} tokens and its image {img:?} starts with {}; {DEF_ISOMORPHISM} compares initial markings",
                    p.initial, q.initial
                ));
            }
            if p.capacity != q.capacity {
                return fail(format!(
                    "subject place {id:?} has capacity {} and its image {img:?} has capacity {}; {DEF_ISOMORPHISM} compares capacities",
                    p.capacity, q.capacity
                ));
            }
            if refine_tags(p) != refine_tags(q) {
                return fail(format!(
                    "subject place {id:?} and its image {img:?} carry different refine.* tags; {DEF_ISOMORPHISM} compares them"
                ));
            }
        }
    }
    // The pair rule: arc-label multisets between every ordered pair of
    // subject nodes equal those between their images.
    let image = |id: &str| -> String {
        mp.places
            .get(id)
            .or_else(|| mp.transitions.get(id))
            .cloned()
            .unwrap_or_default()
    };
    let mut keys: BTreeSet<(String, String)> = na.pairs.keys().cloned().collect();
    for (u, v) in nb.pairs.keys() {
        if let (Some(x), Some(y)) = (inv.get(u.as_str()), inv.get(v.as_str())) {
            keys.insert((x.to_string(), y.to_string()));
        }
    }
    let none = vec![];
    for (x, y) in keys {
        let (u, v) = (image(&x), image(&y));
        let la = na.pairs.get(&(x.clone(), y.clone())).unwrap_or(&none);
        let lb = nb.pairs.get(&(u.clone(), v.clone())).unwrap_or(&none);
        if la != lb {
            return fail(format!(
                "the subject's arcs from {x:?} to {y:?} are {la:?} and the object's arcs from their images {u:?} to {v:?} are {lb:?}; {definition} needs them equal"
            ));
        }
    }
    let _ = na.m;
    let what = if definition == DEF_ISOMORPHISM {
        "an isomorphism (coloured-net-v1: arcs, place sorts, initial markings, capacities, refine.* tags)"
    } else {
        "an induced embedding (induced-incidence-v1)"
    };
    Verdict::valid(format!(
        "the map is {what} of the subject ({} places, {} transitions) into the object",
        mp.places.len(),
        mp.transitions.len()
    ))
}

/// What coloured-net-v1 does not compare on pflow.xyz documents, so a
/// valid verdict is never read as covering it.
pub const NOT_COMPARED: &[&str] = &[
    "rates (pflow.xyz documents carry none; session rates are not part of a model)",
    "labels, positions and the document's name and description",
];

/// The metamodel of a single-color document; colored nets unfold their
/// places, so a map by document ids cannot be checked against them.
pub fn single_color_model(doc: &Document, side: &str) -> std::result::Result<Model, String> {
    if doc.colors() > 1 {
        return Err(format!(
            "the {side} net is colored ({} colors); maps are checked on single-color nets only",
            doc.colors()
        ));
    }
    Compiled::new(doc)
        .map(|c| c.model)
        .map_err(|e| format!("the {side} net does not compile: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_matches_go() {
        let v = json!({"b": [1, 2.5, 1e-7, 1e21, 0.000001, 123456789012345678_i64], "a": "<&>\u{2028}\n\u{1}", "c": null});
        assert_eq!(
            canonical(&v),
            "{\"a\":\"\\u003c\\u0026\\u003e\\u2028\\n\\u0001\",\"b\":[1,2.5,1e-7,1e+21,0.000001,123456789012345678],\"c\":null}"
        );
        assert_eq!(go_float(100.0), "100");
        assert_eq!(go_float(-0.5), "-0.5");
        assert_eq!(go_float(1.5e-10), "1.5e-10");
    }

    #[test]
    fn rfc3339_known_dates() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339(1_792_540_800 + 3661), "2026-10-21T01:01:01Z");
    }

    #[test]
    fn pinned_rev_matches_cargo_toml() {
        let toml = include_str!("../../../Cargo.toml");
        assert!(
            toml.contains(PFLOW_RS_REV),
            "PFLOW_RS_REV is out of step with the root Cargo.toml"
        );
    }

    #[test]
    fn signature_round_trip_and_tamper() {
        let key = ReceiptKey::from_seed([7u8; 32], None);
        let r = issue(
            &key,
            "zTest",
            &BTreeMap::new(),
            json!({"seed": 1}),
            "sha256:00".into(),
            "ssa",
            versions(DIGEST_EXACT),
            0,
        );
        assert_eq!(r.verify_signature().unwrap(), key.key_id());
        let mut bad = r.clone();
        bad.result_hash = "sha256:01".into();
        assert!(bad.verify_signature().is_err());
        // No rates, no field: the shape and signing bytes of before.
        assert!(serde_json::to_value(&r)
            .unwrap()
            .get("modelRates")
            .is_none());
        assert!(!String::from_utf8(r.signing_bytes())
            .unwrap()
            .contains("modelRates"));
    }

    #[test]
    fn model_rates_are_canonical_and_signed() {
        let key = ReceiptKey::from_seed([7u8; 32], None);
        let rates = BTreeMap::from([("a".to_string(), 3.0), ("b".to_string(), 0.002)]);
        let r = issue(
            &key,
            "zTest",
            &rates,
            json!({}),
            "sha256:00".into(),
            "ssa",
            versions(DIGEST_EXACT),
            0,
        );
        // Stored as Go writes them; read back as the f64s the session held.
        assert_eq!(canonical(&json!(r.model_rates)), "{\"a\":3,\"b\":0.002}");
        assert_eq!(
            serde_json::to_value(&r).unwrap()["modelRates"],
            json!({"a": 3, "b": 0.002})
        );
        assert_eq!(r.rates_at_issue(), Some(rates));
        let text = serde_json::to_string(&r).unwrap();
        let back: Receipt = serde_json::from_str(&text).unwrap();
        assert_eq!(back, r);
        assert_eq!(back.verify_signature().unwrap(), key.key_id());

        let mut bad = r.clone();
        bad.model_rates.insert("a".into(), json!(4));
        assert!(bad.verify_signature().is_err());
        let mut bad = r.clone();
        bad.model_rates.clear();
        assert!(bad.verify_signature().is_err());
        let mut odd = r.clone();
        odd.model_rates.insert("a".into(), json!("fast"));
        assert_eq!(odd.rates_at_issue(), None);
    }
}
