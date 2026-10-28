//! The ten composition views (milestone 2 of porting pflow-views-prototype).
//!
//! Every view draws one of two examples from session state: `orders` (the
//! open composition, `bundle.*`) and `connect4` (the open search plan,
//! `plan.*`). What is drawn is read from the same core functions the
//! registry's queries return (`bundle.state`, `bundle.flatten`,
//! `bundle.incidence`, `bundle.lens`, `bundle.run`, `plan.state`,
//! `plan.net`, `plan.opening`), and every action is a registry command:
//! drag a port to a lit port (`bundle.candidates` decides what lights, the
//! drop runs `bundle.add_link`), drag a piece to a lit slot
//! (`plan.slot_candidates`, `plan.add_piece`), and the tabs and toggles
//! (`view.composition`).
//!
//! While it draws, each view appends what it draws to a [`Scene`]: the
//! parity manifest (`docs/composition-views-scene.md`). It is collected by
//! the drawing calls themselves, never recomputed from the model, and
//! [`headless_manifest`] runs the same drawing without a window.

use std::collections::{BTreeMap, BTreeSet};

use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2,
};
use pflow_native_core::{composition as comp, lens, search_plan as sp, Document, Session, Step};
use serde_json::{json, Map, Value};

/// (id, tab label, title), in the prototype's order (`VIEW_IDS`).
pub const VIEWS: [(&str, &str, &str); 10] = [
    ("cospan", "Cospans", "Open Petri nets as cospans"),
    ("spiders", "Spiders", "Sync points as spiders (ZX-style)"),
    ("operad", "Operad", "Wiring diagrams (operad nesting)"),
    ("string", "String", "String diagram"),
    ("flat", "Flat net", "The flattened net"),
    ("boundaries", "Boundaries", "Nets with boundaries"),
    ("lens", "Lenses", "Open games / lenses (analogy)"),
    ("incidence", "Incidence", "Incidence matrices"),
    ("process", "Processes", "Runs as processes"),
    ("laws", "Laws", "Composition laws (checked by Chyp)"),
];

// ------------------------------------------------------------ the manifest

/// The scene manifest a view collects while it draws.
#[derive(Default)]
pub struct Scene {
    entries: BTreeMap<(String, String), Map<String, Value>>,
}

impl Scene {
    /// Records one drawn element. Arrays are sorted; a second call for the
    /// same (kind, id) replaces the first (no duplicates).
    pub fn emit(&mut self, kind: &str, id: &str, attrs: Value) {
        let mut m = attrs.as_object().cloned().unwrap_or_default();
        for v in m.values_mut() {
            if let Value::Array(a) = v {
                a.sort_by_key(|x| x.as_str().map_or_else(|| x.to_string(), str::to_string));
            }
        }
        self.entries.insert((kind.into(), id.into()), m);
    }

    pub fn manifest(&self, view: &str, example: &str, state: &str) -> Value {
        let elements: Vec<Value> = self
            .entries
            .iter()
            .map(|((k, id), a)| json!({"kind": k, "id": id, "attrs": a}))
            .collect();
        json!({"view": view, "example": example, "state": state, "elements": elements})
    }
}

// ------------------------------------------------------------ data

/// What the views draw, read from the session once per revision.
pub struct Data {
    rev: Option<u64>,
    pub state: Option<Value>,
    pub flat: Option<Value>,
    pub incidence: Option<Value>,
    pub lens: Option<Value>,
    pub runs: Vec<Value>,
    pub plan: Option<Value>,
    pub net: Value,
    pub opening: Value,
    pub c4: Option<Document>,
}

impl Default for Data {
    fn default() -> Self {
        Self {
            rev: None,
            state: None,
            flat: None,
            incidence: None,
            lens: None,
            runs: vec![],
            plan: None,
            net: sp::net_json(),
            opening: sp::opening_json(),
            c4: Document::from_json(sp::CONNECT4_JSON.as_bytes()).ok(),
        }
    }
}

impl Data {
    /// Re-reads the session when it changed.
    pub fn refresh(&mut self, sess: &Session) {
        if self.rev == Some(sess.revision) {
            return;
        }
        self.rev = Some(sess.revision);
        match &sess.composition {
            Some(c) => {
                self.state = Some(c.to_json());
                self.flat = Some(comp::flatten_json(&c.bundle));
                self.incidence = Some(comp::incidence_json(&c.bundle));
                self.lens = Some(lens::report(c, true));
                self.runs = match comp::flatten(&c.bundle) {
                    Ok(f) => comp::scenarios()
                        .iter()
                        .map(|s| json!(comp::run_scenario(&f, s)))
                        .collect(),
                    Err(_) => vec![],
                };
            }
            None => {
                self.state = None;
                self.flat = None;
                self.incidence = None;
                self.lens = None;
                self.runs = vec![];
            }
        }
        self.plan = sess.search_plan.as_ref().map(|p| {
            let c = sp::canonical(&p.plan);
            json!({"plan": c, "json": c.to_string(), "check": sp::check(&c)})
        });
    }
}

// ------------------------------------------------------------ interaction

/// Inspector content for a drawn element.
#[derive(Clone, Default, PartialEq)]
pub struct Info {
    pub title: String,
    pub lines: Vec<(String, String)>,
    pub note: String,
    pub cites: Vec<String>,
}

fn info(title: impl Into<String>) -> Info {
    Info {
        title: title.into(),
        ..Default::default()
    }
}

impl Info {
    fn fact(mut self, k: &str, v: impl Into<String>) -> Self {
        self.lines.push((k.into(), v.into()));
        self
    }
    fn note(mut self, n: impl Into<String>) -> Self {
        self.note = n.into();
        self
    }
    fn cites_of(mut self, v: &Value) -> Self {
        for c in v.as_array().into_iter().flatten() {
            self.cites.push(cite_label(c));
        }
        self
    }
    fn cite(mut self, c: impl Into<String>) -> Self {
        self.cites.push(c.into());
        self
    }
}

/// "go-pflow metamodel/queue.go:71-174 @69257b3 (NewQueue)".
fn cite_label(c: &Value) -> String {
    let o = comp::ontology();
    let repo = c["repo"].as_str().unwrap_or("");
    let r = &o["repos"][repo];
    let name = r["name"].as_str().unwrap_or(repo);
    let rev = r["rev"].as_str().or(c["rev"].as_str()).unwrap_or("");
    let (f, t) = (
        c["from"].as_u64().unwrap_or(0),
        c["to"].as_u64().unwrap_or(0),
    );
    let lines = match (f, t) {
        (0, _) => String::new(),
        (f, t) if t != 0 && t != f => format!(":{f}-{t}"),
        (f, _) => format!(":{f}"),
    };
    let note = c["note"].as_str().filter(|n| !n.is_empty());
    format!(
        "{name} {}{lines} @{rev}{}",
        c["path"].as_str().unwrap_or(""),
        note.map(|n| format!(" ({n})")).unwrap_or_default()
    )
}

#[derive(Clone)]
pub enum Drag {
    /// From a port; `cands` is `bundle.candidates`' answer once fetched.
    Port {
        subnet: String,
        port: String,
        cands: Option<Value>,
    },
    /// A palette piece; `cands` is `plan.slot_candidates`' answer.
    Piece { name: String, cands: Option<Value> },
}

/// The app-side state of the views: the data cache, a drag in progress and
/// the pinned inspector entry. None of it is model state.
#[derive(Default)]
pub struct State {
    pub data: Data,
    pub drag: Option<Drag>,
    pub pinned: Option<Info>,
    /// The notes the last frame drew, shown under the view.
    pub last_notes: Vec<(&'static str, String)>,
}

// ------------------------------------------------------------ drawing context

/// One frame of one view.
pub struct Ctx<'a> {
    p: egui::Painter,
    pub rect: Rect,
    pointer: Option<Pos2>,
    press_origin: Option<Pos2>,
    drag_started: bool,
    released: bool,
    clicked: bool,
    d: &'a Data,
    drag: Option<&'a Drag>,
    pub scene: Scene,
    pub notes: Vec<(&'static str, String)>,
    hovered: Option<Info>,
    /// Commands to run after drawing (edits and view toggles).
    out: Vec<Step>,
    /// A drag that starts this frame.
    start_drag: Option<Drag>,
    clicked_info: Option<Info>,
    pub dark: bool,
}

const INK: Color32 = Color32::from_rgb(0x22, 0x28, 0x33);
const MUTED: Color32 = Color32::from_rgb(0x6b, 0x74, 0x82);
const TRANS: Color32 = Color32::from_rgb(0x4a, 0x55, 0x68);
const LEAK: Color32 = Color32::from_rgb(0xc0, 0x39, 0x2b);
const LIT: Color32 = Color32::from_rgb(0xf2, 0xb7, 0x05);

fn hex(s: &str) -> Color32 {
    let h = s.trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).unwrap_or(0x888888);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// RenderDOT's color per link kind (ontology `link_style`).
fn kind_color(kind: &str) -> Color32 {
    hex(comp::ontology()["link_style"][kind]["color"]
        .as_str()
        .unwrap_or("#888888"))
}

fn kind_glyph(kind: &str) -> &'static str {
    match kind {
        "token" => "◆",
        "data" => "▷",
        "event" => "⊗",
        _ => "⊘",
    }
}

fn schema_color(schema: &str) -> Color32 {
    match schema {
        "queue:items" => Color32::from_rgb(0xd9, 0x8c, 0x1f),
        "resource" => Color32::from_rgb(0x2e, 0x8b, 0x57),
        "account" => Color32::from_rgb(0x8e, 0x44, 0xad),
        "saga:outcome" => Color32::from_rgb(0x2f, 0x6f, 0xb3),
        _ => MUTED,
    }
}

