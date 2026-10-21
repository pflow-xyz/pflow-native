//! Display settings and panel layout (`View::settings`). Not journaled:
//! they change how things look, not the net.

use serde_json::{json, Value};

use crate::registry::{always, arg, b, opt, s, ArgType::*, CommandSpec};
use crate::session::{Session, Theme};
use crate::{Error, Result};

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "view.set",
        label: "Display Settings",
        help: "Change display settings: theme (system|dark|light), show_unit_weights, json_pane, inspector, bottom_panel, snap.",
        menu: "",
        shortcut: None,
        args: &[
            opt("theme", Str, "system, dark or light"),
            opt("show_unit_weights", Bool, "show weight badges on weight-1 arcs"),
            opt("json_pane", Bool, "show the JSON-LD source pane"),
            opt("inspector", Bool, "show the inspector"),
            opt("bottom_panel", Bool, "show the plot/run/console panel"),
            opt("snap", Bool, "snap to the 10-unit grid"),
        ],
        enabled: always,
        run: set,
        journal: false,
    },
    CommandSpec {
        id: "view.toggle",
        label: "Toggle Panel",
        help: "Show or hide a panel: json_pane, inspector or bottom_panel.",
        menu: "",
        shortcut: None,
        args: &[arg("panel", Str, "json_pane, inspector or bottom_panel")],
        enabled: always,
        run: toggle,
        journal: false,
    },
    CommandSpec {
        id: "view.toggle_json",
        label: "JSON Editor",
        help: "Show or hide the JSON-LD source pane (live two-way sync).",
        menu: "View",
        shortcut: Some("Cmd+J"),
        args: &[],
        enabled: always,
        run: toggle_json,
        journal: false,
    },
    CommandSpec {
        id: "view.toggle_inspector",
        label: "Inspector",
        help: "Show or hide the inspector.",
        menu: "View",
        shortcut: Some("Cmd+I"),
        args: &[],
        enabled: always,
        run: toggle_inspector,
        journal: false,
    },
    CommandSpec {
        id: "view.toggle_bottom",
        label: "Plot / Run / Console",
        help: "Show or hide the bottom panel.",
        menu: "View",
        shortcut: Some("Cmd+B"),
        args: &[],
        enabled: always,
        run: toggle_bottom,
        journal: false,
    },
    CommandSpec {
        id: "view.cycle_theme",
        label: "Theme: System → Dark → Light",
        help: "Cycle the color theme.",
        menu: "View",
        shortcut: None,
        args: &[],
        enabled: always,
        run: cycle_theme,
        journal: false,
    },
    CommandSpec {
        id: "view.toggle_unit_weights",
        label: "Show Weight-1 Badges",
        help: "Show or hide weight badges on weight-1 arcs.",
        menu: "View",
        shortcut: None,
        args: &[],
        enabled: always,
        run: toggle_unit_weights,
        journal: false,
    },
    CommandSpec {
        id: "view.toggle_snap",
        label: "Snap to Grid",
        help: "Turn 10-unit grid snapping on or off.",
        menu: "View",
        shortcut: None,
        args: &[],
        enabled: always,
        run: toggle_snap,
        journal: false,
    },
];

fn set(sess: &mut Session, a: &Value) -> Result<Value> {
    let st = &mut sess.view.settings;
    if let Some(t) = s(a, "theme") {
        st.theme = match t {
            "system" => Theme::System,
            "dark" => Theme::Dark,
            "light" => Theme::Light,
            _ => {
                return Err(Error::BadArgs(format!(
                    "theme {t:?}: system, dark or light"
                )))
            }
        };
    }
    for (k, slot) in [
        ("show_unit_weights", &mut st.show_unit_weights),
        ("json_pane", &mut st.json_pane),
        ("inspector", &mut st.inspector),
        ("bottom_panel", &mut st.bottom_panel),
        ("snap", &mut st.snap),
    ] {
        if let Some(v) = b(a, k) {
            *slot = v;
        }
    }
    Ok(json!(sess.view.settings))
}

fn flip(sess: &mut Session, panel: &str) -> Result<Value> {
    let st = &mut sess.view.settings;
    let slot = match panel {
        "json_pane" => &mut st.json_pane,
        "inspector" => &mut st.inspector,
        "bottom_panel" => &mut st.bottom_panel,
        "show_unit_weights" => &mut st.show_unit_weights,
        "snap" => &mut st.snap,
        p => return Err(Error::BadArgs(format!("unknown panel {p:?}"))),
    };
    *slot = !*slot;
    Ok(json!(sess.view.settings))
}

fn toggle(sess: &mut Session, a: &Value) -> Result<Value> {
    flip(sess, s(a, "panel").unwrap())
}
fn toggle_json(sess: &mut Session, _: &Value) -> Result<Value> {
    flip(sess, "json_pane")
}
fn toggle_inspector(sess: &mut Session, _: &Value) -> Result<Value> {
    flip(sess, "inspector")
}
fn toggle_bottom(sess: &mut Session, _: &Value) -> Result<Value> {
    flip(sess, "bottom_panel")
}
fn toggle_unit_weights(sess: &mut Session, _: &Value) -> Result<Value> {
    flip(sess, "show_unit_weights")
}
fn toggle_snap(sess: &mut Session, _: &Value) -> Result<Value> {
    flip(sess, "snap")
}

fn cycle_theme(sess: &mut Session, _: &Value) -> Result<Value> {
    let st = &mut sess.view.settings;
    st.theme = match st.theme {
        Theme::System => Theme::Dark,
        Theme::Dark => Theme::Light,
        Theme::Light => Theme::System,
    };
    Ok(json!(st.theme))
}
