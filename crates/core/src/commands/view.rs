use serde_json::{json, Value};

use crate::registry::{always, arg, n, opt, req, s, ArgType::*, CommandSpec};
use crate::session::{Session, Tool};
use crate::{Error, Result};

pub const ZOOM_MIN: f64 = 0.1;
pub const ZOOM_MAX: f64 = 8.0;

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "tool.set",
        label: "Set Tool",
        help: "Pick the canvas tool: select, place, transition, arc, token, delete, fire.",
        menu: "",
        shortcut: None,
        args: &[arg("tool", Str, "tool name")],
        enabled: always,
        run: set_tool,
        journal: false,
    },
    CommandSpec {
        id: "view.zoom",
        label: "Zoom",
        help:
            "Multiply the zoom by `factor`, keeping world point (x, y) fixed (default: the center).",
        menu: "",
        shortcut: None,
        args: &[
            arg("factor", Num, "e.g. 1.25"),
            opt("x", Num, "anchor x (world)"),
            opt("y", Num, "anchor y (world)"),
        ],
        enabled: always,
        run: zoom,
        journal: false,
    },
    CommandSpec {
        id: "view.zoom_in",
        label: "Zoom In",
        help: "Zoom in by 25%.",
        menu: "View",
        shortcut: Some("Cmd+="),
        args: &[],
        enabled: always,
        run: zoom_in,
        journal: false,
    },
    CommandSpec {
        id: "view.zoom_out",
        label: "Zoom Out",
        help: "Zoom out by 20%.",
        menu: "View",
        shortcut: Some("Cmd+-"),
        args: &[],
        enabled: always,
        run: zoom_out,
        journal: false,
    },
    CommandSpec {
        id: "view.pan",
        label: "Pan",
        help: "Pan by (dx, dy) screen points.",
        menu: "",
        shortcut: None,
        args: &[arg("dx", Num, "screen dx"), arg("dy", Num, "screen dy")],
        enabled: always,
        run: pan,
        journal: false,
    },
    CommandSpec {
        id: "view.fit",
        label: "Fit to View",
        help: "Zoom and center so the whole net is visible.",
        menu: "View",
        shortcut: Some("F"),
        args: &[],
        enabled: always,
        run: fit,
        journal: false,
    },
    CommandSpec {
        id: "view.center",
        label: "Center on Node",
        help: "Center the view on a node.",
        menu: "",
        shortcut: None,
        args: &[arg("id", Str, "node id")],
        enabled: always,
        run: center,
        journal: false,
    },
    CommandSpec {
        id: "view.resize",
        label: "Resize Viewport",
        help: "Report the canvas size (the app calls this; headless callers may too).",
        menu: "",
        shortcut: None,
        args: &[arg("width", Num, "points"), arg("height", Num, "points")],
        enabled: always,
        run: resize,
        journal: false,
    },
    CommandSpec {
        id: "view.screenshot",
        label: "Screenshot…",
        help: "Save a PNG of the app window (needs the app; headless, use export.svg).",
        menu: "View",
        shortcut: None,
        args: &[arg("path", Str, "PNG file to write")],
        enabled: has_ui,
        run: screenshot,
        journal: false,
    },
    CommandSpec {
        id: "view.show_artifact",
        label: "Show Result",
        help: "Open a stored result in the app's Results tab (shown the next time the window is drawn). Headless, it only checks \
               the id; use export.png to see it.",
        menu: "",
        shortcut: None,
        args: &[arg("id", Str, "artifact id")],
        enabled: always,
        run: show_artifact,
        journal: false,
    },
    CommandSpec {
        id: "view.get",
        label: "View State",
        help: "Viewport, tool and selection.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: get,
        journal: false,
    },
];

fn has_ui(s: &Session) -> std::result::Result<(), String> {
    s.has_ui
        .then_some(())
        .ok_or_else(|| "no app window attached; use export.svg".into())
}

fn screenshot(sess: &mut Session, a: &Value) -> Result<Value> {
    let path = req(a, "path").to_string();
    sess.ui_requests
        .push(crate::session::UiRequest::Screenshot { path: path.clone() });
    Ok(json!({ "requested": path }))
}

