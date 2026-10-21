//! Loading from the network and sharing: pflow.xyz permalinks (`?data=`,
//! `?cid=`), plain JSON URLs, and generating a `?data=` permalink. Fetching
//! needs the `http` feature (native builds); without it these commands
//! explain that instead of failing obscurely.

use serde_json::{json, Value};

use crate::doc::Document;
use crate::registry::{always, arg, opt, s, ArgType::*, CommandSpec};
use crate::session::Session;
use crate::{Error, Result};

pub const DEFAULT_SERVER: &str = "https://pflow.xyz";

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "file.open_url",
        label: "Open URL…",
        help: "Load a net from a URL: a JSON(-LD) document, or a pflow.xyz permalink with ?data= or ?cid=.",
        menu: "",
        shortcut: None,
        args: &[arg("url", Str, "http(s) URL or pflow.xyz permalink")],
        enabled: always,
        run: open_url,
        journal: true,
    },
    CommandSpec {
        id: "file.open_cid",
        label: "Open CID",
        help: "Load a sealed net by CID from a pflow.xyz server (GET {server}/o/{cid}).",
        menu: "",
        shortcut: None,
        args: &[arg("cid", Str, "content id (z4EB…)"), opt("server", Str, "default https://pflow.xyz")],
        enabled: always,
        run: open_cid,
        journal: true,
    },
    CommandSpec {
        id: "file.permalink",
        label: "Copy Permalink",
        help: "A pflow.xyz ?data= permalink that opens this exact document in the web editor.",
        menu: "File",
        shortcut: None,
        args: &[opt("server", Str, "default https://pflow.xyz")],
        enabled: always,
        run: permalink,
        journal: false,
    },
];

#[cfg(feature = "http")]
pub fn fetch(url: &str) -> Result<String> {
    let resp = ureq::get(url)
        .header("Accept", "application/ld+json, application/json")
        .call()
        .map_err(|e| Error::Io(format!("GET {url}: {e}")))?;
    resp.into_body()
        .with_config()
        .limit(16 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| Error::Io(format!("GET {url}: {e}")))
}

#[cfg(not(feature = "http"))]
pub fn fetch(url: &str) -> Result<String> {
    Err(Error::Io(format!(
        "cannot fetch {url}: this build has no HTTP client (feature \"http\")"
    )))
}

fn query_param<'a>(url: &'a str, key: &str) -> Option<&'a str> {
    let q = url.split_once('?')?.1;
    let q = q.split('#').next().unwrap_or(q);
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then_some(v)
    })
}

fn origin(url: &str) -> String {
    let after = url.find("://").map_or(0, |i| i + 3);
    match url[after..].find('/') {
        Some(i) => url[..after + i].to_string(),
        None => url.split('?').next().unwrap_or(url).to_string(),
    }
}

pub fn percent_decode(s: &str) -> Result<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let h = std::str::from_utf8(&b[i + 1..i + 3]).ok();
                let v = h.and_then(|h| u8::from_str_radix(h, 16).ok());
                match v {
                    Some(v) => {
                        out.push(v);
                        i += 3;
                    }
                    None => return Err(Error::Parse(format!("bad percent escape at {i}"))),
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|e| Error::Parse(e.to_string()))
}

pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for c in s.bytes() {
        match c {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(c as char)
            }
            _ => out.push_str(&format!("%{c:02X}")),
        }
    }
    out
}

fn load(sess: &mut Session, doc: Document, source: &str) -> Result<Value> {
    super::file::replace(sess, doc)?;
    sess.path = None;
    Ok(json!({
        "source": source,
        "places": sess.doc.places.len(),
        "transitions": sess.doc.transitions.len(),
        "arcs": sess.doc.arcs.len(),
    }))
}

/// The document a URL points at, or why not.
pub fn resolve(url: &str) -> Result<Document> {
    if let Some(data) = query_param(url, "data") {
        return Document::from_json(percent_decode(data)?.as_bytes());
    }
    if let Some(cid) = query_param(url, "cid") {
        let body = fetch(&format!("{}/o/{}", origin(url), cid))?;
        return Document::from_json(body.as_bytes());
    }
    Document::from_json(fetch(url)?.as_bytes())
}

fn open_url(sess: &mut Session, a: &Value) -> Result<Value> {
    let url = s(a, "url").unwrap();
    let doc = resolve(url)?;
    load(sess, doc, url)
}

fn open_cid(sess: &mut Session, a: &Value) -> Result<Value> {
    let cid = s(a, "cid").unwrap();
    if !cid.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(Error::BadArgs(format!("not a CID: {cid:?}")));
    }
    let server = s(a, "server")
        .unwrap_or(DEFAULT_SERVER)
        .trim_end_matches('/');
    let url = format!("{server}/o/{cid}");
    let doc = Document::from_json(fetch(&url)?.as_bytes())?;
    load(sess, doc, &url)
}

fn permalink(sess: &mut Session, a: &Value) -> Result<Value> {
    let server = s(a, "server")
        .unwrap_or(DEFAULT_SERVER)
        .trim_end_matches('/');
    let body = serde_json::to_string(&sess.doc.to_value()).expect("serializable");
    Ok(json!(format!("{server}/?data={}", percent_encode(&body))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_permalink_round_trip() {
        let mut s = Session::default();
        s.execute("file.open_example", json!({"name": "sir"}))
            .unwrap();
        let link = s.execute("file.permalink", Value::Null).unwrap();
        let doc = resolve(link.as_str().unwrap()).unwrap();
        assert_eq!(doc, s.doc);
    }

    #[test]
    fn origin_and_params() {
        assert_eq!(
            origin("https://pflow.xyz/?cid=z4EB&title=x"),
            "https://pflow.xyz"
        );
        assert_eq!(
            query_param("https://pflow.xyz/?cid=z4EB&title=x", "cid"),
            Some("z4EB")
        );
        assert_eq!(percent_decode("a%20b+c").unwrap(), "a b c");
    }
}
