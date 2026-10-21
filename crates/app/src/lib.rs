//! The egui front end. It owns a [`Session`] and renders it. Every change it
//! makes goes through [`App::run`], i.e. through the same registry the CLI
//! and MCP server use. Widgets queue commands while the UI borrows the
//! session; the queue is drained at the end of each frame.

mod canvas;
mod inspector;
mod panels;
mod plot;
mod shortcuts;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use eframe::egui;
use pflow_native_core::{Session, Step, Tool, UiRequest};
use serde_json::{json, Value};

pub use canvas::CanvasState;

/// Native-only services (file dialogs). The web build passes `None` and
/// path-taking menu items are disabled there.
pub trait Services {
    fn pick_open(&self) -> Option<String>;
    fn pick_save(&self, default_name: &str, ext: &str) -> Option<String>;
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum BottomTab {
    #[default]
    Plot,
    Run,
    Console,
}

/// Startup options (from the command line on native).
#[derive(Default)]
pub struct Options {
    pub open: Option<String>,
    pub commands: Vec<Step>,
    /// Save a screenshot here after the first frames, then quit.
    pub screenshot: Option<String>,
}

pub struct App {
    pub sess: Session,
    services: Option<Box<dyn Services>>,
    queue: Vec<Step>,
    status: Result<String, String>,
    canvas: CanvasState,
    pub(crate) inspector: inspector::InspectorState,
    pub(crate) bottom: BottomTab,
    pub(crate) plot: plot::PlotState,
    pub(crate) console: panels::ConsoleState,
    frame: u64,
    exit_after_screenshot: Option<String>,
    screenshot_done: Arc<AtomicBool>,
    screenshot_pending: bool,
}

impl App {
    pub fn new(opts: Options, services: Option<Box<dyn Services>>) -> Self {
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
            console: Default::default(),
            frame: 0,
            exit_after_screenshot: opts.screenshot,
            screenshot_done: Arc::new(AtomicBool::new(false)),
            screenshot_pending: false,
        };
        match &opts.open {
            Some(path) => app.run("file.open", json!({ "path": path })),
            None => app.run("file.open_example", json!({ "name": "sir" })),
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
        match r {
            Ok(_) if quiet => {}
            Ok(v) => self.status = Ok(summarize(id, &v)),
            Err(e) => self.status = Err(e.to_string()),
        }
        if id == "sim.ode" {
            self.bottom = BottomTab::Plot;
            if let Some(t) = args.get("t_end").and_then(Value::as_f64) {
                self.plot.t_end = t;
            }
            if let Some(m) = args.get("method").and_then(Value::as_str) {
                self.plot.method = m.to_string();
            }
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
        let needs_path = matches!(id, "file.open" | "export.svg" | "view.screenshot")
            || (id == "file.save" && self.sess.path.is_none());
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
                            self.invoke_from_menu(c.id);
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

    fn handle_ui_requests(&mut self, ctx: &egui::Context) {
        for req in std::mem::take(&mut self.sess.ui_requests) {
            match req {
                UiRequest::Screenshot { path } => self.screenshot(ctx, path, false),
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
        Value::String(s) if s.len() > 80 => format!("{} bytes", s.len()),
        Value::Object(m) if m.contains_key("path") => format!("{}", m["path"]),
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

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frame += 1;
        shortcuts::handle(self, &ctx);

        egui::Panel::top("menu").show(ui, |ui| {
            self.menu_bar(ui);
            self.toolbar(ui);
        });
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(260.0)
            .show(ui, |ui| inspector::ui(self, ui));
        egui::Panel::bottom("bottom")
            .resizable(true)
            .default_size(260.0)
            .show(ui, |ui| panels::bottom(self, ui));
        egui::CentralPanel::default().show(ui, |ui| canvas::ui(self, ui));

        self.drain();
        self.handle_ui_requests(&ctx);

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