fn pattern_tint(pattern: &str) -> Color32 {
    match pattern {
        "queue" => Color32::from_rgb(0xfd, 0xf3, 0xe4),
        "resource-pool" => Color32::from_rgb(0xe8, 0xf5, 0xec),
        "saga" => Color32::from_rgb(0xf1, 0xea, 0xf7),
        _ => Color32::from_rgb(0xf3, 0xf4, 0xf6),
    }
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

fn arr(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn strs(v: &Value) -> Vec<String> {
    arr(v).iter().map(|x| s(x).to_string()).collect()
}

impl<'a> Ctx<'a> {
    fn hover(&mut self, r: Rect, i: Info) {
        if self.pointer.is_some_and(|p| r.contains(p)) {
            if self.clicked {
                self.clicked_info = Some(i.clone());
            }
            self.hovered = Some(i);
        }
    }

    fn text(&self, pos: Pos2, align: Align2, t: &str, size: f32, c: Color32) -> Rect {
        self.p.text(pos, align, t, FontId::proportional(size), c)
    }

    fn mono(&self, pos: Pos2, align: Align2, t: &str, size: f32, c: Color32) -> Rect {
        self.p.text(pos, align, t, FontId::monospace(size), c)
    }

    fn boxr(&self, r: Rect, fill: Color32, stroke: Color32, w: f32) {
        self.p
            .rect(r, 5.0, fill, Stroke::new(w, stroke), StrokeKind::Middle);
    }

    fn line(&self, a: Pos2, b: Pos2, c: Color32, w: f32) {
        self.p.line_segment([a, b], Stroke::new(w, c));
    }

    fn dashed(&self, a: Pos2, b: Pos2, c: Color32, w: f32) {
        self.p
            .extend(Shape::dashed_line(&[a, b], Stroke::new(w, c), 6.0, 4.0));
    }

    fn curve(&self, a: Pos2, b: Pos2, c: Color32, w: f32) {
        let dx = (b.x - a.x).abs().max(40.0) * 0.5;
        let pts = [a, a + Vec2::new(dx, 0.0), b - Vec2::new(dx, 0.0), b];
        self.p
            .add(egui::epaint::CubicBezierShape::from_points_stroke(
                pts,
                false,
                Color32::TRANSPARENT,
                Stroke::new(w, c),
            ));
    }

    fn note(&mut self, level: &'static str, t: impl Into<String>) {
        self.notes.push((level, t.into()));
    }

    fn command(&mut self, id: &str, args: Value) {
        self.out.push(Step {
            command: id.into(),
            args,
        });
    }

    /// A clickable chip that runs a command.
    fn button(&mut self, r: Rect, label: &str, on: bool, id: &str, args: Value) {
        let fill = if on {
            Color32::from_rgb(0x2f, 0x6f, 0xb3)
        } else {
            Color32::from_rgb(0xee, 0xf1, 0xf5)
        };
        self.boxr(r, fill, MUTED, 1.0);
        self.text(
            r.center(),
            Align2::CENTER_CENTER,
            label,
            12.0,
            if on { Color32::WHITE } else { INK },
        );
        if self.clicked && self.pointer.is_some_and(|p| r.contains(p)) {
            self.command(id, args);
        }
    }

    /// A port of the orders example: drawn by type (place ports circles in
    /// their schema's colour, transition ports squares), draggable, and a
    /// drop target lit only where `bundle.candidates` says the link is
    /// legal. Returns its hit rect.
    fn port(
        &mut self,
        at: Pos2,
        subnet: &str,
        port: &Value,
        label: bool,
        label_right: bool,
    ) -> Rect {
        let id = s(&port["id"]);
        let key = format!("{subnet}:{id}");
        let trans = s(&port["target"]) == "transition";
        let hit = Rect::from_center_size(at, Vec2::splat(14.0));
        // drag state
        let mut ring: Option<Color32> = None;
        let mut verdict: Option<Value> = None;
        if let Some(Drag::Port {
            subnet: fs,
            port: fp,
            cands,
        }) = self.drag
        {
            if fs == subnet && fp == id {
                ring = Some(INK);
            } else if let Some(c) = cands
                .as_ref()
                .and_then(|c| arr(&c["candidates"]).iter().find(|x| s(&x["key"]) == key))
            {
                if c["ok"] == true {
                    ring = Some(kind_color(s(&c["link"]["kind"])));
                }
                verdict = Some(c.clone());
            }
        }
        let dim = self.drag.is_some_and(|d| matches!(d, Drag::Port { .. })) && ring.is_none();
        let fill = if trans {
            TRANS
        } else {
            schema_color(s(&port["schema"]))
        };
        let fill = if dim { fill.gamma_multiply(0.3) } else { fill };
        if trans {
            self.p
                .rect_filled(Rect::from_center_size(at, Vec2::splat(9.0)), 1.0, fill);
        } else {
            self.p.circle_filled(at, 5.0, fill);
        }
        if let Some(c) = ring {
            self.p.circle_stroke(at, 9.0, Stroke::new(2.5, c));
        }
        if label {
            let (pos, al) = if label_right {
                (at + Vec2::new(9.0, 0.0), Align2::LEFT_CENTER)
            } else {
                (at - Vec2::new(9.0, 0.0), Align2::RIGHT_CENTER)
            };
            self.mono(pos, al, id, 10.5, if dim { MUTED } else { INK });
        }
        // inspector: the port, or the verdict while dragging
        let pat = s(&port["pattern_port"]);
        let mut i = info(format!("port {key}"))
            .fact("kind", s(&port["kind"]))
            .fact("target", if trans { "transition" } else { "place" })
            .fact(
                "element",
                if trans {
                    s(&port["transition"])
                } else {
                    s(&port["place"])
                },
            )
            .fact("schema", s(&port["schema"]))
            .fact("ontology", pat);
        if let Some(pp) = self.port_concept(subnet, pat) {
            i = i.note(s(&pp["comment"])).cites_of(&pp["cites"]);
        }
        if let Some(v) = &verdict {
            let ok = v["ok"] == true;
            i = info(if ok {
                format!("{key}: legal ({} link)", s(&v["link"]["kind"]))
            } else {
                format!("{key}: refused")
            });
            for e in arr(&v["verdict"]["errors"]) {
                i = i.fact(s(&e["code"]), s(&e["message"]));
                if s(&e["rule"]) == "page" {
                    i = i.cite("page rule (pflow-native composition.rs)");
                }
            }
            for p in arr(&v["verdict"]["passed"]) {
                i = i.fact("passed", s(&p["text"])).cite(cite_label(&p["cite"]));
            }
        }
        self.hover(hit, i);
        // gestures
        if self.drag_started && self.press_origin.is_some_and(|p| hit.contains(p)) {
            self.start_drag = Some(Drag::Port {
                subnet: subnet.into(),
                port: id.into(),
                cands: None,
            });
        }
        if self.released && self.pointer.is_some_and(|p| hit.contains(p)) {
            if let Some(v) = verdict.filter(|v| v["ok"] == true) {
                let l = &v["link"];
                self.command(
                    "bundle.add_link",
                    json!({"kind": l["kind"], "from": l["from"], "to": l["to"]}),
                );
            }
        }
        hit
    }

    fn port_concept(&self, subnet: &str, key: &str) -> Option<Value> {
        let st = self.d.state.as_ref()?;
        let sub = arr(&st["subnets"]).iter().find(|x| s(&x["id"]) == subnet)?;
        let pat = &comp::ontology()["patterns"][s(&sub["pattern"])];
        arr(&pat["ports"])
            .iter()
            .chain(arr(&pat["portTemplates"]))
            .find(|p| s(&p["key"]) == key)
            .cloned()
    }

    /// A plan slot: a drop target lit where `plan.slot_candidates` says the
    /// piece may go.
    fn slot(&mut self, r: Rect, slot: &str, title: &str) {
        let mut stroke = MUTED;
        let mut v: Option<Value> = None;
        if let Some(Drag::Piece { cands: Some(c), .. }) = self.drag {
            if let Some(x) = arr(&c["slots"]).iter().find(|x| s(&x["slot"]) == slot) {
                if x["ok"] == true {
                    stroke = LIT;
                }
                v = Some(x.clone());
            }
        }
        self.p.rect(
            r,
            6.0,
            Color32::TRANSPARENT,
            Stroke::new(if stroke == LIT { 3.0 } else { 1.0 }, stroke),
            StrokeKind::Middle,
        );
        self.text(
            r.left_top() + Vec2::new(6.0, 4.0),
            Align2::LEFT_TOP,
            title,
            11.0,
            MUTED,
        );
        let mut i = info(format!("slot {slot}"));
        if let Some(sl) = sp::slot(slot) {
            i = i.fact("accepts", sl.accepts).fact(
                "field",
                sl.field
                    .unwrap_or("(designed in the ROADMAP, not a SearchPlan field)"),
            );
        }
        if let Some(x) = &v {
            i = i.fact(
                if x["ok"] == true { "drop" } else { "refused" },
                s(&x["why"]).to_string() + if x["ok"] == true { "legal" } else { "" },
            );
        }
        self.hover(r, i);
        if self.released && self.pointer.is_some_and(|p| r.contains(p)) {
            if let (Some(Drag::Piece { name, .. }), Some(x)) = (self.drag, &v) {
                if x["ok"] == true {
                    let n = name.clone();
                    self.command("plan.add_piece", json!({"piece": n, "slot": slot}));
                }
            }
        }
    }

    /// A piece chip (in a slot, or in the palette when `palette`).
    fn chip(&mut self, at: Pos2, name: &str, palette: bool) -> Rect {
        let info_p = sp::piece_info(name);
        let kind = info_p.map_or("", |p| p.kind);
        let color = match kind {
            "Order" => Color32::from_rgb(0x2f, 0x6f, 0xb3),
            "Driver" => Color32::from_rgb(0x17, 0x8f, 0x7f),
            "Filter" => Color32::from_rgb(0x88, 0x66, 0x00),
            "Heuristic" => LEAK,
            _ => Color32::from_rgb(0x7b, 0x4c, 0xc2),
        };
        let galley_w = name.len() as f32 * 6.6 + 14.0;
        let r = Rect::from_min_size(at, Vec2::new(galley_w, 18.0));
        let status = info_p.map_or("", |p| p.status);
        let fill = if status == "built" {
            Color32::WHITE
        } else {
            Color32::from_rgb(0xf6, 0xf6, 0xf8)
        };
        self.boxr(r, fill, color, 1.3);
        self.mono(r.center(), Align2::CENTER_CENTER, name, 10.5, color);
        let mut i = info(format!("piece {name}"));
        if let Some(p) = info_p {
            i = i
                .fact("kind", p.kind)
                .fact("status", p.status)
                .fact("contract", p.contract)
                .note(p.doc)
                .cite(format!(
                    "pflow-rs crates/pflow-metamodel/src/plan.rs @{}",
                    sp::PLAN_RS_REV
                ));
        }
        self.hover(r, i);
        if palette && self.drag_started && self.press_origin.is_some_and(|p| r.contains(p)) {
            self.start_drag = Some(Drag::Piece {
                name: name.into(),
                cands: None,
            });
        }
        r
    }
}

// ------------------------------------------------------------ frame

/// Draws the selected view into `ui`; returns the commands it asks for and
/// the drag that starts, for the caller to run.
pub struct Frame {
    pub scene: Scene,
    pub out: Vec<Step>,
    pub start_drag: Option<Drag>,
    pub hovered: Option<Info>,
    pub clicked: Option<Info>,
    pub notes: Vec<(&'static str, String)>,
    pub released: bool,
}

pub fn draw(
    ui: &mut egui::Ui,
    d: &Data,
    drag: Option<&Drag>,
    cv: &pflow_native_core::CompositionView,
    dark: bool,
) -> Frame {
    let (view, example, lens_mode, rewrite) = (
        cv.view.as_str(),
        cv.example.as_str(),
        cv.lens.as_str(),
        cv.rewrite.as_deref(),
    );
    let size = ui.available_size();
    let (resp, p) = ui.allocate_painter(size, Sense::click_and_drag());
    let rect = resp.rect;
    p.rect_filled(rect, 0.0, Color32::from_rgb(0xfb, 0xfb, 0xfc));
    let mut c = Ctx {
        p,
        rect,
        pointer: resp.interact_pointer_pos().or(resp.hover_pos()),
        press_origin: ui.input(|i| i.pointer.press_origin()),
        drag_started: resp.drag_started(),
        released: resp.drag_stopped(),
        clicked: resp.clicked(),
        d,
        drag,
        scene: Scene::default(),
        notes: vec![],
        hovered: None,
        out: vec![],
        start_drag: None,
        clicked_info: None,
        dark,
    };
    let title = VIEWS.iter().find(|v| v.0 == view).map_or(view, |v| v.2);
    c.text(
        rect.left_top() + Vec2::new(14.0, 10.0),
        Align2::LEFT_TOP,
        title,
        16.0,
        INK,
    );
    let body = Rect::from_min_max(
        rect.left_top() + Vec2::new(14.0, 40.0),
        rect.right_bottom() - Vec2::new(14.0, 10.0),
    );
    let orders = example == "orders";
    if orders && d.state.is_none() {
        c.note("warn", "No composition is open (bundle.open).");
    } else if !orders && d.plan.is_none() {
        c.note("warn", "No search plan is open (plan.open).");
    } else {
        match (view, orders) {
            ("cospan", true) => cospan_orders(&mut c, body),
            ("cospan", false) => cospan_c4(&mut c, body),
            ("spiders", true) => spiders_orders(&mut c, body),
            ("spiders", false) => spiders_c4(&mut c, body),
            ("operad", true) => operad_orders(&mut c, body),
            ("operad", false) => operad_c4(&mut c, body),
            ("string", true) => string_orders(&mut c, body),
            ("string", false) => string_c4(&mut c, body),
            ("flat", true) => flat_orders(&mut c, body),
            ("flat", false) => flat_c4(&mut c, body),
            ("boundaries", true) => boundaries_orders(&mut c, body),
            ("boundaries", false) => boundaries_c4(&mut c, body),
            ("lens", true) => lens_orders(&mut c, body, lens_mode),
            ("lens", false) => lens_c4(&mut c, body),
            ("incidence", true) => incidence_orders(&mut c, body),
            ("incidence", false) => incidence_c4(&mut c, body),
            ("process", true) => process_orders(&mut c, body),
            ("process", false) => process_c4(&mut c, body),
            ("laws", _) => laws(&mut c, body, orders, rewrite),
            _ => {}
        }
    }
    // the rubber band of a port drag
    if let (Some(Drag::Port { .. } | Drag::Piece { .. }), Some(pt), Some(o)) =
        (drag, c.pointer, c.press_origin)
    {
        if resp.dragged() {
            c.dashed(o, pt, INK, 1.5);
        }
    }
    let released = resp.drag_stopped();
    Frame {
        scene: c.scene,
        out: c.out,
        start_drag: c.start_drag,
        hovered: c.hovered,
        clicked: c.clicked_info,
        notes: c.notes,
        released,
    }
}

// ------------------------------------------------------------ shared layout (orders)

struct Laid {
    id: String,
    pattern: String,
    rect: Rect,
    /// (port, its anchors).
    ports: Vec<(Value, Vec<Pos2>)>,
}

fn subnets(c: &Ctx) -> Vec<Value> {
    c.d.state
        .as_ref()
        .map(|st| arr(&st["subnets"]).to_vec())
        .unwrap_or_default()
}

fn links(c: &Ctx) -> Vec<Value> {
    c.d.state
        .as_ref()
        .map(|st| arr(&st["links"]).to_vec())
        .unwrap_or_default()
}

/// The prototype's port sides (cospan.js `portSides`): transition ports on
/// the bottom edge; place ports in on the left, out on the right, inout on
/// both, observe on top.
fn side_of(kind: &str, target: &str) -> &'static str {
    if target == "transition" {
        return "bottom";
    }
    match kind {
        "in" => "left",
        "out" => "right",
        "inout" => "both",
        _ => "top",
    }
}

/// Subnet boxes in a row across `area` (wrapping to two rows past four),
/// each port anchored on its side.
fn lay_boxes(c: &Ctx, area: Rect, h: f32) -> Vec<Laid> {
    let subs = subnets(c);
    let n = subs.len().max(1);
    let per_row = n.min(4);
    let gap = 70.0;
    let w = ((area.width() - gap * (per_row as f32 - 1.0)) / per_row as f32).clamp(150.0, 380.0);
    subs.iter()
        .enumerate()
        .map(|(i, sub)| {
            let (row, col) = (i / per_row, i % per_row);
            let min = area.left_top() + Vec2::new(col as f32 * (w + gap), row as f32 * (h + 60.0));
            let rect = Rect::from_min_size(min, Vec2::new(w, h));
            let ports = arr(&sub["ports"]);
            let mut by_side: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
            for p in ports {
                let k = side_of(s(&p["kind"]), s(&p["target"]));
                if k == "both" {
                    by_side.entry("left").or_default().push(p);
                    by_side.entry("right").or_default().push(p);
                } else {
                    by_side.entry(k).or_default().push(p);
                }
            }
            let mut anchors: Vec<(Value, Vec<Pos2>)> =
                ports.iter().map(|p| (p.clone(), vec![])).collect();
            for (side, list) in &by_side {
                let k = list.len() as f32;
                for (j, p) in list.iter().enumerate() {
                    let t = (j as f32 + 1.0) / (k + 1.0);
                    let pos = match *side {
                        "left" => {
                            Pos2::new(rect.left(), rect.top() + 28.0 + t * (rect.height() - 34.0))
                        }
                        "right" => {
                            Pos2::new(rect.right(), rect.top() + 28.0 + t * (rect.height() - 34.0))
                        }
                        "bottom" => Pos2::new(rect.left() + t * rect.width(), rect.bottom()),
                        _ => Pos2::new(rect.left() + t * rect.width(), rect.top()),
                    };
                    if let Some(a) = anchors.iter_mut().find(|(q, _)| q["id"] == p["id"]) {
                        a.1.push(pos);
                    }
                }
            }
            Laid {
                id: s(&sub["id"]).into(),
                pattern: s(&sub["pattern"]).into(),
                rect,
                ports: anchors,
            }
        })
        .collect()
}

fn subnet_info(sub: &Value) -> Info {
    let pat = &comp::ontology()["patterns"][s(&sub["pattern"])];
    info(format!("{} ({})", s(&sub["id"]), s(&pat["prefLabel"])))
        .fact("net type", s(&sub["net_type"]))
        .fact("constructor", s(&pat["constructor"]))
        .fact("parameters (judgment)", sub["params"].to_string())
        .fact("why", s(&sub["why"]))
        .note(s(&pat["definition"]))
        .cites_of(&pat["cites"])
}

/// Draws the boxes and ports (registering both), returns the layout.
fn draw_boxes(c: &mut Ctx, area: Rect, h: f32, port_labels: bool) -> Vec<Laid> {
    let laid = lay_boxes(c, area, h);
    let subs = subnets(c);
    for l in &laid {
        let sub = subs
            .iter()
            .find(|x| s(&x["id"]) == l.id)
            .cloned()
            .unwrap_or(Value::Null);
        c.boxr(l.rect, pattern_tint(&l.pattern), MUTED, 1.2);
        c.text(
            l.rect.left_top() + Vec2::new(8.0, 6.0),
            Align2::LEFT_TOP,
            &l.id,
            13.0,
            INK,
        );
        c.text(
            l.rect.right_top() + Vec2::new(-8.0, 7.0),
            Align2::RIGHT_TOP,
            &l.pattern,
            10.5,
            MUTED,
        );
        c.hover(l.rect, subnet_info(&sub));
        for (p, anchors) in &l.ports {
            for a in anchors {
                if (a.y - l.rect.bottom()).abs() < 0.5 {
                    // bottom edge: the label below, staggered so neighbours
                    // do not overlap
                    c.port(*a, &l.id, p, false, true);
                    if port_labels {
                        let k = l
                            .ports
                            .iter()
                            .position(|(q, _)| q["id"] == p["id"])
                            .unwrap_or(0);
                        let dy = 8.0 + 11.0 * (k % 3) as f32;
                        c.mono(
                            *a + Vec2::new(0.0, dy),
                            Align2::CENTER_TOP,
                            s(&p["id"]),
                            9.5,
                            INK,
                        );
                    }
                } else {
                    let right = a.x >= l.rect.center().x;
                    c.port(*a, &l.id, p, port_labels, !right);
                }
            }
        }
    }
    laid
}

fn anchor(laid: &[Laid], end: &Value, toward: Pos2) -> Option<Pos2> {
    let l = laid.iter().find(|l| l.id == s(&end["subnet"]))?;
    let (_, a) = l.ports.iter().find(|(p, _)| p["id"] == end["port"])?;
    a.iter()
        .min_by(|x, y| x.distance(toward).total_cmp(&y.distance(toward)))
        .copied()
}

fn link_info(l: &Value) -> Info {
    let k = s(&l["kind"]);
    let lk = &comp::ontology()["link_kinds"][k];
    let mut i = info(format!("{k} link {}", s(&l["key"])))
        .fact("index", l["index"].to_string())
        .fact("idiom", s(&l["idiom"]))
        .note(s(&lk["comment"]))
        .cites_of(&lk["cites"]);
    if l["leak"] == true {
        i = i.fact(
            "caveat",
            "pool leak: release fires only on commit (patterns.ttl:496)",
        );
    }
    i
}

/// A link between two anchors, drawn in RenderDOT's style; a click on it
/// removes it (`bundle.remove_link`).
fn draw_link(c: &mut Ctx, laid: &[Laid], l: &Value) -> Option<Rect> {
    let a0 = anchor(laid, &l["from"], c.rect.center())?;
    let b = anchor(laid, &l["to"], a0)?;
    let a = anchor(laid, &l["from"], b)?;
    let k = s(&l["kind"]);
    let col = kind_color(k);
    let below = |p: Pos2| laid.iter().any(|l| (p.y - l.rect.bottom()).abs() < 0.5);
    if below(a) && below(b) {
        // two event ports on bottom edges: route the link under the boxes
        let dip = 34.0 + 12.0 * (l["index"].as_f64().unwrap_or(0.0) as f32);
        let (a2, b2) = (a + Vec2::new(0.0, dip), b + Vec2::new(0.0, dip));
        for (p, q) in [(a, a2), (a2, b2), (b2, b)] {
            c.dashed(p, q, col, 2.0);
        }
        let mid = a2 + (b2 - a2) * 0.5;
        let r = c.text(mid, Align2::CENTER_CENTER, kind_glyph(k), 14.0, col);
        c.hover(r.expand(4.0), link_info(l).fact("remove", "click"));
        if c.clicked && c.pointer.is_some_and(|p| r.expand(4.0).contains(p)) && c.drag.is_none() {
            c.command("bundle.remove_link", json!({"index": l["index"]}));
        }
        return Some(r);
    }
    match k {
        "event" | "guard" => c.dashed(a, b, col, 2.0),
        _ => c.line(a, b, col, 3.0),
    }
    let mid = a + (b - a) * 0.5;
    let r = c.text(mid, Align2::CENTER_CENTER, kind_glyph(k), 14.0, col);
    c.hover(r.expand(4.0), link_info(l).fact("remove", "click"));
    if c.clicked && c.pointer.is_some_and(|p| r.expand(4.0).contains(p)) && c.drag.is_none() {
        c.command("bundle.remove_link", json!({"index": l["index"]}));
    }
    Some(r)
}

// ------------------------------------------------------------ 1 cospan

fn cospan_orders(c: &mut Ctx, area: Rect) {
    let laid = draw_boxes(c, area, 210.0, true);
    for l in &laid {
        c.scene.emit("subnet", &l.id, json!({"pattern": l.pattern}));
        for (p, _) in &l.ports {
            c.scene.emit(
                "port",
                &format!("{}:{}", l.id, s(&p["id"])),
                json!({"side": side_of(s(&p["kind"]), s(&p["target"])), "target": s(&p["target"])}),
            );
        }
    }
    let ls = links(c);
    let mut glued = 0;
    for l in &ls {
        if draw_link(c, &laid, l).is_some() {
            let k = s(&l["kind"]);
            let style = match k {
                "token" | "data" => {
                    glued += 1;
                    "pushout"
                }
                "event" => "ghost",
                _ => "guard",
            };
            c.scene
                .emit("link", s(&l["key"]), json!({"kind": k, "style": style}));
        }
    }
    let form = if glued > 0 { "pushout" } else { "tensor" };
    let y = laid
        .iter()
        .map(|l| l.rect.bottom())
        .fold(area.top(), f32::max)
        + 110.0;
    let r = c.text(
        Pos2::new(area.left(), y),
        Align2::LEFT_TOP,
        &if glued > 0 {
            format!("composite: a pushout along {glued} glued boundary place(s) (Flatten's place classes)")
        } else {
            "composite: ⊗ only (disjoint union): no token or data link glues two feet; event links are dashed ghosts (transition fusion, not a pushout)".into()
        },
        13.0,
        INK,
    );
    c.scene.emit("composite", "bundle", json!({"form": form}));
    c.hover(
        r,
        info("the composite").fact("form", form).note(
            "Baez–Master: an open net is a cospan X → S ← Y; composing glues boundaries (a pushout).",
        ),
    );
    if glued == 0 {
        c.note(
            "na",
            "Only lk:token and lk:data are cospan composites: a Baez–Master open net has places on its feet, and go-pflow fuses the two places into one (Flatten's place classes). So as a cospan composite the instances are only ⊗ (disjoint union); the event links are drawn as dashed ghosts.",
        );
    } else {
        c.note(
            "na",
            "The pushout is drawn where a token or data link glues two feet: the two places become one, named and marked as go-pflow's Flatten does (wire:<smallest member>, initial markings summed).",
        );
    }
    c.note("judgment", comp::PARAMS_NOTE);
}

fn net_counts(c: &Ctx) -> (i64, i64, i64) {
    (
        c.d.net["places"].as_i64().unwrap_or(0),
        c.d.net["transitions"].as_i64().unwrap_or(0),
        c.d.net["arcs"].as_i64().unwrap_or(0),
    )
}

/// The Connect-4 net at its own coordinates, scaled into `r`.
fn draw_c4_net(c: &mut Ctx, r: Rect) {
    let Some(doc) = &c.d.c4 else { return };
    let xs = doc
        .places
        .iter()
        .map(|p| p.x)
        .chain(doc.transitions.iter().map(|t| t.x));
    let ys = doc
        .places
        .iter()
        .map(|p| p.y)
        .chain(doc.transitions.iter().map(|t| t.y));
    let (x0, x1) = xs.fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
    let (y0, y1) = ys.fold((f64::MAX, f64::MIN), |(a, b), y| (a.min(y), b.max(y)));
    let sc =
        ((r.width() as f64) / (x1 - x0).max(1.0)).min((r.height() as f64) / (y1 - y0).max(1.0));
    let at = |x: f64, y: f64| {
        Pos2::new(
            r.left() + ((x - x0) * sc) as f32,
            r.top() + ((y - y0) * sc) as f32,
        )
    };
    let pos: BTreeMap<&str, Pos2> = doc
        .places
        .iter()
        .map(|p| (p.id.as_str(), at(p.x, p.y)))
        .chain(
            doc.transitions
                .iter()
                .map(|t| (t.id.as_str(), at(t.x, t.y))),
        )
        .collect();
    for a in &doc.arcs {
        if let (Some(p), Some(q)) = (pos.get(a.source.as_str()), pos.get(a.target.as_str())) {
            c.line(*p, *q, Color32::from_gray(205), 0.5);
        }
    }
    for p in &doc.places {
        c.p.circle_filled(pos[p.id.as_str()], 2.2, Color32::from_rgb(0x8a, 0x93, 0xa0));
    }
    for t in &doc.transitions {
        c.p.rect_filled(
            Rect::from_center_size(pos[t.id.as_str()], Vec2::splat(3.0)),
            0.0,
            TRANS,
        );
    }
}

fn cospan_c4(c: &mut Ctx, area: Rect) {
    let (pl, tr, _) = net_counts(c);
    let net = Rect::from_min_size(
        area.left_top() + Vec2::new(180.0, 30.0),
        Vec2::new(area.width() - 360.0, area.height() - 120.0),
    );
    c.boxr(net.expand(8.0), Color32::WHITE, MUTED, 1.0);
    draw_c4_net(c, net);
    c.text(
        Pos2::new(area.left() + 80.0, net.center().y),
        Align2::CENTER_CENTER,
        "∅",
        30.0,
        INK,
    );
    c.text(
        Pos2::new(area.right() - 80.0, net.center().y),
        Align2::CENTER_CENTER,
        "∅",
        30.0,
        INK,
    );
    c.text(
        Pos2::new(area.center().x, net.bottom() + 24.0),
        Align2::CENTER_TOP,
        &format!("connect-4: a closed net ∅ → N ← ∅ ({pl} places, {tr} transitions)"),
        13.0,
        INK,
    );
    c.scene
        .emit("net", "connect-4", json!({"places": pl, "transitions": tr}));
    c.scene.emit("note", "closed", json!({}));
    c.hover(
        net,
        info("connect-4 (pflow-rs tests/fixtures/connect-4.json, MIT)")
            .fact("places", pl.to_string())
            .fact("transitions", tr.to_string())
            .note("No ports: as an open net its feet are empty."),
    );
    c.note("na", "Does not apply as drawn: a SearchPlan is not an open net. It composes searches by chaining order pieces, wrapping drivers around the search and filling slots, and there is no place on its boundary to glue. The nearest honest picture is the net the plan searches, drawn as a closed net ∅ → N ← ∅.");
}

// ------------------------------------------------------------ 2 spiders

fn spiders_orders(c: &mut Ctx, area: Rect) {
    let Some(flat) = c.d.flat.clone() else { return };
    if flat["ok"] != true {
        c.note(
            "warn",
            format!(
                "Flatten refused this composition ({}): no classes to draw.",
                s(&flat["error"])
            ),
        );
        return;
    }
    // member boxes on the left, one spider per class on the right
    let mut y = area.top() + 10.0;
    let x0 = area.left();
    let sx = area.left() + 330.0;
    let ls = links(c);
    let mut classes: Vec<(String, &str, Vec<String>)> = vec![];
    for (k, v) in flat["fused_groups"].as_object().into_iter().flatten() {
        classes.push((k.clone(), "event", strs(v)));
    }
    for (k, v) in flat["wires"].as_object().into_iter().flatten() {
        classes.push((k.clone(), "token", strs(v)));
    }
    if classes.is_empty() {
        c.text(
            Pos2::new(x0, y),
            Align2::LEFT_TOP,
            "No event or token links: no class has two members, so nothing fuses.",
            13.0,
            MUTED,
        );
    }
    for (id, colour, legs) in &classes {
        let n = legs.len() as f32;
        let h = n * 30.0 + 10.0;
        let center = Pos2::new(sx, y + h / 2.0);
        let col = kind_color(colour);
        for (i, m) in legs.iter().enumerate() {
            let my = y + 20.0 + i as f32 * 30.0;
            let r = Rect::from_min_size(Pos2::new(x0, my - 11.0), Vec2::new(220.0, 22.0));
            c.boxr(r, Color32::from_rgb(0xfd, 0xe2, 0xc4), MUTED, 1.0);
            c.mono(r.center(), Align2::CENTER_CENTER, m, 11.0, INK);
            c.curve(r.right_center(), center, col, 2.0);
            c.hover(r, info(format!("leg: {m}")).fact("class", id.as_str()).note("A member: it fires as the class (go-pflow's union-find, compose_fuse.go:9-20)."));
        }
        let dot = Rect::from_center_size(center, Vec2::splat(20.0));
        c.p.circle_filled(center, 9.0, col);
        c.text(
            center + Vec2::new(16.0, 0.0),
            Align2::LEFT_CENTER,
            id,
            11.5,
            col,
        );
        c.hover(
            dot,
            info(format!("spider {id}"))
                .fact("colour", *colour)
                .fact("legs", legs.join(", "))
                .note(if *colour == "event" {
                    "One event class = one spider: its members fuse into one flat transition."
                } else {
                    "A token wire: its places fuse into one flat place."
                }),
        );
        c.scene
            .emit("spider", id, json!({"colour": colour, "legs": legs}));
        y += h + 24.0;
    }
    for l in &ls {
        if l["leak"] == true {
            let r = c.text(
                Pos2::new(x0, y + 6.0),
                Align2::LEFT_TOP,
                &format!(
                    "leak: {} fires only with commit; on rollback the worker never comes back",
                    s(&l["key"])
                ),
                12.0,
                LEAK,
            );
            c.hover(r, link_info(l));
            c.scene.emit("leak", s(&l["key"]), json!({}));
            y += 22.0;
        }
    }
    c.note("analogy", "Analogy of shape only: ZX spiders without phases or any quantum semantics. The one law used, fusion of same-colour spiders that share a leg, is what go-pflow's Flatten does with its union-finds (compose_fuse.go:9-20).");
}

/// The plan's lists as the solver reads them (absent fields are defaults).
fn plan_lists(c: &Ctx) -> (Value, Vec<String>, Vec<Value>, Vec<String>, Vec<String>) {
    let plan =
        c.d.plan
            .as_ref()
            .map(|p| p["plan"].clone())
            .unwrap_or(Value::Null);
    let names = |f: &str| -> Vec<String> {
        sp::field_or_default(&plan, f)
            .iter()
            .map(|x| s(x).to_string())
            .collect()
    };
    let drivers = sp::field_or_default(&plan, "drivers");
    (
        plan.clone(),
        names("order"),
        drivers,
        names("filters"),
        names("heuristics"),
    )
}

fn driver_name(d: &Value) -> String {
    match d {
        Value::String(x) => x.clone(),
        Value::Object(o) => o.keys().next().cloned().unwrap_or_default(),
        other => other.to_string(),
    }
}

fn plan_warn(c: &mut Ctx) {
    if let Some(e) =
        c.d.plan
            .as_ref()
            .and_then(|p| p["check"]["error"].as_str().map(String::from))
    {
        c.note("warn", format!("pflow-rs refuses this plan: {e}"));
    }
}

/// The piece palette, along the bottom of `area` (drag a piece onto a lit
/// slot).
fn palette(c: &mut Ctx, area: Rect) {
    let mut x = area.left();
    let y = area.bottom() - 26.0;
    c.text(
        Pos2::new(x, y - 16.0),
        Align2::LEFT_TOP,
        "pieces (drag onto a slot):",
        11.0,
        MUTED,
    );
    for p in sp::pieces() {
        let r = c.chip(Pos2::new(x, y), p.name, true);
        x = r.right() + 6.0;
    }
}

fn spiders_c4(c: &mut Ctx, area: Rect) {
    let (plan, order, _, _, _) = plan_lists(c);
    let memo = plan.get("memo").is_some_and(|m| !m.is_null());
    let core = Rect::from_min_size(
        area.left_top() + Vec2::new(area.width() / 2.0 - 120.0, 20.0),
        Vec2::new(240.0, 70.0),
    );
    c.boxr(core, Color32::from_rgb(0xee, 0xf1, 0xf5), MUTED, 1.4);
    c.text(
        core.center(),
        Align2::CENTER_CENTER,
        "core · Solver::search",
        13.0,
        INK,
    );
    c.hover(
        core,
        info("the core search (fixed, not a piece)").fact(
            "tables",
            "reads and writes the transposition table; lends the history table to the chain",
        ),
    );
    let mut tt_legs: Vec<String> = vec![];
    if order.iter().any(|x| x == "tt_best") {
        tt_legs.push("tt_best".into());
    }
    if memo {
        tt_legs.push("memo".into());
    }
    let hi_legs: Vec<String> = order.iter().filter(|x| *x == "history").cloned().collect();
    for (k, (id, title, legs)) in [
        ("tt", "transposition table", &tt_legs),
        ("history", "history table", &hi_legs),
    ]
    .iter()
    .enumerate()
    {
        let center = Pos2::new(
            area.left() + area.width() * (0.3 + 0.4 * k as f32),
            core.bottom() + 120.0,
        );
        c.p.circle_stroke(center, 12.0, Stroke::new(2.0, INK));
        c.text(
            center + Vec2::new(0.0, 18.0),
            Align2::CENTER_TOP,
            title,
            12.0,
            INK,
        );
        c.line(
            center - Vec2::new(0.0, 12.0),
            core.center_bottom(),
            MUTED,
            1.0,
        );
        for (i, leg) in legs.iter().enumerate() {
            let at = center + Vec2::new(-60.0 + i as f32 * 120.0, 90.0);
            let r = c.chip(at - Vec2::new(30.0, 0.0), leg, false);
            c.line(center + Vec2::new(0.0, 12.0), r.center_top(), INK, 1.4);
        }
        c.hover(Rect::from_center_size(center, Vec2::splat(26.0)), info(format!("junction: the {title} (shared state, not a spider)")).fact("legs", legs.join(", ")).note("A junction, not a ZX spider: shared state that several parts read or write. Nothing fuses and no spider law applies."));
        c.scene.emit("junction", id, json!({"legs": legs}));
    }
    plan_warn(c);
    c.note("na", "A search plan has no event links, so there are no transition classes and nothing fuses: no spider is drawn here. The nearest honest picture is the plan's two pieces of shared state, drawn as junctions (hollow dots, not ZX spiders) with a leg to each piece in the plan that reads, writes or caps them.");
}

// ------------------------------------------------------------ 3 operad

fn operad_orders(c: &mut Ctx, area: Rect) {
    let outer = Rect::from_min_size(area.left_top(), Vec2::new(area.width(), 330.0));
    c.p.rect(
        outer,
        12.0,
        Color32::from_rgb(0xf7, 0xf8, 0xfa),
        Stroke::new(1.6, INK),
        StrokeKind::Middle,
    );
    c.text(
        outer.left_top() + Vec2::new(10.0, 8.0),
        Align2::LEFT_TOP,
        "the bundle (outer box)",
        12.0,
        MUTED,
    );
    let inner = outer.shrink2(Vec2::new(60.0, 50.0));
    let laid = draw_boxes(c, inner, 200.0, true);
    let ls = links(c);
    let mut used: BTreeSet<String> = BTreeSet::new();
    for l in &ls {
        used.insert(format!(
            "{}:{}",
            s(&l["from"]["subnet"]),
            s(&l["from"]["port"])
        ));
        used.insert(format!("{}:{}", s(&l["to"]["subnet"]), s(&l["to"]["port"])));
        let (Some(a), Some(b)) = (
            anchor(&laid, &l["from"], outer.center()),
            anchor(&laid, &l["to"], outer.center()),
        ) else {
            continue;
        };
        let j = Pos2::new((a.x + b.x) / 2.0, outer.bottom() - 22.0);
        let col = kind_color(s(&l["kind"]));
        c.line(a, j, col, 1.6);
        c.line(b, j, col, 1.6);
        c.p.circle_filled(j, 5.0, col);
        c.hover(Rect::from_center_size(j, Vec2::splat(12.0)), link_info(l).note("A junction: an undirected cable soldered onto both ports (Spivak's wiring diagrams are cospans)."));
        c.scene
            .emit("junction", s(&l["key"]), json!({"kind": s(&l["kind"])}));
    }
    for l in &laid {
        c.scene.emit("hole", &l.id, json!({}));
        for (p, anchors) in &l.ports {
            let key = format!("{}:{}", l.id, s(&p["id"]));
            if !used.contains(&key) {
                if let Some(a) = anchors.first() {
                    let ex = Pos2::new(a.x, outer.top());
                    c.dashed(*a, ex, Color32::from_gray(200), 0.8);
                    c.p.circle_stroke(ex, 3.0, Stroke::new(1.0, MUTED));
                }
                c.scene.emit("exposed", &key, json!({}));
            }
        }
    }
    if ls.iter().any(|l| s(&l["kind"]) == "guard") {
        c.note("na", "Guard links are not cables: a guard restricts one subnet on another's place (W_RESTRICTIVE_LINK) and sits outside the free-SMC fragment (ROADMAP Phase 6). It is drawn and flagged; no operadic composite is claimed.");
    }
    c.note("judgment", "Exposed ports: the outer box's own ports are every port no link uses, drawn faintly on its boundary. That choice of interface is this view's reading; a go-pflow Bundle declares subnets, links and constraints but no ports of its own.");
    c.note("honesty", "Spivak's operad of wiring diagrams (arXiv:1305.0297, Ex. 2.1.7) is undirected: a wiring diagram is a cospan, every port is soldered onto a cable, and filling a hole is a pushout.");
}

fn operad_c4(c: &mut Ctx, area: Rect) {
    let (plan, order, drivers, filters, heur) = plan_lists(c);
    let names: Vec<String> = drivers.iter().map(driver_name).collect();
    // frames: budget outermost, then null_window (game.rs's nesting)
    let mut frame = Rect::from_min_size(
        area.left_top(),
        Vec2::new(area.width(), area.height() - 60.0),
    );
    let mut nest: Vec<String> = vec![];
    for d in ["budget", "null_window"] {
        if names.iter().any(|n| n == d) {
            nest.push(d.into());
        }
    }
    for n in names
        .iter()
        .filter(|n| *n != "budget" && *n != "null_window")
    {
        nest.push(n.clone());
    }
    for d in &nest {
        c.p.rect(
            frame,
            10.0,
            Color32::TRANSPARENT,
            Stroke::new(1.5, Color32::from_rgb(0x17, 0x8f, 0x7f)),
            StrokeKind::Middle,
        );
        c.text(
            frame.left_top() + Vec2::new(8.0, 4.0),
            Align2::LEFT_TOP,
            &format!("driver {d}"),
            11.5,
            Color32::from_rgb(0x17, 0x8f, 0x7f),
        );
        c.hover(Rect::from_min_size(frame.left_top(), Vec2::new(frame.width(), 18.0)), info(format!("driver {d}")).note("Drivers wrap the core (plan.rs reads them into flags; their JSON order does not change the nesting)."));
        c.scene.emit("frame", d, json!({}));
        frame = frame.shrink(22.0);
    }
    let core = frame;
    c.boxr(core, Color32::from_rgb(0xf7, 0xf8, 0xfa), INK, 1.4);
    c.text(
        core.left_top() + Vec2::new(8.0, 4.0),
        Align2::LEFT_TOP,
        "core: typed holes",
        12.0,
        INK,
    );
    let slots = sp::SLOTS;
    let w = (core.width() - 30.0) / 4.0;
    for (i, sl) in slots.iter().enumerate() {
        let (row, col) = (i / 4, i % 4);
        let r = Rect::from_min_size(
            core.left_top()
                + Vec2::new(
                    10.0 + col as f32 * (w + 4.0),
                    28.0 + row as f32 * ((core.height() - 40.0) / 2.0),
                ),
            Vec2::new(w - 4.0, (core.height() - 50.0) / 2.0),
        );
        c.slot(
            r,
            sl.id,
            &format!(
                "{} ({}){}",
                sl.label,
                sl.accepts,
                if sl.designed { " · designed" } else { "" }
            ),
        );
        if sl.id != "drivers" {
            // drivers are frames (above), not a core hole; the drop
            // target stays
            c.scene.emit("hole", sl.id, json!({"accepts": sl.accepts}));
        }
        let entries: Vec<String> = match sl.id {
            "order" => order.clone(),
            "filters" => filters.clone(),
            "heuristics" => heur.clone(),
            "memo" if plan.get("memo").is_some_and(|m| !m.is_null()) => vec!["memo".into()],
            _ => vec![],
        };
        for (j, e) in entries.iter().enumerate() {
            let ch = c.chip(
                r.left_top() + Vec2::new(8.0, 22.0 + j as f32 * 22.0),
                e,
                false,
            );
            if c.clicked && c.pointer.is_some_and(|p| ch.contains(p)) && c.drag.is_none() {
                c.command("plan.remove_piece", json!({"slot": sl.id, "index": j}));
            }
            c.scene.emit("piece", &format!("{}/{e}", sl.id), json!({}));
        }
    }
    palette(c, area);
    plan_warn(c);
    c.note("honesty", "Holes are typed by piece kind, not by wires: a piece has no wires of its own, so this is read as an operad in the sense of Fong-Spivak (Rough Def. 6.91: types, operations, substitution), not as Spivak's cospan operad of wiring diagrams.");
}

// ------------------------------------------------------------ 4 string

fn string_orders(c: &mut Ctx, area: Rect) {
    // instances stacked (⊗), wires leave each box on the right
    let subs = subnets(c);
    let n = subs.len().max(1) as f32;
    let h = ((area.height() - 40.0) / n - 14.0).clamp(60.0, 140.0);
    let bx = area.left() + 200.0;
    let mut wire_end: BTreeMap<String, Pos2> = BTreeMap::new();
    for (i, sub) in subs.iter().enumerate() {
        let id = s(&sub["id"]);
        let r = Rect::from_min_size(
            Pos2::new(bx, area.top() + i as f32 * (h + 14.0)),
            Vec2::new(170.0, h),
        );
        c.boxr(r, pattern_tint(s(&sub["pattern"])), INK, 1.3);
        c.text(r.center(), Align2::CENTER_CENTER, id, 13.0, INK);
        c.hover(r, subnet_info(sub));
        c.scene.emit("box", id, json!({}));
        let ports = arr(&sub["ports"]);
        for (j, p) in ports.iter().enumerate() {
            let y = r.top() + (j as f32 + 1.0) * h / (ports.len() as f32 + 1.0);
            let a = Pos2::new(r.right(), y);
            let b = Pos2::new(r.right() + 140.0, y);
            let sch = s(&p["schema"]);
            let col = if sch.is_empty() {
                TRANS
            } else {
                schema_color(sch)
            };
            c.line(a, b, col, 1.6);
            let key = format!("{id}:{}", s(&p["id"]));
            c.mono(
                b + Vec2::new(4.0, 0.0),
                Align2::LEFT_CENTER,
                &format!(
                    "{} {}",
                    s(&p["id"]),
                    if sch.is_empty() {
                        "(transition)".into()
                    } else {
                        format!(": {sch}")
                    }
                ),
                10.0,
                MUTED,
            );
            c.port(a, id, p, false, true);
            if s(&p["target"]) == "place" {
                c.scene.emit("wire", &key, json!({"schema": sch}));
            }
            wire_end.insert(key, b);
        }
    }
    let ls = links(c);
    for (i, l) in ls.iter().enumerate() {
        let k = s(&l["kind"]);
        let fa = wire_end.get(&format!(
            "{}:{}",
            s(&l["from"]["subnet"]),
            s(&l["from"]["port"])
        ));
        let ta = wire_end.get(&format!(
            "{}:{}",
            s(&l["to"]["subnet"]),
            s(&l["to"]["port"])
        ));
        if let (Some(a), Some(b)) = (fa, ta) {
            let x = area.left() + 620.0 + i as f32 * 26.0;
            let col = kind_color(k);
            c.line(*a + Vec2::new(150.0, 0.0), Pos2::new(x, a.y), col, 1.4);
            c.line(*b + Vec2::new(150.0, 0.0), Pos2::new(x, b.y), col, 1.4);
            c.line(Pos2::new(x, a.y), Pos2::new(x, b.y), col, 1.4);
            let m = Pos2::new(x, (a.y + b.y) / 2.0);
            let r = c.text(m, Align2::CENTER_CENTER, kind_glyph(k), 15.0, col);
            c.hover(r.expand(3.0), link_info(l));
            c.scene.emit("fusion", s(&l["key"]), json!({}));
        }
    }
    c.note("honesty", "Pattern level: instances side by side (⊗, drawn stacked), schema-typed wires, fusion with RenderDOT's glyphs. The transition-level drawing (pflow-xyz diagram-viewer.js) is not ported here; the flattened net is in the Flat net view.");
}

fn string_c4(c: &mut Ctx, area: Rect) {
    let (_, order, drivers, _, _) = plan_lists(c);
    let names: Vec<String> = drivers.iter().map(driver_name).collect();
    let mut frame = Rect::from_min_size(
        area.left_top() + Vec2::new(60.0, 10.0),
        Vec2::new(area.width() - 120.0, area.height() - 90.0),
    );
    for d in ["budget", "null_window"]
        .iter()
        .filter(|d| names.iter().any(|n| n == *d))
    {
        c.p.rect(
            frame,
            8.0,
            Color32::TRANSPARENT,
            Stroke::new(1.4, Color32::from_rgb(0x17, 0x8f, 0x7f)),
            StrokeKind::Middle,
        );
        c.text(
            frame.left_top() + Vec2::new(8.0, 4.0),
            Align2::LEFT_TOP,
            &format!("frame: {d}"),
            11.0,
            Color32::from_rgb(0x17, 0x8f, 0x7f),
        );
        c.scene.emit("frame", d, json!({}));
        frame = frame.shrink(26.0);
    }
    let core = Rect::from_center_size(frame.center(), Vec2::new(220.0, 70.0));
    c.boxr(core, Color32::WHITE, INK, 1.6);
    c.text(
        core.center(),
        Align2::CENTER_CENTER,
        "core: Position → Value⊥",
        13.0,
        INK,
    );
    c.scene.emit("box", "core", json!({}));
    c.line(
        Pos2::new(frame.left(), core.center().y),
        core.left_center(),
        INK,
        1.8,
    );
    c.line(
        core.right_center(),
        Pos2::new(frame.right(), core.center().y),
        INK,
        1.8,
    );
    // the order chain on the parameter wire
    let chain_r = Rect::from_min_size(
        Pos2::new(core.left(), frame.top() + 24.0),
        Vec2::new(core.width(), 24.0 * order.len().max(1) as f32 + 10.0),
    );
    c.slot(
        chain_r.expand2(Vec2::new(60.0, 4.0)),
        "order",
        "order chain",
    );
    for (i, p) in order.iter().enumerate() {
        c.chip(
            chain_r.left_top() + Vec2::new(10.0, 6.0 + i as f32 * 24.0),
            p,
            false,
        );
        c.scene.emit("chain", p, json!({"index": i}));
    }
    c.line(
        chain_r.center_bottom(),
        core.center_top(),
        Color32::from_rgb(0x2f, 0x6f, 0xb3),
        1.4,
    );
    let mut back = vec![
        ("recursion", "recursion: search calls itself on each child"),
        (
            "tt",
            "the transposition table: what a search found, read back",
        ),
        (
            "cutoff",
            "chain.cutoff: a cutoff reported back to the pieces",
        ),
    ];
    if names.iter().any(|n| n == "null_window") {
        back.insert(
            0,
            (
                "nullwindow",
                "null-window loop: yes/no searches, one per gap between rewards",
            ),
        );
    }
    for (i, (id, t)) in back.iter().enumerate() {
        let y = core.bottom() + 20.0 + i as f32 * 16.0;
        c.dashed(
            Pos2::new(core.right(), y),
            Pos2::new(core.left(), y),
            MUTED,
            1.0,
        );
        let r = c.text(
            Pos2::new(core.right() + 8.0, y),
            Align2::LEFT_CENTER,
            t,
            10.5,
            MUTED,
        );
        c.hover(
            r,
            info(format!("back edge: {id}"))
                .cite("pflow-rs crates/pflow-metamodel/src/game.rs (Solver::search)"),
        );
        c.scene.emit("backedge", id, json!({}));
    }
    palette(c, area);
    plan_warn(c);
    c.note("honesty", "The core is fixed; a plan chooses the order chain on its parameter wire and the drivers drawn as frames. Feedback (null-window loop, recursion, the transposition table, cutoffs) is drawn as dashed back edges: a string diagram of a recursive search is a sketch, not a term.");
}

// ------------------------------------------------------------ 5 flat

fn flat_orders(c: &mut Ctx, area: Rect) {
    let Some(flat) = c.d.flat.clone() else { return };
    if flat["ok"] != true {
        c.note(
            "warn",
            format!(
                "Flatten refuses this composition, so there is no flat net to draw: {}",
                s(&flat["error"])
            ),
        );
        return;
    }
    let places = arr(&flat["places"]);
    let trans = arr(&flat["transitions"]);
    let cols = 6usize;
    let net = Rect::from_min_size(
        area.left_top(),
        Vec2::new(area.width() * 0.68, area.height() - 20.0),
    );
    let pos_p: BTreeMap<String, Pos2> = places
        .iter()
        .enumerate()
        .map(|(i, p)| {
            (
                s(&p["id"]).to_string(),
                Pos2::new(
                    net.left()
                        + 40.0
                        + (i % cols) as f32 * (net.width() - 80.0) / (cols - 1) as f32,
                    net.top() + 20.0 + (i / cols) as f32 * 64.0,
                ),
            )
        })
        .collect();
    let ty = net.top() + 20.0 + (places.len().div_ceil(cols)) as f32 * 64.0 + 30.0;
    let pos_t: BTreeMap<String, Pos2> = trans
        .iter()
        .enumerate()
        .map(|(i, t)| {
            (
                s(&t["id"]).to_string(),
                Pos2::new(
                    net.left() + 40.0 + (i % 4) as f32 * (net.width() - 80.0) / 3.0,
                    ty + (i / 4) as f32 * 56.0,
                ),
            )
        })
        .collect();
    for a in arr(&flat["arcs"]) {
        let (f, t) = (s(&a["from"]), s(&a["to"]));
        if let (Some(p), Some(q)) = (pos_p.get(f).or(pos_t.get(f)), pos_t.get(t).or(pos_p.get(t))) {
            let col = if s(&a["type"]) == "read" {
                kind_color("guard")
            } else {
                Color32::from_gray(170)
            };
            c.line(*p, *q, col, 0.8);
        }
    }
    for p in places {
        let id = s(&p["id"]);
        let at = pos_p[id];
        let fused = arr(&p["members"]).len() > 1;
        c.p.circle_filled(at, 11.0, Color32::WHITE);
        c.p.circle_stroke(at, 11.0, Stroke::new(1.2, INK));
        if fused {
            c.p.circle_stroke(at, 14.0, Stroke::new(1.0, INK));
        }
        let n = p["initial"].as_i64().unwrap_or(0);
        if n > 0 {
            c.text(at, Align2::CENTER_CENTER, &n.to_string(), 11.0, INK);
        }
        c.mono(
            at + Vec2::new(0.0, 16.0),
            Align2::CENTER_TOP,
            id,
            8.5,
            MUTED,
        );
        c.hover(
            Rect::from_center_size(at, Vec2::splat(24.0)),
            info(format!("place {id}"))
                .fact("initial", n.to_string())
                .fact("capacity", p["capacity"].to_string())
                .fact("members", strs(&p["members"]).join(", "))
                .note(s(&p["description"])),
        );
        c.scene
            .emit("place", id, json!({"initial": n, "fused": fused}));
    }
    for t in trans {
        let id = s(&t["id"]);
        let at = pos_t[id];
        let fused = t["fused"] == true;
        let r = Rect::from_center_size(at, Vec2::new(18.0, 26.0));
        c.p.rect_filled(r, 1.0, TRANS);
        if fused {
            c.p.rect_stroke(
                r.expand(3.0),
                1.0,
                Stroke::new(1.0, INK),
                StrokeKind::Middle,
            );
        }
        c.mono(
            at + Vec2::new(0.0, 18.0),
            Align2::CENTER_TOP,
            id,
            8.5,
            MUTED,
        );
        let en = arr(&flat["initial_enablement"])
            .iter()
            .find(|e| s(&e["transition"]) == id)
            .cloned()
            .unwrap_or(Value::Null);
        c.hover(
            r.expand(3.0),
            info(format!("transition {id}"))
                .fact("members", strs(&t["members"]).join(", "))
                .fact(
                    "enabled at the initial marking",
                    if en["enabled"] == true {
                        "yes".to_string()
                    } else {
                        s(&en["why"]).to_string()
                    },
                )
                .note(s(&t["description"])),
        );
        c.scene.emit("transition", id, json!({"fused": fused}));
    }
    // scenarios
    let mut y = area.top();
    let x = net.right() + 20.0;
    c.text(
        Pos2::new(x, y),
        Align2::LEFT_TOP,
        "scenarios (judgment, frozen):",
        12.0,
        INK,
    );
    y += 20.0;
    for r in c.d.runs.clone() {
        let pass = arr(&r["expect"]).iter().all(|e| e["pass"] == true);
        let stopped = s(&r["stopped_step"]).to_string();
        let line = format!(
            "{} {} — {}{}",
            if pass { "PASS" } else { "FAIL" },
            s(&r["id"]),
            s(&r["title"]),
            if stopped.is_empty() {
                String::new()
            } else {
                format!(" (stops at {stopped})")
            }
        );
        let rr = c.text(
            Pos2::new(x, y),
            Align2::LEFT_TOP,
            &line,
            11.5,
            if pass {
                Color32::from_rgb(0x2e, 0x8b, 0x57)
            } else {
                LEAK
            },
        );
        let mut i = info(format!("scenario {}", s(&r["id"])));
        for st in arr(&r["steps"]) {
            i = i.fact(
                s(&st["member"]),
                if st["enabled"] == true {
                    s(&st["flat"]).to_string()
                } else {
                    format!("refused: {}", s(&st["why"]))
                },
            );
        }
        for e in arr(&r["expect"]) {
            i = i.fact(
                &format!("{} == {}", s(&e["place"]), e["n"]),
                format!(
                    "{} ({})",
                    e["actual"],
                    if e["pass"] == true { "PASS" } else { "FAIL" }
                ),
            );
        }
        c.hover(
            rr,
            i.note("Fired through pflow-metamodel's enabled/fire on the flat net (bundle.run)."),
        );
        c.scene.emit(
            "scenario",
            s(&r["id"]),
            json!({"pass": pass, "stopped_step": stopped}),
        );
        y += 18.0;
    }
    c.note("na", "No ports are drawn here: Flatten removes the boundary, and a flat net has only places, transitions and arcs. Links are made in the pattern-level views; this view shows the one net they produce, under go-pflow's flat IDs.");
    c.note("judgment", "The scenarios and their expected predicates are judgment, written from the description and frozen (ROADMAP Phase 4).");
}

fn flat_c4(c: &mut Ctx, area: Rect) {
    let (pl, tr, ar) = net_counts(c);
    let r = Rect::from_min_size(
        area.left_top(),
        Vec2::new(area.width(), area.height() - 30.0),
    );
    draw_c4_net(c, r);
    c.hover(
        r,
        info("connect-4")
            .fact("places", pl.to_string())
            .fact("transitions", tr.to_string())
            .fact("arcs", ar.to_string()),
    );
    c.scene.emit(
        "net",
        "connect-4",
        json!({"places": pl, "transitions": tr, "arcs": ar}),
    );
    c.note("na", "A search plan is not a net, so Flatten has nothing to flatten and no plan slot is drawn here. The net the plan searches (pflow-rs's fixture connect-4.json) is drawn at its own coordinates.");
}

// ------------------------------------------------------------ 6 boundaries

fn boundaries_orders(c: &mut Ctx, area: Rect) {
    let Some(flat) = c.d.flat.clone() else { return };
    if flat["ok"] != true {
        c.note(
            "warn",
            format!(
                "Flatten refused the links ({}); nothing to synchronise.",
                s(&flat["error"])
            ),
        );
        return;
    }
    let groups: Vec<(String, Vec<String>)> = flat["fused_groups"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, v)| (k.clone(), strs(v)))
        .collect();
    let bx = area.left() + area.width() * 0.45;
    c.dashed(
        Pos2::new(bx, area.top()),
        Pos2::new(bx, area.bottom() - 30.0),
        kind_color("event"),
        1.5,
    );
    c.text(
        Pos2::new(bx, area.top()),
        Align2::CENTER_BOTTOM,
        "shared boundary",
        11.0,
        MUTED,
    );
    let mut y = area.top() + 20.0;
    for (i, (id, members)) in groups.iter().enumerate() {
        let letter = (b'A' + i as u8) as char;
        let port = Pos2::new(bx, y + members.len() as f32 * 13.0);
        for (j, m) in members.iter().enumerate() {
            let my = y + j as f32 * 26.0 + 10.0;
            let r = Rect::from_min_size(Pos2::new(area.left(), my - 10.0), Vec2::new(230.0, 20.0));
            c.boxr(r, Color32::WHITE, MUTED, 1.0);
            c.mono(r.center(), Align2::CENTER_CENTER, m, 10.5, INK);
            c.line(r.right_center(), port, kind_color("event"), 1.4);
            c.hover(r, info(format!("{m} is wired to boundary port {letter}")));
        }
        c.p.rect_filled(
            Rect::from_center_size(port, Vec2::splat(12.0)),
            1.0,
            kind_color("event"),
        );
        c.text(
            port + Vec2::new(10.0, -10.0),
            Align2::LEFT_BOTTOM,
            &letter.to_string(),
            12.0,
            INK,
        );
        c.hover(
            Rect::from_center_size(port, Vec2::splat(14.0)),
            info(format!(
                "boundary port {letter}: one event class, {} members",
                members.len()
            ))
            .fact("class", id.as_str()),
        );
        c.scene.emit("boundary", id, json!({"legs": members}));
        let card = Rect::from_min_size(
            Pos2::new(bx + 40.0, port.y - 16.0),
            Vec2::new(area.right() - bx - 50.0, 32.0),
        );
        c.boxr(
            card,
            Color32::from_rgb(0xee, 0xf6, 0xf0),
            kind_color("event"),
            1.0,
        );
        c.mono(
            card.left_center() + Vec2::new(8.0, 0.0),
            Align2::LEFT_CENTER,
            id,
            10.5,
            INK,
        );
        c.line(port, card.left_center(), kind_color("event"), 1.0);
        c.hover(card, info(format!("composite {id}")).fact("members", members.join(", ")).note("The rendezvous rule: the class becomes one transition whose pre/postset is the union of its members'; it fires only when every member can."));
        c.scene.emit("composite", id, json!({"members": members}));
        y += members.len() as f32 * 26.0 + 30.0;
    }
    if groups.is_empty() {
        c.text(
            Pos2::new(bx + 20.0, area.top() + 20.0),
            Align2::LEFT_TOP,
            "an empty shared boundary: every transition is its own minimal synchronisation",
            12.0,
            MUTED,
        );
    }
    c.note("honesty", "Honesty: Bruni et al. compose two nets along one boundary; go-pflow's event classes are n-ary (union-find over binary links). Each class is drawn as one boundary port, and the composite transitions are exactly Flatten's fused groups.");
}

fn boundaries_c4(c: &mut Ctx, area: Rect) {
    let r = Rect::from_min_size(
        area.left_top() + Vec2::new(area.width() * 0.25, 40.0),
        Vec2::new(area.width() * 0.5, area.height() * 0.6),
    );
    c.boxr(r, Color32::WHITE, MUTED, 1.0);
    draw_c4_net(c, r.shrink(10.0));
    c.text(
        r.center_bottom() + Vec2::new(0.0, 10.0),
        Align2::CENTER_TOP,
        "connect-4: a closed net, 0 boundary ports on either side",
        12.5,
        INK,
    );
    c.scene.emit("na", "boundaries", json!({}));
    c.note("na", "Does not apply: a search plan composes pieces into slots (plan.rs's check decides); nothing synchronises transitions across a boundary. And the Connect-4 net is closed: it has no ports, so as a net with boundaries it is one with both boundaries empty.");
}

// ------------------------------------------------------------ 7 lens

fn lens_orders(c: &mut Ctx, area: Rect, mode: &str) {
    let Some(rep) = c.d.lens.clone() else { return };
    let a = &rep["analysis"];
    // the mode toggle (a registry command)
    let t1 = Rect::from_min_size(area.left_top(), Vec2::new(90.0, 22.0));
    let t2 = Rect::from_min_size(
        area.left_top() + Vec2::new(96.0, 0.0),
        Vec2::new(150.0, 22.0),
    );
    c.button(
        t1,
        "strict",
        mode == "strict",
        "view.composition",
        json!({"lens": "strict"}),
    );
    c.button(
        t2,
        "assume a backward pass",
        mode == "assume",
        "view.composition",
        json!({"lens": "assume"}),
    );
    let holds = a["holds"] == true;
    let legal_token = a["legal"]["token"].as_i64().unwrap_or(0);
    let legal_data = a["legal"]["data"].as_i64().unwrap_or(0);
    let glued = a["glued"].as_i64().unwrap_or(0);
    let mut y = area.top() + 36.0;
    let v = c.text(Pos2::new(area.left(), y), Align2::LEFT_TOP, &format!("strict lens test: {} — {} boundary glued (legal: {legal_token} token, {legal_data} data links)", if holds { "a lens composite" } else { "not a lens composite" }, glued), 13.5, if holds { Color32::from_rgb(0x2e, 0x8b, 0x57) } else { LEAK });
    c.hover(v, info("the strict test").fact("holds", holds.to_string()).fact("glued", glued.to_string()).fact("no backward pass", strs(&a["no_backward"]).join(", ")).note("Lens composition is sequential: it needs a boundary to compose along, a backward map in every part, and the parts' backward maps to fire together as the forward pass does."));
    c.scene.emit("verdict", "strict", json!({"holds": holds, "glued": glued, "legal_token": legal_token, "legal_data": legal_data}));
    y += 28.0;
    let assume = mode == "assume";
    let colw = area.width() / 3.0 - 10.0;
    // pairs (the fallback's lenses)
    let x0 = area.left();
    c.text(
        Pos2::new(x0, y),
        Align2::LEFT_TOP,
        "backward pairs (found mechanically)",
        12.0,
        INK,
    );
    let mut py = y + 20.0;
    for p in arr(&a["pairs"]) {
        let id = format!("{}/{}->{}", s(&p["subnet"]), s(&p["t"]), s(&p["u"]));
        let r = Rect::from_min_size(
            Pos2::new(x0, py),
            Vec2::new(colw, if assume { 34.0 } else { 18.0 }),
        );
        if assume {
            c.boxr(r, Color32::from_rgb(0xf1, 0xea, 0xf7), MUTED, 1.0);
            c.mono(
                r.left_top() + Vec2::new(6.0, 3.0),
                Align2::LEFT_TOP,
                &format!("→ {} (forward)", s(&p["t"])),
                10.5,
                INK,
            );
            c.mono(
                r.left_top() + Vec2::new(6.0, 18.0),
                Align2::LEFT_TOP,
                &format!("← {} (backward)", s(&p["u"])),
                10.5,
                MUTED,
            );
        } else {
            c.mono(r.left_top(), Align2::LEFT_TOP, &id, 10.5, INK);
        }
        c.hover(
            r,
            info(format!("pair {id}"))
                .fact("residuals", strs(&p["residuals"]).join(", "))
                .fact("resources", strs(&p["resources"]).join(", "))
                .fact("exact inverse", p["exact"].to_string()),
        );
        c.scene.emit("pair", &id, json!({}));
        py = r.bottom() + 4.0;
    }
    // steps
    let x1 = area.left() + colw + 20.0;
    c.text(
        Pos2::new(x1, y),
        Align2::LEFT_TOP,
        "forward pass of the commit run, step by step",
        12.0,
        INK,
    );
    let mut sy = y + 20.0;
    for st in arr(&a["steps"]) {
        let id = s(&st["flat"]);
        let composes = st["composes"] == true;
        let split = st["split"] == true;
        let dead = st["dead_forward"]["constraint"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let r = Rect::from_min_size(Pos2::new(x1, sy), Vec2::new(colw, 36.0));
        c.boxr(
            r,
            Color32::WHITE,
            if composes {
                Color32::from_rgb(0x2e, 0x8b, 0x57)
            } else {
                LEAK
            },
            if composes { 1.0 } else { 2.0 },
        );
        c.mono(
            r.left_top() + Vec2::new(6.0, 3.0),
            Align2::LEFT_TOP,
            id,
            10.0,
            INK,
        );
        c.text(
            r.left_top() + Vec2::new(6.0, 19.0),
            Align2::LEFT_TOP,
            &format!(
                "{}{}{}",
                if composes {
                    "composes"
                } else {
                    "does not compose"
                },
                if split { " · backward maps split" } else { "" },
                if dead.is_empty() {
                    String::new()
                } else {
                    format!(" · dead by {dead}")
                }
            ),
            10.5,
            MUTED,
        );
        let back: Vec<String> = arr(&st["back"])
            .iter()
            .map(|b| s(&b["u"]).to_string())
            .collect();
        c.hover(
            r,
            info(format!("step {id}"))
                .fact("members", strs(&st["members"]).join(", "))
                .fact("backward maps", back.join(", "))
                .fact("composes", composes.to_string()),
        );
        c.scene.emit(
            "step",
            id,
            json!({"composes": composes, "split": split, "dead_forward": dead}),
        );
        sy = r.bottom() + 6.0;
    }
    // fixes
    let x2 = area.left() + 2.0 * (colw + 20.0);
    c.text(
        Pos2::new(x2, y),
        Align2::LEFT_TOP,
        "one-link repairs, each re-run",
        12.0,
        INK,
    );
    let mut fy = y + 20.0;
    for f in arr(&rep["fixes"]) {
        let id = format!("{}:{}", s(&f["how"]), s(&f["u"]));
        let fh = f["result"]["holds"] == true;
        let r = c.mono(
            Pos2::new(x2, fy),
            Align2::LEFT_TOP,
            &format!(
                "{id} ⊗ {} → {}",
                s(&f["target"]),
                if f["legal"] != true {
                    "refused"
                } else if fh {
                    "a lens"
                } else {
                    "not a lens"
                }
            ),
            10.5,
            INK,
        );
        let mut i = info(format!("repair {id}"))
            .fact("link", f["link"].to_string())
            .fact("legal", f["legal"].to_string());
        for run in arr(&f["result"]["runs"]) {
            i = i.fact(
                s(&run["id"]),
                if s(&run["stopped_step"]).is_empty() {
                    format!(
                        "{} predicates pass",
                        arr(&run["expect"])
                            .iter()
                            .filter(|e| e["pass"] == true)
                            .count()
                    )
                } else {
                    format!("stops at {}", s(&run["stopped_step"]))
                },
            );
        }
        c.hover(r, i);
        c.scene.emit("fix", &id, json!({"holds": fh}));
        fy += 18.0;
    }
    if assume {
        c.note("judgment", "Assumption (fallback): the pairs drawn as lenses were found mechanically (a residual only t produces and only u consumes; u returns what t moved on resource places), and the forward pass is the \"commit\" scenario's run (judgment).");
    } else {
        c.note("na", "Strict test: the composition as composed, no assumption. Switch to the fallback to see the pairs that do have a backward pass drawn as lenses.");
    }
    c.note("analogy", "Open games / lenses (Ghani, Hedges, Winschel, Zahn): forward play, backward values. Used here as a test of shape on the flat net, not as game theory.");
}

fn lens_c4(c: &mut Ctx, area: Rect) {
    let (plan, order, drivers, _, _) = plan_lists(c);
    let names: Vec<String> = drivers.iter().map(driver_name).collect();
    let mut y = area.top();
    for d in &names {
        let r = Rect::from_min_size(
            Pos2::new(area.left(), y),
            Vec2::new(area.width() * 0.6, 26.0),
        );
        c.boxr(
            r,
            Color32::from_rgb(0xe8, 0xf5, 0xf2),
            Color32::from_rgb(0x17, 0x8f, 0x7f),
            1.2,
        );
        c.text(
            r.left_center() + Vec2::new(8.0, 0.0),
            Align2::LEFT_CENTER,
            &format!("driver {d}: positions forward, value back"),
            11.5,
            INK,
        );
        c.hover(
            r,
            info(format!("driver {d}"))
                .note("A frame around the lenses below (drivers wrap the core)."),
        );
        c.scene.emit("frame", d, json!({}));
        y += 32.0;
    }
    for p in &order {
        let r = Rect::from_min_size(
            Pos2::new(area.left() + 30.0, y),
            Vec2::new(area.width() * 0.55, 30.0),
        );
        c.boxr(r, Color32::WHITE, Color32::from_rgb(0x2f, 0x6f, 0xb3), 1.2);
        c.mono(
            r.left_center() + Vec2::new(8.0, 0.0),
            Align2::LEFT_CENTER,
            &format!("{p}: ↓ position · ↑ value"),
            11.0,
            INK,
        );
        c.hover(
            r,
            info(format!("lens {p}")).note(
                "An order piece changes how soon a cutoff comes, never the value (order.rs:1-7).",
            ),
        );
        c.scene.emit("lens", p, json!({}));
        y += 36.0;
    }
    let memo = plan.get("memo").is_some_and(|m| !m.is_null());
    let r = c.text(
        Pos2::new(area.left(), y + 8.0),
        Align2::LEFT_TOP,
        &if memo {
            format!(
                "memo: {} entries (the transposition table, shared by every lens above)",
                plan["memo"]["entries"]
            )
        } else {
            "memo: none (the table is unbounded within one search)".into()
        },
        11.5,
        MUTED,
    );
    c.hover(r, info("memo").fact("present", memo.to_string()));
    c.scene.emit("memo", "memo", json!({"present": memo}));
    plan_warn(c);
    c.note("analogy", "Analogy of shape only. Open games (Ghani, Hedges, Winschel, Zahn) characterise equilibria: a best-response condition on strategy profiles. This plan computes a value: minimax with alpha-beta under a node budget. The lens drawing borrows only the shape (positions forward down the tree, values back up); no equilibrium is computed or checked.");
}

// ------------------------------------------------------------ 8 incidence

fn heat(c: &mut Ctx, r: Rect, m: &[Value], max_cell: f32) -> Rect {
    let rows = m.len().max(1);
    let cols = m.first().map_or(1, |x| arr(x).len().max(1));
    let cell = (r.width() / cols as f32)
        .min(r.height() / rows as f32)
        .min(max_cell);
    for (i, row) in m.iter().enumerate() {
        for (j, v) in arr(row).iter().enumerate() {
            let x = v.as_i64().unwrap_or(0);
            if x == 0 {
                continue;
            }
            let col = if x > 0 {
                Color32::from_rgb(0x2e, 0x8b, 0x57)
            } else {
                LEAK
            };
            c.p.rect_filled(
                Rect::from_min_size(
                    r.left_top() + Vec2::new(j as f32 * cell, i as f32 * cell),
                    Vec2::splat(cell.max(1.0)),
                ),
                0.0,
                col,
            );
        }
    }
    let used = Rect::from_min_size(
        r.left_top(),
        Vec2::new(cols as f32 * cell, rows as f32 * cell),
    );
    c.p.rect_stroke(used, 0.0, Stroke::new(0.8, MUTED), StrokeKind::Middle);
    used
}

fn incidence_orders(c: &mut Ctx, area: Rect) {
    let Some(inc) = c.d.incidence.clone() else {
        return;
    };
    if inc["ok"] != true {
        c.note(
            "warn",
            format!(
                "Flatten refused the composition ({}), so there is no flat matrix.",
                s(&inc["error"])
            ),
        );
        return;
    }
    // disjoint union (block diagonal) beside the flat matrix
    let left = Rect::from_min_size(
        area.left_top() + Vec2::new(0.0, 20.0),
        Vec2::new(area.width() * 0.45, area.height() * 0.55),
    );
    c.text(
        area.left_top(),
        Align2::LEFT_TOP,
        "⊗: the subnets' blocks (block diagonal)",
        12.0,
        INK,
    );
    let mut off = left.left_top();
    for b in arr(&inc["blocks"]) {
        let rows = arr(&b["rows"]).len();
        let cols = arr(&b["cols"]).len();
        let used = heat(
            c,
            Rect::from_min_size(off, Vec2::new(cols as f32 * 10.0, rows as f32 * 10.0)),
            arr(&b["matrix"]),
            10.0,
        );
        c.hover(
            used,
            info(format!("block {}", s(&b["subnet"])))
                .fact("rows (places)", rows.to_string())
                .fact("cols (transitions)", cols.to_string()),
        );
        c.scene.emit(
            "block",
            s(&b["subnet"]),
            json!({"rows": rows, "cols": cols}),
        );
        off = used.right_bottom();
    }
    let right = Rect::from_min_size(
        Pos2::new(area.left() + area.width() * 0.5, area.top() + 20.0),
        Vec2::new(area.width() * 0.48, area.height() * 0.55),
    );
    c.text(
        right.left_top() - Vec2::new(0.0, 20.0),
        Align2::LEFT_TOP,
        "the flat net's C = post − pre (event fusion sums columns, token fusion sums rows)",
        12.0,
        INK,
    );
    let (rows, cols) = (arr(&inc["rows"]).len(), arr(&inc["cols"]).len());
    let used = heat(c, right, arr(&inc["matrix"]), 12.0);
    c.hover(
        used,
        info("the flat matrix")
            .fact("rows", rows.to_string())
            .fact("cols", cols.to_string()),
    );
    c.scene
        .emit("matrix", "flat", json!({"rows": rows, "cols": cols}));
    let mut y = area.top() + area.height() * 0.62;
    c.text(
        Pos2::new(area.left(), y),
        Align2::LEFT_TOP,
        "declared invariants, checked as y·C = 0:",
        12.0,
        INK,
    );
    y += 20.0;
    for k in arr(&inc["invariants"]) {
        let holds = k["holds"] == true;
        let r = c.mono(
            Pos2::new(area.left(), y),
            Align2::LEFT_TOP,
            &format!(
                "{} {} : {}",
                if holds { "✓" } else { "✗" },
                s(&k["id"]),
                s(&k["expr"])
            ),
            10.5,
            if holds { INK } else { LEAK },
        );
        c.hover(
            r,
            info(format!("invariant {}", s(&k["id"])))
                .fact("holds", holds.to_string())
                .note(
                    "Computed by bundle.incidence: y has a 1 on each place the expression names.",
                ),
        );
        c.scene
            .emit("invariant", s(&k["id"]), json!({"holds": holds}));
        y += 16.0;
    }
    c.note("na", "No ports here: an incidence matrix has no boundary. Make links in a view that draws ports (Cospans, Spiders, …); this view recomputes both matrices from the links.");
}

fn incidence_c4(c: &mut Ctx, area: Rect) {
    let (pl, tr, _) = net_counts(c);
    let m = sp::connect4_model()
        .map(comp::incidence)
        .unwrap_or_default();
    let mv: Vec<Value> = m.iter().map(|r| json!(r)).collect();
    let used = heat(
        c,
        Rect::from_min_size(
            area.left_top(),
            Vec2::new(area.width(), area.height() - 30.0),
        ),
        &mv,
        6.0,
    );
    c.hover(
        used,
        info("connect-4's incidence matrix")
            .fact("rows (places)", pl.to_string())
            .fact("cols (transitions)", tr.to_string()),
    );
    c.scene
        .emit("matrix", "connect-4", json!({"rows": pl, "cols": tr}));
    c.note("na", "A SearchPlan has no incidence matrix: it is a composite of search pieces (plan.rs), not a P/T net, and its pieces have no places or transitions to put in one. Drawn instead, as the nearest honest thing: the incidence matrix of the net the plan searches.");
}

// ------------------------------------------------------------ 9 process

fn process_orders(c: &mut Ctx, area: Rect) {
    let mut y = area.top();
    for r in c.d.runs.clone() {
        let id = s(&r["id"]).to_string();
        c.text(
            Pos2::new(area.left(), y),
            Align2::LEFT_TOP,
            &format!("{id}: {}", s(&r["title"])),
            12.0,
            INK,
        );
        y += 18.0;
        let mut x = area.left();
        let mut fired: Vec<String> = vec![];
        for st in arr(&r["steps"]) {
            let ok = st["enabled"] == true;
            let label = if ok {
                s(&st["flat"]).to_string()
            } else {
                s(&st["member"]).to_string()
            };
            let w = (label.len() as f32 * 6.0 + 12.0).min(330.0);
            let b = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, 24.0));
            c.boxr(
                b,
                if ok {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(0xfb, 0xe9, 0xe7)
                },
                if ok { INK } else { LEAK },
                1.0,
            );
            c.mono(b.center(), Align2::CENTER_CENTER, &label, 9.5, INK);
            c.hover(
                b,
                info(format!("step {}", s(&st["member"])))
                    .fact("flat", s(&st["flat"]))
                    .fact("why not", s(&st["why"])),
            );
            if ok {
                fired.push(label.clone());
            }
            if x > area.left() {
                c.line(Pos2::new(x - 8.0, y + 12.0), b.left_center(), MUTED, 1.0);
            }
            x = b.right() + 8.0;
        }
        let leak = arr(&r["expect"])
            .iter()
            .any(|e| s(&e["place"]) == "workers/available" && e["pass"] != true);
        if leak {
            c.text(
                Pos2::new(x + 6.0, y + 12.0),
                Align2::LEFT_CENTER,
                "a worker never comes back",
                11.0,
                LEAK,
            );
        }
        let stopped = s(&r["stopped_step"]).to_string();
        fired.sort();
        c.scene
            .emit("run", &id, json!({"fired": fired, "stopped_step": stopped}));
        y += 40.0;
    }
    c.note("honesty", "Individual tokens are a choice here: the net itself does not order tokens. The collective-token reading (Petri nets generate commutative monoidal categories) identifies runs that differ only in which token was used; Baez, Genovese, Master and Shulman treat both readings (Σ-nets).");
}

