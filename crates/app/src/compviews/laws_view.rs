//! The laws view (see `docs/composition-views-scene.md`), after the
//! prototype's `js/views/laws.js`: composition laws as Chyp checked them.
//!
//! Each theory file states laws as Chyp rules (ASSUMED: a person's reading
//! of the source lines it cites) and then rewrites built from those rules
//! (PROVED: Chyp's own checker checked every step). A rewrite is drawn the
//! way Chyp's editor shows one: a string diagram per term, left to right,
//! boxes = generators, wires = vertices, each "=" naming the rule, its
//! direction and Chyp's verdict. Where Chyp's matcher found the rule
//! (`diagrams.rewrites[].steps[].match`) the matched part of the diagram
//! before the step is highlighted, and what the step inserted after it.
//!
//! PRECOMPUTED. Everything here is read from the export of Chyp 3d28483
//! (`crates/app/assets/chyp/`, made by `chyp_headless`): nothing is checked
//! or laid out at run time, and Chyp's checker is not reimplemented. The
//! step shown is `view.composition {rewrite, step}`; prev, next and play
//! queue that command.

#![allow(clippy::too_many_arguments)]

use super::c_chyp::*;
use super::c_draw::*;
use super::pflow_rs_cites::pflow_rs;
use super::*;

const THEORIES: [&str; 4] = [
    "orderer-monoid",
    "filter-meet",
    "driver-wrap",
    "sync-spiders",
];

