//! The egui front end. It owns a [`Session`] and renders it. Every change it
//! makes goes through [`App::run`], i.e. through the same registry the CLI
//! and MCP server use. Widgets queue commands while the UI borrows the
//! session; the queue is drained at the end of each frame.

mod canvas;
mod clipboard;
#[cfg(not(target_arch = "wasm32"))]
pub mod control;
mod dialogs;
mod inspector;
mod json_pane;
mod notes;
mod panels;
mod plot;
mod results;
mod shortcuts;
mod views;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use eframe::egui;
use pflow_native_core::{Session, Step, Theme, Tool, UiRequest};
use serde_json::{json, Value};

pub use canvas::CanvasState;

/// Native-only services (file dialogs). The web build passes `None` and
/// path-taking menu items are disabled there.
pub trait Services {
    fn pick_open(&self) -> Option<String>;
    /// Open dialog for other file kinds (event logs).
    fn pick_open_kind(&self, name: &str, exts: &[&str]) -> Option<String>;
    fn pick_save(&self, default_name: &str, ext: &str) -> Option<String>;
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum BottomTab {
    #[default]
    Plot,
    Workbench,
    Run,
    Models,
    Console,
}

/// Startup options (from the command line on native).
#[derive(Default)]
pub struct Options {
    pub open: Option<String>,
    pub commands: Vec<Step>,
    /// Save a screenshot here after the first frames, then quit.
    pub screenshot: Option<String>,
    /// Control port address (native); `None` disables it.
    pub control: Option<String>,
    /// Don't restore the autosaved document and settings.
    pub fresh: bool,
}

const KEY_DOC: &str = "pflow-native/doc";
const KEY_PATH: &str = "pflow-native/path";
const KEY_SETTINGS: &str = "pflow-native/settings";
const KEY_MODELS: &str = "pflow-native/models";
const KEY_SYSTEMS: &str = "pflow-native/systems";

pub struct App {
    pub sess: Session,
    services: Option<Box<dyn Services>>,
    queue: Vec<Step>,
    status: Result<String, String>,
    canvas: CanvasState,
    pub(crate) inspector: inspector::InspectorState,
    pub(crate) bottom: BottomTab,
    pub(crate) plot: plot::PlotState,
    pub(crate) results: results::ResultsState,
    pub(crate) console: panels::ConsoleState,
    pub(crate) dialogs: dialogs::DialogState,
    pub(crate) json: json_pane::JsonPaneState,
    /// Color index the token tool adds/removes.
    pub(crate) token_color: usize,
    applied_theme: Option<Theme>,
    /// Text for the system clipboard, and the status line that says so.
    clipboard_out: Option<(String, String)>,
    /// Paste requests waiting on the system clipboard (clipboard.rs).
    clip: clipboard::ClipState,
    #[cfg(not(target_arch = "wasm32"))]
    control: Option<control::Control>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    control_addr: Option<String>,
    frame: u64,
    exit_after_screenshot: Option<String>,
    screenshot_done: Arc<AtomicBool>,
    screenshot_pending: bool,
    /// Whether the window is on screen (not minimized, hidden or covered),
    /// as of the last logic pass. A hidden window cannot be captured.
    window_visible: bool,
    /// What was last sent to view.report_focus.
    last_focus: Option<(Option<String>, BottomTab, bool, bool)>,
}

impl App {
    pub fn new(
        opts: Options,
        services: Option<Box<dyn Services>>,
        storage: Option<&dyn eframe::Storage>,
    ) -> Self {
        let mut sess = Session::default();
        sess.has_ui = true;
        let mut app = Self {
            sess,
            services,
            queue: vec![],
            status: Ok(
                "Ready. Keys 1–6 pick a tool, 8 toggles play (fire), Cmd+R runs the ODE.".into(),
            ),
            canvas: CanvasState::default(),
            inspector: Default::default(),
            bottom: BottomTab::Plot,
            plot: Default::default(),
            results: Default::default(),
            console: Default::default(),
            dialogs: Default::default(),
            json: Default::default(),
            token_color: 0,
            applied_theme: None,
            clipboard_out: None,
            clip: Default::default(),
            #[cfg(not(target_arch = "wasm32"))]
            control: None,
            control_addr: opts.control.clone(),
            frame: 0,
            exit_after_screenshot: opts.screenshot,
            screenshot_done: Arc::new(AtomicBool::new(false)),
            screenshot_pending: false,
            window_visible: true,
            last_focus: None,
        };
        let restore = storage.filter(|_| !opts.fresh && app.exit_after_screenshot.is_none());
        if let Some(st) = restore.and_then(|s| s.get_string(KEY_SETTINGS)) {
            if let Ok(v) = serde_json::from_str::<Value>(&st) {
                app.run("view.set", v);
            }
        }
        if let Some(Ok(Value::Object(models))) = restore
            .and_then(|s| s.get_string(KEY_MODELS))
            .map(|t| serde_json::from_str::<Value>(&t))
        {
            for (name, doc) in models {
                app.run(
                    "model.import",
                    json!({ "name": name, "json": doc, "overwrite": true }),
                );
            }
        }
        if let Some(Ok(st)) = restore
            .and_then(|s| s.get_string(KEY_SYSTEMS))
            .map(|t| serde_json::from_str::<Value>(&t))
        {
            app.run("system.restore_state", json!({ "state": st }));
        }
        let autosaved = restore.and_then(|s| s.get_string(KEY_DOC));
        match (&opts.open, autosaved) {
            (Some(path), _) => app.run("file.open", json!({ "path": path })),
            (None, Some(doc)) => {
                app.run("file.load_json", json!({ "json": doc }));
                let path = restore
                    .and_then(|s| s.get_string(KEY_PATH))
                    .filter(|p| !p.is_empty());
                app.sess.path = path;
                app.status = Ok("Restored the autosaved document.".into());
            }
            (None, None) => app.run("file.open_example", json!({ "name": "sir" })),
        }
        for st in opts.commands {
            app.run(&st.command, st.args);
        }
        app
    }

