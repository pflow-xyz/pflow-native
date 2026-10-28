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

mod a_draw;
mod a_operad_c4;
mod a_operad_theme;
mod boundaries_view;
mod cospan_view;
mod flat_view;
mod incidence_view;
mod laws_view;
mod lens_view;
mod operad_view;
#[allow(dead_code)]
mod pflow_rs_cites;
mod process_view;
mod spiders_view;
mod string_view;
use a_draw::*;
use boundaries_view::*;
use cospan_view::*;
use flat_view::*;
use incidence_view::*;
use laws_view::*;
use lens_view::*;
use operad_view::*;
use process_view::*;
use spiders_view::*;
use string_view::*;

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
    /// Link keys and `slot/name` pieces as of the last refresh, to name what
    /// a change added (`take_event`).
    seen: Option<Seen>,
    event: Option<Event>,
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
            seen: None,
            event: None,
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

/// What just changed in the composition or the plan (the prototype's
/// `ctx.event`): a view animates the thing that merged, once.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    LinkAdded { key: String, kind: String },
    LinkRemoved { key: String },
    InstanceAdded { id: String },
    PieceAdded { name: String, slot: String },
}

#[derive(Default)]
struct Seen {
    links: BTreeSet<String>,
    subnets: BTreeSet<String>,
    pieces: BTreeSet<String>,
}

/// How long a merge animation runs (VIEWS.md rule 5: about 600-900 ms).
pub const ANIM_SECS: f32 = 0.75;

impl Data {
    /// The event the last refresh found, once.
    pub fn take_event(&mut self) -> Option<Event> {
        self.event.take()
    }

    fn seen_now(&self) -> Seen {
        let mut s = Seen::default();
        if let Some(st) = &self.state {
            s.links = arr(&st["links"])
                .iter()
                .map(|l| self::s(&l["key"]).to_string())
                .collect();
            s.subnets = arr(&st["subnets"])
                .iter()
                .map(|x| self::s(&x["id"]).to_string())
                .collect();
        }
        if let Some(p) = &self.plan {
            let plan = &p["plan"];
            for slot in ["order", "filters", "heuristics"] {
                for x in arr(&plan[slot]) {
                    s.pieces.insert(format!("{slot}/{}", driver_name(x)));
                }
            }
            for x in arr(&plan["drivers"]) {
                s.pieces.insert(format!("drivers/{}", driver_name(x)));
            }
            if plan.get("memo").is_some_and(|m| !m.is_null()) {
                s.pieces.insert("memo/memo".into());
            }
        }
        s
    }

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
        let now = self.seen_now();
        if let Some(prev) = &self.seen {
            self.event = if let Some(k) = now.links.difference(&prev.links).next() {
                let kind = self
                    .state
                    .as_ref()
                    .and_then(|st| arr(&st["links"]).iter().find(|l| self::s(&l["key"]) == k))
                    .map_or("", |l| self::s(&l["kind"]))
                    .to_string();
                Some(Event::LinkAdded {
                    key: k.clone(),
                    kind,
                })
            } else if let Some(k) = prev.links.difference(&now.links).next() {
                Some(Event::LinkRemoved { key: k.clone() })
            } else if let Some(i) = now.subnets.difference(&prev.subnets).next() {
                Some(Event::InstanceAdded { id: i.clone() })
            } else if let Some(p) = now.pieces.difference(&prev.pieces).next() {
                let (slot, name) = p.split_once('/').unwrap_or(("", p));
                Some(Event::PieceAdded {
                    name: name.into(),
                    slot: slot.into(),
                })
            } else {
                None
            };
        }
        self.seen = Some(now);
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
    /// The merge animation in progress: the event and the `ui.input.time`
    /// it started at.
    pub anim: Option<(Event, f64)>,
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
    /// The merge animation: what just merged and how far along it is, 0..1
    /// (None when none is running; always None headless).
    pub event: Option<(Event, f32)>,
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

/// A drawing in design coordinates (the prototype's SVG viewBox) mapped
/// into a rectangle of the window: uniform scale, left-top aligned.
#[derive(Clone, Copy)]
pub struct Xf {
    pub origin: Pos2,
    pub k: f32,
}

impl Xf {
    /// Fits a `w`×`h` design into `area` (never scaled up past `max_k`).
    pub fn fit(area: Rect, w: f32, h: f32, max_k: f32) -> Self {
        let k = (area.width() / w.max(1.0))
            .min(area.height() / h.max(1.0))
            .min(max_k);
        Self {
            origin: area.left_top(),
            k,
        }
    }
    pub fn pt(&self, x: f32, y: f32) -> Pos2 {
        self.origin + Vec2::new(x, y) * self.k
    }
    pub fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(self.pt(x, y), Vec2::new(w, h) * self.k)
    }
    pub fn len(&self, d: f32) -> f32 {
        d * self.k
    }
}

/// Smooth 0..1 ease for the merge animations.
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl<'a> Ctx<'a> {
    /// Progress (eased, 0..1) of the running merge animation when it is
    /// the event `f` accepts; keeps the window repainting while it runs.
    /// Same state, same drawing: with no event this is `None` and the view
    /// draws its resting state.
    pub fn anim<T>(&self, f: impl Fn(&Event) -> Option<T>) -> Option<(T, f32)> {
        let (e, t) = self.event.as_ref()?;
        let v = f(e)?;
        self.p.ctx().request_repaint();
        Some((v, ease(*t)))
    }

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
    anim: Option<(Event, f32)>,
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
        event: anim,
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

#[allow(dead_code)] // the shared box layout: no view of the three uses it now; the lead may delete it
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
#[allow(dead_code)] // the shared box layout: no view of the three uses it now; the lead may delete it
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
#[allow(dead_code)] // the shared box layout: no view of the three uses it now; the lead may delete it
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

#[allow(dead_code)] // the shared box layout: no view of the three uses it now; the lead may delete it
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
#[allow(dead_code)] // the shared box layout: no view of the three uses it now; the lead may delete it
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

pub fn panel(app_state: &mut State, sess: &Session, ui: &mut egui::Ui) -> Vec<Step> {
    app_state.data.refresh(sess);
    let now = ui.input(|i| i.time);
    if let Some(e) = app_state.data.take_event() {
        app_state.anim = Some((e, now));
    }
    let anim = match &app_state.anim {
        Some((e, t0)) if ((now - t0) as f32) < ANIM_SECS => {
            Some((e.clone(), (now - t0) as f32 / ANIM_SECS))
        }
        _ => {
            app_state.anim = None;
            None
        }
    };
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
            anim,
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
        let f = draw(ui, &data, None, &cv, false, None);
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
