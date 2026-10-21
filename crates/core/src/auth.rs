//! Signing in to sim.pflow.xyz and pflow.xyz.
//!
//! - **sim.pflow.xyz** runs its own OAuth 2.1 authorization server. A sign-in
//!   registers pflow-native as a public client (RFC 7591), opens the browser
//!   on `/oauth/authorize` with PKCE (S256) and a loopback redirect
//!   (`http://127.0.0.1:<port>/callback`, RFC 8252), and exchanges the code.
//!   The browser signs in with whatever sim offers (Google today). Access
//!   tokens last a day; [`sim_token`] refreshes them (refresh tokens rotate
//!   and last 90 days).
//! - **pflow.xyz** checks its bearer against `api.github.com/user`, so its
//!   sign-in is a GitHub token. It runs GitHub's device flow (RFC 8628) with
//!   pflow.xyz's OAuth app: the user enters a short code at
//!   github.com/login/device. The token is stored as `GITHUB_TOKEN`, so Gists
//!   use it too.
//!
//! Tokens go to `<data_dir>/credentials.json` (owner-only) under the names
//! [`outbound::credential`] reads, so every bridge and the host allowlist
//! apply unchanged. The sign-in state beside them (refresh token, expiry,
//! client id) is kept under `sim_oauth` / `github_oauth`. No command returns
//! a token, and an environment variable still wins over the file.

// Without an HTTP client (the wasm build) only status and sign-out remain.
#![cfg_attr(
    not(all(feature = "http", not(target_arch = "wasm32"))),
    allow(dead_code, unused_imports)
)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde_json::{json, Map, Value};

use crate::outbound::{self, credential_allowed, credential_for};
use crate::{Error, Result};

pub const SIM_CREDENTIAL: &str = "SIM_PFLOW_TOKEN";
pub const GITHUB_CREDENTIAL: &str = "GITHUB_TOKEN";
pub const SIM_SERVER: &str = "https://sim.pflow.xyz";
/// Every scope sim.pflow.xyz grants; the bridges use all of them.
pub const SIM_SCOPES: &str = "sim.models.read sim.models.write sim.runs sim.publish";
/// What pflow.xyz's own GitHub login asks for.
pub const GITHUB_SCOPES: &str = "read:user user:email gist";
/// pflow.xyz's GitHub OAuth app (public; it is in pflow.xyz's login URL).
/// The device flow needs "Enable Device Flow" ticked in the app's settings.
pub const PFLOW_GITHUB_CLIENT_ID: &str = "Ov23liWIM9hVnb5fnXxc";
/// The redirect URI pflow-native registers with sim. Loopback URIs match
/// with the port ignored (RFC 8252 §7.3), so each sign-in binds a fresh one.
const REGISTERED_REDIRECT: &str = "http://127.0.0.1/callback";
/// How long a sign-in waits for the browser.
const SIGN_IN_SECS: u64 = 600;

/// Environment overrides, for tests against a local mock and for a
/// different GitHub OAuth app.
const GITHUB_BASE_ENV: &str = "PFLOW_NATIVE_GITHUB_BASE";
const GITHUB_API_ENV: &str = "PFLOW_NATIVE_GITHUB_API";
const GITHUB_CLIENT_ENV: &str = "PFLOW_NATIVE_GITHUB_CLIENT_ID";

/// A service pflow-native can sign in to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Service {
    /// sim.pflow.xyz, through its OAuth server (Google sign-in).
    Sim,
    /// pflow.xyz, with a GitHub token (GitHub device flow).
    Pflow,
}

impl Service {
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sim" | "sim.pflow.xyz" | "google" => Ok(Self::Sim),
            "pflow" | "pflow.xyz" | "github" => Ok(Self::Pflow),
            other => Err(Error::BadArgs(format!(
                "unknown service {other:?}: \"sim\" (sim.pflow.xyz) or \"pflow\" (pflow.xyz)"
            ))),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Sim => "sim",
            Self::Pflow => "pflow",
        }
    }
}

// ------------------------------------------------------------ sign-in flows

/// A sign-in in progress or just failed, per service. A finished one is
/// removed: its result is the stored credential.
#[derive(Debug, Clone)]
enum Flow {
    Pending { gen: u64, info: Value },
    Failed(String),
}