    /// Runs a command now and reports the outcome in the status bar.
    pub fn run(&mut self, id: &str, args: Value) {
        let r = self.sess.execute(id, args.clone());
        self.console.record(id, &args, &r);
        let quiet = id.starts_with("view.") || id == "tool.set" || id.starts_with("edit.select");
        if let (Ok(Value::String(link)), "file.permalink") = (&r, id) {
            self.clipboard_out = Some((link.clone(), "Permalink copied to the clipboard.".into()));
        }
        // Copy and Cut also go to the system clipboard, in the format
        // shared with pflow.xyz (docs/clipboard-format.md).
        if let (Ok(v), "edit.copy" | "edit.cut") = (&r, id) {
            if let Some(clip) = v["clip"].as_str() {
                let verb = if id == "edit.cut" { "Cut" } else { "Copied" };
                let note = format!("{verb} {} to the clipboard.", clipboard::counts(v));
                self.clipboard_out = Some((clip.to_string(), note));
            }
        }
        // A remote write that was only previewed: ask before sending.
        if let Ok(v) = &r {
            if v.get("sent") == Some(&Value::Bool(false)) && v.get("preview").is_some() {
                self.dialogs.confirm_send =
                    Some((id.to_string(), args.clone(), v["preview"].clone()));
            }
        }
        match r {
            Ok(_) if quiet => {}
            Ok(v) => self.status = Ok(summarize(id, &v)),
            Err(e) => self.status = Err(e.to_string()),
        }
        if id == "sim.ode" {
            self.bottom = BottomTab::Plot;
        }
        if id.starts_with("auth.") {
            self.dialogs.show_accounts();
        }
    }

    pub(crate) fn set_status(&mut self, s: Result<String, String>) {
        self.status = s;
    }

    pub(crate) fn canvas_selected_arc(&self) -> Option<(String, String)> {
        self.canvas.selected_arc.clone()
    }

    pub(crate) fn cancel_gesture(&mut self) {
        self.canvas.selected_arc = None;
        self.canvas.cancel();
    }

    /// Runs a menu-style invocation (collects a path first if needed).
    pub(crate) fn menu(&mut self, id: &str) {
        self.invoke_from_menu(id);
    }

    /// Fills transition/place arguments from the current selection, so
    /// "select a transition, click Rate Scan" needs no typing.
    fn selection_prefill(&self, id: &str) -> Value {
        let Some(spec) = self.sess.registry().find(id) else {
            return Value::Null;
        };
        let doc = &self.sess.doc;
        let ts: Vec<&String> = self
            .sess
            .selection
            .iter()
            .filter(|s| doc.transition(s).is_some())
            .collect();
        let ps: Vec<&String> = self
            .sess
            .selection
            .iter()
            .filter(|s| doc.place(s).is_some())
            .collect();
        let mut m = serde_json::Map::new();
        let (mut ti, mut pi) = (0, 0);
        for a in spec.args {
            let pick = match a.name {
                "transition" | "param_x" | "param_y" => {
                    let v = ts.get(ti).map(|s| json!(s));
                    ti += 1;
                    v
                }
                "place" | "observable" => {
                    let v = ps.get(pi).map(|s| json!(s));
                    pi += 1;
                    v
                }
                "id" if ts.len() + ps.len() == 1 => ts.first().or(ps.first()).map(|s| json!(s)),
                _ => None,
            };
            if let Some(v) = pick {
                m.insert(a.name.into(), v);
            }
        }
        Value::Object(m)
    }

