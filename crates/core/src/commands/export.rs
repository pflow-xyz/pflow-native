use serde_json::{json, Value};

use crate::export;
use crate::registry::{always, arg, opt, s, ArgType::*, CommandSpec};
use crate::session::Session;
use crate::{Error, Result};

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "export.svg",
        label: "Export SVG…",
        help: "The net (with current marking) as SVG; written to `path` if given, else returned.",
        menu: "File",
        shortcut: None,
        args: &[opt("path", Str, "file to write")],
        enabled: always,
        run: svg,
        journal: false,
    },
    CommandSpec {
        id: "export.artifact",
        label: "Export Result…",
        help: "An artifact (default: the newest) as json, csv or svg; written to `path` if given, else returned.",
        menu: "",
        shortcut: None,
        args: &[
            arg("format", Str, "json, csv or svg (see artifact.list for what each supports)"),
            opt("id", Str, "artifact id; default the newest"),
            opt("path", Str, "file to write"),
        ],
        enabled: always,
        run: artifact,
        journal: false,
    },
    CommandSpec {
        id: "artifact.list",
        label: "List Results",
        help: "Stored artifacts: id, title, kind, source command, export formats.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: list,
        journal: false,
    },
    CommandSpec {
        id: "artifact.get",
        label: "Get Result",
        help: "One artifact in full.",
        menu: "",
        shortcut: None,
        args: &[arg("id", Str, "artifact id")],
        enabled: always,
        run: get,
        journal: false,
    },
];

fn write_or_return(path: Option<&str>, body: String) -> Result<Value> {
    match path {
        Some(p) => {
            std::fs::write(p, &body).map_err(|e| Error::Io(format!("{p}: {e}")))?;
            Ok(json!({ "path": p, "bytes": body.len() }))
        }
        None => Ok(Value::String(body)),
    }
}

fn svg(sess: &mut Session, a: &Value) -> Result<Value> {
    let enabled = sess.enabled_transitions();
    let body = export::net_svg(&sess.doc, &sess.marking, &enabled);
    write_or_return(s(a, "path"), body)
}

fn artifact(sess: &mut Session, a: &Value) -> Result<Value> {
    let art = match s(a, "id") {
        Some(id) => sess
            .artifact(id)
            .ok_or_else(|| Error::NotFound(format!("no artifact {id:?}")))?,
        None => sess
            .artifacts
            .last()
            .ok_or_else(|| Error::NotFound("no artifacts yet (run e.g. sim.ode)".into()))?,
    };
    let body = export::artifact(art, s(a, "format").unwrap())?;
    write_or_return(s(a, "path"), body)
}

fn list(sess: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!(sess
        .artifacts
        .iter()
        .map(|a| json!({
            "id": a.id, "title": a.title, "kind": a.data.kind(),
            "source": a.source, "formats": a.data.formats(),
        }))
        .collect::<Vec<_>>()))
}

fn get(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = s(a, "id").unwrap();
    let art = sess
        .artifact(id)
        .ok_or_else(|| Error::NotFound(format!("no artifact {id:?}")))?;
    Ok(json!(art))
}