fn process_c4(c: &mut Ctx, area: Rect) {
    let op = c.d.opening.clone();
    let mut x = area.left();
    for st in arr(&op["steps"]) {
        let b = Rect::from_min_size(Pos2::new(x, area.top() + 20.0), Vec2::new(120.0, 30.0));
        let ok = st["enabled"] == true;
        c.boxr(b, Color32::WHITE, if ok { INK } else { LEAK }, 1.0);
        c.mono(
            b.center(),
            Align2::CENTER_CENTER,
            s(&st["transition"]),
            10.5,
            INK,
        );
        c.hover(
            b,
            info(format!("drop {}", s(&st["transition"])))
                .fact("enabled", ok.to_string())
                .fact("why not", s(&st["why"])),
        );
        x = b.right() + 14.0;
    }
    let fired = op["fired"].as_i64().unwrap_or(0);
    c.scene.emit("run", "opening", json!({"fired": fired}));
    c.note("na", "No search is run and the search plan is not drawn here, so the plan pieces have no slot to snap into in this view. The plan (plan.rs) computes one value over all plays from a position; one play is one process. This is a fixed four-drop opening, each drop checked enabled by the firing rule (plan.opening).");
}

// ------------------------------------------------------------ 10 laws

fn chyp(name: &str) -> &'static Value {
    use std::sync::OnceLock;
    static T: OnceLock<BTreeMap<&'static str, Value>> = OnceLock::new();
    let t = T.get_or_init(|| {
        let src: [(&str, &str); 4] = [
            (
                "orderer-monoid",
                include_str!("../assets/chyp/orderer-monoid.json"),
            ),
            (
                "filter-meet",
                include_str!("../assets/chyp/filter-meet.json"),
            ),
            (
                "driver-wrap",
                include_str!("../assets/chyp/driver-wrap.json"),
            ),
            (
                "sync-spiders",
                include_str!("../assets/chyp/sync-spiders.json"),
            ),
        ];
        src.iter()
            .map(|(k, v)| (*k, serde_json::from_str(v).unwrap_or(Value::Null)))
            .collect()
    });
    t.get(name).unwrap_or(&Value::Null)
}