    /// Runs `id` the way a menu or the palette does: path arguments come
    /// from a file dialog, other required arguments from a form.
    pub(crate) fn invoke(&mut self, id: &'static str) {
        if id == "edit.paste" {
            // The system clipboard's text first (natively; see clipboard.rs).
            self.clip.paste_command(clipboard::NATIVE);
            return;
        }
        let picker = matches!(
            id,
            "file.open" | "file.save" | "export.svg" | "view.screenshot"
        );
        let needs_form = self
            .sess
            .registry()
            .find(id)
            .is_some_and(|c| c.args.iter().any(|a| a.required));
        if needs_form && !picker {
            let prefill = match id {
                "net.set_details" => json!({
                    "name": self.sess.doc.extra.get("name"),
                    "description": self.sess.doc.extra.get("description"),
                }),
                "net.set_colors" => json!({
                    "token": self.sess.doc.token.iter()
                        .map(|t| t.rsplit('/').next().unwrap_or(t).to_string())
                        .collect::<Vec<_>>().join(", "),
                }),
                "file.open_cid" => json!({ "server": "https://pflow.xyz" }),
                _ => self.selection_prefill(id),
            };
            self.ask_args(id, &prefill);
        } else {
            self.invoke_from_menu(id);
        }
    }

    /// Queues a command for the end of this frame.
    pub fn queue(&mut self, id: &str, args: Value) {
        self.queue.push(Step {
            command: id.into(),
            args,
        });
    }

    fn drain(&mut self) {
        for st in std::mem::take(&mut self.queue) {
            self.run(&st.command, st.args);
        }
    }

