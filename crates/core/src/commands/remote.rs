//! Remote bridges: pflow.xyz (server save, My Diagrams, delete, share
//! links, GitHub Gist) and sim.pflow.xyz's MCP tool surface.
//!
//! Safety: every write builds an [`Outbound`] and, unless called with
//! `"send": true`, returns its preview (credentials redacted) instead of
//! sending. sim.pflow.xyz tool calls are all treated as writes, since many
//! store, publish or spend compute. The read-only exceptions are
//! `remote.pflow.share` (pure), `remote.pflow.list` (a GET of the signed-in
//! user's own list) and `remote.sim.tools` (`tools/list`, sent without a
//! token). Credentials come only from [`crate::outbound::credential`].
//!
//! Data policy (the user's): only datasets *computed on the server* may be
//! stored there. pflow-native may ask sim.pflow.xyz to compute and store
//! results (sim_dataset, sim_scenario, sim_calibrate, sim_publish run
//! server-side) and may fetch them, but never uploads locally computed
//! datasets or results. So no command here sends a local artifact (event
//! log, time series, heatmap, table…) as data, and `remote.sim.call`
//! refuses the tools in [`REFUSED_TOOLS`], whose purpose is storing
//! caller-supplied data. Models are not datasets: `remote.sim.create_model`
//! and `remote.pflow.save` send the net. Tools that take a log only as the
//! *input* of a server computation (sim_conformance, sim_conformance_stream,
//! sim_calibrate) stay allowed: the server parses the log, computes, and
//! persists at most the derived model, never the log (sim-pflow-xyz
//! origin/main `pkg/simmcp/parity_tools.go:50`, `stream_tool.go:23`,
//! `simmcp.go:1195`/`:1213`).
//!
//! pflow.xyz's API (pflow-xyz `cmd/webserver/main.go`):
//! - `POST /api/save` takes the JSON-LD document and answers `{"cid"}`; with
//!   `Authorization: Bearer <GitHub token>` the object records its author.
//! - `GET /api/diagrams` (auth) answers `{"diagrams": [{cid, name,
//!   description, createdAt, authorId, authorName}]}`.
//! - `DELETE /o/{cid}` (auth, author only) answers 204.
//! - The bearer is a GitHub OAuth access token, verified against
//!   `api.github.com/user` on every request.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::artifact::ArtifactData;
use crate::doc::Document;
use crate::outbound::{credential_for, send_or_preview, Outbound};
use crate::registry::{always, arg, b, opt, s, ArgSpec, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::simclient::{self, Client, Transport};
use crate::{Error, Result};

use super::fileio::DEFAULT_SERVER;

pub const GIST_ENDPOINT: &str = "https://api.github.com/gists";
/// The description pflow.xyz gives its Gists.
pub const GIST_DESCRIPTION: &str = "pflow Petri net diagram";

/// Tools `remote.sim.call` refuses under the data policy (module docs),
/// with the reason.
pub const REFUSED_TOOLS: &[(&str, &str)] = &[(
    "sim_map_put",
    "it stores a caller-supplied key->value table as its own content-addressed entity \
     (sim-pflow-xyz pkg/simmcp/simmcp.go:2280, Store.PutMap at :2288)",
)];

/// Refuses a tool whose purpose is storing caller-supplied data.
pub fn check_data_policy(tool: &str) -> Result<()> {
    match REFUSED_TOOLS.iter().find(|(t, _)| *t == tool) {
        Some((t, why)) => Err(Error::BadArgs(format!(
            "remote.sim.call refuses {t}: {why}. Policy: only datasets computed on sim.pflow.xyz may be \
             stored there; pflow-native never uploads locally computed datasets or results."
        ))),
        None => Ok(()),
    }
}

const SERVER: ArgSpec = opt(
    "server",
    Str,
    "pflow.xyz base URL (default https://pflow.xyz)",
);
const SEND: ArgSpec = opt(
    "send",
    Bool,
    "actually send; without it the request is only previewed",
);
const ENDPOINT: ArgSpec = opt(
    "endpoint",
    Str,
    "MCP endpoint (default https://sim.pflow.xyz/mcp)",
);
const OPEN: ArgSpec = opt(
    "open",
    Bool,
    "open a model the response contains (or names) as the current net; needs send",
);

pub const SET: &[CommandSpec] = &[
    // ---- pflow.xyz ----
    CommandSpec {
        id: "remote.pflow.save",
        label: "Save to pflow.xyz",
        help: "POST the document to pflow.xyz's /api/save, as the web editor's Save does. Signed with \
               PFLOW_XYZ_TOKEN (a GitHub token; GITHUB_TOKEN as fallback) so it lands in My Diagrams. \
               On success the returned CID is recorded as the document's @id (undoable). Previews unless send.",
        menu: "Share",
        shortcut: None,
        args: &[SERVER, SEND],
        enabled: always,
        run: pflow_save,
        journal: true,
    },
    CommandSpec {
        id: "remote.pflow.list",
        label: "My Diagrams (pflow.xyz)",
        help: "List the signed-in user's diagrams on pflow.xyz (GET /api/diagrams; needs PFLOW_XYZ_TOKEN). \
               It sends your token, so it previews unless send; stores a table artifact.",
        menu: "Share",
        shortcut: None,
        args: &[SERVER, SEND],
        enabled: always,
        run: pflow_list,
        journal: false,
    },
    CommandSpec {
        id: "remote.pflow.delete",
        label: "Delete from pflow.xyz…",
        help: "DELETE /o/{cid} on pflow.xyz (author only; needs PFLOW_XYZ_TOKEN). Previews unless send.",
        menu: "Share",
        shortcut: None,
        args: &[arg("cid", Str, "content id of the stored diagram"), SERVER, SEND],
        enabled: always,
        run: pflow_delete,
        journal: true,
    },
    CommandSpec {
        id: "remote.pflow.share",
        label: "Share Links",
        help: "The links pflow.xyz's Share dialog gives for a CID: the page (?cid=), the SVG image \
               (/img/{cid}.svg), the markdown badge, and the JSON-LD (/o/{cid}). Without cid, the open \
               document's CID (the links work once it is saved there). Pure: nothing is sent.",
        menu: "Share",
        shortcut: None,
        args: &[opt("cid", Str, "content id (default: the open document's)"), SERVER],
        enabled: always,
        run: pflow_share,
        journal: false,
    },
    CommandSpec {
        id: "remote.gist",
        label: "Save as GitHub Gist",
        help: "Create a GitHub Gist as pflow.xyz does ({cid}.md holding the markdown badge), plus the \
               sealed JSON-LD as {cid}.jsonld. Uses GITHUB_TOKEN (PFLOW_XYZ_TOKEN as fallback; it needs \
               the gist scope). Previews unless send.",
        menu: "Share",
        shortcut: None,
        args: &[
            opt("public", Bool, "public Gist (default true, as pflow.xyz)"),
            opt("description", Str, "Gist description (default \"pflow Petri net diagram\")"),
            opt("jsonld", Bool, "include {cid}.jsonld (default true)"),
            SERVER,
            opt("endpoint", Str, "Gist API URL (default https://api.github.com/gists)"),
            SEND,
        ],
        enabled: always,
        run: gist,
        journal: true,
    },
    // ---- sim.pflow.xyz ----
    CommandSpec {
        id: "remote.sim.tools",
        label: "sim.pflow.xyz Tools",
        help: "List sim.pflow.xyz's MCP tools (initialize + tools/list; read-only, sent without a token). \
               Stores a table artifact.",
        menu: "Remote",
        shortcut: None,
        args: &[ENDPOINT],
        enabled: always,
        run: sim_tools,
        journal: false,
    },
    CommandSpec {
        id: "remote.sim.call",
        label: "Call sim Tool…",
        help: "Call any sim.pflow.xyz MCP tool with JSON arguments (SIM_PFLOW_TOKEN as bearer when set). \
               Every call is treated as outbound: previews unless send. Refuses tools that store \
               caller-supplied data (sim_map_put): only server-computed datasets may be stored there.",
        menu: "Remote",
        shortcut: None,
        args: &[
            arg("tool", Str, "tool name, e.g. sim_code_to_flow"),
            opt("args", Json, "the tool's arguments object"),
            ENDPOINT,
            SEND,
            OPEN,
        ],
        enabled: always,
        run: sim_call,
        journal: true,
    },
    CommandSpec {
        id: "remote.sim.create_model",
        label: "Store Model on sim",
        help: "sim_create_model with the open net (as a go-pflow metamodel JSON, session rates included). \
               Needs SIM_PFLOW_TOKEN. Previews unless send.",
        menu: "Remote",
        shortcut: None,
        args: &[ENDPOINT, SEND],
        enabled: always,
        run: sim_create_model,
        journal: true,
    },
    CommandSpec {
        id: "remote.sim.publish",
        label: "Publish to Google Sheets…",
        help: "sim_publish: a stored model into the signed-in user's Google Sheets. id omitted or \
               \"current\" stores the open net first (sim_create_model) in the same session. Previews unless send.",
        menu: "Remote",
        shortcut: None,
        args: &[
            opt("id", Str, "sim model id (default: store the open net)"),
            opt("scenario", Json, "scenario JSON for the data tabs"),
            ENDPOINT,
            SEND,
        ],
        enabled: always,
        run: sim_publish,
        journal: true,
    },
    CommandSpec {
        id: "remote.sim.publish_app",
        label: "Publish App…",
        help: "sim_publish_app: host a single-file HTML app for a model you own, at /app/{id}. id omitted \
               or \"current\" stores the open net first. Previews unless send.",
        menu: "Remote",
        shortcut: None,
        args: &[
            arg("html", Str, "the complete self-contained HTML document"),
            opt("id", Str, "sim model id (default: store the open net)"),
            ENDPOINT,
            SEND,
        ],
        enabled: always,
        run: sim_publish_app,
        journal: true,
    },
    CommandSpec {
        id: "remote.sim.prompt",
        label: "Derive with LLM (sim_prompt)…",
        help: "sim_prompt: ask sim's LLM to derive a model, report or code from a stored entity. parent \
               omitted or \"current\" stores the open net first. With open, a model reply becomes the open net. \
               Previews unless send.",
        menu: "Remote",
        shortcut: None,
        args: &[
            arg("text", Str, "the instruction"),
            opt("parent", Str, "id of a model, prompt, artifact or map (default: store the open net)"),
            opt("system", Str, "optional system instructions"),
            ENDPOINT,
            SEND,
            OPEN,
        ],
        enabled: always,
        run: sim_prompt,
        journal: true,
    },
    CommandSpec {
        id: "remote.sim.prove",
        label: "Prove Relation (sim_prove)…",
        help: "sim_prove: isomorphicTo / subnetOf / preservesInvariant / refines between two stored models, \
               recorded with its witness. \"current\" for subject or object stores the open net first. \
               Previews unless send.",
        menu: "Remote",
        shortcut: None,
        args: &[
            arg("subject", Str, "model id the edge starts from (or \"current\")"),
            arg("predicate", Str, "isomorphicTo, subnetOf, preservesInvariant or refines"),
            arg("object", Str, "model id the edge points to (or \"current\")"),
            opt("record", Bool, "store the witness and edge when proved (sim default true)"),
            ENDPOINT,
            SEND,
        ],
        enabled: always,
        run: sim_prove,
        journal: true,
    },
    CommandSpec {
        id: "remote.sim.check_witness",
        label: "Check Witness (sim)…",
        help: "sim_check_witness: re-check a stored witness, an edge's witnesses, a sim_query derivation, \
               a sim_reach answer or a refutation with sim's independent checker. Previews unless send.",
        menu: "Remote",
        shortcut: None,
        args: &[
            opt("id", Str, "a witness id or a structural edge's relation id"),
            opt("derivation", Json, "a sim_query derivation"),
            opt("reach", Json, "a sim_reach answer"),
            opt("reachResult", Str, "content id of a stored sim_reach answer"),
            opt("refutation", Json, "a refuted sim_prove answer's refutation"),
            ENDPOINT,
            SEND,
        ],
        enabled: always,
        run: sim_check_witness,
        journal: true,
    },
];

fn source(id: &str, a: &Value) -> Step {
    Step {
        command: id.into(),
        args: a.clone(),
    }
}

fn sending(a: &Value) -> bool {
    b(a, "send").unwrap_or(false)
}

// ---------------------------------------------------------------- pflow.xyz

fn server(a: &Value) -> String {
    s(a, "server")
        .unwrap_or(DEFAULT_SERVER)
        .trim_end_matches('/')
        .to_string()
}

/// The first configured credential of `names` that may go to `url`, with
/// its name. A configured credential whose hosts do not include `url`'s is
/// refused outright (`outbound::credential_for`): a caller-supplied
/// `server` / `endpoint` can never receive a token.
fn first_credential(names: &[&'static str], url: &str) -> Result<Option<(&'static str, String)>> {
    for n in names {
        if let Some(v) = credential_for(n, url)? {
            return Ok(Some((*n, v)));
        }
    }
    Ok(None)
}

/// pflow.xyz verifies its bearer against GitHub, so a GitHub token works.
fn pflow_token(url: &str) -> Result<Option<(&'static str, String)>> {
    first_credential(&["PFLOW_XYZ_TOKEN", "GITHUB_TOKEN"], url)
}

fn gist_token(url: &str) -> Result<Option<(&'static str, String)>> {
    first_credential(&["GITHUB_TOKEN", "PFLOW_XYZ_TOKEN"], url)
}

fn bearer(req: Outbound, tok: &Option<(&'static str, String)>) -> Outbound {
    match tok {
        Some((_, t)) => req.secret("Authorization", &format!("Bearer {t}")),
        None => req,
    }
}

fn check_cid(cid: &str) -> Result<()> {
    if cid.is_empty() || !cid.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(Error::BadArgs(format!("not a CID: {cid:?}")));
    }
    Ok(())
}

/// The links pflow.xyz's Share dialog shows (petri-view.js `_showShareDialog`).
pub fn share_links(server: &str, cid: &str) -> Value {
    let server = server.trim_end_matches('/');
    let svg = format!("{server}/img/{cid}.svg");
    let page = format!("{server}/?cid={cid}");
    json!({
        "cid": cid,
        "page": page,
        "svg": svg,
        "markdown": format!("[![pflow]({svg})]({page})"),
        "jsonld": format!("{server}/o/{cid}"),
    })
}

fn pflow_save(sess: &mut Session, a: &Value) -> Result<Value> {
    let server = server(a);
    let doc = sess.doc.to_value();
    let expected = crate::cid::seal(&doc)?;
    let url = format!("{server}/api/save");
    let tok = pflow_token(&url)?;
    let req = bearer(Outbound::new("POST", url).json(doc), &tok);
    let mut out = send_or_preview(&req, sending(a), "Saving to pflow.xyz is outbound.")?;
    out["credential"] = json!(tok.as_ref().map(|t| t.0));
    if tok.is_none() {
        out["warning"] = json!(
            "no PFLOW_XYZ_TOKEN or GITHUB_TOKEN: the diagram is stored without an author, so it is \
             not in My Diagrams and cannot be deleted"
        );
    }
    if out["sent"] != true {
        out["expected_cid"] = json!(expected);
        out["links"] = share_links(&server, &expected);
        return Ok(out);
    }
    let cid = out["response"]["cid"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| {
            Error::Io(format!(
                "pflow.xyz answered without a cid: {}",
                out["response"]
            ))
        })?;
    check_cid(&cid)?;
    let recorded = sess.doc.extra.get("@id").and_then(Value::as_str) != Some(cid.as_str());
    if recorded {
        sess.edit("record pflow.xyz CID", |s| {
            s.doc.extra.insert("@id".into(), json!(cid));
            Ok(())
        })?;
    }
    Ok(json!({
        "sent": true,
        "cid": cid,
        "expected_cid": expected,
        "matches": cid == expected,
        "recorded": recorded,
        "links": share_links(&server, &cid),
        "credential": out["credential"],
        "warning": out.get("warning"),
    }))
}

fn pflow_list(sess: &mut Session, a: &Value) -> Result<Value> {
    let server = server(a);
    let url = format!("{server}/api/diagrams");
    let tok = pflow_token(&url)?.ok_or_else(|| {
        Error::BadArgs(
            "My Diagrams needs PFLOW_XYZ_TOKEN (or GITHUB_TOKEN): see remote.credentials".into(),
        )
    })?;
    let name = tok.0;
    let req = bearer(
        Outbound::new("GET", url).header("Accept", "application/json"),
        &Some(tok),
    );
    // It sends a credential, so it is a credentialed request like any write:
    // previewed (and confirmed in the app) unless `send` is set.
    if !sending(a) {
        let mut out = req.preview("Listing My Diagrams sends your token to pflow.xyz.");
        out["credential"] = json!(name);
        return Ok(out);
    }
    let resp = req.send()?;
    let diagrams = resp
        .get("diagrams")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let columns: Vec<String> = ["cid", "name", "description", "createdAt", "authorName"]
        .map(String::from)
        .to_vec();
    let rows: Vec<Vec<Value>> = diagrams
        .iter()
        .map(|d| {
            columns
                .iter()
                .map(|c| d.get(c).cloned().unwrap_or(Value::Null))
                .collect()
        })
        .collect();
    let id = sess.add_artifact(
        "My Diagrams (pflow.xyz)".into(),
        source("remote.pflow.list", a),
        ArtifactData::Table { columns, rows },
    );
    Ok(json!({ "server": server, "count": diagrams.len(), "diagrams": diagrams, "artifact": id }))
}

fn pflow_delete(_: &mut Session, a: &Value) -> Result<Value> {
    let server = server(a);
    let cid = s(a, "cid").unwrap_or_default();
    check_cid(cid)?;
    let url = format!("{server}/o/{cid}");
    let tok = pflow_token(&url)?;
    if tok.is_none() && sending(a) {
        return Err(Error::BadArgs(
            "deleting from pflow.xyz needs PFLOW_XYZ_TOKEN (or GITHUB_TOKEN)".into(),
        ));
    }
    let req = bearer(Outbound::new("DELETE", url), &tok);
    let mut out = send_or_preview(
        &req,
        sending(a),
        "Deleting from pflow.xyz is outbound and cannot be undone.",
    )?;
    out["cid"] = json!(cid);
    out["credential"] = json!(tok.as_ref().map(|t| t.0));
    Ok(out)
}

fn pflow_share(sess: &mut Session, a: &Value) -> Result<Value> {
    let server = server(a);
    let (cid, of_open) = match s(a, "cid") {
        Some(c) => (c.to_string(), false),
        None => (crate::cid::seal(&sess.doc.to_value())?, true),
    };
    check_cid(&cid)?;
    let mut out = share_links(&server, &cid);
    if of_open {
        out["note"] = json!(
            "the open document's CID; the links resolve once it is saved there (remote.pflow.save)"
        );
    }
    Ok(out)
}

/// The Gist request body: pflow.xyz's `{cid}.md` badge, plus the sealed
/// JSON-LD when `jsonld`.
pub fn gist_body(
    server: &str,
    cid: &str,
    doc: Option<&Value>,
    description: &str,
    public: bool,
) -> Value {
    let links = share_links(server, cid);
    let mut files = Map::new();
    files.insert(format!("{cid}.md"), json!({ "content": links["markdown"] }));
    if let Some(doc) = doc {
        let mut sealed = doc.clone();
        sealed["@id"] = json!(cid);
        files.insert(
            format!("{cid}.jsonld"),
            json!({ "content": serde_json::to_string_pretty(&sealed).expect("serializable") }),
        );
    }
    json!({ "description": description, "public": public, "files": files })
}

fn gist(sess: &mut Session, a: &Value) -> Result<Value> {
    let server = server(a);
    let endpoint = s(a, "endpoint").unwrap_or(GIST_ENDPOINT).to_string();
    let doc = sess.doc.to_value();
    let cid = crate::cid::seal(&doc)?;
    let body = gist_body(
        &server,
        &cid,
        b(a, "jsonld").unwrap_or(true).then_some(&doc),
        s(a, "description").unwrap_or(GIST_DESCRIPTION),
        b(a, "public").unwrap_or(true),
    );
    let tok = gist_token(&endpoint)?;
    if tok.is_none() && sending(a) {
        return Err(Error::BadArgs(
            "creating a Gist needs GITHUB_TOKEN (or PFLOW_XYZ_TOKEN) with the gist scope".into(),
        ));
    }
    let req = bearer(
        Outbound::new("POST", endpoint)
            .header("Accept", "application/vnd.github.v3+json")
            .json(body),
        &tok,
    );
    let mut out = send_or_preview(&req, sending(a), "Creating a GitHub Gist is outbound.")?;
    out["cid"] = json!(cid);
    out["links"] = share_links(&server, &cid);
    out["credential"] = json!(tok.as_ref().map(|t| t.0));
    if out["sent"] == true {
        out["url"] = out["response"]["html_url"].clone();
    }
    Ok(out)
}

// ---------------------------------------------------------------- sim.pflow.xyz

fn endpoint(a: &Value) -> String {
    s(a, "endpoint")
        .unwrap_or(simclient::DEFAULT_ENDPOINT)
        .to_string()
}

/// The open net as the model sim_create_model takes: the go-pflow metamodel
/// JSON the token game compiles (colors unfolded), named after the
/// document, with the session's rates on its transitions.
pub fn model_for_sim(sess: &Session) -> Result<Value> {
    let compiled = crate::sim::Compiled::new(&sess.doc)?;
    let mut v = serde_json::to_value(&compiled.model).map_err(|e| Error::Invalid(e.to_string()))?;
    if v["name"].as_str().is_none_or(str::is_empty) {
        let name = sess
            .doc
            .extra
            .get("name")
            .and_then(Value::as_str)
            .filter(|n| !n.is_empty())
            .unwrap_or("pflow-native model");
        v["name"] = json!(name);
    }
    if let Some(ts) = v.get_mut("transitions").and_then(Value::as_array_mut) {
        for t in ts {
            let rate = t
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| sess.rates.get(id))
                .copied();
            if let Some(r) = rate {
                t["rate"] = json!(r);
            }
        }
    }
    Ok(v)
}

/// A sim/go-pflow model (places as an array) or a pflow.xyz document
/// (places keyed by id) as a document, with the transitions' rates.
pub fn document_from_model(v: &Value) -> Result<(Document, BTreeMap<String, f64>)> {
    if v.get("places").is_some_and(Value::is_object) {
        return Ok((Document::from_value(v)?, BTreeMap::new()));
    }
    let arr = |k: &str| {
        v.get(k)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let id_of = |e: &Value| {
        e.get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let num = |e: &Value, k: &str| e.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let mut places = Map::new();
    for p in arr("places") {
        let mut o = json!({ "initial": [num(&p, "initial").max(0.0)], "x": num(&p, "x"), "y": num(&p, "y") });
        if num(&p, "capacity") > 0.0 {
            o["capacity"] = json!([num(&p, "capacity")]);
        }
        places.insert(id_of(&p), o);
    }
    let mut transitions = Map::new();
    let mut rates = BTreeMap::new();
    for t in arr("transitions") {
        let id = id_of(&t);
        if num(&t, "rate") > 0.0 {
            rates.insert(id.clone(), num(&t, "rate"));
        }
        transitions.insert(id, json!({ "x": num(&t, "x"), "y": num(&t, "y") }));
    }
    let mut arcs = vec![];
    for e in arr("arcs") {
        let from = e.get("from").and_then(Value::as_str).unwrap_or("");
        let to = e.get("to").and_then(Value::as_str).unwrap_or("");
        let w = num(&e, "weight").max(1.0);
        let arc = match e.get("type").and_then(Value::as_str).unwrap_or("") {
            "inhibitor" => {
                json!({ "source": from, "target": to, "weight": [w], "inhibitTransition": true })
            }
            // A read arc (place → transition) is the editor's output-side
            // inhibit arc, as in compose::from_model.
            "read" => {
                json!({ "source": to, "target": from, "weight": [w], "inhibitTransition": true })
            }
            _ => json!({ "source": from, "target": to, "weight": [w] }),
        };
        arcs.push(arc);
    }
    let mut d = json!({
        "@context": "https://pflow.xyz/schema",
        "@type": "PetriNet",
        "places": places,
        "transitions": transitions,
        "arcs": arcs,
    });
    if let Some(n) = v.get("name").and_then(Value::as_str) {
        d["name"] = json!(n);
    }
    let mut doc = Document::from_value(&d)?;
    let unplaced = doc.places.iter().all(|p| p.x == 0.0 && p.y == 0.0)
        && doc.transitions.iter().all(|t| t.x == 0.0 && t.y == 0.0);
    if unplaced {
        for (id, x, y) in crate::layout::layout(&doc, crate::layout::Algorithm::Sugiyama) {
            if let Some(p) = doc.place_mut(&id) {
                p.x = x;
                p.y = y;
            } else if let Some(t) = doc.transition_mut(&id) {
                t.x = x;
                t.y = y;
            }
        }
    }
    Ok((doc, rates))
}

/// The first object (depth-first) shaped like a model.
fn find_model(v: &Value, depth: usize) -> Option<&Value> {
    if depth > 6 {
        return None;
    }
    match v {
        Value::Object(o) => {
            if o.contains_key("places") && o.contains_key("transitions") {
                return Some(v);
            }
            o.values().find_map(|x| find_model(x, depth + 1))
        }
        Value::Array(a) => a.iter().find_map(|x| find_model(x, depth + 1)),
        _ => None,
    }
}

/// Placeholder for "the id sim_create_model returns".
const CURRENT: &str = "current";
const PLACEHOLDER: &str = "<id returned by sim_create_model>";

/// One tool call of a plan; `model_args` name arguments that take the id of
/// the open net once stored.
struct Plan {
    tool: String,
    args: Map<String, Value>,
    model_args: Vec<String>,
}

impl Plan {
    fn new(tool: &str) -> Self {
        Plan {
            tool: tool.into(),
            args: Map::new(),
            model_args: vec![],
        }
    }

    /// Copies `keys` from the command args when present.
    fn take(mut self, a: &Value, keys: &[&str]) -> Self {
        for k in keys {
            if let Some(v) = a.get(*k).filter(|v| !v.is_null()) {
                self.args.insert((*k).into(), v.clone());
            }
        }
        self
    }

    /// `key` takes a model id; absent or "current" stores the open net first.
    fn model_arg(mut self, a: &Value, key: &str) -> Self {
        match s(a, key).filter(|v| *v != CURRENT && !v.is_empty()) {
            Some(id) => {
                self.args.insert(key.into(), json!(id));
            }
            None => {
                self.args.insert(key.into(), json!(PLACEHOLDER));
                self.model_args.push(key.into());
            }
        }
        self
    }
}

fn sim_token(endpoint: &str) -> Result<Option<String>> {
    credential_for("SIM_PFLOW_TOKEN", endpoint)
}

/// The steps a plan runs: sim_create_model first when it needs the open net.
fn steps(sess: &Session, plan: &Plan) -> Result<Vec<(String, Value)>> {
    let mut out = vec![];
    if !plan.model_args.is_empty() {
        out.push(("sim_create_model".to_string(), create_args(sess)?));
    }
    out.push((plan.tool.clone(), Value::Object(plan.args.clone())));
    Ok(out)
}

fn create_args(sess: &Session) -> Result<Value> {
    let model = model_for_sim(sess)?;
    Ok(json!({ "model": serde_json::to_string(&model).expect("serializable") }))
}

/// The HTTP request one tools/call is, for previews.
fn call_request(endpoint: &str, id: u64, tool: &str, args: &Value) -> Result<Outbound> {
    let mut req = Outbound::new("POST", endpoint)
        .header("Accept", simclient::ACCEPT)
        .json(simclient::tool_call(id, tool, args.clone()));
    if let Some(t) = sim_token(endpoint)? {
        req = req.secret("Authorization", &format!("Bearer {t}"));
    }
    Ok(req)
}

/// Previews the plan, or runs it in one MCP session.
fn run_plan(sess: &mut Session, a: &Value, cmd: &str, plan: Plan) -> Result<Value> {
    let endpoint = endpoint(a);
    let steps = steps(sess, &plan)?;
    let open = b(a, "open").unwrap_or(false);
    if !sending(a) {
        let first = &steps[0];
        let req = call_request(&endpoint, 2, &first.0, &first.1)?;
        let mut out = req.preview(&format!(
            "Calling sim.pflow.xyz's {} is outbound (it may store, publish or spend compute).",
            plan.tool
        ));
        out["steps"] = json!(steps
            .iter()
            .map(|(t, args)| json!({ "tool": t, "arguments": args }))
            .collect::<Vec<_>>());
        out["session"] = json!(
            "each send runs initialize + notifications/initialized first, then the steps in one MCP session"
        );
        out["credential"] = json!(sim_token(&endpoint)?.map(|_| "SIM_PFLOW_TOKEN"));
        if open {
            out["open"] = json!("open applies only with send");
        }
        return Ok(out);
    }
    let client = simclient::connect(&endpoint, sim_token(&endpoint)?)?;
    run_steps(sess, a, cmd, client, plan, steps, open)
}

fn run_steps<T: Transport>(
    sess: &mut Session,
    a: &Value,
    cmd: &str,
    mut client: Client<T>,
    plan: Plan,
    steps: Vec<(String, Value)>,
    open: bool,
) -> Result<Value> {
    client.initialize()?;
    let mut done = vec![];
    let mut model_id: Option<String> = None;
    let mut last = Value::Null;
    for (tool, mut args) in steps {
        if tool == plan.tool {
            if let Some(id) = &model_id {
                for k in &plan.model_args {
                    args[k.as_str()] = json!(id);
                }
            }
        }
        let result = client.call_tool(&tool, args.clone())?;
        if tool == "sim_create_model" && tool != plan.tool {
            model_id = Some(
                result
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        Error::Io(format!("sim_create_model answered without an id: {result}"))
                    })?
                    .to_string(),
            );
        }
        done.push(json!({ "tool": tool, "arguments": args, "result": result }));
        last = result;
    }
    let mut out = json!({
        "sent": true,
        "endpoint": client.endpoint,
        "server": client.server_info,
        "tool": plan.tool,
        "result": last,
        "steps": done,
    });
    if let Some(id) = &model_id {
        out["stored_model"] = json!(id);
    }
    if open {
        out["opened"] = open_from(sess, &mut client, &last)?;
    }
    let art = sess.add_artifact(
        format!("sim: {}", plan.tool),
        source(cmd, a),
        ArtifactData::Report(out.clone()),
    );
    out["artifact"] = json!(art);
    Ok(out)
}

/// Opens the model in `result`, or the one it names (`kind: "model"`,
/// fetched with sim_get_model in the same session).
fn open_from<T: Transport>(
    sess: &mut Session,
    client: &mut Client<T>,
    result: &Value,
) -> Result<Value> {
    let fetched;
    let model = match find_model(result, 0) {
        Some(m) => m,
        None => {
            let id = result
                .get("id")
                .and_then(Value::as_str)
                .filter(|_| {
                    result
                        .get("kind")
                        .and_then(Value::as_str)
                        .is_none_or(|k| k == "model")
                })
                .ok_or_else(|| Error::NotFound("the response holds no model to open".into()))?;
            fetched = client.call_tool("sim_get_model", json!({ "id": id }))?;
            find_model(&fetched, 0)
                .ok_or_else(|| Error::NotFound(format!("sim_get_model {id} returned no model")))?
        }
    };
    let (doc, rates) = document_from_model(model)?;
    super::file::replace(sess, doc)?;
    sess.path = None;
    for (t, r) in &rates {
        if sess.doc.transition(t).is_some() {
            sess.rates.insert(t.clone(), *r);
        }
    }
    Ok(json!({
        "places": sess.doc.places.len(),
        "transitions": sess.doc.transitions.len(),
        "arcs": sess.doc.arcs.len(),
        "rates": rates.len(),
    }))
}

fn sim_tools(sess: &mut Session, a: &Value) -> Result<Value> {
    let endpoint = endpoint(a);
    // Discovery is public: no token is sent.
    let mut client = simclient::connect(&endpoint, None)?;
    list_tools(sess, a, &mut client)
}

fn list_tools<T: Transport>(
    sess: &mut Session,
    a: &Value,
    client: &mut Client<T>,
) -> Result<Value> {
    client.initialize()?;
    let tools = client.list_tools()?;
    let rows = tools
        .iter()
        .map(|t| {
            let desc = t.get("description").and_then(Value::as_str).unwrap_or("");
            let first = desc.split(". ").next().unwrap_or(desc);
            let required = t
                .pointer("/inputSchema/required")
                .cloned()
                .unwrap_or(json!([]));
            vec![t["name"].clone(), json!(first), required]
        })
        .collect();
    let art = sess.add_artifact(
        "sim.pflow.xyz tools".into(),
        source("remote.sim.tools", a),
        ArtifactData::Table {
            columns: ["tool", "summary", "required"].map(String::from).to_vec(),
            rows,
        },
    );
    Ok(json!({
        "endpoint": client.endpoint,
        "server": client.server_info,
        "count": tools.len(),
        "tools": tools,
        "artifact": art,
    }))
}

fn sim_call(sess: &mut Session, a: &Value) -> Result<Value> {
    let tool = s(a, "tool").unwrap_or_default();
    if tool.is_empty() || !tool.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(Error::BadArgs(format!("not a tool name: {tool:?}")));
    }
    check_data_policy(tool)?;
    let mut plan = Plan::new(tool);
    match a.get("args") {
        None | Some(Value::Null) => {}
        Some(Value::Object(m)) => plan.args = m.clone(),
        Some(other) => {
            return Err(Error::BadArgs(format!(
                "remote.sim.call: args must be an object, got {other}"
            )))
        }
    }
    run_plan(sess, a, "remote.sim.call", plan)
}