/// The rewrites the live link order connects to (`liveDefaultRewrite`,
/// laws.js:1029): L1 = orders:dequeue ⊗ workers:acquire, L2 =
/// orders:dequeue ⊗ payment:do:reserve, L3 = workers:release ⊗
/// payment:commit. `fuse_L1_L2` or `fuse_L2_L1` by the order of L1 and L2
/// among the event links, and `L3_position_irrelevant` when all three are
/// there.
fn covers(c: &Ctx) -> Vec<&'static str> {
    let ends = |l: &Value| {
        let mut v = [
            format!("{}:{}", s(&l["from"]["subnet"]), s(&l["from"]["port"])),
            format!("{}:{}", s(&l["to"]["subnet"]), s(&l["to"]["port"])),
        ];
        v.sort();
        v.join("~")
    };
    let ev: Vec<String> = links(c)
        .iter()
        .filter(|l| s(&l["kind"]) == "event")
        .map(ends)
        .collect();
    let at = |k: &str| ev.iter().position(|e| e == k);
    let (l1, l2, l3) = (
        at("orders:dequeue~workers:acquire"),
        at("orders:dequeue~payment:do:reserve"),
        at("payment:commit~workers:release"),
    );
    let mut out = vec![];
    if let (Some(i1), Some(i2)) = (l1, l2) {
        out.push(if i1 < i2 { "fuse_L1_L2" } else { "fuse_L2_L1" });
        if l3.is_some() {
            out.push("L3_position_irrelevant");
        }
    }
    out
}