    /// Menu items: a command whose required argument is a path gets it from
    /// a native file dialog; everything else runs with no args.
    fn invoke_from_menu(&mut self, id: &str) {
        let pick = |svc: &dyn Services, id: &str, sess: &Session| -> Option<Option<Value>> {
            Some(match id {
                "file.open" => svc.pick_open().map(|p| json!({ "path": p })),
                "data.load_log" => svc
                    .pick_open_kind("Event log", &["csv", "jsonl", "ndjson", "txt"])
                    .map(|p| json!({ "path": p })),
                "file.save" if sess.path.is_none() => svc
                    .pick_save("net.json", "json")
                    .map(|p| json!({ "path": p })),
                "export.svg" => svc
                    .pick_save("net.svg", "svg")
                    .map(|p| json!({ "path": p })),
                "view.screenshot" => svc
                    .pick_save("pflow-native.png", "png")
                    .map(|p| json!({ "path": p })),
                _ => return None,
            })
        };
        let needs_path = matches!(
            id,
            "file.open" | "export.svg" | "view.screenshot" | "data.load_log"
        ) || (id == "file.save" && self.sess.path.is_none());
        match self.services.as_deref() {
            Some(svc) => match pick(svc, id, &self.sess) {
                Some(Some(args)) => self.queue(id, args),
                Some(None) => {} // dialog cancelled
                None => self.queue(id, Value::Null),
            },
            None if needs_path => {
                self.status = Err(format!(
                    "{id} needs a file system (not available on the web)"
                ));
            }
            None => self.queue(id, Value::Null),
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let reg = self.sess.registry();
        egui::MenuBar::new().ui(ui, |ui| {
            for menu in reg.menus() {
                ui.menu_button(menu, |ui| {
                    for c in reg.commands().iter().filter(|c| c.menu == menu) {
                        let enabled = (c.enabled)(&self.sess).is_ok();
                        let mut b = egui::Button::new(c.label);
                        if let Some(sc) = c.shortcut {
                            b = b.shortcut_text(shortcuts::display(sc));
                        }
                        let r = ui.add_enabled(enabled, b).on_hover_text(c.help);
                        if r.clicked() {
                            self.invoke(c.id);
                            ui.close();
                        }
                    }
                    if menu == "Help" {
                        ui.separator();
                        if ui
                            .add(
                                egui::Button::new("Command Palette…")
                                    .shortcut_text(shortcuts::display("Cmd+K")),
                            )
                            .clicked()
                        {
                            self.dialogs.palette_open = true;
                            ui.close();
                        }
                        if ui.button("Keyboard & Mouse…").clicked() {
                            self.dialogs.help_open = true;
                            ui.close();
                        }
                        if let Some(addr) = self.control_status() {
                            ui.separator();
                            ui.weak(addr);
                        }
                    }
                    if menu == "Remote" {
                        ui.separator();
                        if ui.button("Accounts…").clicked() {
                            self.dialogs.show_accounts();
                            ui.close();
                        }
                    }
                    if menu == "File" {
                        ui.separator();
                        ui.menu_button("Examples", |ui| {
                            for (name, _) in pflow_native_core::commands::file::EXAMPLES {
                                if ui.button(*name).clicked() {
                                    self.queue("file.open_example", json!({ "name": name }));
                                    ui.close();
                                }
                            }
                        });
                        if ui.button("Export Plot CSV…").clicked() {
                            self.export_artifact("csv");
                            ui.close();
                        }
                        if ui.button("Export Plot SVG…").clicked() {
                            self.export_artifact("svg");
                            ui.close();
                        }
                    }
                });
            }
        });
    }

    pub(crate) fn export_artifact_as(&mut self, id: &str, format: &str) {
        let Some(svc) = self.services.as_deref() else {
            self.status = Err("exports need a file system".into());
            return;
        };
        if let Some(p) = svc.pick_save(&format!("{id}.{format}"), format) {
            self.queue(
                "export.artifact",
                json!({ "id": id, "format": format, "path": p }),
            );
        }
    }

    pub(crate) fn export_artifact(&mut self, format: &str) {
        let Some(svc) = self.services.as_deref() else {
            self.status = Err("exports need a file system".into());
            return;
        };
        let Some(a) = self.sess.latest("timeseries") else {
            self.status = Err("no plot yet — run the ODE first".into());
            return;
        };
        let id = a.id.clone();
        if let Some(p) = svc.pick_save(&format!("ode.{format}"), format) {
            self.queue(
                "export.artifact",
                json!({ "id": id, "format": format, "path": p }),
            );
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (i, t) in Tool::ALL.iter().enumerate() {
                let key = if *t == Tool::Fire {
                    "8".to_string()
                } else {
                    (i + 1).to_string()
                };
                let label = format!("{} {}", tool_icon(*t), t.name());
                let r = ui
                    .selectable_label(self.sess.tool == *t, label)
                    .on_hover_text(format!("{} ({key})", tool_help(*t)));
                if r.clicked() {
                    self.queue("tool.set", json!({ "tool": t.name() }));
                }
            }
            if self.sess.tool == Tool::Token && self.sess.doc.colors() > 1 {
                ui.separator();
                for (i, t) in self.sess.doc.token.iter().enumerate() {
                    let (r, g, b) = pflow_native_core::export::token_rgb(t, i);
                    let name = t.rsplit('/').next().unwrap_or(t);
                    let text = egui::RichText::new(format!("● {name}"))
                        .color(egui::Color32::from_rgb(r, g, b));
                    if ui.selectable_label(self.token_color == i, text).clicked() {
                        self.token_color = i;
                    }
                }
            }
            ui.separator();
            if ui.button("⟲ Undo").clicked() {
                self.queue("edit.undo", Value::Null);
            }
            if ui.button("⟳ Redo").clicked() {
                self.queue("edit.redo", Value::Null);
            }
            ui.separator();
            if ui
                .button("↺ Reset")
                .on_hover_text("Reset the marking (R)")
                .clicked()
            {
                self.queue("sim.reset", Value::Null);
            }
            if ui
                .button("📈 ODE")
                .on_hover_text("Run the ODE (Cmd+R)")
                .clicked()
            {
                self.queue("sim.ode", self.plot.args());
            }
            if ui
                .button("⛶ Fit")
                .on_hover_text("Fit to view (F)")
                .clicked()
            {
                self.queue("view.fit", Value::Null);
            }
        });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let name = self.sess.path.as_deref().unwrap_or("untitled");
            let dirty = if self.sess.is_dirty() { " •" } else { "" };
            ui.label(format!("{name}{dirty}"));
            ui.separator();
            ui.label(format!(
                "{}P {}T {}A · fired {} · {:.0}%",
                self.sess.doc.places.len(),
                self.sess.doc.transitions.len(),
                self.sess.doc.arcs.len(),
                self.sess.fired,
                self.sess.view.zoom * 100.0
            ));
            if self.sess.recording.is_some() {
                ui.separator();
                ui.colored_label(egui::Color32::RED, "● REC");
            }
            ui.separator();
            match &self.status {
                Ok(s) => ui.label(s),
                Err(e) => ui.colored_label(egui::Color32::from_rgb(220, 60, 60), e),
            };
        });
    }

    /// Tells the session what the window shows (view.report_focus) when that
    /// changes, so an agent's view.focus matches what the person sees.
    fn report_focus(&mut self) {
        let now = (
            self.results.selected.clone(),
            self.bottom,
            self.sess.view.settings.bottom_panel,
            self.window_visible,
        );
        if self.last_focus.as_ref() == Some(&now) {
            return;
        }
        let tab = match now.1 {
            BottomTab::Plot => "results",
            BottomTab::Workbench => "workbench",
            BottomTab::Run => "run",
            BottomTab::Models => "models",
            BottomTab::Console => "console",
        };
        let _ = self.sess.execute(
            "view.report_focus",
            json!({ "artifact": now.0, "tab": tab, "bottom_panel": now.2, "window_visible": now.3 }),
        );
        self.last_focus = Some(now);
    }

    fn handle_ui_requests(&mut self, ctx: &egui::Context) {
        for req in std::mem::take(&mut self.sess.ui_requests) {
            match req {
                UiRequest::Screenshot { path } => self.screenshot(ctx, path, false),
                UiRequest::ShowArtifact { id } => {
                    self.results.selected = Some(id);
                    self.bottom = BottomTab::Plot;
                    let _ = self
                        .sess
                        .execute("view.set", json!({ "bottom_panel": true }));
                }
            }
        }
    }

    fn screenshot(&mut self, ctx: &egui::Context, path: String, then_exit: bool) {
        let done = self.screenshot_done.clone();
        if self.services.is_none() {
            self.status = Err("screenshots need a file system".into());
            return;
        }
        self.screenshot_pending = then_exit;
        let ctx2 = ctx.clone();
        ctx.request_screenshot(move |img| {
            match native_png(&path, &img) {
                Ok(()) => eprintln!("screenshot: {path} ({}x{})", img.size[0], img.size[1]),
                Err(e) => eprintln!("screenshot failed: {e}"),
            }
            done.store(true, Ordering::SeqCst);
            ctx2.request_repaint();
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn native_png(path: &str, img: &egui::ColorImage) -> Result<(), String> {
    let [w, h] = img.size;
    let mut rgba = Vec::with_capacity(w * h * 4);
    for p in &img.pixels {
        rgba.extend_from_slice(&p.to_array());
    }
    image::save_buffer(path, &rgba, w as u32, h as u32, image::ColorType::Rgba8)
        .map_err(|e| e.to_string())
}

#[cfg(target_arch = "wasm32")]
fn native_png(_: &str, _: &egui::ColorImage) -> Result<(), String> {
    Err("no file system".into())
}

fn summarize(id: &str, v: &Value) -> String {
    let short = match v {
        Value::Object(m) if id.starts_with("auth.") && m.contains_key("note") => {
            m["note"].as_str().unwrap_or("").to_string()
        }
        Value::String(s) if s.len() > 80 => format!("{} bytes", s.len()),
        Value::Object(m) if m.get("path").is_some_and(|p| !p.is_null()) => format!("{}", m["path"]),
        Value::Object(m) if m.contains_key("artifact") => {
            format!("artifact {}", m["artifact"].as_str().unwrap_or(""))
        }
        v => {
            let s = v.to_string();
            if s.len() > 100 {
                format!("{}…", &s[..s.floor_char_boundary(100)])
            } else {
                s
            }
        }
    };
    format!("{id} ✓ {short}")
}

pub(crate) fn tool_icon(t: Tool) -> &'static str {
    match t {
        Tool::Select => "☝",
        Tool::Place => "◯",
        Tool::Transition => "▭",
        Tool::Arc => "↗",
        Tool::Token => "•",
        Tool::Delete => "✖",
        Tool::Fire => "▶",
    }
}

fn tool_help(t: Tool) -> &'static str {
    match t {
        Tool::Select => "Select & move; drag empty space to pan, shift-drag to box-select, double-click a transition to fire",
        Tool::Place => "Click to add a place",
        Tool::Transition => "Click to add a transition",
        Tool::Arc => "Click source then target; right-click the target for an inhibitor/read arc",
        Tool::Token => "Click a place for +1 token, right-click for −1",
        Tool::Delete => "Click a node or arc to delete it",
        Tool::Fire => "Play: click an enabled (green) transition to fire it",
    }
}

impl App {
    fn control_status(&self) -> Option<String> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(c) = &self.control {
            return Some(format!("control port {}", c.addr));
        }
        None
    }

    /// Starts the control port on the first logic pass (it needs the egui
    /// context to wake the app), then answers what has arrived.
    fn serve_control(&mut self, ctx: &egui::Context) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(addr) = self.control_addr.take() {
                // A fresh token per launch, in an owner-only file: without it
                // nothing on this machine (or a web page) can drive the app.
                let started = pflow_native_core::control_auth::new_token().and_then(|token| {
                    let c = control::Control::start(&addr, ctx.clone(), token.clone())?;
                    pflow_native_core::control_auth::write_control_file(&c.addr, &token)?;
                    Ok(c)
                });
                match started {
                    Ok(c) => {
                        self.status = Ok(format!("Agents can attach: control port {}", c.addr));
                        self.control = Some(c);
                    }
                    Err(e) => eprintln!("pflow-native: {e} (control port disabled)"),
                }
            }
            let reqs = self
                .control
                .as_ref()
                .map(|c| c.drain(64))
                .unwrap_or_default();
            for req in reqs {
                let reply = self.handle_control(&req.body);
                let _ = req.reply.send(reply);
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = ctx;
    }

    /// One control-port request (already authenticated by its token): a
    /// command or a batch, with the confirm-before-send rule applied, as
    /// the JSON reply line.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn handle_control(&mut self, body: &Value) -> Value {
        let id = body.get("id").cloned().unwrap_or(Value::Null);
        let r = if let Some(steps) = body.get("batch") {
            match steps.as_array().and_then(|st| {
                st.iter()
                    .find(|x| self.is_remote_send(x["command"].as_str().unwrap_or(""), &x["args"]))
            }) {
                Some(x) => Err(pflow_native_core::Error::Invalid(format!(
                    "{} sends to a remote service and needs the user's confirmation in the app; \
                     send it on its own, not in a batch",
                    x["command"].as_str().unwrap_or("?")
                ))),
                None => self
                    .sess
                    .execute("command.batch", json!({ "steps": steps })),
            }
        } else if let Some(cmd) = body.get("command").and_then(Value::as_str) {
            let args = body.get("args").cloned().unwrap_or(Value::Null);
            if self.is_remote_send(cmd, &args) {
                // The confirm-before-send rule holds on this path too:
                // run it as a preview and ask the user in the app.
                let mut preview_args = args.clone();
                preview_args["send"] = json!(false);
                let r = self.sess.execute(cmd, preview_args);
                self.console.record(cmd, &args, &r);
                r.map(|mut v| {
                    if v.get("preview").is_some() {
                        self.dialogs.confirm_send = Some((cmd.to_string(), args.clone(), v["preview"].clone()));
                        v["pending_confirmation"] = json!(true);
                        v["note"] = json!("Not sent: the request is waiting for the user to press Send in the app.");
                    }
                    v
                })
            } else if cmd == "view.screenshot" && !self.window_visible {
                Err(pflow_native_core::Error::Invalid(
                    "the app's window is minimized, hidden or covered, so there are no pixels to capture; \
                     bring it to the front, or export instead (export.svg for the net, export.artifact for a result)"
                        .into(),
                ))
            } else {
                let r = self.sess.execute(cmd, args.clone());
                self.console.record(cmd, &args, &r);
                r
            }
        } else {
            Err(pflow_native_core::Error::BadArgs(
                "expected \"command\" or \"batch\"".into(),
            ))
        };
        match r {
            Ok(v) => json!({ "id": id, "ok": true, "result": v }),
            Err(e) => json!({ "id": id, "ok": false, "error": e.to_string() }),
        }
    }

    /// A request that would send to a remote service: a command that takes
    /// a `send` argument, called with `send: true`.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fn is_remote_send(&self, cmd: &str, args: &Value) -> bool {
        args.get("send").and_then(Value::as_bool) == Some(true)
            && self
                .sess
                .registry()
                .find(cmd)
                .is_some_and(|c| c.args.iter().any(|a| a.name == "send"))
    }

    /// This pass's Copy / Cut / Paste events, before any widget sees them:
    /// the net's unless a text field (or selected label text) has them.
    fn clipboard_events(&mut self, ctx: &egui::Context) {
        use egui::Event;
        let events: Vec<Event> = ctx.input(|i| {
            i.events
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        Event::Copy | Event::Cut | Event::Paste(_) | Event::PasteImage(_)
                    )
                })
                .cloned()
                .collect()
        });
        let label_selection = ctx
            .with_plugin(|l: &mut egui::text_selection::LabelSelectionState| l.has_selection())
            .unwrap_or(false);
        let acts = self.clip.on_pass(clipboard::Pass {
            events: &events,
            frame: ctx.cumulative_frame_nr(),
            text_focus: ctx.text_edit_focused(),
            label_selection,
            has_selection: !self.sess.selection.is_empty(),
            native: clipboard::NATIVE,
        });
        for act in acts {
            match act {
                clipboard::Act::Copy => self.queue("edit.copy", Value::Null),
                clipboard::Act::Cut => self.queue("edit.cut", Value::Null),
                clipboard::Act::Paste(args) => {
                    // Taken for the net: no text field inserts it too.
                    ctx.input_mut(|i| i.events.retain(|e| !matches!(e, Event::Paste(_))));
                    self.queue("edit.paste", args);
                }
                clipboard::Act::Refuse(why) => {
                    ctx.input_mut(|i| i.events.retain(|e| !matches!(e, Event::Paste(_))));
                    self.status = Err(format!("Paste refused: {why}"));
                }
                clipboard::Act::Repaint => ctx.request_repaint(),
            }
        }
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        let want = self.sess.view.settings.theme;
        if self.applied_theme != Some(want) {
            ctx.set_theme(match want {
                Theme::System => egui::ThemePreference::System,
                Theme::Dark => egui::ThemePreference::Dark,
                Theme::Light => egui::ThemePreference::Light,
            });
            self.applied_theme = Some(want);
        }
    }
}

