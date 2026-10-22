//! Sign-in flows (`auth.*`) against mock servers on 127.0.0.1: sim's OAuth
//! server (registration, PKCE, loopback redirect, refresh) and GitHub's
//! device flow. The tests play the browser themselves; nothing reaches a
//! real host, and no token ever appears in a command's result.

#![cfg(all(feature = "services", feature = "http", not(target_arch = "wasm32")))]

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine as _;
use pflow_native_core::auth::{self, Service};
use pflow_native_core::Session;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// Tests share the process environment and the credentials file, so they
/// run one at a time, each from an empty data directory.
static SERIAL: Mutex<()> = Mutex::new(());

fn setup() -> std::sync::MutexGuard<'static, ()> {
    let g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let home = std::env::temp_dir().join(format!("pflow-native-auth-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::env::set_var("PFLOW_NATIVE_HOME", &home);
    std::env::set_var("PFLOW_NATIVE_CREDENTIAL_HOSTS", "127.0.0.1");
    for k in [
        "SIM_PFLOW_TOKEN",
        "GITHUB_TOKEN",
        "PFLOW_XYZ_TOKEN",
        "PFLOW_NATIVE_GITHUB_CLIENT_ID",
    ] {
        std::env::remove_var(k);
    }
    g
}

fn creds_file() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("PFLOW_NATIVE_HOME").unwrap()).join("credentials.json")
}

fn creds() -> Value {
    serde_json::from_str(&std::fs::read_to_string(creds_file()).unwrap()).unwrap()
}

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn assert_no_tokens(v: &Value) {
    let t = v.to_string();
    for secret in ["simat_", "simrt_", "gho_", "the-code"] {
        assert!(!t.contains(secret), "token leaked: {t}");
    }
}

// ---------------------------------------------------------------- mock HTTP