/// The rewrite each theory opens on when none is chosen (laws.js DEFAULT_RW).
fn default_rewrite(theory: &str) -> &'static str {
    match theory {
        "orderer-monoid" => "doc_chain_bracketing",
        "filter-meet" => "meet_repeat_absorbed",
        "driver-wrap" => "json_driver_order_irrelevant",
        _ => "fuse_L1_L2",
    }
}

fn draw_graph(c: &mut Ctx, g: &Value, r: Rect) {
    let verts = arr(&g["vertices"]);
    let edges = arr(&g["edges"]);
    let xs = verts
        .iter()
        .map(|v| v["x"].as_f64().unwrap_or(0.0))
        .chain(edges.iter().map(|e| e["x"].as_f64().unwrap_or(0.0)));
    let ys = verts
        .iter()
        .map(|v| v["y"].as_f64().unwrap_or(0.0))
        .chain(edges.iter().map(|e| e["y"].as_f64().unwrap_or(0.0)));
    let (x0, x1) = xs.fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
    let (y0, y1) = ys.fold((f64::MAX, f64::MIN), |(a, b), y| (a.min(y), b.max(y)));
    let sc = (((r.width() - 20.0) as f64) / (x1 - x0).max(1.0))
        .min(((r.height() - 20.0) as f64) / (y1 - y0).max(1.0))
        .min(50.0);
    let at = |x: f64, y: f64| {
        Pos2::new(
            r.left() + 10.0 + ((x - x0) * sc) as f32,
            r.top() + 10.0 + ((y - y0) * sc) as f32,
        )
    };
    let vpos: BTreeMap<&str, Pos2> = verts
        .iter()
        .map(|v| {
            (
                s(&v["id"]),
                at(
                    v["x"].as_f64().unwrap_or(0.0),
                    v["y"].as_f64().unwrap_or(0.0),
                ),
            )
        })
        .collect();
    for e in edges {
        let ep = at(
            e["x"].as_f64().unwrap_or(0.0),
            e["y"].as_f64().unwrap_or(0.0),
        );
        for v in arr(&e["inputs"]).iter().chain(arr(&e["outputs"])) {
            if let Some(p) = vpos.get(s(v)) {
                c.curve(*p, ep, INK, 1.2);
            }
        }
    }
    for e in edges {
        let ep = at(
            e["x"].as_f64().unwrap_or(0.0),
            e["y"].as_f64().unwrap_or(0.0),
        );
        let size = Vec2::new(
            (e["w"].as_f64().unwrap_or(1.1) * sc) as f32,
            (e["h"].as_f64().unwrap_or(0.8) * sc) as f32,
        );
        let b = Rect::from_center_size(ep, size.max(Vec2::splat(14.0)));
        c.boxr(b, Color32::WHITE, INK, 1.0);
        c.text(b.center(), Align2::CENTER_CENTER, s(&e["label"]), 10.0, INK);
    }
}