impl eframe::App for App {
    /// Clipboard keys become clipboard events before egui sees them
    /// (clipboard.rs).
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if clipboard::route_keys(&mut raw.events, clipboard::NATIVE) {
            self.clip.paste_key(clipboard::NATIVE);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // With session rates, so fitted / calibrated rates survive a restart
        // (file.load_json reads them back).
        let doc = self
            .sess
            .execute("file.export_json", json!({ "rates": true }))
            .map(|v| serde_json::to_string_pretty(&v).unwrap_or_default())
            .unwrap_or_else(|_| self.sess.doc.to_json_pretty());
        storage.set_string(KEY_DOC, doc);
        storage.set_string(KEY_PATH, self.sess.path.clone().unwrap_or_default());
        let models: serde_json::Map<String, Value> = self
            .sess
            .models
            .iter()
            .map(|(k, d)| (k.clone(), d.to_value()))
            .collect();
        storage.set_string(KEY_MODELS, Value::Object(models).to_string());
        if let Ok(st) = self.sess.execute("system.dump_state", Value::Null) {
            storage.set_string(KEY_SYSTEMS, st.to_string());
        }
        storage.set_string(
            KEY_SETTINGS,
            serde_json::to_string(&self.sess.view.settings).unwrap_or_default(),
        );
    }