#[derive(Debug, Clone)]
struct Req {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Req {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    fn form(&self) -> BTreeMap<String, String> {
        query(&self.body)
    }
}

fn dec(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
            i += 3;
        } else {
            out.push(if b[i] == b'+' { b' ' } else { b[i] });
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
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

/// Serves until the test ends; returns the base URL and the request log.
fn mock(handler: impl Fn(&Req) -> (u16, Value) + Send + 'static) -> (String, Arc<Mutex<Vec<Req>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let log = Arc::new(Mutex::new(vec![]));
    let seen = log.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let mut r = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or("").to_string();
            let path = parts.next().unwrap_or("").to_string();
            let mut headers = vec![];
            let mut len = 0usize;
            loop {
                let mut h = String::new();
                r.read_line(&mut h).unwrap();
                let h = h.trim_end();
                if h.is_empty() {
                    break;
                }
                if let Some((k, v)) = h.split_once(':') {
                    let (k, v) = (k.trim().to_string(), v.trim().to_string());
                    if k.eq_ignore_ascii_case("content-length") {
                        len = v.parse().unwrap();
                    }
                    headers.push((k, v));
                }
            }
            let mut body = vec![0u8; len];
            r.read_exact(&mut body).unwrap();
            let req = Req {
                method,
                path,
                headers,
                body: String::from_utf8(body).unwrap(),
            };
            let (status, body) = handler(&req);
            seen.lock().unwrap().push(req);
            let body = body.to_string();
            let mut out = stream;
            let _ = write!(
                out,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (base, log)
}

/// What a browser does at the loopback redirect: GET it, return the status.
fn browse(url: &str) -> u16 {
    let rest = url.strip_prefix("http://").unwrap();
    let (host, path) = rest.split_once('/').unwrap();
    let mut s = TcpStream::connect(host).unwrap();
    write!(s, "GET /{path} HTTP/1.1\r\nHost: {host}\r\n\r\n").unwrap();
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).unwrap();
    line.split_whitespace().nth(1).unwrap().parse().unwrap()
}

fn wait_for_pending(service: &str) {
    for _ in 0..100 {
        let st = auth::status();
        let row = st
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["service"] == service)
            .unwrap()
            .clone();
        if row["sign_in"]["state"] == "pending" {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("{service} sign-in never went pending");
}

fn row(service: &str) -> Value {
    auth::status()
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["service"] == service)
        .unwrap()
        .clone()
}

// ------------------------------------------------------------ sim.pflow.xyz

/// sim's OAuth endpoints. `challenge` is filled in by the test (from the
/// authorize URL), as the real server would remember it with the code.
fn sim_server(
    challenge: Arc<Mutex<String>>,
    redirect: Arc<Mutex<String>>,
) -> (String, Arc<Mutex<Vec<Req>>>) {
    mock(move |r| match (r.method.as_str(), r.path.as_str()) {
        ("POST", "/oauth/register") => {
            let b: Value = serde_json::from_str(&r.body).unwrap();
            assert_eq!(b["redirect_uris"], json!(["http://127.0.0.1/callback"]));
            (
                201,
                json!({ "client_id": "mcp-test", "token_endpoint_auth_method": "none" }),
            )
        }
        ("POST", "/oauth/token") => {
            assert_eq!(
                r.header("Content-Type"),
                Some("application/x-www-form-urlencoded")
            );
            let f = r.form();
            match f["grant_type"].as_str() {
                "authorization_code" => {
                    let sum = B64.encode(Sha256::digest(f["code_verifier"].as_bytes()));
                    if f["code"] != "the-code"
                        || f["client_id"] != "mcp-test"
                        || sum != *challenge.lock().unwrap()
                        || f["redirect_uri"] != *redirect.lock().unwrap()
                    {
                        return (
                            400,
                            json!({ "error": "invalid_grant", "error_description": "PKCE verification failed" }),
                        );
                    }
                    (
                        200,
                        json!({ "access_token": "simat_A", "refresh_token": "simrt_A", "expires_in": 86400, "scope": "sim.models.read sim.models.write sim.runs sim.publish", "token_type": "Bearer" }),
                    )
                }
                "refresh_token"
                    if f["refresh_token"] == "simrt_old" && f["client_id"] == "mcp-test" =>
                {
                    (
                        200,
                        json!({ "access_token": "simat_new", "refresh_token": "simrt_new", "expires_in": 86400, "scope": "sim.models.read" }),
                    )
                }
                _ => (
                    400,
                    json!({ "error": "invalid_grant", "error_description": "unknown refresh token" }),
                ),
            }
        }
        _ => (404, json!({})),
    })
}

#[test]
fn sim_sign_in_runs_pkce_through_a_loopback_redirect() {
    let _g = setup();
    let challenge = Arc::new(Mutex::new(String::new()));
    let redirect = Arc::new(Mutex::new(String::new()));
    let (base, log) = sim_server(challenge.clone(), redirect.clone());
    let mut s = Session::default();

    let r = run(
        &mut s,
        "auth.login",
        json!({ "service": "sim", "server": base, "browser": false }),
    );
    assert_no_tokens(&r);
    let url = r["open"].as_str().unwrap();
    let (head, q) = url.split_once('?').unwrap();
    assert_eq!(head, format!("{base}/oauth/authorize"));
    let q = query(q);
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["client_id"], "mcp-test");
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["resource"], format!("{base}/mcp"));
    assert_eq!(q["scope"], auth::SIM_SCOPES);
    assert!(
        q["redirect_uri"].starts_with("http://127.0.0.1:")
            && q["redirect_uri"].ends_with("/callback")
    );
    assert!(q["state"].len() >= 20 && q["code_challenge"].len() == 43);
    *challenge.lock().unwrap() = q["code_challenge"].clone();
    *redirect.lock().unwrap() = q["redirect_uri"].clone();
    wait_for_pending("sim");

    // Not our callback: ignored, still waiting.
    let cb = &q["redirect_uri"];
    let port_root = cb.trim_end_matches("/callback");
    assert_eq!(browse(&format!("{port_root}/favicon.ico")), 404);
    assert_eq!(browse(&format!("{cb}?code=the-code&state=forged")), 400);
    assert_eq!(row("sim")["sign_in"]["state"], "pending");
    assert_eq!(row("sim")["signed_in"], false);

    // The real one.
    let state = &q["state"];
    let iss: String = base.replace(':', "%3A").replace('/', "%2F");
    assert_eq!(
        browse(&format!("{cb}?code=the-code&state={state}&iss={iss}")),
        200
    );
    let st = auth::wait(Service::Sim, Duration::from_secs(10)).unwrap();
    assert_eq!(st["signed_in"], true);
    assert_eq!(st["from"], "sign-in");
    assert_eq!(st["refreshable"], true);
    assert!(st["sign_in"].is_null());
    assert_no_tokens(&run(&mut s, "auth.status", Value::Null));
    assert_no_tokens(&run(&mut s, "remote.credentials", Value::Null));