fn laws(c: &mut Ctx, area: Rect, orders: bool, rewrite: Option<&str>) {
    let theories: &[&str] = if orders {
        &["sync-spiders"]
    } else {
        &["orderer-monoid", "filter-meet", "driver-wrap"]
    };
    // the selected theory: the one holding the chosen rewrite, else the first
    let selected = rewrite
        .and_then(|w| {
            theories.iter().copied().find(|t| {
                arr(&chyp(t)["check"]["rewrites"])
                    .iter()
                    .any(|r| s(&r["name"]) == w)
            })
        })
        .unwrap_or(theories[0]);
    let live = if orders { covers(c) } else { vec![] };
    // theory tabs (view.composition {rewrite} picks one)
    let mut x = area.left();
    for t in theories {
        let r = Rect::from_min_size(Pos2::new(x, area.top()), Vec2::new(150.0, 22.0));
        let first = s(&arr(&chyp(t)["check"]["rewrites"])
            .first()
            .cloned()
            .unwrap_or(Value::Null)["name"])
        .to_string();
        let pick = if *t == selected {
            ""
        } else {
            default_rewrite(t)
        };
        let pick = if pick.is_empty() {
            first.as_str()
        } else {
            pick
        };
        c.button(
            r,
            t,
            *t == selected,
            "view.composition",
            json!({"rewrite": pick}),
        );
        c.scene.emit("theory", t, json!({}));
        x = r.right() + 8.0;
    }
    let th = chyp(selected);
    let shown = rewrite
        .filter(|w| {
            arr(&th["check"]["rewrites"])
                .iter()
                .any(|r| s(&r["name"]) == *w)
        })
        .map(str::to_string)
        .or_else(|| live.first().map(|x| x.to_string()))
        .unwrap_or_else(|| default_rewrite(selected).to_string());
    let mut y = area.top() + 34.0;
    for rw in arr(&th["check"]["rewrites"]) {
        let name = s(&rw["name"]);
        let status = s(&rw["status"]);
        let col = match status {
            "valid" => Color32::from_rgb(0x2e, 0x8b, 0x57),
            "invalid" => LEAK,
            _ => LIT,
        };
        let is_live = live.contains(&name);
        let r = c.mono(
            Pos2::new(area.left(), y),
            Align2::LEFT_TOP,
            &format!(
                "{} {name}{}{}",
                if status == "valid" { "✓" } else { "✗" },
                if is_live { "  ← live link order" } else { "" },
                if name == shown { "  (shown)" } else { "" }
            ),
            10.5,
            col,
        );
        c.hover(
            r,
            info(format!("rewrite {name}"))
                .fact("status (Chyp's checker)", status)
                .fact("expected", s(&rw["expected"]))
                .fact("line", rw["line"].to_string())
                .note("Precomputed: Chyp 3d28483 checked it when the export was made; nothing is re-checked here.")
                .cite(format!("chyp/{selected}.chyp:{}", rw["line"])),
        );
        if c.clicked && c.pointer.is_some_and(|p| r.contains(p)) {
            c.command("view.composition", json!({"rewrite": name}));
        }
        c.scene.emit("rewrite", name, json!({"status": status}));
        y += 16.0;
    }
    for l in &live {
        c.scene.emit("cover", l, json!({}));
    }
    if let Some(d) = arr(&th["diagrams"]["rewrites"])
        .iter()
        .find(|d| s(&d["name"]) == shown)
        .cloned()
    {
        let gx = area.left() + area.width() * 0.4;
        let graphs = arr(&d["graphs"]);
        c.text(
            Pos2::new(gx, area.top() + 34.0),
            Align2::LEFT_TOP,
            &format!(
                "{}: first and last term of its rewrite (Chyp's layout)",
                s(&d["name"])
            ),
            12.0,
            INK,
        );
        let w = (area.right() - gx) / 2.0 - 10.0;
        if let Some(g0) = graphs.first() {
            let r = Rect::from_min_size(
                Pos2::new(gx, area.top() + 58.0),
                Vec2::new(w, area.height() * 0.45),
            );
            draw_graph(c, g0, r);
            c.mono(
                r.center_bottom(),
                Align2::CENTER_TOP,
                s(&g0["term"]),
                9.5,
                MUTED,
            );
        }
        if let Some(g1) = graphs.last() {
            let r = Rect::from_min_size(
                Pos2::new(gx + w + 20.0, area.top() + 58.0),
                Vec2::new(w, area.height() * 0.45),
            );
            c.text(
                Pos2::new(gx + w + 10.0, r.center().y),
                Align2::CENTER_CENTER,
                "=",
                20.0,
                INK,
            );
            draw_graph(c, g1, r);
            c.mono(
                r.center_bottom(),
                Align2::CENTER_TOP,
                s(&g1["term"]),
                9.5,
                MUTED,
            );
        }
    }
    c.note("honesty", "Precomputed: these verdicts and diagrams are the export of Chyp 3d28483 (chyp_headless, Apache-2.0, crates/app/assets/chyp/), made when the theories were checked. pflow-native does not run or reimplement Chyp's checker. Chyp checked every rewrite from its file's rules; it did not check the rules. Those (ASSUMED) are readings of the source lines each file cites.");
    if !orders {
        plan_warn(c);
    }
}