static FLOWS: Mutex<BTreeMap<Service, Flow>> = Mutex::new(BTreeMap::new());
/// Each sign-in gets a generation; a newer sign-in or a sign-out bumps it,
/// and a background flow whose generation is stale stops without storing.
static GEN: AtomicU64 = AtomicU64::new(0);

fn flows() -> std::sync::MutexGuard<'static, BTreeMap<Service, Flow>> {
    FLOWS.lock().unwrap_or_else(|e| e.into_inner())
}

fn current(service: Service, gen: u64) -> bool {
    matches!(flows().get(&service), Some(Flow::Pending { gen: g, .. }) if *g == gen)
}

fn finish(service: Service, gen: u64, r: Result<()>) {
    let mut f = flows();
    if matches!(f.get(&service), Some(Flow::Pending { gen: g, .. }) if *g == gen) {
        match r {
            Ok(()) => f.remove(&service),
            Err(e) => f.insert(service, Flow::Failed(e.to_string())),
        };
    }
}

/// Options for [`login`].
#[derive(Debug, Clone)]
pub struct LoginOpts {
    /// sim only: the sim server (default https://sim.pflow.xyz). It must be
    /// one of `SIM_PFLOW_TOKEN`'s hosts.
    pub server: Option<String>,
    pub scope: Option<String>,
    /// Open the system browser (default). Off, the caller opens `open`.
    pub browser: bool,
}

impl Default for LoginOpts {
    fn default() -> Self {
        Self {
            server: None,
            scope: None,
            browser: true,
        }
    }
}

/// Starts signing in to `service` and returns at once with what the user
/// has to do (`open`, and for GitHub a `user_code`). The sign-in finishes
/// in the background; [`status`] shows it and [`wait`] blocks on it.
pub fn login(service: Service, opts: &LoginOpts) -> Result<Value> {
    #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
    {
        let gen = GEN.fetch_add(1, Ordering::SeqCst) + 1;
        flows().insert(
            service,
            Flow::Pending {
                gen,
                info: json!({ "state": "starting" }),
            },
        );
        let r = match service {
            Service::Sim => net::sim_login(opts, gen),
            Service::Pflow => net::github_login(opts, gen),
        };
        match &r {
            Ok(info) => {
                let mut f = flows();
                if let Some(Flow::Pending { gen: g, info: i }) = f.get_mut(&service) {
                    if *g == gen {
                        *i = info.clone();
                    }
                }
            }
            Err(e) => finish(service, gen, Err(Error::Invalid(e.to_string()))),
        }
        let mut info = r?;
        if opts.browser {
            let url = info["open"].as_str().unwrap_or_default().to_string();
            if let Err(e) = open_browser(&url) {
                info["browser"] = json!(format!(
                    "could not open a browser ({e}); open the link yourself"
                ));
            }
        }
        Ok(info)
    }
    #[cfg(not(all(feature = "http", not(target_arch = "wasm32"))))]
    {
        let _ = (service, opts);
        Err(Error::Io(
            "this build cannot sign in: it has no HTTP client (feature \"http\")".into(),
        ))
    }
}