    /// Runs before every `ui` pass and also while the window is hidden,
    /// minimized or covered, when `ui` is not called: agents on the control
    /// port get answers whether or not anyone is looking at the window.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.window_visible = ctx.input(|i| i.viewport().visible().unwrap_or(true));
        self.serve_control(ctx);
        // Finished background jobs join the session (their results appear
        // in the Results tab); while any runs, keep checking.
        self.sess.poll_jobs();
        self.report_focus();
        if self.sess.jobs.running() > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        // A screenshot is taken by a `ui` pass, which a hidden window gets
        // only when the platform lets eframe paint it (not macOS: there a
        // covered window gets logic passes only, so handle_control refuses).
        if !self.sess.ui_requests.is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            ctx.request_repaint();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frame += 1;
        self.apply_theme(&ctx);
        self.clipboard_events(&ctx);
        shortcuts::handle(self, &ctx);
        let settings = self.sess.view.settings;

        egui::Panel::top("menu").show(ui, |ui| {
            self.menu_bar(ui);
            self.toolbar(ui);
        });
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        if settings.inspector {
            egui::Panel::right("inspector")
                .resizable(true)
                .default_size(260.0)
                .show(ui, |ui| inspector::ui(self, ui));
        }
        if settings.bottom_panel {
            egui::Panel::bottom("bottom")
                .resizable(true)
                .default_size(330.0)
                .show(ui, |ui| panels::bottom(self, ui));
        }
        if settings.json_pane {
            egui::Panel::left("json")
                .resizable(true)
                .default_size(380.0)
                .show(ui, |ui| json_pane::ui(self, ui));
        }
        egui::CentralPanel::default().show(ui, |ui| canvas::ui(self, ui));
        dialogs::ui(self, &ctx);

