//! Windows that collect arguments: the generic command dialog (a form built
//! from a command's `ArgSpec`s, so any command can be run from a menu or the
//! palette), the command palette (Cmd+K), and the shortcuts help.

use eframe::egui;
use pflow_native_core::{ArgType, CommandSpec};
use serde_json::{json, Map, Value};

use crate::{shortcuts, App};

#[derive(Default)]
pub struct DialogState {
    /// Command whose args are being collected, and one text buffer per arg.
    args_for: Option<(&'static str, Vec<String>)>,
    error: Option<String>,
    pub palette_open: bool,
    palette_query: String,
    pub help_open: bool,
    /// A previewed remote write awaiting the user's go-ahead:
    /// (command, args it ran with, the request preview).
    pub confirm_send: Option<(String, Value, Value)>,
    /// The Accounts window, and its last `auth.status` (with when it was read).
    pub accounts_open: bool,
    accounts: Option<(f64, Value)>,
}

impl DialogState {
    /// Opens the Accounts window with a fresh status.
    pub(crate) fn show_accounts(&mut self) {
        self.accounts_open = true;
        self.accounts = None;
    }
}

impl App {
    /// Opens the argument form for `id`, prefilled from `prefill`.
    pub(crate) fn ask_args(&mut self, id: &'static str, prefill: &Value) {
        let Some(spec) = self.sess.registry().find(id) else {
            return;
        };
        let fields = spec
            .args
            .iter()
            .map(|a| match prefill.get(a.name) {
                Some(Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => String::new(),
            })
            .collect();
        self.dialogs.args_for = Some((id, fields));
        self.dialogs.error = None;
    }
}

fn parse_field(ty: ArgType, s: &str) -> Result<Option<Value>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    let v = match ty {
        ArgType::Str => json!(s),
        ArgType::Num => json!(s
            .parse::<f64>()
            .map_err(|_| format!("{s:?} is not a number"))?),
        ArgType::Int => json!(s
            .parse::<i64>()
            .map_err(|_| format!("{s:?} is not an integer"))?),
        ArgType::Bool => json!(matches!(s, "true" | "yes" | "1" | "on")),
        ArgType::StrList => {
            if s.starts_with('[') {
                serde_json::from_str(s).map_err(|e| e.to_string())?
            } else {
                json!(s
                    .split(',')
                    .map(|p| p.trim())
                    .filter(|p| !p.is_empty())
                    .collect::<Vec<_>>())
            }
        }
        ArgType::NumList => {
            if s.starts_with('[') {
                serde_json::from_str(s).map_err(|e| e.to_string())?
            } else {
                let v: Result<Vec<f64>, _> =
                    s.split(',').map(|p| p.trim().parse::<f64>()).collect();
                json!(v.map_err(|_| format!("{s:?} is not a number list"))?)
            }
        }
        ArgType::Json => serde_json::from_str(s).map_err(|e| e.to_string())?,
    };
    Ok(Some(v))
}

fn build_args(spec: &CommandSpec, fields: &[String]) -> Result<Value, String> {
    let mut m = Map::new();
    for (a, f) in spec.args.iter().zip(fields) {
        if let Some(v) = parse_field(a.ty, f).map_err(|e| format!("{}: {e}", a.name))? {
            m.insert(a.name.into(), v);
        }
    }
    Ok(Value::Object(m))
}

pub fn ui(app: &mut App, ctx: &egui::Context) {
    confirm_send(app, ctx);
    accounts(app, ctx);
    args_dialog(app, ctx);
    palette(app, ctx);
    help(app, ctx);
}

fn args_dialog(app: &mut App, ctx: &egui::Context) {
    let Some((id, mut fields)) = app.dialogs.args_for.take() else {
        return;
    };
    let spec = *app.sess.registry().find(id).expect("known command");
    let mut open = true;
    let mut submit = false;
    egui::Window::new(spec.label)
        .id(egui::Id::unique(("args-dialog", id)))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, [0.0, 80.0])
        .show(ctx, |ui| {
            ui.label(spec.help);
            ui.add_space(4.0);
            egui::Grid::new("args").num_columns(2).show(ui, |ui| {
                for (i, (a, f)) in spec.args.iter().zip(fields.iter_mut()).enumerate() {
                    let name = if a.required {
                        format!("{} *", a.name)
                    } else {
                        a.name.to_string()
                    };
                    ui.label(name).on_hover_text(a.help);
                    let r = ui.add(
                        egui::TextEdit::singleline(f)
                            .hint_text(a.help)
                            .desired_width(340.0),
                    );
                    if i == 0 && ui.memory(|m| m.focused().is_none()) {
                        r.request_focus();
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        submit = true;
                    }
                    ui.end_row();
                }
            });
            if let Some(e) = &app.dialogs.error {
                ui.colored_label(egui::Color32::from_rgb(220, 60, 60), e);
            }
            ui.horizontal(|ui| {
                if ui.button("Run").clicked() {
                    submit = true;
                }
                ui.weak(spec.id);
            });
        });
    if submit {
        match build_args(&spec, &fields) {
            Ok(args) => match spec.check_args(args) {
                Ok(args) => {
                    app.queue(id, args);
                    return;
                }
                Err(e) => app.dialogs.error = Some(e.to_string()),
            },
            Err(e) => app.dialogs.error = Some(e),
        }
    }
    if open && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.dialogs.args_for = Some((id, fields));
    }
}