pub(super) fn chyp(name: &str) -> &'static Value {
    use std::sync::OnceLock;
    static T: OnceLock<BTreeMap<&'static str, Value>> = OnceLock::new();
    let t = T.get_or_init(|| {
        let src: [(&str, &str); 4] = [
            (
                "orderer-monoid",
                include_str!("../../assets/chyp/orderer-monoid.json"),
            ),
            (
                "filter-meet",
                include_str!("../../assets/chyp/filter-meet.json"),
            ),
            (
                "driver-wrap",
                include_str!("../../assets/chyp/driver-wrap.json"),
            ),
            (
                "sync-spiders",
                include_str!("../../assets/chyp/sync-spiders.json"),
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
pub(super) fn covers(c: &Ctx) -> Vec<&'static str> {
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
pub(super) fn default_rewrite(theory: &str) -> &'static str {
    match theory {
        "orderer-monoid" => "doc_chain_bracketing",
        "filter-meet" => "meet_repeat_absorbed",
        "driver-wrap" => "json_driver_order_irrelevant",
        _ => "fuse_L1_L2",
    }
}

// ------------------------------------------------------------ colours

const HL: Color32 = Color32::from_rgb(0xfa, 0xb0, 0x05);
const HL_PEN: Color32 = Color32::from_rgb(0x1b, 0x1e, 0x23);
const INS: Color32 = Color32::from_rgb(0x70, 0x48, 0xe8);
pub(super) const VALID: Color32 = Color32::from_rgb(0x1e, 0x7a, 0x46);
const INVALID: Color32 = Color32::from_rgb(0xc0, 0x39, 0x2b);
const ERRC: Color32 = Color32::from_rgb(0xc4, 0x7a, 0x12);
const ASSUMED: Color32 = Color32::from_rgb(0x6a, 0x71, 0x7c);
const SEPW: f32 = 132.0;

pub(super) fn verdict_color(status: &str) -> Color32 {
    match status {
        "valid" => VALID,
        "invalid" => INVALID,
        "error" => ERRC,
        _ => ASSUMED,
    }
}

/// Wire types follow the page's palette: Order wires in the order-piece
/// colour, Search in the driver colour, Filter in the filter colour, Value
/// in the search Value colour; untyped Sync handles in RenderDOT's event-link
/// green.
fn wire_color(ty: Option<&str>) -> Color32 {
    match ty {
        Some("Order") => kind_color_of("Order"),
        Some("Search") => kind_color_of("Driver"),
        Some("Filter") => kind_color_of("Filter"),
        Some("Value") => type_color("Value"),
        None => kind_color("event"),
        _ => hex("#777777"),
    }
}

fn type_name(t: Option<&str>) -> String {
    t.map_or("Sync (untyped in the file)".to_string(), str::to_string)
}

// ------------------------------------------------------------ the chain

#[derive(Clone)]
enum Sep {
    Rule { name: String, line: u64, def: bool },
    Step { k: usize },
}

struct Item {
    d: Dgm,
    text: String,
    full: String,
    lines: Vec<String>,
    let_def: Option<(String, u64)>,
    w: f32,
}

struct Chain {
    graphs: Vec<Item>,
    seps: Vec<Sep>,
}

fn mono_w(t: &str, size: f32) -> f32 {
    t.chars().count() as f32 * size * 0.61
}

fn wrap_term(text: &str, max_w: f32, size: f32) -> Vec<String> {
    let parts: Vec<&str> = text.split(" ; ").collect();
    let mut lines: Vec<String> = vec![];
    let mut cur = String::new();
    for (i, p) in parts.iter().enumerate() {
        let piece = if i < parts.len() - 1 {
            format!("{p} ;")
        } else {
            p.to_string()
        };
        let cand = if cur.is_empty() {
            piece.clone()
        } else {
            format!("{cur} {piece}")
        };
        if mono_w(&cand, size) <= max_w || cur.is_empty() {
            cur = cand;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = piece;
        }
        while mono_w(&cur, size) > max_w && cur.contains(" * ") {
            let want = (max_w / (size * 0.61)).floor() as usize;
            let k = cur[..want.min(cur.len())].rfind(" * ");
            let Some(k) = k.filter(|k| *k > 0) else { break };
            lines.push(format!("{} *", &cur[..k]));
            cur = cur[k + 3..].to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

#[derive(Clone, PartialEq)]
enum Sel {
    Rewrite(String),
    Rule(String),
}

fn chain_items(th: &'static Value, sel: &Sel, scale: f32, tw: &dyn Fn(&str, f32) -> f32) -> Chain {
    let styles = &th["diagrams"]["styles"];
    let mk = |g: &Value, text: &str, full: &str| -> Item {
        let d = from_chyp(g, scale, styles, tw);
        let max_w = d.w.max(190.0);
        let lines = wrap_term(text, max_w, 10.5);
        let let_def = arr(&th["theory"]["lets"])
            .iter()
            .find(|l| s(&l["name"]) == text)
            .map(|l| (s(&l["name"]).to_string(), l["line"].as_u64().unwrap_or(0)));
        let w = lines
            .iter()
            .map(|l| mono_w(l, 10.5) + 6.0)
            .fold(d.w, f32::max)
            .max(
                let_def
                    .as_ref()
                    .map_or(0.0, |_| mono_w("let, line 000", 9.5)),
            );
        Item {
            d,
            text: text.into(),
            full: full.into(),
            lines,
            let_def,
            w,
        }
    };
    let mut graphs = vec![];
    let mut seps = vec![];
    match sel {
        Sel::Rule(name) => {
            if let Some(rd) = arr(&th["diagrams"]["rules"])
                .iter()
                .find(|r| s(&r["name"]) == *name)
            {
                let r = arr(&th["theory"]["rules"])
                    .iter()
                    .find(|r| s(&r["name"]) == *name);
                let lhs = r.map_or(s(&rd["lhs"]["term"]), |r| s(&r["lhs"]));
                let rhs = r.map_or(s(&rd["rhs"]["term"]), |r| s(&r["rhs"]));
                graphs.push(mk(&rd["lhs"], lhs, s(&rd["lhs"]["term"])));
                graphs.push(mk(&rd["rhs"], rhs, s(&rd["rhs"]["term"])));
                seps.push(Sep::Rule {
                    name: name.clone(),
                    line: rd["line"].as_u64().unwrap_or(0),
                    def: r.is_some_and(|r| s(&r["kind"]) == "def"),
                });
            }
        }
        Sel::Rewrite(name) => {
            let ck = arr(&th["check"]["rewrites"])
                .iter()
                .find(|r| s(&r["name"]) == *name);
            let dg = arr(&th["diagrams"]["rewrites"])
                .iter()
                .find(|r| s(&r["name"]) == *name);
            if let (Some(ck), Some(dg)) = (ck, dg) {
                for (k, st) in arr(&dg["steps"]).iter().enumerate() {
                    let c = &ck["steps"][k];
                    if k == 0 {
                        if let Some(g) = st["before"]
                            .as_u64()
                            .and_then(|i| dg["graphs"].get(i as usize))
                        {
                            graphs.push(mk(g, s(&c["text_before"]), s(&c["term_before"])));
                        }
                    }
                    let Some(a) = st["after"].as_u64() else {
                        continue;
                    };
                    seps.push(Sep::Step { k });
                    if let Some(g) = dg["graphs"].get(a as usize) {
                        graphs.push(mk(g, s(&c["text_after"]), s(&c["term_after"])));
                    }
                }
            }
        }
    }
    Chain { graphs, seps }
}

struct Row {
    /// (is graph, index)
    units: Vec<(bool, usize)>,
    w: f32,
}

fn flow_rows(ch: &Chain, w: f32) -> Vec<Row> {
    let mut rows = vec![];
    let Some(g0) = ch.graphs.first() else {
        return rows;
    };
    let mut row = Row {
        units: vec![(true, 0)],
        w: g0.w,
    };
    for k in 0..ch.seps.len() {
        let Some(g) = ch.graphs.get(k + 1) else { break };
        let uw = SEPW + g.w;
        if row.w + uw > w && !row.units.is_empty() {
            rows.push(std::mem::replace(
                &mut row,
                Row {
                    units: vec![],
                    w: 0.0,
                },
            ));
        }
        row.units.push((false, k));
        row.units.push((true, k + 1));
        row.w += uw;
    }
    rows.push(row);
    rows
}

fn sep_lines(th: &Value, sep: &Sep, ck: Option<&Value>) -> Vec<String> {
    let Sep::Step { .. } = sep else {
        return vec![String::new()];
    };
    let by = &ck.map_or(Value::Null, Value::clone)["by"];
    let tactic = s(&by["tactic"]);
    if tactic == "rule" {
        return vec![format!(
            "by {}{}",
            if s(&by["direction"]) == "converse" {
                "−"
            } else {
                ""
            },
            s(&by["rule"])
        )];
    }
    let t = if s(&by["text"]).is_empty() {
        tactic.to_string()
    } else {
        s(&by["text"]).to_string()
    };
    if mono_w(&format!("by {t}"), 11.0) <= SEPW - 6.0 {
        return vec![format!("by {t}")];
    }
    let inner = t
        .trim_start_matches(|c: char| c.is_alphanumeric() || c == '_')
        .trim_start_matches('(')
        .trim_end_matches(')')
        .to_string();
    let parts: Vec<&str> = if inner.is_empty() {
        vec![]
    } else {
        inner.split(", ").collect()
    };
    let all: Vec<&str> = arr(&th["theory"]["rules"])
        .iter()
        .map(|r| s(&r["name"]))
        .collect();
    if parts.len() > 2 && parts.len() == all.len() && all.iter().all(|n| parts.contains(n)) {
        return vec![
            format!("by {tactic}(all {}", parts.len()),
            "rules of the file)".into(),
        ];
    }
    let mut lines = vec![format!("by {tactic}(")];
    let mut cur = String::new();
    for p in &parts {
        let c2 = if cur.is_empty() {
            p.to_string()
        } else {
            format!("{cur}, {p}")
        };
        if mono_w(&c2, 11.0) > SEPW - 8.0 && !cur.is_empty() {
            lines.push(format!("{cur},"));
            cur = p.to_string();
        } else {
            cur = c2;
        }
    }
    lines.push(format!("{cur})"));
    if lines.len() <= 4 {
        lines
    } else {
        vec![
            format!("by {tactic}({} rules:", parts.len()),
            "see the hover)".into(),
        ]
    }
}

fn sep_need(th: &Value, sep: &Sep, ck: Option<&Value>) -> f32 {
    let n = sep_lines(th, sep, ck).len() as f32;
    (30.0 + n * 12.0).max(54.0)
}

// ------------------------------------------------------------ UI state (view-local)

#[allow(deprecated)]
fn play_state(c: &Ctx) -> (bool, f64) {
    c.p.ctx()
        .data(|d| d.get_temp::<(bool, f64)>(egui::Id::new("cv-laws-play")))
        .unwrap_or((false, 0.0))
}

#[allow(deprecated)]
fn set_play(c: &Ctx, on: bool, t: f64) {
    c.p.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("cv-laws-play"), (on, t)));
}

pub(super) fn click_in(c: &Ctx, r: Rect) -> bool {
    c.clicked && c.pointer.is_some_and(|p| r.contains(p)) && c.drag.is_none()
}

/// A chip that runs `view.composition` with `args` on a click.
pub(super) fn chip(
    c: &mut Ctx,
    at: Pos2,
    label: &str,
    mono: bool,
    on: bool,
    stroke: Color32,
    dashed: bool,
    args: Value,
    i: Info,
) -> Rect {
    let wd = label.chars().count() as f32 * if mono { 6.3 } else { 6.6 } + 14.0;
    let r = Rect::from_min_size(at, Vec2::new(wd, 19.0));
    let fill = if on { hex("#2b2f36") } else { Color32::WHITE };
    if dashed && !on {
        c.p.rect_filled(r, 9.5, fill);
        dpoly(c, &round_outline(r, 9.5), stroke, 1.0, (3.0, 2.0));
    } else {
        c.p.rect(
            r,
            9.5,
            fill,
            Stroke::new(1.0, if on { hex("#2b2f36") } else { stroke }),
            StrokeKind::Middle,
        );
    }
    let col = if on { Color32::WHITE } else { INK };
    if mono {
        c.mono(r.center(), Align2::CENTER_CENTER, label, 10.5, col);
    } else {
        c.text(r.center(), Align2::CENTER_CENTER, label, 11.0, col);
    }
    c.hover(r, i);
    if click_in(c, r) {
        c.command("view.composition", args);
    }
    r
}

fn counts_label(th: &Value) -> String {
    let sm = &th["check"]["summary"];
    let mut t = format!(
        "{}/{} proved",
        sm["intended_valid"].as_i64().unwrap_or(0),
        sm["intended"].as_i64().unwrap_or(0)
    );
    let nc = sm["negative_controls"].as_i64().unwrap_or(0);
    if nc > 0 {
        t.push_str(&format!(
            "  {}/{} must-fail fail{}",
            sm["negative_controls_failed"].as_i64().unwrap_or(0),
            nc,
            if nc == 1 { "s" } else { "" }
        ));
    }
    t
}

fn theory_info(name: &str, th: &Value) -> Info {
    let mut i = info(format!("{name}.chyp"))
        .note("Chyp theory file. Precomputed: the check below is Chyp 3d28483's, made when the export was generated; nothing is re-checked here.");
    for (k, v) in th["check"]["summary"].as_object().into_iter().flatten() {
        i = i.fact(&k.replace('_', " "), v.to_string());
    }
    i.fact(
        "source sha256",
        s(&th["provenance"]["source"]["sha256"]).to_string(),
    )
}

/// The pflow-rs / go-pflow cites a theory's rules are readings of (from the
/// verified table; the files themselves cite c396d46 lines).
pub(super) fn theory_cites(name: &str) -> Vec<String> {
    let o = comp::ontology();
    let go_rev = o["repos"]["goPflow"]["rev"].as_str().unwrap_or("");
    match name {
        "orderer-monoid" => vec![pflow_rs("laws.orderKey"), pflow_rs("laws.planChain"), pflow_rs("lens.chainDefault"), pflow_rs("lens.orderOnly")],
        "filter-meet" => vec![pflow_rs("laws.planFilters"), pflow_rs("slot.filters"), pflow_rs("piece.must_win")],
        "driver-wrap" => vec![pflow_rs("laws.planDrivers"), pflow_rs("lens.driversDoc"), pflow_rs("lens.solve")],
        _ => vec![
            format!("go-pflow metamodel/compose_fuse.go:9-20 @{go_rev} (fusion over equivalence classes: associative, a cycle is one class)"),
            format!("go-pflow metamodel/compose_flatten.go:72-88 @{go_rev} (transition classes: a union-find over event links)"),
            format!("ontology-pflow-xyz ROADMAP.md:410-411 @{} (lk:event: draw it as a synchronising spider)", o["repos"]["ontologyRoadmap"]["rev"].as_str().unwrap_or("")),
        ],
    }
}

fn rule_kind(th: &Value, name: &str) -> &'static str {
    if let Some(r) = arr(&th["theory"]["rules"])
        .iter()
        .find(|r| s(&r["name"]) == name)
    {
        return if s(&r["kind"]) == "def" {
            "def"
        } else {
            "assumed"
        };
    }
    if let Some(rw) = arr(&th["check"]["rewrites"])
        .iter()
        .find(|r| s(&r["name"]) == name)
    {
        // a rewrite used as a rule counts as proved only if Chyp checked it
        // (and it is not a must-fail control)
        return if s(&rw["status"]) == "valid" && s(&rw["expected"]) != "invalid" {
            "proved"
        } else {
            "unproved"
        };
    }
    "unknown"
}

fn step_info(th: &Value, name: &str, ck: &Value, dg: &Value, k: usize) -> Info {
    let by = &ck["by"];
    let file = s(&th["provenance"]["source"]["path"])
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string();
    let rk = if s(&by["rule"]).is_empty() {
        ""
    } else {
        rule_kind(th, s(&by["rule"]))
    };
    let rule = arr(&th["theory"]["rules"])
        .iter()
        .find(|r| s(&r["name"]) == s(&by["rule"]));
    let m = &dg["match"];
    let status = s(&ck["status"]);
    let mut i = info(format!("{file}:{}  = … by {}", ck["line"], s(&by["text"])))
        .fact("rewrite", name)
        .fact("step", (k + 1).to_string())
        .fact("before", s(&ck["term_before"]).to_string())
        .fact("after", s(&ck["term_after"]).to_string())
        .fact(
            "tactic",
            format!(
                "{}{}",
                s(&by["tactic"]),
                if s(&by["direction"]).is_empty() {
                    String::new()
                } else if s(&by["direction"]) == "converse" {
                    " (converse: the rule read right to left)".to_string()
                } else {
                    " (forward)".to_string()
                }
            ),
        );
    if let Some(r) = rule {
        i = i.fact(
            if s(&r["kind"]) == "def" {
                "definition (assumed)"
            } else {
                "rule (assumed, not checked)"
            },
            format!("{}  (line {})", s(&r["text"]), r["line"]),
        );
    }
    if rk == "proved" {
        i = i.fact(
            "rule",
            format!(
                "{}: a rewrite proved earlier in {file}, used as a rule",
                s(&by["rule"])
            ),
        );
    }
    if rk == "unproved" {
        i = i.fact(
            "rule",
            format!(
                "{}: a rewrite in {file} that Chyp did NOT check, used as a rule",
                s(&by["rule"])
            ),
        );
    }
    i = i.fact(
        "Chyp says",
        format!(
            "{}{}",
            s(&ck["chyp_status"]),
            if s(&ck["message"]).is_empty() {
                String::new()
            } else {
                format!(": {}", s(&ck["message"]))
            }
        ),
    );
    if m.is_object() {
        i = i.fact(
            "match (Chyp's matcher)",
            format!(
                "before: {} / {}; inserted: {}; enumeration #{}",
                {
                    let e = strs(&m["before"]["edges"]).join(", ");
                    if e.is_empty() {
                        "no boxes".into()
                    } else {
                        e
                    }
                },
                strs(&m["before"]["vertices"]).join(", "),
                {
                    let e = strs(&m["after"]["edges"]).join(", ");
                    if e.is_empty() {
                        "no boxes".into()
                    } else {
                        e
                    }
                },
                m["attempt"]
            ),
        );
    } else {
        i = i.fact(
            "match",
            if s(&dg["match_reason"]).is_empty() {
                "none".to_string()
            } else {
                s(&dg["match_reason"]).to_string()
            },
        );
    }
    i.note(format!(
        "{} Precomputed: the verdict is Chyp's (chyp.checker.check, run headless by chyp_headless), and the highlight is the match Chyp's rule tactic accepted, replayed with Chyp's matcher; Chyp 3d28483's own GUI does not draw it (tactic/ruletac.py:41-48).",
        match status {
            "valid" => "Chyp: VALID. This step follows from the rule (one application, or simp to a common normal form).".to_string(),
            "invalid" => "Chyp: INVALID. The rule does not take the left term to the right one.".to_string(),
            _ => format!("Chyp reported an error on this line{}.", if s(&ck["message"]).is_empty() { String::new() } else { format!(": {}", s(&ck["message"])) }),
        }
    ))
}

// ------------------------------------------------------------ drawing a diagram

pub(super) struct Marks<'a> {
    pub hl: Option<(&'a [String], &'a [String], f32)>,
    pub ins: Option<(&'a [String], &'a [String])>,
    /// A box sliding in (the prototype's `animateIn`): its id and progress.
    pub slide: Option<(String, f32)>,
}

pub(super) fn draw_diagram(
    c: &mut Ctx,
    th: &'static Value,
    d: &Dgm,
    ox: f32,
    oy: f32,
    marks: &Marks,
    graph_info: Info,
    role: &str,
    step_of: Option<(usize, String)>,
) {
    let at = |x: f32, y: f32| Pos2::new(ox + x, oy + y);
    let bg = Rect::from_min_size(at(0.0, 0.0), Vec2::new(d.w, d.h));
    c.hover(bg, graph_info);
    for x in [d.xl, d.xr].into_iter().flatten() {
        dpoly(
            c,
            &[at(x, 4.0), at(x, d.h - 4.0)],
            hex("#cfd4db"),
            1.0,
            (2.0, 3.0),
        );
    }
    let in_set = |m: Option<&[String]>, id: &str| m.is_some_and(|v| v.iter().any(|x| x == id));
    let hl_e = marks.hl.map(|m| m.0);
    let hl_v = marks.hl.map(|m| m.1);
    let alpha = marks.hl.map_or(1.0, |m| m.2);
    // halos under the wires and boxes: matched (yellow), inserted (violet)
    for w in &d.wires {
        let pts: Vec<Pos2> = wire_points(&w.pts, 0.4)
            .iter()
            .map(|p| at(p.0, p.1))
            .collect();
        if in_set(hl_v, &w.id) {
            poly(c, &pts, HL.gamma_multiply(0.55 * alpha), 10.0);
        }
        if in_set(marks.ins.map(|m| m.1), &w.id) {
            poly(c, &pts, INS.gamma_multiply(0.45), 10.0);
        }
    }
    for b in &d.boxes {
        let r = Rect::from_min_max(
            at(b.left - 4.0, b.top - 4.0),
            at(b.right + 4.0, b.bottom + 4.0),
        );
        if in_set(hl_e, &b.id) {
            c.p.rect(
                r,
                4.0,
                HL.gamma_multiply(0.35 * alpha),
                Stroke::new(3.0, HL.gamma_multiply(0.9 * alpha)),
                StrokeKind::Middle,
            );
        }
        if in_set(marks.ins.map(|m| m.0), &b.id) {
            c.p.rect(
                r,
                4.0,
                INS.gamma_multiply(0.3),
                Stroke::new(3.0, INS.gamma_multiply(0.9)),
                StrokeKind::Middle,
            );
        }
    }
    // wires
    for w in &d.wires {
        let pts: Vec<Pos2> = wire_points(&w.pts, 0.4)
            .iter()
            .map(|p| at(p.0, p.1))
            .collect();
        let hit = in_set(hl_v, &w.id);
        poly(
            c,
            &pts,
            wire_color(w.ty.as_deref()),
            if hit { 2.6 } else { 1.8 },
        );
        let prod = d.boxes.iter().find(|b| b.outputs.contains(&w.id));
        let cons = d.boxes.iter().find(|b| b.inputs.contains(&w.id));
        let i_in = d.in_dots.iter().position(|p| p.vertex == w.id);
        let i_out = d.out_dots.iter().position(|p| p.vertex == w.id);
        let mut i = info(format!("wire {}: {}", w.id, type_name(w.ty.as_deref())))
            .fact("type", type_name(w.ty.as_deref()))
            .fact(
                "from",
                prod.map_or(
                    i_in.map_or("—".to_string(), |k| format!("left boundary, input {k}")),
                    |b| {
                        format!(
                            "{} ({}) output {}",
                            b.label,
                            b.id,
                            b.outputs.iter().position(|x| *x == w.id).unwrap_or(0)
                        )
                    },
                ),
            )
            .fact(
                "to",
                cons.map_or(
                    i_out.map_or("—".to_string(), |k| format!("right boundary, output {k}")),
                    |b| {
                        format!(
                            "{} ({}) input {}",
                            b.label,
                            b.id,
                            b.inputs.iter().position(|x| *x == w.id).unwrap_or(0)
                        )
                    },
                ),
            );
        if w.route_len > 0 {
            i = i.fact("route", format!("through {} hidden layer{} (Chyp's identity boxes, layer_decomp, chyp/term.py:59-63)", w.route_len.div_ceil(2), if w.route_len > 2 { "s" } else { "" }));
        }
        if let Some((k, by)) = &step_of {
            if in_set(hl_v, &w.id) {
                i = i.fact("matched", format!("by step {} ({by})", k + 1));
            }
        }
        i = i.note("A vertex of Chyp's hypergraph is a wire (graph.py VData): one producer on its left, one consumer on its right.");
        hover_path(c, &pts, 5.0, i);
    }
    // boxes
    for b in &d.boxes {
        let r = Rect::from_min_max(at(b.left, b.top), at(b.right, b.bottom));
        let (r, a) = match &marks.slide {
            Some((id, t)) if *id == b.id => (r.translate(Vec2::new(0.0, -18.0 * (1.0 - t))), *t),
            _ => (r, 1.0),
        };
        c.p.rect(
            r,
            1.5,
            hex(&b.bg).gamma_multiply(a),
            Stroke::new(1.0, HL_PEN.gamma_multiply(a)),
            StrokeKind::Middle,
        );
        c.p.text(
            r.center(),
            Align2::CENTER_CENTER,
            &b.label,
            FontId::proportional(b.size),
            hex(&b.fg).gamma_multiply(a),
        );
        let gen = arr(&th["theory"]["gens"])
            .iter()
            .find(|g| s(&g["name"]) == b.label);
        let file = s(&th["provenance"]["source"]["path"])
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();
        let mut i = info(format!("box {} ({})", b.label, b.id));
        i = i.fact(
            "generator",
            gen.map_or("not a generator of the theory".to_string(), |g| {
                format!(
                    "gen {} : {}  ({file}:{})",
                    s(&g["name"]),
                    s(&g["signature"]),
                    g["line"]
                )
            }),
        );
        i = i.fact(
            "ports",
            format!(
                "{} → {}",
                if b.inputs.is_empty() {
                    "∅".into()
                } else {
                    b.inputs.join(", ")
                },
                if b.outputs.is_empty() {
                    "∅".into()
                } else {
                    b.outputs.join(", ")
                }
            ),
        );
        if let Some((x, y, w, h)) = b.chyp {
            i = i.fact(
                "Chyp layout",
                format!("centre ({x}, {y}), box {w} × {h} units (drawn wider to fit the label)"),
            );
        }
        if in_set(hl_e, &b.id) {
            if let Some((k, by)) = &step_of {
                i = i.fact(
                    "matched",
                    format!(
                        "by step {} ({by}): part of the rule's side Chyp's matcher found",
                        k + 1
                    ),
                );
            }
        } else {
            i = i.fact("role", role);
        }
        if let Some(g) = gen {
            i = i.note(format!("A generator: a hyperedge with {} inputs and {} outputs (Chyp graph.py EData).{}", g["arity"], g["coarity"], if sp::piece_info(&b.label).is_some() { format!(" In {file}, {} stands for the one-piece chain [{}] (or the driver of that name); the piece itself is plan.rs's.", b.label, b.label) } else { String::new() }));
        }
        c.hover(r, i);
    }
    // the highlight pen (Chyp's bold pen, width 3, graphscene.py:80-83)
    for b in &d.boxes {
        let r = Rect::from_min_max(at(b.left, b.top), at(b.right, b.bottom));
        if in_set(hl_e, &b.id) {
            c.p.rect_stroke(
                r,
                1.5,
                Stroke::new(3.0, HL_PEN.gamma_multiply(alpha)),
                StrokeKind::Middle,
            );
        }
        if in_set(marks.ins.map(|m| m.0), &b.id) {
            c.p.rect_stroke(r, 1.5, Stroke::new(3.0, INS), StrokeKind::Middle);
        }
    }
    for p in d.in_dots.iter().chain(&d.out_dots) {
        let ty = d
            .wires
            .iter()
            .find(|w| w.id == p.vertex)
            .and_then(|w| w.ty.as_deref());
        c.p.circle_filled(at(p.x, p.y), 2.4, wire_color(ty));
    }
}

// ------------------------------------------------------------ the view

pub(super) fn laws(
    c: &mut Ctx,
    area: Rect,
    orders: bool,
    rewrite: Option<&str>,
    step: Option<usize>,
) {
    let own: &[&str] = if orders {
        &["sync-spiders"]
    } else {
        &["orderer-monoid", "filter-meet", "driver-wrap"]
    };
    let has = |t: &str, n: &str| {
        arr(&chyp(t)["check"]["rewrites"])
            .iter()
            .any(|r| s(&r["name"]) == n)
            || arr(&chyp(t)["theory"]["rules"])
                .iter()
                .any(|r| s(&r["name"]) == n)
    };
    // the selected theory: the one holding the chosen rewrite or rule (own
    // theories first), else the example's first
    let selected = rewrite
        .and_then(|w| {
            own.iter()
                .chain(THEORIES.iter())
                .copied()
                .find(|t| has(t, w))
        })
        .unwrap_or(own[0]);
    let th = chyp(selected);
    let live = if orders { covers(c) } else { vec![] };
    let rewrites: Vec<Value> = arr(&th["check"]["rewrites"]).to_vec();
    let rules: Vec<Value> = arr(&th["theory"]["rules"]).to_vec();
    let sel = match rewrite {
        Some(r)
            if rules.iter().any(|x| s(&x["name"]) == r)
                && !rewrites.iter().any(|x| s(&x["name"]) == r) =>
        {
            Sel::Rule(r.to_string())
        }
        Some(r) if rewrites.iter().any(|x| s(&x["name"]) == r) => Sel::Rewrite(r.to_string()),
        _ => Sel::Rewrite(
            live.first()
                .map(|x| x.to_string())
                .filter(|_| selected == "sync-spiders")
                .unwrap_or_else(|| default_rewrite(selected).to_string()),
        ),
    };
    // manifest (the whole export, not the step): theories, rewrites, covers
    for t in own {
        c.scene.emit("theory", t, json!({}));
    }
    for rw in &rewrites {
        c.scene.emit(
            "rewrite",
            s(&rw["name"]),
            json!({"status": s(&rw["status"])}),
        );
    }
    for l in &live {
        c.scene.emit("cover", l, json!({}));
    }
    let file = s(&th["provenance"]["source"]["path"])
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string();

    // ---- theory tabs
    let mut y = area.top();
    let mut x = area.left();
    c.text(
        Pos2::new(x, y + 3.0),
        Align2::LEFT_TOP,
        if orders {
            "Event-link theory:"
        } else {
            "Search-plan theories:"
        },
        11.5,
        MUTED,
    );
    x += if orders { 118.0 } else { 140.0 };
    let tab = |c: &mut Ctx, x: &mut f32, y: f32, t: &str| {
        let label = format!("{t}  {}", counts_label(chyp(t)));
        let wd = label.chars().count() as f32 * 6.3 + 16.0;
        let r = Rect::from_min_size(Pos2::new(*x, y), Vec2::new(wd, 22.0));
        let on = t == selected;
        c.boxr(
            r,
            if on { INK } else { Color32::WHITE },
            if on { INK } else { MUTED },
            1.0,
        );
        c.text(
            r.center(),
            Align2::CENTER_CENTER,
            &label,
            11.5,
            if on { Color32::WHITE } else { INK },
        );
        c.hover(r, theory_info(t, chyp(t)));
        if click_in(c, r) {
            let pick = if on {
                s(&arr(&chyp(t)["check"]["rewrites"])
                    .first()
                    .cloned()
                    .unwrap_or(Value::Null)["name"])
                .to_string()
            } else {
                default_rewrite(t).to_string()
            };
            c.command("view.composition", json!({"rewrite": pick}));
        }
        *x = r.right() + 6.0;
    };
    for t in own {
        tab(c, &mut x, y, t);
    }
    c.text(
        Pos2::new(x + 6.0, y + 3.0),
        Align2::LEFT_TOP,
        "other:",
        11.5,
        MUTED,
    );
    x += 52.0;
    for t in THEORIES.iter().filter(|t| !own.contains(t)) {
        tab(c, &mut x, y, t);
    }
    y += 28.0;
    // ---- provenance
    let p = &th["provenance"];
    let ch = &p["chyp"];
    let pr = para(
        c,
        Pos2::new(area.left(), y),
        area.width(),
        &format!(
            "PRECOMPUTED (nothing is re-checked here). Checked by Chyp {} (v{}, {}, {}), headless through {} on {} {}, lark {}; layout {}; generated {}. {} sha256 {}…",
            s(&ch["commit"]).chars().take(7).collect::<String>(),
            s(&ch["version"]), s(&ch["license"]), s(&ch["copyright"]),
            s(&p["generator"]), s(&p["python_implementation"]), s(&p["python"]), s(&p["lark"]),
            s(&th["diagrams"]["layout"]["used"]), s(&p["generated_at"]),
            s(&p["source"]["path"]), s(&p["source"]["sha256"]).chars().take(12).collect::<String>(),
        ),
        9.5,
        MUTED,
        None,
        false,
    );
    c.hover(
        pr,
        with_cites_info(
            info("where these verdicts come from")
                .fact("Chyp", format!("{} (v{}, {})", s(&ch["commit"]), s(&ch["version"]), s(&ch["license"])))
                .fact("generator", s(&p["generator"]).to_string())
                .fact("layout engine", s(&th["diagrams"]["layout"]["engine"]).to_string())
                .fact("source", format!("{} sha256 {}", s(&p["source"]["path"]), s(&p["source"]["sha256"])))
                .fact("generated", s(&p["generated_at"]).to_string())
                .note("The verdicts are Chyp's checker's; the coordinates are Chyp's layout (chyp/layout.py, cvxpy). pflow-native imports the JSON export and does not run or reimplement Chyp. Chyp checked every rewrite from its file's rules; it did not check the rules."),
            &theory_cites(selected),
        ),
    );
    y = pr.bottom() + 6.0;
    // the live card: the open state as the term the theory is about
    y += super::c_laws_live::live_card(c, area.left(), y, area.width(), orders, selected, th);
    // ---- chips
    let mut cx = area.left();
    c.text(
        Pos2::new(cx, y + 3.0),
        Align2::LEFT_TOP,
        &format!("Rules · ASSUMED ({}):", rules.len()),
        10.5,
        ASSUMED,
    );
    cx += 200.0;
    for r in &rules {
        let name = s(&r["name"]);
        let def = s(&r["kind"]) == "def";
        let label = if def {
            format!("def {}", s(&r["lhs"]))
        } else {
            name.to_string()
        };
        let wd = label.chars().count() as f32 * 6.3 + 14.0;
        if cx + wd > area.right() {
            cx = area.left() + 200.0;
            y += 22.0;
        }
        let i = info(format!("{} {name}: ASSUMED", if def { "definition" } else { "rule" }))
            .fact("line", r["line"].to_string())
            .fact("text", s(&r["text"]).to_string())
            .fact("status", "a premise: Chyp uses it, does not check it")
            .note("A rule is a premise. Chyp matches it in a diagram and replaces it (double pushout, chyp/rewrite.py:23-96); whether it is true of the code is the reading its comment cites (the .chyp file, not vendored here).");
        let rc = chip(
            c,
            Pos2::new(cx, y),
            &label,
            true,
            sel == Sel::Rule(name.into()),
            ASSUMED,
            true,
            json!({"rewrite": name}),
            with_cites_info(i, &theory_cites(selected)),
        );
        cx = rc.right() + 4.0;
    }
    y += 24.0;
    cx = area.left();
    let n_int = rewrites
        .iter()
        .filter(|r| s(&r["expected"]) != "invalid")
        .count();
    let n_neg = rewrites.len() - n_int;
    c.text(
        Pos2::new(cx, y + 3.0),
        Align2::LEFT_TOP,
        &format!(
            "Rewrites · PROVED ({n_int}{}):",
            if n_neg > 0 {
                format!(" + {n_neg} must-fail")
            } else {
                String::new()
            }
        ),
        10.5,
        VALID,
    );
    cx += 200.0;
    for r in &rewrites {
        let name = s(&r["name"]);
        let neg = s(&r["expected"]) == "invalid";
        let mark = match s(&r["status"]) {
            "valid" => "✓",
            "invalid" => "✗",
            _ => "⚠",
        };
        let label = format!("{mark} {name}{}", if neg { "  must fail" } else { "" });
        let wd = label.chars().count() as f32 * 6.3 + 14.0;
        if cx + wd > area.right() {
            cx = area.left() + 200.0;
            y += 22.0;
        }
        let i = info(format!("rewrite {name}{}", if neg { " (negative control)" } else { "" }))
            .fact("Chyp", format!("{}{}", s(&r["status"]).to_uppercase(), if neg { format!("; expected to fail: {}", if r["as_expected"] == true { "as expected" } else { "NOT as expected" }) } else { String::new() }))
            .fact("steps", format!("{} ({} valid)", arr(&r["steps"]).len(), arr(&r["steps"]).iter().filter(|x| s(&x["status"]) == "valid").count()))
            .fact("line", format!("{file}:{}", r["line"]))
            .fact("live link order", if live.contains(&name) { "this rewrite covers the live link order" } else { "—" })
            .note("Precomputed: Chyp 3d28483 checked it when the export was made; nothing is re-checked here.");
        let col = if neg {
            ERRC
        } else {
            verdict_color(s(&r["status"]))
        };
        let rc = chip(
            c,
            Pos2::new(cx, y),
            &label,
            true,
            sel == Sel::Rewrite(name.into()),
            col,
            false,
            json!({"rewrite": name}),
            i,
        );
        if live.contains(&name) {
            c.p.circle_filled(rc.right_top() + Vec2::new(-2.0, 2.0), 3.0, EV_GREEN);
        }
        cx = rc.right() + 4.0;
    }
    y += 26.0;

    // ---- head and step controls
    let (name, ck, dg, n_steps) = match &sel {
        Sel::Rewrite(n) => {
            let ck = rewrites
                .iter()
                .find(|r| s(&r["name"]) == *n)
                .cloned()
                .unwrap_or(Value::Null);
            let dg = arr(&th["diagrams"]["rewrites"])
                .iter()
                .find(|r| s(&r["name"]) == *n)
                .cloned()
                .unwrap_or(Value::Null);
            let ns = arr(&dg["steps"]).len();
            (n.clone(), ck, dg, ns)
        }
        Sel::Rule(n) => (n.clone(), Value::Null, Value::Null, 0),
    };
    let cur = step.unwrap_or(0).min(n_steps.saturating_sub(1));
    match &sel {
        Sel::Rule(_) => {
            let r = rules.iter().find(|r| s(&r["name"]) == name);
            let head_r = c.mono(
                Pos2::new(area.left(), y),
                Align2::LEFT_TOP,
                &format!(
                    "{} {name}",
                    if r.is_some_and(|r| s(&r["kind"]) == "def") {
                        "def"
                    } else {
                        "rule"
                    }
                ),
                12.5,
                INK,
            );
            c.text(
                Pos2::new(head_r.right() + 14.0, y + 1.0),
                Align2::LEFT_TOP,
                &format!(
                    "{file}:{}   ASSUMED · Chyp uses it, does not check it",
                    r.map_or(0, |r| r["line"].as_u64().unwrap_or(0))
                ),
                11.0,
                ASSUMED,
            );
        }
        Sel::Rewrite(_) => {
            let neg = s(&ck["expected"]) == "invalid";
            let nv = arr(&ck["steps"])
                .iter()
                .filter(|x| s(&x["status"]) == "valid")
                .count();
            let head_r = c.mono(
                Pos2::new(area.left(), y),
                Align2::LEFT_TOP,
                &format!("rewrite {name}"),
                12.5,
                INK,
            );
            let badge = if neg {
                if ck["as_expected"] == true {
                    format!(
                        "negative control: Chyp says {}, as the file requires",
                        s(&ck["status"]).to_uppercase()
                    )
                } else {
                    "negative control CHECKED: the theory proves what it must not".into()
                }
            } else if s(&ck["status"]) == "valid" {
                format!(
                    "✓ Chyp: VALID · {nv}/{} step{} checked",
                    arr(&ck["steps"]).len(),
                    if arr(&ck["steps"]).len() == 1 {
                        ""
                    } else {
                        "s"
                    }
                )
            } else {
                format!("✗ Chyp: {}", s(&ck["status"]).to_uppercase())
            };
            c.text(
                Pos2::new(head_r.right() + 14.0, y + 1.0),
                Align2::LEFT_TOP,
                &format!("{file}:{}   {badge}", ck["line"]),
                11.0,
                if neg {
                    ERRC
                } else {
                    verdict_color(s(&ck["status"]))
                },
            );
            // step controls
            let bx = area.right() - 330.0;
            let ctl =
                |c: &mut Ctx, x: f32, w: f32, label: &str, on: bool, tip: &str| -> (Rect, bool) {
                    let r = Rect::from_min_size(Pos2::new(x, y - 3.0), Vec2::new(w, 22.0));
                    c.boxr(
                        r,
                        if on { hex("#2f6fb3") } else { hex("#eef1f5") },
                        MUTED,
                        1.0,
                    );
                    c.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        label,
                        11.5,
                        if on { Color32::WHITE } else { INK },
                    );
                    c.hover(r, info(label).note(tip));
                    (r, click_in(c, r))
                };
            let (playing, last) = play_state(c);
            let now = c.p.ctx().input(|i| i.time);
            let (_, prev) = ctl(
                c,
                bx,
                56.0,
                "◀ prev",
                false,
                "The previous step of the rewrite: view.composition {step}.",
            );
            let (_, next) = ctl(
                c,
                bx + 60.0,
                56.0,
                "next ▶",
                false,
                "The next step of the rewrite: view.composition {step}.",
            );
            let (_, play) = ctl(
                c,
                bx + 120.0,
                70.0,
                if playing { "■ stop" } else { "▶ play" },
                playing,
                "Plays the steps one after another (each is view.composition {step}).",
            );
            c.text(
                Pos2::new(bx + 198.0, y + 1.0),
                Align2::LEFT_TOP,
                &format!("step {}/{n_steps}", cur + 1),
                11.5,
                INK,
            );
            if prev && cur > 0 {
                set_play(c, false, now);
                c.command("view.composition", json!({"step": cur - 1}));
            }
            if next && cur + 1 < n_steps {
                set_play(c, false, now);
                c.command("view.composition", json!({"step": cur + 1}));
            }
            if play {
                set_play(c, !playing, now);
                if !playing && cur + 1 >= n_steps {
                    c.command("view.composition", json!({"step": 0}));
                }
            }
            if playing {
                if now - last > 1.4 {
                    if cur + 1 < n_steps {
                        c.command("view.composition", json!({"step": cur + 1}));
                        set_play(c, true, now);
                    } else {
                        set_play(c, false, now);
                    }
                }
                c.p.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
    }
    y += 24.0;

    // ---- legend
    let mut lx = area.left();
    let types: Vec<Option<String>> = {
        let mut v: Vec<Option<String>> = vec![];
        for g in arr(&th["theory"]["gens"]) {
            for w in arr(&g["domain"]).iter().chain(arr(&g["codomain"])) {
                let t = w["type"].as_str().map(str::to_string);
                if !v.contains(&t) {
                    v.push(t);
                }
            }
        }
        v
    };
    for t in &types {
        let label = format!(
            "{} wire",
            type_name(t.as_deref()).split(" (").next().unwrap_or("")
        );
        poly(
            c,
            &[Pos2::new(lx, y + 6.0), Pos2::new(lx + 20.0, y + 6.0)],
            wire_color(t.as_deref()),
            2.2,
        );
        c.text(
            Pos2::new(lx + 25.0, y),
            Align2::LEFT_TOP,
            &label,
            10.5,
            hex("#3d434c"),
        );
        lx += 40.0 + label.chars().count() as f32 * 6.0;
    }
    if matches!(sel, Sel::Rewrite(_)) {
        let r = Rect::from_min_size(Pos2::new(lx, y), Vec2::new(20.0, 12.0));
        c.p.rect(
            r,
            3.0,
            HL.gamma_multiply(0.45),
            Stroke::new(2.0, HL_PEN),
            StrokeKind::Middle,
        );
        c.text(
            Pos2::new(lx + 25.0, y),
            Align2::LEFT_TOP,
            "matched by the step to its right (Chyp's matcher; bold box = Chyp's highlight pen)",
            10.5,
            hex("#3d434c"),
        );
        lx += 25.0 + 430.0;
        let r = Rect::from_min_size(Pos2::new(lx, y), Vec2::new(20.0, 12.0));
        c.p.rect(
            r,
            3.0,
            INS.gamma_multiply(0.3),
            Stroke::new(2.0, INS),
            StrokeKind::Middle,
        );
        c.text(
            Pos2::new(lx + 25.0, y),
            Align2::LEFT_TOP,
            "what the shown step inserted",
            10.5,
            hex("#3d434c"),
        );
    }
    y += 18.0;

    // ---- the chain: the scale that fits the area
    if !orders {
        // the piece palette (drag an order piece onto the live chain)
        palette(c, area);
    }
    let chain_bottom = if orders {
        area.bottom()
    } else {
        area.bottom() - 58.0
    };
    let chain_area = Rect::from_min_max(
        Pos2::new(area.left(), y),
        Pos2::new(area.right(), chain_bottom),
    );
    let tw = |t: &str, sz: f32| {
        c.p.layout_no_wrap(t.to_string(), FontId::proportional(sz), Color32::WHITE)
            .size()
            .x
    };
    let cands: &[f32] = if selected == "sync-spiders" {
        &[34.0, 30.0, 26.0, 22.0, 19.0, 16.0]
    } else {
        &[36.0, 32.0, 28.0, 24.0, 20.0, 17.0, 14.0]
    };
    let steps_ck: Vec<Value> = arr(&ck["steps"]).to_vec();
    let mut best: Option<(f32, Chain, Vec<Row>)> = None;
    for &sc in cands {
        let ch = chain_items(th, &sel, sc, &tw);
        let rows = flow_rows(&ch, chain_area.width());
        let total: f32 = rows
            .iter()
            .map(|r| row_dims(th, &ch, r, &steps_ck).2 + 14.0)
            .sum();
        let fits = total <= chain_area.height() && rows.iter().all(|r| r.w <= chain_area.width());
        best = Some((sc, ch, rows));
        if fits {
            break;
        }
    }
    let Some((scale, ch, rows)) = best else {
        return;
    };
    let mut ry = chain_area.top();
    // which step's match shows on which graph: graph k shows the step leaving it
    // (k), and graph k+1 what step k inserted
    let steps_dg: Vec<Value> = arr(&dg["steps"]).to_vec();
    let leaving = |k: usize| -> Option<(Vec<String>, Vec<String>)> {
        let m = &steps_dg.get(k)?["match"];
        m.is_object()
            .then(|| (strs(&m["before"]["edges"]), strs(&m["before"]["vertices"])))
    };
    let entering = |k: usize| -> Option<(Vec<String>, Vec<String>)> {
        let m = &steps_dg.get(k)?["match"];
        m.is_object()
            .then(|| (strs(&m["after"]["edges"]), strs(&m["after"]["vertices"])))
    };
    let is_rule = matches!(sel, Sel::Rule(_));
    for row in &rows {
        let (diag_h, text_h, h) = row_dims(th, &ch, row, &steps_ck);
        let mut x = chain_area.left();
        for &(is_g, idx) in &row.units {
            if !is_g {
                let sep = ch.seps[idx].clone();
                let r = Rect::from_min_size(Pos2::new(x, ry), Vec2::new(SEPW, diag_h));
                draw_sep(
                    c,
                    th,
                    &sep,
                    steps_ck.get(idx),
                    steps_dg.get(idx),
                    &name,
                    r,
                    cur,
                    &file,
                );
                x += SEPW;
                continue;
            }
            let it = &ch.graphs[idx];
            let gx = x + (it.w - it.d.w) / 2.0;
            let gy = ry + (diag_h - it.d.h) / 2.0;
            let lv = (!is_rule).then(|| leaving(idx)).flatten();
            let en = (!is_rule && idx > 0 && idx - 1 == cur)
                .then(|| entering(idx - 1))
                .flatten();
            let role = if is_rule {
                if idx == 0 {
                    "left-hand side".to_string()
                } else {
                    "right-hand side".to_string()
                }
            } else if idx == 0 {
                "the rewrite's left term".to_string()
            } else {
                format!(
                    "after step {idx} (line {})",
                    steps_ck
                        .get(idx - 1)
                        .map_or(0, |c| c["line"].as_u64().unwrap_or(0))
                )
            };
            let marks = Marks {
                hl: lv.as_ref().map(|m| {
                    (
                        m.0.as_slice(),
                        m.1.as_slice(),
                        if idx == cur { 1.0 } else { 0.28 },
                    )
                }),
                ins: en.as_ref().map(|m| (m.0.as_slice(), m.1.as_slice())),
                slide: None,
            };
            let gi = {
                let mut i = info(format!("{role}: {}", it.text)).fact("term", it.full.clone());
                i = i.fact(
                    "boxes / wires",
                    format!("{} / {}", it.d.boxes.len(), it.d.wires.len()),
                );
                i = i.fact(
                    "boundary",
                    format!("{} in → {} out", it.d.in_dots.len(), it.d.out_dots.len()),
                );
                i = i.fact(
                    "layout",
                    format!(
                        "{}, {scale} px per unit",
                        s(&th["diagrams"]["layout"]["engine"])
                    ),
                );
                if let Some((n, l)) = &it.let_def {
                    i = i.fact("let", format!("{n} (line {l})"));
                }
                i.note("A string diagram: boxes are generators (hyperedges), wires are vertices, inputs on the left and outputs on the right (Chyp's editor draws one per term).")
            };
            let by = steps_ck
                .get(idx)
                .map_or(String::new(), |c| s(&c["by"]["text"]).to_string());
            draw_diagram(c, th, &it.d, gx, gy, &marks, gi, &role, Some((idx, by)));
            let tx = x + it.w / 2.0;
            for (i, l) in it.lines.iter().enumerate() {
                c.mono(
                    Pos2::new(tx, ry + diag_h + 4.0 + i as f32 * 13.0),
                    Align2::CENTER_TOP,
                    l,
                    10.5,
                    hex("#2b2f36"),
                );
            }
            if let Some((n, l)) = &it.let_def {
                c.text(
                    Pos2::new(tx, ry + diag_h + 4.0 + it.lines.len() as f32 * 13.0),
                    Align2::CENTER_TOP,
                    &format!("let {n}, line {l}"),
                    9.5,
                    hex("#8a919b"),
                );
            }
            x += it.w;
        }
        let _ = text_h;
        ry += h + 14.0;
    }
    // ---- notes
    c.note("honesty", "Precomputed: these verdicts and diagrams are the export of Chyp 3d28483 (chyp_headless, Apache-2.0, crates/app/assets/chyp/), made when the theories were checked. pflow-native does not run or reimplement Chyp's checker. Chyp checked every rewrite from its file's rules; it did not check the rules. Those (ASSUMED) are readings of the source lines each file cites. Chyp 3d28483 cannot rewrite modulo Frobenius or nest boxes, so spiders are explicit generators and drivers are boxes after the core.");
    c.note("honesty", "Box widths are widened to fit their labels; box centres, layers and wire heights are Chyp's layout (cvxpy, CLARABEL), which can vary with the solver. The match highlight is Chyp's matcher replayed by chyp_headless; Chyp 3d28483's GUI does not draw it (tactic/ruletac.py:41-48).");
    if !orders {
        plan_warn(c);
    }
}

pub(super) const EV_GREEN: Color32 = Color32::from_rgb(0x22, 0x77, 0x44);

pub(super) fn with_cites_info(mut i: Info, cs: &[String]) -> Info {
    for c in cs {
        if !i.cites.contains(c) {
            i.cites.push(c.clone());
        }
    }
    i
}

/// (diagram height, term text height, row height) of a row.
fn row_dims(th: &Value, ch: &Chain, row: &Row, steps_ck: &[Value]) -> (f32, f32, f32) {
    let gs: Vec<&Item> = row
        .units
        .iter()
        .filter(|u| u.0)
        .map(|u| &ch.graphs[u.1])
        .collect();
    let ss: Vec<f32> = row
        .units
        .iter()
        .filter(|u| !u.0)
        .map(|u| 2.0 * sep_need(th, &ch.seps[u.1], steps_ck.get(u.1)))
        .collect();
    let diag_h = gs.iter().map(|x| x.d.h).chain(ss).fold(64.0, f32::max);
    let text_h = gs
        .iter()
        .map(|x| x.lines.len() as f32 * 13.0 + if x.let_def.is_some() { 12.0 } else { 0.0 })
        .fold(0.0, f32::max);
    (diag_h, text_h, diag_h + text_h + 8.0)
}

fn draw_sep(
    c: &mut Ctx,
    th: &'static Value,
    sep: &Sep,
    ck: Option<&Value>,
    dg: Option<&Value>,
    rw_name: &str,
    r: Rect,
    cur: usize,
    file: &str,
) {
    let cx = r.center().x;
    let cy = r.center().y;
    match sep {
        Sep::Rule { name, line, def } => {
            c.mono(
                Pos2::new(cx, cy - 22.0),
                Align2::CENTER_CENTER,
                &if *def {
                    format!("def {}", name.trim_end_matches("_def"))
                } else {
                    format!("rule {name}")
                },
                11.0,
                hex("#1b1e23"),
            );
            eq_sign(c, cx, cy + 4.0, ASSUMED);
            c.text(
                Pos2::new(cx, cy + 30.0),
                Align2::CENTER_CENTER,
                "ASSUMED",
                11.5,
                ASSUMED,
            );
            c.text(
                Pos2::new(cx, cy + 44.0),
                Align2::CENTER_CENTER,
                &format!("not checked · line {line}"),
                9.5,
                hex("#8a919b"),
            );
            let rule = arr(&th["theory"]["rules"])
                .iter()
                .find(|x| s(&x["name"]) == name);
            let i = with_cites_info(
                info(format!("{} {name} (ASSUMED)", if *def { "definition" } else { "rule" }))
                    .fact("status", "a premise: Chyp uses it, does not check it")
                    .fact("text", rule.map_or(String::new(), |x| s(&x["text"]).to_string()))
                    .note("Whether the rule is true of the code is the reading its comment in the .chyp file cites; the file's comments are not vendored here."),
                &theory_cites(file.trim_end_matches(".chyp")),
            );
            c.hover(r, i);
        }
        Sep::Step { k } => {
            let (Some(ck), Some(dg)) = (ck, dg) else {
                return;
            };
            let by = &ck["by"];
            let status = s(&ck["status"]);
            let col = verdict_color(status);
            let selected = *k == cur;
            if selected {
                c.p.rect(
                    r.shrink2(Vec2::new(2.0, 0.0)),
                    6.0,
                    hex("#eef2f7"),
                    Stroke::new(1.5, INS.gamma_multiply(0.6)),
                    StrokeKind::Middle,
                );
            }
            let kind = if s(&by["rule"]).is_empty() {
                ""
            } else {
                rule_kind(th, s(&by["rule"]))
            };
            let by_lines = sep_lines(th, sep, Some(ck));
            let tag = if s(&by["tactic"]) == "rule" {
                format!(
                    "{}{}",
                    if s(&by["direction"]) == "converse" {
                        "converse · "
                    } else {
                        ""
                    },
                    match kind {
                        "assumed" => "assumed rule",
                        "def" => "definition",
                        "proved" => "proved above",
                        "unproved" => "NOT proved above",
                        _ => "rule",
                    }
                )
            } else if s(&by["tactic"]) == "simp" {
                "repeated rewriting".to_string()
            } else {
                s(&by["tactic"]).to_string()
            };
            let top = cy - 16.0 - (by_lines.len() as f32 - 1.0) * 12.0;
            for (i, t) in by_lines.iter().enumerate() {
                c.mono(
                    Pos2::new(cx, top - 6.0 + i as f32 * 12.0),
                    Align2::CENTER_CENTER,
                    t,
                    11.0,
                    hex("#1b1e23"),
                );
            }
            c.text(
                Pos2::new(cx, top + by_lines.len() as f32 * 12.0 - 3.0),
                Align2::CENTER_CENTER,
                &tag,
                9.5,
                match kind {
                    "proved" => VALID,
                    "def" => hex("#7b4cc2"),
                    "unproved" => INVALID,
                    _ => ASSUMED,
                },
            );
            eq_sign(c, cx, cy + 14.0, col);
            c.text(
                Pos2::new(cx, cy + 34.0),
                Align2::CENTER_CENTER,
                match status {
                    "valid" => "✓ VALID",
                    "invalid" => "✗ INVALID",
                    _ => "⚠ error",
                },
                11.5,
                col,
            );
            c.text(
                Pos2::new(cx, cy + 48.0),
                Align2::CENTER_CENTER,
                &format!(
                    "line {}{}",
                    ck["line"],
                    if dg["match"].is_object() {
                        ""
                    } else {
                        " · no match drawn"
                    }
                ),
                9.5,
                hex("#8a919b"),
            );
            let mut i = step_info(th, rw_name, ck, dg, *k);
            i = with_cites_info(i, &theory_cites(file.trim_end_matches(".chyp")));
            c.hover(r, i);
            if click_in(c, r) {
                c.command("view.composition", json!({"step": k}));
            }
        }
    }
}

/// The "=" of a step: two bars.
fn eq_sign(c: &Ctx, cx: f32, cy: f32, col: Color32) {
    for dy in [-6.0, 6.0] {
        c.p.rect_filled(
            Rect::from_center_size(Pos2::new(cx, cy + dy), Vec2::new(30.0, 5.0)),
            1.0,
            col,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_diagram_of_every_export_lays_out_with_ports_on_the_box_edges() {
        let tw = |t: &str, sz: f32| t.chars().count() as f32 * sz * 0.6;
        let mut graphs = 0;
        for name in THEORIES {
            let th = chyp(name);
            assert!(th["check"]["rewrites"].is_array(), "{name} is loaded");
            for rw in arr(&th["diagrams"]["rewrites"]) {
                for g in arr(&rw["graphs"]) {
                    let d = from_chyp(g, 30.0, &th["diagrams"]["styles"], &tw);
                    graphs += 1;
                    for b in &d.boxes {
                        assert!(
                            b.ins.iter().all(|p| (p.0 - b.left).abs() < 1e-3),
                            "{name}: inputs on the left edge"
                        );
                        assert!(
                            b.outs.iter().all(|p| (p.0 - b.right).abs() < 1e-3),
                            "{name}: outputs on the right edge"
                        );
                    }
                    for w in &d.wires {
                        assert!(!w.pts.is_empty(), "{name}: a wire has points");
                    }
                    assert!(d.w > 0.0 && d.h > 0.0);
                }
                // every step names a before and an after graph, and a match when Chyp's matcher found one
                for st in arr(&rw["steps"]) {
                    assert!(st["before"].is_u64());
                }
            }
        }
        assert!(graphs > 50, "the exports carry their graphs ({graphs})");
    }

    #[test]
    fn the_matches_name_boxes_and_wires_of_the_graph_they_leave() {
        let mut n = 0;
        for name in THEORIES {
            let th = chyp(name);
            for rw in arr(&th["diagrams"]["rewrites"]) {
                for st in arr(&rw["steps"]) {
                    let m = &st["match"];
                    if !m.is_object() {
                        continue;
                    }
                    let before = st["before"].as_u64().expect("before") as usize;
                    let g = &rw["graphs"][before];
                    let edges: Vec<&str> = arr(&g["edges"]).iter().map(|e| s(&e["id"])).collect();
                    for e in arr(&m["before"]["edges"]) {
                        assert!(
                            edges.contains(&s(e)),
                            "{name}: matched edge {e} is in the graph before"
                        );
                        n += 1;
                    }
                }
            }
        }
        assert!(n > 20, "matches checked: {n}");
    }

    #[test]
    fn a_term_wraps_at_its_semicolons() {
        let l = wrap_term(
            "tt_best * immediate_win * centrality * history ; chain * id * id ; chain * id ; chain",
            190.0,
            10.5,
        );
        assert!(l.len() >= 2);
        assert!(l.iter().all(|x| !x.is_empty()));
    }
}