        self.drain();
        self.handle_ui_requests(&ctx);
        if let Some((text, note)) = self.clipboard_out.take() {
            ctx.copy_text(text);
            self.status = Ok(note);
        }
        if self.clip.take_read_os() {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            ctx.request_repaint();
        }

        // --screenshot: let layout and fonts settle, capture, then quit.
        if let Some(path) = self.exit_after_screenshot.clone() {
            if self.frame == 8 {
                self.screenshot(&ctx, path, true);
            }
            ctx.request_repaint();
        }
        if self.screenshot_pending && self.screenshot_done.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod control_policy_tests {
    use super::*;

    fn app() -> App {
        App::new(
            Options {
                fresh: true,
                ..Options::default()
            },
            None,
            None,
        )
    }

    /// A remote send that arrives over the control port is not sent: it is
    /// previewed and waits for the user's Send in the app.
    #[cfg(feature = "services")]
    #[test]
    fn port_sends_wait_for_the_user() {
        let mut a = app();
        let r = a.handle_control(
            &json!({"id": 1, "command": "remote.pflow.save", "args": {"send": true}}),
        );
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["result"]["sent"], false);
        assert_eq!(r["result"]["pending_confirmation"], true);
        let (cmd, args, _) = a
            .dialogs
            .confirm_send
            .clone()
            .expect("confirmation pending");
        assert_eq!(cmd, "remote.pflow.save");
        assert_eq!(args["send"], true, "Send re-runs with the caller's args");
    }