fn palette(app: &mut App, ctx: &egui::Context) {
    if !app.dialogs.palette_open {
        return;
    }
    let reg = app.sess.registry();
    let q = app.dialogs.palette_query.to_lowercase();
    let mut chosen = None;
    let mut open = true;
    egui::Window::new("Command Palette")
        .open(&mut open)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
        .default_width(520.0)
        .show(ctx, |ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut app.dialogs.palette_query)
                    .hint_text("type to filter commands…")
                    .desired_width(f32::INFINITY),
            );
            r.request_focus();
            let matches: Vec<&CommandSpec> = reg
                .commands()
                .iter()
                .filter(|c| {
                    q.is_empty()
                        || c.id.contains(&q)
                        || c.label.to_lowercase().contains(&q)
                        || c.help.to_lowercase().contains(&q)
                })
                .collect();
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                chosen = matches.first().map(|c| c.id);
            }
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    for c in matches {
                        let enabled = (c.enabled)(&app.sess);
                        let text = format!("{}   {}", c.label, c.id);
                        let mut b = egui::Button::new(text).frame(false);
                        if let Some(sc) = c.shortcut {
                            b = b.shortcut_text(shortcuts::display(sc));
                        }
                        let r = ui.add_enabled(enabled.is_ok(), b);
                        let r = match enabled {
                            Ok(()) => r.on_hover_text(c.help),
                            Err(why) => r.on_disabled_hover_text(why),
                        };
                        if r.clicked() {
                            chosen = Some(c.id);
                        }
                    }
                });
        });
    if let Some(id) = chosen {
        app.dialogs.palette_open = false;
        app.dialogs.palette_query.clear();
        app.invoke(id);
    } else if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.dialogs.palette_open = false;
    }
}

fn help(app: &mut App, ctx: &egui::Context) {
    let mut open = app.dialogs.help_open;
    egui::Window::new("Keyboard & Mouse")
        .open(&mut open)
        .default_width(460.0)
        .show(ctx, |ui| {
            egui::Grid::new("help").striped(true).show(ui, |ui| {
                for (k, v) in [
                    (
                        "1 – 6",
                        "select · place · transition · arc · token · delete",
                    ),
                    ("8 / X", "play mode: click a green transition to fire"),
                    ("double-click transition", "fire (select tool)"),
                    ("right-click target (arc tool)", "inhibitor / read arc"),
                    (
                        "token tool click / right-click",
                        "+1 / −1 token (palette color)",
                    ),
                    ("shift-click / shift-drag", "multi-select / box select"),
                    ("drag empty · middle-drag · space+drag · wheel", "pan"),
                    ("pinch · ⌘+wheel", "zoom"),
                    ("Esc", "cancel arc / clear selection"),
                    ("⌘K", "command palette (every command)"),
                ] {
                    ui.strong(k);
                    ui.label(v);
                    ui.end_row();
                }
                for c in app.sess.registry().commands() {
                    if let Some(sc) = c.shortcut {
                        ui.strong(shortcuts::display(sc));
                        ui.label(format!("{}  ({})", c.label, c.id));
                        ui.end_row();
                    }
                }
            });
        });
    app.dialogs.help_open = open;
}

/// Outbound policy, app side: a remote write runs once as a preview; the
/// user sees exactly what would leave the machine and must press Send.
fn confirm_send(app: &mut App, ctx: &egui::Context) {
    let Some((id, args, preview)) = app.dialogs.confirm_send.clone() else {
        return;
    };
    let mut open = true;
    let mut decision = None;
    egui::Window::new("Send to a remote service?")
        .open(&mut open)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.label(format!(
                "{id} wants to send this request. Nothing has been sent yet."
            ));
            ui.add_space(4.0);
            let method = preview["method"].as_str().unwrap_or("?");
            let url = preview["url"].as_str().unwrap_or("?");
            ui.monospace(format!("{method} {url}"));
            if let Some(h) = preview["headers"].as_array() {
                for kv in h {
                    if let (Some(k), Some(v)) = (
                        kv.get(0).and_then(Value::as_str),
                        kv.get(1).and_then(Value::as_str),
                    ) {
                        ui.monospace(format!("{k}: {v}"));
                    }
                }
            }
            if !preview["body"].is_null() {
                let body = serde_json::to_string_pretty(&preview["body"]).unwrap_or_default();
                let shown = if body.len() > 4000 {
                    format!(
                        "{}\n… ({} bytes)",
                        &body[..body.floor_char_boundary(4000)],
                        body.len()
                    )
                } else {
                    body
                };
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut shown.as_str())
                                .code_editor()
                                .desired_width(f32::INFINITY),
                        );
                    });
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Send").clicked() {
                    decision = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    decision = Some(false);
                }
            });
        });
    match decision {
        Some(true) => {
            let mut args = if args.is_object() { args } else { json!({}) };
            args["send"] = json!(true);
            app.dialogs.confirm_send = None;
            app.queue(&id, args);
        }
        Some(false) => app.dialogs.confirm_send = None,
        None if !open => app.dialogs.confirm_send = None,
        None => {}
    }
}