fn show_artifact(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "id").to_string();
    let art = sess
        .artifact(&id)
        .ok_or_else(|| Error::NotFound(format!("no artifact {id:?}")))?;
    let title = art.title.clone();
    if sess.has_ui {
        sess.ui_requests
            .push(crate::session::UiRequest::ShowArtifact { id: id.clone() });
    }
    Ok(json!({ "shown": id, "title": title, "ui": sess.has_ui }))
}

fn set_tool(sess: &mut Session, a: &Value) -> Result<Value> {
    let name = req(a, "tool");
    sess.tool = Tool::parse(name).ok_or_else(|| {
        let all: Vec<&str> = Tool::ALL.iter().map(|t| t.name()).collect();
        Error::BadArgs(format!("unknown tool {name:?}; one of {all:?}"))
    })?;
    Ok(json!(sess.tool))
}

fn zoom_by(sess: &mut Session, f: f64, anchor: Option<(f64, f64)>) {
    let v = &mut sess.view;
    let z = (v.zoom * f).clamp(ZOOM_MIN, ZOOM_MAX);
    if let Some((x, y)) = anchor {
        // Keep (x, y) at the same screen position.
        let k = v.zoom / z;
        v.cx = x + (v.cx - x) * k;
        v.cy = y + (v.cy - y) * k;
    }
    v.zoom = z;
}

fn zoom(sess: &mut Session, a: &Value) -> Result<Value> {
    let f = n(a, "factor").unwrap();
    if !(f > 0.0 && f.is_finite()) {
        return Err(Error::BadArgs("factor must be positive".into()));
    }
    let anchor = n(a, "x").zip(n(a, "y"));
    zoom_by(sess, f, anchor);
    Ok(json!(sess.view))
}

fn zoom_in(sess: &mut Session, _: &Value) -> Result<Value> {
    zoom_by(sess, 1.25, None);
    Ok(json!(sess.view))
}

fn zoom_out(sess: &mut Session, _: &Value) -> Result<Value> {
    zoom_by(sess, 0.8, None);
    Ok(json!(sess.view))
}

fn pan(sess: &mut Session, a: &Value) -> Result<Value> {
    let v = &mut sess.view;
    v.cx -= n(a, "dx").unwrap() / v.zoom;
    v.cy -= n(a, "dy").unwrap() / v.zoom;
    Ok(json!(sess.view))
}

pub(crate) fn fit_view(sess: &mut Session) {
    let v = &mut sess.view;
    match sess.doc.bounds() {
        None => {
            v.cx = v.width / 2.0;
            v.cy = v.height / 2.0;
            v.zoom = 1.0;
        }
        Some((x0, y0, x1, y1)) => {
            let pad = 80.0;
            let (w, h) = (x1 - x0 + 2.0 * pad, y1 - y0 + 2.0 * pad);
            v.cx = (x0 + x1) / 2.0;
            v.cy = (y0 + y1) / 2.0;
            v.zoom = (v.width / w).min(v.height / h).clamp(ZOOM_MIN, 2.5);
        }
    }
}

fn fit(sess: &mut Session, _: &Value) -> Result<Value> {
    fit_view(sess);
    Ok(json!(sess.view))
}

fn center(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "id");
    let (x, y) = sess
        .doc
        .position(id)
        .ok_or_else(|| Error::NotFound(format!("no node {id:?}")))?;
    sess.view.cx = x;
    sess.view.cy = y;
    Ok(json!(sess.view))
}

fn resize(sess: &mut Session, a: &Value) -> Result<Value> {
    sess.view.width = n(a, "width").unwrap().max(1.0);
    sess.view.height = n(a, "height").unwrap().max(1.0);
    Ok(json!(sess.view))
}

fn get(sess: &mut Session, _: &Value) -> Result<Value> {
    let _ = s;
    Ok(json!({ "view": sess.view, "tool": sess.tool, "selection": sess.selection }))
}