// ------------------------------------------------------------ the panel

/// The whole views panel inside the app: tabs, example switch, the view,
/// the inspector and the notes.
pub fn panel(app_state: &mut State, sess: &Session, ui: &mut egui::Ui) -> Vec<Step> {
    app_state.data.refresh(sess);
    let cv = sess.composition_view.clone();
    let mut out: Vec<Step> = vec![];
    ui.horizontal_wrapped(|ui| {
        for (i, (id, label, _)) in VIEWS.iter().enumerate() {
            if ui
                .selectable_label(cv.view == *id, format!("{} {label}", i + 1))
                .clicked()
            {
                out.push(Step {
                    command: "view.composition".into(),
                    args: json!({"view": id}),
                });
            }
        }
        ui.separator();
        for (ex, label) in [
            ("orders", "orders / workers / refund"),
            ("connect4", "connect-4 exact plan"),
        ] {
            if ui.selectable_label(cv.example == ex, label).clicked() {
                out.push(Step {
                    command: "view.composition".into(),
                    args: json!({"example": ex}),
                });
            }
        }
        ui.separator();
        if cv.example == "orders" {
            for id in ["payment2", "orders2", "workers2"] {
                let present = sess
                    .composition
                    .as_ref()
                    .is_some_and(|c| c.spec(id).is_some());
                if ui.selectable_label(present, format!("+ {id}")).clicked() {
                    let cmd = if present {
                        "bundle.remove_instance"
                    } else {
                        "bundle.add_instance"
                    };
                    out.push(Step {
                        command: cmd.into(),
                        args: json!({"id": id}),
                    });
                }
            }
            if ui.button("reset links").clicked() {
                out.push(Step {
                    command: "bundle.reset_links".into(),
                    args: Value::Null,
                });
            }
        } else if ui.button("reset plan").clicked() {
            out.push(Step {
                command: "plan.open".into(),
                args: Value::Null,
            });
        }
        if ui.button("close").clicked() {
            out.push(Step {
                command: "view.composition".into(),
                args: json!({"open": false}),
            });
        }
    });
    let mut frame: Option<Frame> = None;
    egui::Panel::right("compviews-inspector")
        .resizable(true)
        .default_size(300.0)
        .show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let shown = app_state.pinned.clone();
                ui.heading("Inspector");
                if let Some(i) = shown {
                    info_ui(ui, &i);
                } else {
                    ui.label(
                        "Hover anything for its ontology concept and source; click to pin it.",
                    );
                }
            });
        });
    egui::Panel::bottom("compviews-notes")
        .resizable(true)
        .default_size(120.0)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("notes")
                .show(ui, |ui| {
                    notes_ui(ui, &app_state.last_notes);
                });
        });
    egui::CentralPanel::default().show(ui, |ui| {
        let f = draw(
            ui,
            &app_state.data,
            app_state.drag.as_ref(),
            &cv,
            ui.visuals().dark_mode,
        );
        frame = Some(f);
    });
    let f = frame.expect("drawn");
    if let Some(h) = &f.hovered {
        if app_state.pinned.is_none() || f.clicked.is_some() {
            app_state.pinned = Some(h.clone());
        }
    }
    if let Some(cl) = &f.clicked {
        app_state.pinned = Some(cl.clone());
    }
    out.extend(f.out.iter().cloned());
    if f.released {
        app_state.drag = None;
    }
    if let Some(d) = f.start_drag {
        app_state.drag = Some(d);
    }
    // the notes are drawn on the next frame's bottom panel
    app_state.last_notes = f.notes.clone();
    out
}