/// Sign-in to sim.pflow.xyz and pflow.xyz (`auth.*`). The status is re-read
/// every second while a sign-in is waiting on the browser.
fn accounts(app: &mut App, ctx: &egui::Context) {
    if !app.dialogs.accounts_open {
        app.dialogs.accounts = None;
        return;
    }
    let t = ctx.input(|i| i.time);
    let stale = match &app.dialogs.accounts {
        None => true,
        Some((at, _)) => pflow_native_core::auth::pending() && t - at > 1.0,
    };
    if stale {
        let st = app
            .sess
            .execute("auth.status", Value::Null)
            .unwrap_or(Value::Null);
        app.dialogs.accounts = Some((t, st));
    }
    if pflow_native_core::auth::pending() {
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
    }
    let rows = app
        .dialogs
        .accounts
        .as_ref()
        .and_then(|(_, v)| v.as_array().cloned())
        .unwrap_or_default();
    let mut open = true;
    let mut act: Option<(&'static str, String)> = None;
    let mut copy = None;
    egui::Window::new("Accounts")
        .open(&mut open)
        .collapsible(false)
        .default_width(440.0)
        .show(ctx, |ui| {
            for r in &rows {
                let service = r["service"].as_str().unwrap_or("").to_string();
                ui.horizontal(|ui| {
                    ui.strong(r["site"].as_str().unwrap_or(""));
                    ui.weak(format!("signs in with {}", r["signs_in_with"].as_str().unwrap_or("")));
                });
                let from = r["from"].as_str().unwrap_or("");
                if r["signed_in"] == true {
                    let who = r["login"].as_str().map(|l| format!(" as {l}")).unwrap_or_default();
                    let how = match from {
                        "sign-in" => String::new(),
                        "env" => format!(" (from ${})", r["credential"].as_str().unwrap_or("")),
                        _ => " (from credentials.json)".into(),
                    };
                    ui.label(format!("✔ Signed in{who}{how}"));
                    if let Some(s) = r["expires_in_s"].as_i64() {
                        let refresh = if r["refreshable"] == true { ", renews itself" } else { "" };
                        ui.weak(if s > 0 {
                            format!("token valid for {}h{refresh}", s / 3600)
                        } else {
                            format!("token expired{refresh}")
                        });
                    }
                    if let Some(n) = r["note"].as_str() {
                        ui.weak(n);
                    }
                } else {
                    ui.label("Not signed in");
                }
                let sign_in = &r["sign_in"];
                match sign_in["state"].as_str() {
                    Some("pending") => {
                        if let Some(code) = sign_in["user_code"].as_str() {
                            ui.horizontal(|ui| {
                                ui.label("Enter this code on GitHub:");
                                ui.heading(egui::RichText::new(code).monospace());
                                if ui.small_button("Copy").clicked() {
                                    copy = Some(code.to_string());
                                }
                            });
                        } else {
                            ui.label("Finish signing in in your browser…");
                        }
                        if let Some(url) = sign_in["open"].as_str() {
                            ui.hyperlink_to("Open the sign-in page again", url);
                        }
                        if ui.button("Cancel").clicked() {
                            act = Some(("auth.logout", service.clone()));
                        }
                    }
                    Some("failed") => {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 60, 60),
                            sign_in["error"].as_str().unwrap_or("sign-in failed"),
                        );
                    }
                    _ => {}
                }
                if sign_in["state"] != "pending" {
                    ui.horizontal(|ui| {
                        let label = if r["signed_in"] == true { "Sign in again" } else { "Sign in" };
                        if ui.button(label).clicked() {
                            act = Some(("auth.login", service.clone()));
                        }
                        if from == "sign-in" && ui.button("Sign out").clicked() {
                            act = Some(("auth.logout", service.clone()));
                        }
                    });
                }
                ui.separator();
            }
            ui.weak("Tokens are kept in credentials.json in pflow-native's data folder, readable only by you.");
        });
    if let Some(text) = copy {
        ctx.copy_text(text);
    }
    if let Some((id, service)) = act {
        app.queue(id, json!({ "service": service }));
        app.dialogs.accounts = None; // re-read after it runs
    }
    app.dialogs.accounts_open = open;
}