    let c = creds();
    assert_eq!(c["SIM_PFLOW_TOKEN"], "simat_A");
    assert_eq!(c["sim_oauth"]["refresh_token"], "simrt_A");
    assert_eq!(c["sim_oauth"]["client_id"], "mcp-test");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(creds_file())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    // The bridges pick it up, for sim's hosts only.
    assert_eq!(
        auth::sim_token(&format!("{base}/mcp")).unwrap().as_deref(),
        Some("simat_A")
    );
    assert!(auth::sim_token("https://evil.example/mcp").is_err());
    assert_eq!(log.lock().unwrap().len(), 2, "register + token, no refresh");
}

#[test]
fn sim_sign_in_reports_a_refusal() {
    let _g = setup();
    let (base, _) = sim_server(Default::default(), Default::default());
    let mut s = Session::default();
    let r = run(
        &mut s,
        "auth.login",
        json!({ "service": "sim", "server": base, "browser": false }),
    );
    let q = query(r["open"].as_str().unwrap().split_once('?').unwrap().1);
    wait_for_pending("sim");
    let cb = format!(
        "{}?error=access_denied&error_description=no&state={}",
        q["redirect_uri"], q["state"]
    );
    assert_eq!(browse(&cb), 200);
    let e = auth::wait(Service::Sim, Duration::from_secs(10))
        .unwrap_err()
        .to_string();
    assert!(e.contains("access_denied"), "{e}");
    assert_eq!(row("sim")["sign_in"]["state"], "failed");
    assert!(!creds_file().exists());
}

#[test]
fn sim_sign_in_only_to_sims_own_hosts() {
    let _g = setup();
    let mut s = Session::default();
    let e = s
        .execute(
            "auth.login",
            json!({ "service": "sim", "server": "https://evil.example", "browser": false }),
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("refusing"), "{e}");
    let e = s
        .execute("auth.login", json!({ "service": "nope" }))
        .unwrap_err()
        .to_string();
    assert!(e.contains("unknown service"), "{e}");
}

#[test]
fn sim_refreshes_an_expired_token_and_rotates() {
    let _g = setup();
    let (base, log) = sim_server(Default::default(), Default::default());
    std::fs::create_dir_all(creds_file().parent().unwrap()).unwrap();
    let old = json!({
        "SIM_PFLOW_TOKEN": "simat_old",
        "PFLOW_XYZ_TOKEN": "kept-as-is",
        "sim_oauth": { "server": base, "client_id": "mcp-test", "refresh_token": "simrt_old", "expires_at": 1 },
    });
    std::fs::write(creds_file(), old.to_string()).unwrap();
    let endpoint = format!("{base}/mcp");
    assert_eq!(
        auth::sim_token(&endpoint).unwrap().as_deref(),
        Some("simat_new")
    );
    let c = creds();
    assert_eq!(c["sim_oauth"]["refresh_token"], "simrt_new");
    assert_eq!(c["PFLOW_XYZ_TOKEN"], "kept-as-is");
    // Fresh now: no second refresh.
    assert_eq!(
        auth::sim_token(&endpoint).unwrap().as_deref(),
        Some("simat_new")
    );
    assert_eq!(log.lock().unwrap().len(), 1);
    // A refresh token the server no longer knows: sign in again.
    let mut c = creds();
    c["sim_oauth"]["expires_at"] = json!(1);
    std::fs::write(creds_file(), c.to_string()).unwrap();
    let e = auth::sim_token(&endpoint).unwrap_err().to_string();
    assert!(e.contains("sign in again"), "{e}");
    // An environment token is used as is, never refreshed.
    std::env::set_var("SIM_PFLOW_TOKEN", "from-env");
    assert_eq!(
        auth::sim_token(&endpoint).unwrap().as_deref(),
        Some("from-env")
    );
    std::env::remove_var("SIM_PFLOW_TOKEN");
    assert_eq!(log.lock().unwrap().len(), 2);
}

// ---------------------------------------------------------- GitHub/pflow.xyz