    #[cfg(feature = "services")]
    #[test]
    fn port_batches_cannot_carry_remote_sends() {
        let mut a = app();
        let r = a.handle_control(&json!({"id": 2, "batch": [
            {"command": "net.add_place", "args": {"x": 0, "y": 0}},
            {"command": "remote.gist", "args": {"send": true}}
        ]}));
        assert_eq!(r["ok"], false, "{r}");
        assert!(r["error"].as_str().unwrap().contains("confirmation"));
        assert!(a.dialogs.confirm_send.is_none());
    }

    #[test]
    fn ordinary_commands_run_directly() {
        let mut a = app();
        let r = a.handle_control(
            &json!({"id": 3, "command": "net.add_place", "args": {"x": 5, "y": 5}}),
        );
        assert_eq!(r["ok"], true, "{r}");
        #[cfg(feature = "services")]
        {
            let r =
                a.handle_control(&json!({"id": 4, "command": "remote.pflow.share", "args": {}}));
            assert_eq!(r["ok"], true, "pure commands are unaffected: {r}");
        }
        assert!(a.dialogs.confirm_send.is_none());
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod clipboard_tests {
    use super::*;
    use eframe::App as _;
    use egui::{Event, Key, Modifiers};

    const FIXTURE: &str = include_str!("../../core/tests/fixtures/clip-v1.json");

    fn app() -> App {
        let mut a = App::new(
            Options {
                fresh: true,
                ..Options::default()
            },
            None,
            None,
        );
        a.run("edit.select", json!({ "ids": ["S", "infect"] }));
        a
    }

    fn press(key: Key) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }
    }

    /// One frame: the raw-input hook, then the clipboard step and the
    /// command queue, as `ui` runs them.
    fn frame(a: &mut App, ctx: &egui::Context, events: Vec<Event>) {
        let mut raw = egui::RawInput {
            events,
            ..Default::default()
        };
        a.raw_input_hook(ctx, &mut raw);
        ctx.run_ui(raw, |ui| {
            a.clipboard_events(ui.ctx());
            a.drain();
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn cmd_c_puts_the_clip_on_the_system_clipboard() {
        let (mut a, ctx) = (app(), egui::Context::default());
        frame(&mut a, &ctx, vec![press(Key::C)]);
        let (text, note) = a
            .clipboard_out
            .take()
            .expect("clip for the system clipboard");
        let clip = pflow_native_core::commands::clipboard::parse_clip(&text).unwrap();
        assert_eq!((clip.places.len(), clip.transitions.len()), (1, 1));
        assert_eq!(
            note,
            "Copied 1 place, 1 transition and 1 arc to the clipboard."
        );
    }

    #[test]
    fn cmd_v_pastes_the_system_clipboards_clip_or_the_session_one() {
        let (mut a, ctx) = (app(), egui::Context::default());
        let places = a.sess.doc.places.len();
        frame(&mut a, &ctx, vec![press(Key::V)]);
        assert!(a.clip.take_read_os(), "asks eframe for the clipboard text");
        frame(&mut a, &ctx, vec![]);
        frame(&mut a, &ctx, vec![Event::Paste(FIXTURE.into())]);
        assert_eq!(a.sess.doc.places.len(), places + 2, "{:?}", a.status);
        assert_eq!(a.sess.rates.get("t0"), Some(&2.5));

        // Nothing on the system clipboard: the session clip, a few frames on.
        a.run("edit.select", json!({ "ids": ["S"] }));
        a.run("edit.copy", Value::Null);
        let places = a.sess.doc.places.len();
        frame(&mut a, &ctx, vec![press(Key::V)]);
        for _ in 0..3 {
            frame(&mut a, &ctx, vec![]);
        }
        assert_eq!(a.sess.doc.places.len(), places + 1, "{:?}", a.status);
    }

    #[test]
    fn agents_copy_without_touching_the_system_clipboard() {
        let mut a = app();
        let r = a.handle_control(&json!({"id": 1, "command": "edit.copy"}));
        assert!(r["result"]["clip"].is_string(), "{r}");
        assert!(a.clipboard_out.is_none());
    }
}