fn info_ui(ui: &mut egui::Ui, i: &Info) {
    ui.strong(&i.title);
    egui::Grid::new("info").num_columns(2).show(ui, |ui| {
        for (k, v) in &i.lines {
            if v.is_empty() {
                continue;
            }
            ui.weak(k);
            ui.label(v);
            ui.end_row();
        }
    });
    if !i.note.is_empty() {
        ui.label(&i.note);
    }
    for c in &i.cites {
        ui.monospace(c);
    }
}

fn notes_ui(ui: &mut egui::Ui, notes: &[(&'static str, String)]) {
    for (level, t) in notes {
        let tag = match *level {
            "na" => "Not applicable",
            "analogy" => "Analogy",
            "judgment" => "Judgment",
            "warn" => "Warning",
            _ => "Honesty",
        };
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!("{tag}:"));
            ui.label(t);
        });
    }
}

// ------------------------------------------------------------ headless

/// Draws `view` × `example` for `sess` in a headless egui context (no
/// window) and returns the manifest the drawing collected.
pub fn headless_manifest(sess: &Session, view: &str, example: &str, state: &str) -> Value {
    let mut data = Data::default();
    data.refresh(sess);
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 1000.0))),
        ..Default::default()
    };
    let mut manifest = Value::Null;
    let mut cv = sess.composition_view.clone();
    cv.view = view.into();
    cv.example = example.into();
    cv.rewrite = None;
    ctx.run_ui(raw, |ui| {
        let f = draw(ui, &data, None, &cv, false);
        manifest = f.scene.manifest(view, example, state);
    })
    .drop_without_applying_deltas();
    manifest
}

/// The spec's states, each from a fresh session by commands only.
pub const STATES: [(&str, &[(&str, &str)]); 5] = [
    ("default", &[]),
    (
        "payment2-pushout",
        &[
            ("bundle.add_instance", r#"{"id":"payment2"}"#),
            (
                "bundle.add_link",
                r#"{"kind":"token","from":"payment:account:customer","to":"payment2:account:customer"}"#,
            ),
        ],
    ),
    ("no-links", &[("bundle.clear_links", "{}")]),
    (
        "release-undo",
        &[(
            "bundle.add_link",
            r#"{"kind":"event","from":"payment:undo:reserve","to":"workers:release"}"#,
        )],
    ),
    (
        "plan-threats",
        &[("plan.add_piece", r#"{"piece":"threats","slot":"order"}"#)],
    ),
];

/// A fresh session in `state`.
pub fn session_in(state: &str) -> Result<Session, String> {
    let (_, steps) = STATES
        .iter()
        .find(|(id, _)| *id == state)
        .ok_or_else(|| format!("no state {state:?}"))?;
    let mut s = Session::default();
    s.execute("bundle.open", json!({}))
        .map_err(|e| e.to_string())?;
    s.execute("plan.open", json!({}))
        .map_err(|e| e.to_string())?;
    for (id, args) in *steps {
        let a: Value = serde_json::from_str(args).map_err(|e| e.to_string())?;
        s.execute(id, a)
            .map_err(|e| format!("{state}: {id}: {e}"))?;
    }
    Ok(s)
}

/// Every view × example × state's manifest, as
/// `parity/composition-views/goldens.json` holds them.
pub fn goldens() -> Result<Value, String> {
    let mut manifests = vec![];
    for (view, _, _) in VIEWS {
        for example in ["orders", "connect4"] {
            for (state, _) in STATES {
                let s = session_in(state)?;
                manifests.push(headless_manifest(&s, view, example, state));
            }
        }
    }
    Ok(json!({
        "format": "composition-views-scene/1",
        "spec": "docs/composition-views-scene.md",
        "generated_by": "pflow-native crates/app/src/compviews.rs (egui renderers, headless)",
        "pflow_rs": pflow_native_core::provenance::PFLOW_RS_REV,
        "manifests": manifests,
    }))
}