#[test]
fn pflow_sign_in_is_a_github_device_flow() {
    let _g = setup();
    let polls = Arc::new(Mutex::new(0));
    let p = polls.clone();
    let (base, log) = mock(move |r| match (r.method.as_str(), r.path.as_str()) {
        ("POST", "/login/device/code") => {
            let f = r.form();
            assert_eq!(f["client_id"], auth::PFLOW_GITHUB_CLIENT_ID);
            assert_eq!(f["scope"], auth::GITHUB_SCOPES);
            (
                200,
                json!({ "device_code": "dev-123", "user_code": "ABCD-1234", "verification_uri": "https://github.com/login/device", "expires_in": 900, "interval": 0 }),
            )
        }
        ("POST", "/login/oauth/access_token") => {
            let f = r.form();
            assert_eq!(f["device_code"], "dev-123");
            assert_eq!(
                f["grant_type"],
                "urn:ietf:params:oauth:grant-type:device_code"
            );
            let mut n = p.lock().unwrap();
            *n += 1;
            if *n < 2 {
                (200, json!({ "error": "authorization_pending" }))
            } else {
                (
                    200,
                    json!({ "access_token": "gho_X", "token_type": "bearer", "scope": "gist,read:user,user:email" }),
                )
            }
        }
        ("GET", "/user") if r.header("Authorization") == Some("Bearer gho_X") => {
            (200, json!({ "login": "octo" }))
        }
        _ => (404, json!({ "message": "Not Found" })),
    });
    std::env::set_var("PFLOW_NATIVE_GITHUB_BASE", &base);
    std::env::set_var("PFLOW_NATIVE_GITHUB_API", &base);
    let mut s = Session::default();

    let r = run(
        &mut s,
        "auth.login",
        json!({ "service": "pflow", "browser": false }),
    );
    assert_eq!(r["user_code"], "ABCD-1234");
    assert_eq!(r["open"], "https://github.com/login/device");
    assert_no_tokens(&r);
    let st = auth::wait(Service::Pflow, Duration::from_secs(10)).unwrap();
    assert_eq!(st["signed_in"], true);
    assert_eq!(st["login"], "octo");
    assert_eq!(st["from"], "sign-in");
    assert_eq!(st["credential"], "GITHUB_TOKEN");
    assert_no_tokens(&run(&mut s, "auth.status", Value::Null));
    assert_eq!(creds()["GITHUB_TOKEN"], "gho_X");
    assert_eq!(*polls.lock().unwrap(), 2);
    assert_eq!(log.lock().unwrap().len(), 4);

    // An environment token shadows it, and status says so.
    std::env::set_var("GITHUB_TOKEN", "from-env");
    assert!(row("pflow")["note"]
        .as_str()
        .unwrap()
        .contains("environment"));
    std::env::remove_var("GITHUB_TOKEN");

    let out = run(&mut s, "auth.logout", json!({ "service": "pflow" }));
    assert_eq!(out["removed"], json!(["GITHUB_TOKEN", "github_oauth"]));
    assert_eq!(row("pflow")["signed_in"], false);
    assert!(creds().get("GITHUB_TOKEN").is_none());
}

#[test]
fn pflow_sign_in_explains_a_disabled_device_flow() {
    let _g = setup();
    let (base, _) = mock(|_| {
        (
            200,
            json!({ "error": "device_flow_disabled", "error_description": "Device Flow must be explicitly enabled for this App" }),
        )
    });
    std::env::set_var("PFLOW_NATIVE_GITHUB_BASE", &base);
    let mut s = Session::default();
    let e = s
        .execute(
            "auth.login",
            json!({ "service": "pflow", "browser": false }),
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("Enable Device Flow"), "{e}");
    assert_eq!(row("pflow")["sign_in"]["state"], "failed");
}

#[test]
fn a_broken_credentials_file_is_never_overwritten() {
    let _g = setup();
    std::fs::create_dir_all(creds_file().parent().unwrap()).unwrap();
    std::fs::write(creds_file(), "{ not json").unwrap();
    let mut s = Session::default();
    let e = s
        .execute("auth.logout", json!({ "service": "sim" }))
        .unwrap_err()
        .to_string();
    assert!(e.contains("not a JSON object"), "{e}");
    assert_eq!(std::fs::read_to_string(creds_file()).unwrap(), "{ not json");
}
