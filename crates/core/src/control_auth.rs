//! Authentication for the app's control port (127.0.0.1:7979).
//!
//! The port executes registry commands in the live app, so anything that
//! can reach it could otherwise run `remote.*`, `file.save`, … as the user.
//! Each app launch makes a random token and writes it, with the port's
//! address, to `<data_dir>/control.json`, readable only by the user (0600).
//! Every request must carry it as `"token"`; `pflow-native-cli mcp
//! --attach` reads the file (or `$PFLOW_NATIVE_CONTROL_TOKEN`). Other local
//! users and web pages cannot read the file, so they cannot drive the app.

use std::path::PathBuf;

use serde_json::{json, Value};

/// Environment override for the token (and `..._ADDR` for the address).
pub const TOKEN_ENV: &str = "PFLOW_NATIVE_CONTROL_TOKEN";

/// Where the running app records its control address and token.
pub fn control_file() -> Option<PathBuf> {
    crate::outbound::data_dir().map(|d| d.join("control.json"))
}

/// A fresh 256-bit token, hex encoded.
pub fn new_token() -> Result<String, String> {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b)
        .map_err(|e| format!("no randomness for the control token: {e}"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

/// Writes `{addr, token, pid}` to [`control_file`], owner-only.
pub fn write_control_file(addr: &str, token: &str) -> Result<PathBuf, String> {
    let path = control_file().ok_or("no data directory for the control token")?;
    let body = json!({ "addr": addr, "token": token, "pid": std::process::id() }).to_string();
    crate::outbound::write_private(&path, body.as_bytes())?;
    Ok(path)
}

/// The token for attaching to the app at `addr`: `$PFLOW_NATIVE_CONTROL_TOKEN`,
/// else the control file's token when its address matches.
pub fn read_token(addr: &str) -> Option<String> {
    if let Ok(t) = std::env::var(TOKEN_ENV) {
        if !t.is_empty() {
            return Some(t);
        }
    }
    let v: Value = serde_json::from_str(&std::fs::read_to_string(control_file()?).ok()?).ok()?;
    let recorded = v.get("addr").and_then(Value::as_str)?;
    (recorded == addr || same_port(recorded, addr))
        .then(|| v.get("token").and_then(Value::as_str).map(str::to_string))
        .flatten()
}

fn same_port(a: &str, b: &str) -> bool {
    let port = |s: &str| s.rsplit_once(':').map(|(_, p)| p.to_string());
    let local = |s: &str| {
        s.starts_with("127.0.0.1:") || s.starts_with("localhost:") || s.starts_with("[::1]:")
    };
    local(a) && local(b) && port(a) == port(b)
}

/// Constant-time comparison, so response timing does not leak the token.
pub fn tokens_equal(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_and_compare_exactly() {
        let (a, b) = (new_token().unwrap(), new_token().unwrap());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert!(tokens_equal(&a, &a.clone()));
        assert!(!tokens_equal(&a, &b));
        assert!(!tokens_equal(&a, &a[..63]));
    }
}