/// Blocks until `service`'s sign-in finishes (or `timeout` passes) and
/// returns its status, or the sign-in's error. Not for the UI thread.
pub fn wait(service: Service, timeout: std::time::Duration) -> Result<Value> {
    let deadline = web_time::Instant::now() + timeout;
    loop {
        // Bound first: a guard in the match scrutinee would live through
        // the arms, and service_status locks again.
        let flow = flows().get(&service).cloned();
        match flow {
            Some(Flow::Failed(e)) => return Err(Error::Invalid(e)),
            Some(Flow::Pending { .. }) if web_time::Instant::now() < deadline => {}
            Some(Flow::Pending { .. }) => {
                return Err(Error::Io(format!(
                    "{} sign-in is still waiting after {}s",
                    service.name(),
                    timeout.as_secs()
                )))
            }
            None => return Ok(service_status(service, &read_creds_lossy())),
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Forgets `service`'s stored sign-in (and stops one in progress). Tokens
/// set in the environment stay; the result says so.
pub fn logout(service: Service) -> Result<Value> {
    GEN.fetch_add(1, Ordering::SeqCst);
    flows().remove(&service);
    let keys: &[&str] = match service {
        Service::Sim => &[SIM_CREDENTIAL, "sim_oauth"],
        Service::Pflow => &[GITHUB_CREDENTIAL, "PFLOW_XYZ_TOKEN", "github_oauth"],
    };
    let mut removed = vec![];
    if creds_path().is_some_and(|p| p.exists()) {
        update_creds(|m| {
            for k in keys {
                if m.remove(*k).is_some() {
                    removed.push(*k);
                }
            }
        })?;
    }
    let still_env: Vec<&str> = keys
        .iter()
        .copied()
        .filter(|k| std::env::var(k).is_ok_and(|v| !v.is_empty()))
        .collect();
    Ok(json!({
        "service": service.name(),
        "removed": removed,
        "still_set_in_environment": still_env,
        "note": match service {
            Service::Sim => "Deleted here. sim.pflow.xyz cannot revoke a token yet; the access token it issued lapses within a day.",
            Service::Pflow => "Deleted here. To revoke pflow.xyz's access to your GitHub account, see https://github.com/settings/applications.",
        },
    }))
}

/// Every service's sign-in status (never a token value).
pub fn status() -> Value {
    let creds = read_creds_lossy();
    json!([
        service_status(Service::Sim, &creds),
        service_status(Service::Pflow, &creds)
    ])
}

/// Whether any sign-in is still waiting on the user.
pub fn pending() -> bool {
    flows().values().any(|f| matches!(f, Flow::Pending { .. }))
}

fn service_status(service: Service, creds: &Map<String, Value>) -> Value {
    let env = |k: &str| std::env::var(k).is_ok_and(|v| !v.is_empty());
    let in_file = |k: &str| creds.get(k).and_then(Value::as_str).is_some();
    let sign_in = match flows().get(&service) {
        Some(Flow::Pending { info, .. }) => {
            let mut i = info.clone();
            i["state"] = json!("pending");
            i
        }
        Some(Flow::Failed(e)) => json!({ "state": "failed", "error": e }),
        None => Value::Null,
    };
    match service {
        Service::Sim => {
            let o = creds.get("sim_oauth").cloned().unwrap_or(Value::Null);
            let expires_in = o["expires_at"].as_u64().map(|t| t as i64 - now() as i64);
            let from = if env(SIM_CREDENTIAL) {
                "env"
            } else if in_file(SIM_CREDENTIAL) && o.is_object() {
                "sign-in"
            } else if in_file(SIM_CREDENTIAL) {
                "credentials.json"
            } else {
                ""
            };
            let refreshable = o["refresh_token"].as_str().is_some();
            json!({
                "service": "sim",
                "site": o["server"].as_str().unwrap_or(SIM_SERVER),
                "signs_in_with": "Google, on sim.pflow.xyz's sign-in page",
                "credential": SIM_CREDENTIAL,
                "signed_in": from == "env" || from == "credentials.json"
                    || (from == "sign-in" && (refreshable || expires_in.is_none_or(|s| s > 0))),
                "from": from,
                "scope": o["scope"],
                "expires_in_s": if from == "sign-in" { json!(expires_in) } else { Value::Null },
                "refreshable": from == "sign-in" && refreshable,
                "sign_in": sign_in,
            })
        }
        Service::Pflow => {
            let o = creds.get("github_oauth").cloned().unwrap_or(Value::Null);
            let (cred, from) = if env("PFLOW_XYZ_TOKEN") {
                ("PFLOW_XYZ_TOKEN", "env")
            } else if in_file("PFLOW_XYZ_TOKEN") {
                ("PFLOW_XYZ_TOKEN", "credentials.json")
            } else if env(GITHUB_CREDENTIAL) {
                (GITHUB_CREDENTIAL, "env")
            } else if in_file(GITHUB_CREDENTIAL) && o.is_object() {
                (GITHUB_CREDENTIAL, "sign-in")
            } else if in_file(GITHUB_CREDENTIAL) {
                (GITHUB_CREDENTIAL, "credentials.json")
            } else {
                (GITHUB_CREDENTIAL, "")
            };
            let mut v = json!({
                "service": "pflow",
                "site": "https://pflow.xyz",
                "signs_in_with": "GitHub",
                "credential": cred,
                "signed_in": !from.is_empty(),
                "from": from,
                "login": if from == "sign-in" { o["login"].clone() } else { Value::Null },
                "scope": if from == "sign-in" { o["scope"].clone() } else { Value::Null },
                "sign_in": sign_in,
            });
            if from == "env" && o.is_object() {
                v["note"] = json!(format!(
                    "{cred} in the environment is used instead of the signed-in token"
                ));
            }
            v
        }
    }
}

/// `SIM_PFLOW_TOKEN` for a request to `endpoint`, refreshed first when it
/// came from a sign-in and has expired (or is about to). Like
/// [`outbound::credential_for`]: `Ok(None)` when there is none, an error
/// when `endpoint` is not one of its hosts.
pub fn sim_token(endpoint: &str) -> Result<Option<String>> {
    #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
    if !std::env::var(SIM_CREDENTIAL).is_ok_and(|v| !v.is_empty()) {
        let o = read_creds_lossy().get("sim_oauth").cloned();
        if let Some(o) = o.filter(|o| o["refresh_token"].is_string()) {
            if o["expires_at"].as_u64().is_none_or(|t| t <= now() + 60) {
                credential_allowed(SIM_CREDENTIAL, endpoint).map_err(Error::BadArgs)?;
                net::sim_refresh()?;
            }
        }
    }
    credential_for(SIM_CREDENTIAL, endpoint)
}

// ------------------------------------------------------- credentials.json

static FILE_LOCK: Mutex<()> = Mutex::new(());

fn creds_path() -> Option<PathBuf> {
    outbound::data_dir().map(|d| d.join("credentials.json"))
}

fn read_creds() -> Result<Map<String, Value>> {
    let Some(path) = creds_path() else {
        return Ok(Map::new());
    };
    match std::fs::read_to_string(&path) {
        Err(_) => Ok(Map::new()),
        Ok(text) if text.trim().is_empty() => Ok(Map::new()),
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(m)) => Ok(m),
            _ => Err(Error::Invalid(format!(
                "{} is not a JSON object; fix it before signing in (it is never overwritten)",
                path.display()
            ))),
        },
    }
}

fn read_creds_lossy() -> Map<String, Value> {
    read_creds().unwrap_or_default()
}

/// Read-modify-write of credentials.json, owner-only. Holds the file lock
/// for the whole update.
fn update_creds(f: impl FnOnce(&mut Map<String, Value>)) -> Result<()> {
    let _g = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    update_creds_locked(f)
}

fn update_creds_locked(f: impl FnOnce(&mut Map<String, Value>)) -> Result<()> {
    let path = creds_path().ok_or_else(|| Error::Io("no data directory".into()))?;
    let mut m = read_creds()?;
    f(&mut m);
    let body = serde_json::to_string_pretty(&Value::Object(m)).expect("serializable");
    outbound::write_private(&path, body.as_bytes()).map_err(Error::Io)
}

fn now() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Opens `url` in the system browser. Only web and loopback URLs.
fn open_browser(url: &str) -> std::result::Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://127.0.0.1")) {
        return Err(format!("not opening {url:?}"));
    }
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(url);
        c
    };
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("rundll32");
        c.args(["url.dll,FileProtocolHandler", url]);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    #[cfg(any(unix, windows))]
    {
        let mut child = cmd
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        std::thread::spawn(move || child.wait());
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    Err("no browser on this platform".into())
}