fn sim_create_model(sess: &mut Session, a: &Value) -> Result<Value> {
    let mut plan = Plan::new("sim_create_model");
    if let Value::Object(m) = create_args(sess)? {
        plan.args = m;
    }
    run_plan(sess, a, "remote.sim.create_model", plan)
}

fn sim_publish(sess: &mut Session, a: &Value) -> Result<Value> {
    let plan = Plan::new("sim_publish")
        .model_arg(a, "id")
        .take(a, &["scenario"]);
    run_plan(sess, a, "remote.sim.publish", plan)
}

fn sim_publish_app(sess: &mut Session, a: &Value) -> Result<Value> {
    let plan = Plan::new("sim_publish_app")
        .model_arg(a, "id")
        .take(a, &["html"]);
    run_plan(sess, a, "remote.sim.publish_app", plan)
}

fn sim_prompt(sess: &mut Session, a: &Value) -> Result<Value> {
    let plan = Plan::new("sim_prompt")
        .model_arg(a, "parent")
        .take(a, &["text", "system"]);
    run_plan(sess, a, "remote.sim.prompt", plan)
}

fn sim_prove(sess: &mut Session, a: &Value) -> Result<Value> {
    let mut plan = Plan::new("sim_prove");
    for k in ["subject", "object"] {
        if s(a, k) == Some(CURRENT) {
            plan = plan.model_arg(a, k);
        } else {
            plan = plan.take(a, &[k]);
        }
    }
    let plan = plan.take(a, &["predicate", "record"]);
    run_plan(sess, a, "remote.sim.prove", plan)
}

fn sim_check_witness(sess: &mut Session, a: &Value) -> Result<Value> {
    const KEYS: [&str; 5] = ["id", "derivation", "reach", "reachResult", "refutation"];
    if !KEYS.iter().any(|k| a.get(*k).is_some_and(|v| !v.is_null())) {
        return Err(Error::BadArgs(format!(
            "remote.sim.check_witness: give one of {KEYS:?}"
        )));
    }
    let plan = Plan::new("sim_check_witness").take(a, &KEYS);
    run_plan(sess, a, "remote.sim.check_witness", plan)
}