// -------------------------------------------------------------- the network

#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
mod net {
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};
    use std::time::Duration;

    use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
    use base64::Engine as _;
    use sha2::{Digest, Sha256};

    use super::*;

    fn agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .new_agent()
    }

    fn read_json(
        url: &str,
        resp: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Result<Value> {
        let mut resp = resp.map_err(|e| Error::Io(format!("{url}: {e}")))?;
        let status = resp.status().as_u16();
        let text = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| Error::Io(format!("{url}: {e}")))?;
        serde_json::from_str(&text).map_err(|_| {
            let head: String = text.chars().take(200).collect();
            Error::Io(format!("{url} → HTTP {status}: {head}"))
        })
    }

    /// POSTs a form and returns the JSON answer whatever the status: OAuth
    /// servers report errors as `{error, error_description}`.
    pub(super) fn post_form(url: &str, form: &[(&str, &str)]) -> Result<Value> {
        let body = form
            .iter()
            .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
            .collect::<Vec<_>>()
            .join("&");
        read_json(
            url,
            agent()
                .post(url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .header("Accept", "application/json")
                .send(body.as_bytes()),
        )
    }

    fn post_json(url: &str, body: &Value) -> Result<Value> {
        read_json(
            url,
            agent()
                .post(url)
                .header("Content-Type", "application/json")
                .header("Accept", "application/json")
                .send(body.to_string().as_bytes()),
        )
    }

    fn oauth_error(v: &Value) -> Option<String> {
        let e = v["error"].as_str()?;
        Some(match v["error_description"].as_str() {
            Some(d) => format!("{e}: {d}"),
            None => e.to_string(),
        })
    }

    fn random(n: usize) -> Result<String> {
        let mut b = vec![0u8; n];
        getrandom::getrandom(&mut b).map_err(|e| Error::Io(format!("no randomness: {e}")))?;
        Ok(B64.encode(b))
    }

    /// Percent-encoding for query strings and forms (RFC 3986 unreserved
    /// characters pass through).
    fn enc(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    out.push(b as char)
                }
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        out
    }

    fn dec(s: &str) -> String {
        let b = s.as_bytes();
        let mut out = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'+' => out.push(b' '),
                b'%' if i + 2 < b.len() => {
                    match std::str::from_utf8(&b[i + 1..i + 3])
                        .ok()
                        .and_then(|h| u8::from_str_radix(h, 16).ok())
                    {
                        Some(x) => {
                            out.push(x);
                            i += 2;
                        }
                        None => out.push(b'%'),
                    }
                }
                c => out.push(c),
            }
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    fn query(q: &str) -> BTreeMap<String, String> {
        q.split('&')
            .filter(|kv| !kv.is_empty())
            .map(|kv| {
                let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
                (dec(k), dec(v))
            })
            .collect()
    }

    // ------------------------------------------------------- sim.pflow.xyz

    pub(super) fn sim_login(opts: &LoginOpts, gen: u64) -> Result<Value> {
        let server = opts
            .server
            .as_deref()
            .unwrap_or(SIM_SERVER)
            .trim_end_matches('/')
            .to_string();
        // The token this yields is only ever sent to SIM_PFLOW_TOKEN's
        // hosts, so a sign-in elsewhere would be useless (or a way to plant
        // someone else's account): refuse it up front.
        credential_allowed(SIM_CREDENTIAL, &server).map_err(Error::BadArgs)?;
        let scope = opts.scope.clone().unwrap_or_else(|| SIM_SCOPES.into());
        let reg = post_json(
            &format!("{server}/oauth/register"),
            &json!({ "client_name": "pflow-native", "redirect_uris": [REGISTERED_REDIRECT] }),
        )?;
        if let Some(e) = oauth_error(&reg) {
            return Err(Error::Io(format!(
                "{server} refused to register pflow-native: {e}"
            )));
        }
        let client_id = reg["client_id"]
            .as_str()
            .ok_or_else(|| Error::Io(format!("{server}/oauth/register returned no client_id")))?
            .to_string();
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|e| Error::Io(format!("no loopback port for the sign-in: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| Error::Io(e.to_string()))?
            .port();
        let redirect = format!("http://127.0.0.1:{port}/callback");
        let verifier = random(32)?;
        let challenge = B64.encode(Sha256::digest(verifier.as_bytes()));
        let state = random(16)?;
        let resource = format!("{server}/mcp");
        let url = format!(
            "{server}/oauth/authorize?response_type=code&client_id={}&redirect_uri={}&scope={}\
             &state={}&code_challenge={}&code_challenge_method=S256&resource={}",
            enc(&client_id),
            enc(&redirect),
            enc(&scope),
            enc(&state),
            enc(&challenge),
            enc(&resource),
        );
        let flow = SimFlow {
            server: server.clone(),
            client_id,
            redirect,
            verifier,
            state,
            resource,
            gen,
        };
        std::thread::Builder::new()
            .name("pflow-native-sign-in".into())
            .spawn(move || {
                let r = flow.finish(listener);
                finish(Service::Sim, flow.gen, r);
            })
            .map_err(|e| Error::Io(e.to_string()))?;
        Ok(json!({
            "service": "sim",
            "open": url,
            "expires_in": SIGN_IN_SECS,
            "note": format!("Sign in on {server} in the browser (it uses Google) and allow pflow-native."),
        }))
    }

    struct SimFlow {
        server: String,
        client_id: String,
        redirect: String,
        verifier: String,
        state: String,
        resource: String,
        gen: u64,
    }

    impl SimFlow {
        fn finish(&self, listener: TcpListener) -> Result<()> {
            let code = self.callback(listener)?;
            let tok = post_form(
                &format!("{}/oauth/token", self.server),
                &[
                    ("grant_type", "authorization_code"),
                    ("code", &code),
                    ("redirect_uri", &self.redirect),
                    ("client_id", &self.client_id),
                    ("code_verifier", &self.verifier),
                    ("resource", &self.resource),
                ],
            )?;
            if let Some(e) = oauth_error(&tok) {
                return Err(Error::Io(format!(
                    "{} refused the sign-in: {e}",
                    self.server
                )));
            }
            if !current(Service::Sim, self.gen) {
                return Ok(());
            }
            store_sim(&self.server, &self.client_id, &tok, None)
        }

        /// Waits for the browser to come back to the loopback redirect with
        /// this sign-in's `state`, and returns the code.
        fn callback(&self, listener: TcpListener) -> Result<String> {
            listener
                .set_nonblocking(true)
                .map_err(|e| Error::Io(e.to_string()))?;
            let deadline = web_time::Instant::now() + Duration::from_secs(SIGN_IN_SECS);
            loop {
                if !current(Service::Sim, self.gen) {
                    return Err(Error::Invalid("sign-in cancelled".into()));
                }
                if web_time::Instant::now() > deadline {
                    return Err(Error::Invalid(format!(
                        "no answer from the browser within {} minutes",
                        SIGN_IN_SECS / 60
                    )));
                }
                let stream = match listener.accept() {
                    Ok((s, _)) => s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(100));
                        continue;
                    }
                    Err(e) => return Err(Error::Io(e.to_string())),
                };
                if let Some(r) = self.answer(stream) {
                    return r;
                }
            }
        }

        /// One request at the loopback port: `None` to keep waiting (not our
        /// callback), else the code or the error the server sent back.
        fn answer(&self, mut stream: TcpStream) -> Option<Result<String>> {
            let _ = stream.set_nonblocking(false);
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut line = String::new();
            BufReader::new(stream.try_clone().ok()?)
                .read_line(&mut line)
                .ok()?;
            let target = line.split_whitespace().nth(1).unwrap_or("");
            let (path, q) = target.split_once('?').unwrap_or((target, ""));
            if !line.starts_with("GET ") || path != "/callback" {
                page(&mut stream, 404, "Not here.");
                return None;
            }
            let q = query(q);
            if q.get("state") != Some(&self.state) {
                page(
                    &mut stream,
                    400,
                    "This is not the sign-in pflow-native started. Start it again from pflow-native.",
                );
                return None;
            }
            if let Some(iss) = q.get("iss") {
                // RFC 9207: the answer must come from the server we asked.
                if iss.trim_end_matches('/') != self.server {
                    page(
                        &mut stream,
                        400,
                        "The sign-in came back from an unexpected server.",
                    );
                    return Some(Err(Error::Invalid(format!(
                        "the sign-in answer names issuer {iss}, not {}",
                        self.server
                    ))));
                }
            }
            if let Some(e) = q.get("error") {
                let d = q.get("error_description").cloned().unwrap_or_default();
                page(
                    &mut stream,
                    200,
                    "Sign-in was not completed. You can close this tab.",
                );
                return Some(Err(Error::Invalid(format!(
                    "{} did not sign you in: {e} {d}",
                    self.server
                ))));
            }
            match q.get("code") {
                Some(code) if !code.is_empty() => {
                    page(
                        &mut stream,
                        200,
                        "Signed in to sim.pflow.xyz. You can close this tab and return to pflow-native.",
                    );
                    Some(Ok(code.clone()))
                }
                _ => {
                    page(&mut stream, 400, "The sign-in answer carried no code.");
                    Some(Err(Error::Invalid(
                        "the sign-in answer carried no code".into(),
                    )))
                }
            }
        }
    }

    fn page(stream: &mut TcpStream, status: u16, msg: &str) {
        let body = format!(
            "<!doctype html><meta charset=utf-8><title>pflow-native</title>\
             <body style=\"font:16px system-ui;margin:3em\"><p>{msg}</p></body>"
        );
        let reason = match status {
            200 => "OK",
            400 => "Bad Request",
            _ => "Not Found",
        };
        let _ = write!(
            stream,
            "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\n\
             Content-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.flush();
    }

    /// Stores a token answer from sim's token endpoint. With `held` the
    /// caller already holds the file lock.
    fn store_sim(
        server: &str,
        client_id: &str,
        tok: &Value,
        held: Option<&std::sync::MutexGuard<'_, ()>>,
    ) -> Result<()> {
        let access = tok["access_token"]
            .as_str()
            .ok_or_else(|| Error::Io(format!("{server} returned no access token")))?;
        let expires_at = now() + tok["expires_in"].as_u64().unwrap_or(86_400);
        let write = |m: &mut Map<String, Value>| {
            m.insert(SIM_CREDENTIAL.into(), json!(access));
            m.insert(
                "sim_oauth".into(),
                json!({
                    "server": server,
                    "client_id": client_id,
                    "refresh_token": tok["refresh_token"],
                    "expires_at": expires_at,
                    "scope": tok["scope"],
                }),
            );
        };
        match held {
            Some(_) => update_creds_locked(write),
            None => update_creds(write),
        }
    }

    /// Swaps the stored refresh token for a new pair. Refresh tokens rotate
    /// (each is single use), so this holds the file lock throughout, and a
    /// token another process refreshed first is simply re-read.
    pub(super) fn sim_refresh() -> Result<()> {
        let g = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let creds = read_creds()?;
        let Some(o) = creds.get("sim_oauth") else {
            return Ok(());
        };
        if o["expires_at"].as_u64().is_some_and(|t| t > now() + 60) {
            return Ok(()); // someone else refreshed it
        }
        let (Some(server), Some(client_id), Some(refresh)) = (
            o["server"].as_str(),
            o["client_id"].as_str(),
            o["refresh_token"].as_str(),
        ) else {
            return Ok(());
        };
        credential_allowed(SIM_CREDENTIAL, server).map_err(Error::BadArgs)?;
        let tok = post_form(
            &format!("{server}/oauth/token"),
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh),
                ("client_id", client_id),
                ("resource", &format!("{server}/mcp")),
            ],
        )?;
        if let Some(e) = oauth_error(&tok) {
            return Err(Error::Invalid(format!(
                "the sim.pflow.xyz sign-in has expired ({e}); sign in again: auth.login {{\"service\":\"sim\"}}"
            )));
        }
        store_sim(server, client_id, &tok, Some(&g))
    }

    // -------------------------------------------------- GitHub (pflow.xyz)

    fn github_base() -> String {
        std::env::var(GITHUB_BASE_ENV)
            .unwrap_or_else(|_| "https://github.com".into())
            .trim_end_matches('/')
            .to_string()
    }

    fn github_api() -> String {
        std::env::var(GITHUB_API_ENV)
            .unwrap_or_else(|_| "https://api.github.com".into())
            .trim_end_matches('/')
            .to_string()
    }

    pub(super) fn github_login(opts: &LoginOpts, gen: u64) -> Result<Value> {
        if opts.server.is_some() {
            return Err(Error::BadArgs(
                "pflow.xyz sign-in is a GitHub sign-in; it takes no server".into(),
            ));
        }
        let client_id = std::env::var(GITHUB_CLIENT_ENV)
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| PFLOW_GITHUB_CLIENT_ID.into());
        let scope = opts.scope.clone().unwrap_or_else(|| GITHUB_SCOPES.into());
        let base = github_base();
        let d = post_form(
            &format!("{base}/login/device/code"),
            &[("client_id", &client_id), ("scope", &scope)],
        )?;
        match d["error"].as_str() {
            Some("device_flow_disabled") => {
                return Err(Error::Invalid(format!(
                    "GitHub refused the device flow for OAuth app {client_id}: \"Enable Device Flow\" is off. \
                     Its owner can turn it on at https://github.com/settings/developers (the pflow.xyz app), \
                     or set {GITHUB_CLIENT_ENV} to an OAuth app that has it."
                )))
            }
            Some(_) => {
                return Err(Error::Io(format!(
                    "GitHub refused the sign-in: {}",
                    oauth_error(&d).unwrap_or_default()
                )))
            }
            None => {}
        }
        let field = |k: &str| {
            d[k].as_str()
                .map(str::to_string)
                .ok_or_else(|| Error::Io(format!("GitHub's device code answer has no {k}")))
        };
        let device_code = field("device_code")?;
        let user_code = field("user_code")?;
        let verify = field("verification_uri")?;
        let expires_in = d["expires_in"].as_u64().unwrap_or(900);
        let interval = d["interval"].as_u64().unwrap_or(5);
        let poll = DevicePoll {
            token_url: format!("{base}/login/oauth/access_token"),
            client_id,
            device_code,
            interval,
            deadline: web_time::Instant::now() + Duration::from_secs(expires_in),
            gen,
        };
        std::thread::Builder::new()
            .name("pflow-native-sign-in".into())
            .spawn(move || {
                let r = poll.run();
                finish(Service::Pflow, poll.gen, r);
            })
            .map_err(|e| Error::Io(e.to_string()))?;
        Ok(json!({
            "service": "pflow",
            "open": verify,
            "user_code": user_code,
            "expires_in": expires_in,
            "note": format!("Enter the code {user_code} at {verify} and authorize pflow.xyz."),
        }))
    }

    struct DevicePoll {
        token_url: String,
        client_id: String,
        device_code: String,
        interval: u64,
        deadline: web_time::Instant,
        gen: u64,
    }

    impl DevicePoll {
        fn run(&self) -> Result<()> {
            let mut interval = self.interval;
            let tok = loop {
                std::thread::sleep(Duration::from_millis((interval * 1000).max(200)));
                if !current(Service::Pflow, self.gen) {
                    return Ok(());
                }
                if web_time::Instant::now() > self.deadline {
                    return Err(Error::Invalid(
                        "the GitHub code expired; sign in again".into(),
                    ));
                }
                let r = post_form(
                    &self.token_url,
                    &[
                        ("client_id", &self.client_id),
                        ("device_code", &self.device_code),
                        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ],
                )?;
                match r["error"].as_str() {
                    None => break r,
                    Some("authorization_pending") => {}
                    Some("slow_down") => interval = r["interval"].as_u64().unwrap_or(interval + 5),
                    Some("access_denied") => {
                        return Err(Error::Invalid("the GitHub sign-in was declined".into()))
                    }
                    Some("expired_token") => {
                        return Err(Error::Invalid(
                            "the GitHub code expired; sign in again".into(),
                        ))
                    }
                    Some(_) => {
                        return Err(Error::Io(format!(
                            "GitHub refused the sign-in: {}",
                            oauth_error(&r).unwrap_or_default()
                        )))
                    }
                }
            };
            let token = tok["access_token"]
                .as_str()
                .ok_or_else(|| Error::Io("GitHub returned no access token".into()))?;
            let user_url = format!("{}/user", github_api());
            credential_allowed(GITHUB_CREDENTIAL, &user_url).map_err(Error::BadArgs)?;
            let user = read_json(
                &user_url,
                agent()
                    .get(&user_url)
                    .header("Authorization", &format!("Bearer {token}"))
                    .header("Accept", "application/vnd.github+json")
                    .header("User-Agent", "pflow-native")
                    .call(),
            )?;
            let login = user["login"].as_str().ok_or_else(|| {
                Error::Io("GitHub did not accept the new token (no login from /user)".into())
            })?;
            if !current(Service::Pflow, self.gen) {
                return Ok(());
            }
            update_creds(|m| {
                m.insert(GITHUB_CREDENTIAL.into(), json!(token));
                m.insert(
                    "github_oauth".into(),
                    json!({
                        "client_id": self.client_id,
                        "login": login,
                        "scope": tok["scope"],
                        "obtained_at": now(),
                    }),
                );
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn percent_coding_round_trips() {
            let s = "a b&c=d/é~_-.";
            assert_eq!(dec(&enc(s)), s);
            assert_eq!(
                enc("http://127.0.0.1:5/cb"),
                "http%3A%2F%2F127.0.0.1%3A5%2Fcb"
            );
            let q = query("code=x%2By&state=a+b&iss=https%3A%2F%2Fsim");
            assert_eq!(q["code"], "x+y");
            assert_eq!(q["state"], "a b");
            assert_eq!(q["iss"], "https://sim");
            assert_eq!(dec("100%"), "100%");
        }
    }
}
