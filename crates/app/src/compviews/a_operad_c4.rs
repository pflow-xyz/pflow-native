//! The operad view of the Connect-4 example: the search plan as one
//! operadic composite, Solver = drivers ∘ core[order, filters, heuristics,
//! memo], drawn as nested boxes (a port of the prototype's `renderPlan`).
//!
//! Drivers are one-hole boxes around the core, nested in the order the code
//! runs them; the core's holes are the plan's slots, iterated from
//! `sp::SLOTS`, with the order chain's pieces as sub-boxes. Drop targets are
//! the slots (`plan.slot_candidates` lights the legal ones); a piece added
//! slides into its slot. Cites into pflow-rs are at the pinned rev.

use super::a_draw::*;
use super::a_operad_theme::*;
use super::*;

const PW: f32 = 1150.0;

// ---- cites (pflow-rs at the pinned rev; go-pflow and the ontology as recorded)

fn rs(path: &str, from: u64, to: u64, note: &str) -> String {
    rs_cite(path, from, to, note)
}
fn road(from: u64, to: u64, note: &str) -> String {
    cite_of("ontologyRoadmap", "ROADMAP.md", from, to, note)
}

fn c_plan_doc() -> String {
    rs("plan.rs", 6, 13, "the module-doc example plan")
}
fn c_drivers() -> String {
    rs(
        "plan.rs",
        205,
        281,
        "drivers(): the list is read into flags and a budget",
    )
}
fn c_nesting() -> String {
    rs("plan.rs", 35, 38, "each driver at most once; the search runs them in one order, budget(null_window(core)), whatever the list's")
}
fn c_chain() -> String {
    rs(
        "plan.rs",
        284,
        298,
        "chain(): absent means Chain::default; a name listed twice is refused",
    )
}
fn c_check() -> String {
    rs(
        "plan.rs",
        360,
        395,
        "check(): every name known and in its place; exact mode refuses what can change the value",
    )
}
fn c_exact_refuses() -> String {
    rs("plan.rs", 385, 392, "a filter or heuristic that can change the value: refused in exact mode, not built in play mode")
}
fn c_unsound() -> String {
    rs(
        "plan.rs",
        105,
        106,
        "UNSOUND: beam and static_eval, reserved names, never built",
    )
}
fn c_filters() -> String {
    rs(
        "plan.rs",
        341,
        358,
        "filters(): absent means Filter::DEFAULT; a name listed twice is refused",
    )
}
fn c_solver() -> String {
    rs(
        "plan.rs",
        400,
        440,
        "solver(): one Solver with budget (usize::MAX if none), null_window and memo",
    )
}
fn c_no_budget() -> String {
    rs("plan.rs", 415, 415, "d.budget.unwrap_or(usize::MAX)")
}
fn c_mode() -> String {
    rs("plan.rs", 65, 74, "Mode: Exact (default) or Play")
}
fn c_memo_struct() -> String {
    rs("plan.rs", 148, 153, "Memo {entries}")
}
fn c_order_only() -> String {
    rs(
        "order.rs",
        1,
        7,
        "order never changes a value, only how soon a cutoff comes",
    )
}
fn c_decisive() -> String {
    rs(
        "order.rs",
        28,
        31,
        "DECISIVE: a nonzero key settles the rank",
    )
}
fn c_chain_new() -> String {
    rs(
        "order.rs",
        197,
        244,
        "Chain, Chain::default, Chain::new (at most 8 pieces), Chain::then (the monoid operation)",
    )
}
fn c_chain_key() -> String {
    rs(
        "order.rs",
        259,
        275,
        "keys compared piece by piece; a decisive nonzero key stops the comparison",
    )
}
fn c_memo() -> String {
    rs(
        "game.rs",
        634,
        641,
        "with_memo: the table kept to N entries",
    )
}
fn c_mtd_f() -> String {
    rs(
        "game.rs",
        667,
        681,
        "with_mtd_f: the yes/no searches follow the bounds the last proved",
    )
}
fn c_aspiration() -> String {
    rs(
        "game.rs",
        697,
        706,
        "with_aspiration: start from the root's estimate",
    )
}
fn c_value() -> String {
    rs(
        "game.rs",
        809,
        815,
        "value: Option<f64>, None past the budget",
    )
}
fn c_run() -> String {
    rs(
        "game.rs",
        902,
        935,
        "run: nodes reset once per call, so budget counts the whole null-window run",
    )
}
fn c_solve() -> String {
    rs(
        "game.rs",
        936,
        996,
        "solve: one yes/no search per gap between adjacent rewards",
    )
}
fn c_core() -> String {
    rs(
        "game.rs",
        1046,
        1179,
        "Solver::search: fail-soft alpha-beta with the transposition table",
    )
}
fn c_budget_check() -> String {
    rs(
        "game.rs",
        1057,
        1060,
        "search: nodes > budget gives up (None)",
    )
}
fn c_no_bound() -> String {
    road(307, 307, "plan.rs has no Bound kind and no engine choice")
}
fn c_operad_row() -> String {
    road(
        401,
        401,
        "operad of wiring diagrams: the best fit for a SearchPlan",
    )
}
fn c_monoid() -> String {
    road(
        420,
        423,
        "orderers: a partial monoid under chaining, [] the identity, lexicographic",
    )
}
fn c_driver_order() -> String {
    road(
        426,
        427,
        "drivers: endomorphisms around the core; order matters in general",
    )
}
fn c_composite() -> String {
    road(
        428,
        430,
        "Solver = drivers ∘ core[order chain, filters, bounds, memo]; core is Position → Value⊥",
    )
}

// ---- the plan as the solver reads it

struct PlanView {
    mode: String,
    order: Vec<String>,
    order_default: bool,
    budget: Option<(Option<i64>, Option<i64>)>,
    null_window: bool,
    mtd_f: bool,
    aspiration: bool,
    drivers_default: bool,
    filters: Vec<String>,
    filters_default: bool,
    heur: Vec<String>,
    memo: Option<i64>,
    err: Option<String>,
    json: String,
}

fn read_plan(c: &Ctx) -> PlanView {
    let (plan, order, _, filters, heur) = plan_lists(c);
    let dr = plan.get("drivers").and_then(Value::as_array);
    let mut v = PlanView {
        mode: plan["mode"].as_str().unwrap_or("exact").into(),
        order,
        order_default: plan.get("order").is_none_or(Value::is_null),
        budget: None,
        null_window: dr.is_none(),
        mtd_f: false,
        aspiration: false,
        drivers_default: dr.is_none(),
        filters,
        filters_default: plan.get("filters").is_none_or(Value::is_null),
        heur,
        memo: plan
            .get("memo")
            .filter(|m| !m.is_null())
            .map(|m| m["entries"].as_i64().unwrap_or(0)),
        err: c
            .d
            .plan
            .as_ref()
            .and_then(|p| p["check"]["error"].as_str().map(String::from)),
        json: c
            .d
            .plan
            .as_ref()
            .map_or(String::new(), |p| s(&p["json"]).into()),
    };
    for d in dr.into_iter().flatten() {
        match d {
            Value::String(n) => match n.as_str() {
                "null_window" => v.null_window = true,
                "mtd_f" => v.mtd_f = true,
                "aspiration" => v.aspiration = true,
                _ => {}
            },
            Value::Object(o) => {
                if let Some(b) = o.get("budget") {
                    v.budget = Some((b["nodes"].as_i64(), b["ms"].as_i64()));
                }
            }
            _ => {}
        }
    }
    v
}

fn budget_text(b: (Option<i64>, Option<i64>)) -> String {
    match b {
        (Some(n), Some(ms)) => format!("{{nodes: {}, ms: {ms}}}", fmt_n(n)),
        (Some(n), None) => format!("{{nodes: {}}}", fmt_n(n)),
        (None, Some(ms)) => format!("{{ms: {ms}}}"),
        _ => "{}".into(),
    }
}

thread_local! {
    static OWN: std::cell::RefCell<BTreeMap<String, Option<String>>> = const { std::cell::RefCell::new(BTreeMap::new()) };
}

/// What `SearchPlan::check` says about this field alone (the same function
/// and words as for the whole plan; `check` stops at the first error it
/// meets, which may be in another hole). Cached by input.
fn own_error(mode: &str, field: &str, value: &Value) -> Option<String> {
    let key = format!("{mode}/{field}/{value}");
    if let Some(r) = OWN.with(|m| m.borrow().get(&key).cloned()) {
        return r;
    }
    // `root` holds its list as {"order": [...]}
    let value = if field == "root" {
        json!({ "order": value })
    } else {
        value.clone()
    };
    let r = sp::check(&json!({"mode": mode, field: value})).error;
    OWN.with(|m| m.borrow_mut().insert(key, r.clone()));
    r
}

// ---- small drawing pieces

fn piece_i(name: &str) -> Info {
    let mut i = info(format!("piece {name}"));
    if let Some(p) = sp::piece_info(name) {
        i = i
            .fact("kind", p.kind)
            .fact("status", p.status)
            .fact("contract", p.contract)
            .fact(
                "decisive",
                if p.decisive {
                    "yes: a nonzero key settles the rank"
                } else {
                    ""
                },
            )
            .fact("reads", p.reads)
            .note(p.doc);
    }
    i.cite(format!(
        "pflow-rs crates/pflow-metamodel/src/plan.rs @{} (ns:SearchPiece and the ct:* contracts are PROPOSED vocabulary; names, kinds and refusals are plan.rs's)",
        sp::PLAN_RS_REV
    ))
}

fn slot_i(slot: &str) -> Info {
    let mut i = info(format!(
        "slot: {}",
        sp::slot(slot).map_or(slot, |s| s.label)
    ));
    if let Some(sl) = sp::slot(slot) {
        i = i.fact("accepts", sl.accepts).fact(
            "field",
            sl.field
                .unwrap_or("(designed in the ROADMAP, not a SearchPlan field)"),
        );
        i.note = if sl.designed {
            "A slot the design has and plan.rs does not: shown, never fillable.".into()
        } else {
            format!("SearchPlan field {:?}.", sl.field.unwrap_or(""))
        };
    }
    i
}

/// A slot's drop zone without any drawing: lit where `plan.slot_candidates`
/// says the piece may go; the drop runs `plan.add_piece`. Returns whether
/// it is lit.
fn slot_zone(c: &mut Ctx, r: Rect, slot: &str, base: Info) -> bool {
    let mut lit = false;
    let mut v: Option<Value> = None;
    if let Some(Drag::Piece {
        cands: Some(cd), ..
    }) = c.drag
    {
        if let Some(x) = arr(&cd["slots"]).iter().find(|x| s(&x["slot"]) == slot) {
            lit = x["ok"] == true;
            v = Some(x.clone());
        }
    }
    let mut i = base;
    if let Some(x) = &v {
        i = i.fact(
            if x["ok"] == true { "drop" } else { "refused" },
            s(&x["why"]).to_string() + if x["ok"] == true { "legal" } else { "" },
        );
    }
    c.hover(r, i);
    if c.released && c.pointer_in(r) {
        if let (Some(Drag::Piece { name, .. }), Some(x)) = (c.drag, &v) {
            if x["ok"] == true {
                let n = name.clone();
                c.command("plan.add_piece", json!({"piece": n, "slot": slot}));
            }
        }
    }
    lit
}

enum Glyph {
    Cross,
    Left,
    Right,
}

/// A small round button (the prototype's `obtn`); runs `cmd` when clicked.
#[allow(clippy::too_many_arguments)]
fn button(
    c: &mut Ctx,
    xf: Xf,
    x: f32,
    y: f32,
    g: Glyph,
    title: &str,
    enabled: bool,
    cmd: Option<(&str, Value)>,
) {
    let at = xf.pt(x, y);
    let hit = Rect::from_center_size(at, Vec2::splat(xf.len(16.0).max(14.0)));
    let hov = c.pointer_in(hit) && enabled;
    let a = if enabled { 1.0 } else { 0.25 };
    c.p.circle(
        at,
        xf.len(8.0),
        if hov { hex("#eef3fb") } else { Color32::WHITE }.gamma_multiply(a),
        Stroke::new(
            1.0,
            if hov { hex("#2f6fb3") } else { hex("#c9ced6") }.gamma_multiply(a),
        ),
    );
    let col = SLATE.gamma_multiply(a);
    let d = xf.len(3.2);
    match g {
        Glyph::Cross => {
            c.line(at + Vec2::new(-d, -d), at + Vec2::new(d, d), col, 1.4);
            c.line(at + Vec2::new(d, -d), at + Vec2::new(-d, d), col, 1.4);
        }
        Glyph::Left | Glyph::Right => {
            let s_ = if matches!(g, Glyph::Left) { -1.0 } else { 1.0 };
            c.p.add(Shape::convex_polygon(
                vec![
                    at + Vec2::new(s_ * d * 1.2, 0.0),
                    at + Vec2::new(-s_ * d * 0.9, -d * 1.2),
                    at + Vec2::new(-s_ * d * 0.9, d * 1.2),
                ],
                col,
                Stroke::NONE,
            ));
        }
    }
    if enabled {
        c.hover_if(c.pointer_in(hit), info(title));
        if let Some((id, args)) = cmd {
            if c.is_clicked_in(hit) {
                c.command(id, args);
            }
        }
    }
}

/// A hole's frame and its type tab; lit while a drag may drop here.
#[allow(clippy::too_many_arguments)]
fn hole_frame(
    c: &Ctx,
    xf: Xf,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: Color32,
    label: &str,
    designed: bool,
    lit: bool,
) {
    if designed {
        c.rrect(xf, x, y, w, h, 8.0, Some(hex("#f7f7f8")), None, None);
        c.hatch(xf, x + 1.0, y + 1.0, w - 2.0, h - 2.0);
    } else {
        c.rrect(xf, x, y, w, h, 8.0, Some(hex("#fdfdfd")), None, None);
    }
    if lit {
        c.rrect(
            xf,
            x,
            y,
            w,
            h,
            8.0,
            Some(LIT.gamma_multiply(0.14)),
            Some((LIT, 3.0)),
            None,
        );
    } else {
        c.rrect(
            xf,
            x,
            y,
            w,
            h,
            8.0,
            None,
            Some((color, 1.5)),
            Some(if designed { (4.0, 4.0) } else { (6.0, 4.0) }),
        );
    }
    let tabw = tw(label, 10.5, true) + 14.0;
    c.rrect(
        xf,
        x + 10.0,
        y - 9.0,
        tabw,
        18.0,
        4.0,
        Some(Color32::WHITE),
        Some((color, 1.1)),
        if designed { Some((3.0, 3.0)) } else { None },
    );
    let mut o = tx(10.5, color).mono();
    if !designed {
        o = o.bold();
    }
    c.txt(xf, x + 10.0 + tabw / 2.0, y + 4.0, Al::M, label, o);
}

/// `xf` moved and scaled about a design point (a piece sliding in).
fn about(xf: Xf, cx: f32, cy: f32, dy: f32, sc: f32) -> Xf {
    let k = xf.k * sc;
    let at = xf.pt(cx, cy + dy);
    Xf {
        origin: at - Vec2::new(cx, cy) * k,
        k,
    }
}

pub(super) fn draw(c: &mut Ctx, area: Rect) {
    let pv = read_plan(c);
    let mode = pv.mode.clone();
    let mut frames: Vec<(&'static str, bool, bool)> = vec![("budget", pv.budget.is_some(), false)];
    if pv.aspiration {
        frames.push(("aspiration", true, false));
    }
    if pv.mtd_f && !pv.null_window {
        frames.push(("mtd_f", true, false));
    } else {
        frames.push((
            "null_window",
            pv.null_window,
            pv.null_window && pv.drivers_default,
        ));
    }
    let nf = frames.len();
    let core_x = 64.0 + 36.0 * nf as f32;
    let core_y = 80.0 + 48.0 * nf as f32;
    let (core_x1, core_y1) = (PW - core_x, core_y + 404.0);
    let frame_rect = |i: usize| -> (f32, f32, f32, f32) {
        (
            64.0 + 36.0 * i as f32,
            80.0 + 48.0 * i as f32,
            PW - 64.0 - 36.0 * i as f32,
            core_y1 + 30.0 * (nf - i) as f32,
        )
    };
    let cap_y = frame_rect(0).3 + 24.0;
    let leg_y = cap_y + 88.0;
    let ph = leg_y + 48.0;
    let xf = Xf::fit(area, PW, ph, 1.6);
    let anim = c.anim(|e| match e {
        Event::PieceAdded { name, slot } => Some((name.clone(), slot.clone())),
        _ => None,
    });
    let anim_for = |slot: &str, name: &str| -> Option<f32> {
        anim.as_ref()
            .filter(|((n, sl), _)| sl == slot && n == name)
            .map(|(_, t)| *t)
    };
    let drv_col = proto_piece_color("Driver");
    let pos_c = hex("#2f6fb3");
    let val_c = hex("#b0413e");
    let err = pv.err.clone();

    // ---- header: the composite as a formula, and the mode badge ----
    let drv_parts: Vec<String> = [
        pv.budget
            .map(|b| format!("budget{}", budget_text(b).replace("nodes: ", ""))),
        pv.aspiration.then(|| "aspiration".to_string()),
        if pv.mtd_f && !pv.null_window {
            Some("mtd_f".into())
        } else {
            pv.null_window.then(|| "null_window".into())
        },
    ]
    .into_iter()
    .flatten()
    .collect();
    let f1 = format!(
        "Solver = {}core[order, filters, heuristics, memo]",
        if drv_parts.is_empty() {
            String::new()
        } else {
            format!("{} ∘ ", drv_parts.join(" ∘ "))
        }
    );
    let join = |l: &[String]| {
        if l.is_empty() {
            "∅".to_string()
        } else {
            l.join(" · ")
        }
    };
    let f2 = format!(
        "core[ order = {}{},  filters = {}{},  heuristics = {},  memo = {} ]",
        if pv.order.is_empty() {
            "[]".to_string()
        } else {
            pv.order.join(" · ")
        },
        if pv.order_default { " (default)" } else { "" },
        join(&pv.filters),
        if pv.filters_default { " (default)" } else { "" },
        join(&pv.heur),
        pv.memo
            .map_or("∅".to_string(), |n| format!("{{entries: {}}}", fmt_n(n)))
    );
    c.txt(xf, 14.0, 24.0, Al::L, &f1, tx(15.0, OP_INK).bold());
    c.txt(xf, 14.0, 44.0, Al::L, &f2, tx(11.5, SLATE).mono());
    c.txt(
        xf,
        14.0,
        62.0,
        Al::L,
        "One operadic composite (ROADMAP:428): each driver is a one-hole box around the search it wraps; the core's holes are typed by piece kind, and SearchPlan::check is the typing (plan.rs:360-395).",
        tx(10.5, OP_MUTED),
    );
    c.hover(
        xf.rect(14.0, 8.0, 980.0, 62.0),
        info("the plan as one operadic composite")
            .fact("plan JSON", pv.json.clone())
            .fact(
                "check",
                match &err {
                    Some(e) => format!("refused: {e}"),
                    None => "SearchPlan::check and solver(connect-4) accept it".into(),
                },
            )
            .note("A whole plan is one operadic composite: Solver = drivers ∘ core[order chain, filters, bounds, memo] (ns:Composite with op:wrap and op:fill are PROPOSED vocabulary, ROADMAP Phase 6).")
            .cite(c_composite())
            .cite(c_operad_row())
            .cite(c_check())
            .cite(c_plan_doc()),
    );
    let badge_col = if mode == "exact" {
        hex("#1e7a46")
    } else {
        hex("#7b4cc2")
    };
    c.rrect(
        xf,
        1008.0,
        10.0,
        132.0,
        26.0,
        13.0,
        Some(badge_col),
        None,
        None,
    );
    c.txt(
        xf,
        1074.0,
        27.5,
        Al::M,
        &format!("mode: {mode}"),
        tx(12.0, Color32::WHITE).bold(),
    );
    c.hover(
        xf.rect(1008.0, 10.0, 132.0, 26.0),
        info(format!("mode: {mode}"))
            .fact("switch", "plan.set_mode (exact / play)")
            .note(if mode == "exact" {
                "\"exact\" (the default) holds every piece to returning the value: a filter or heuristic that can change it is refused. ns:ModeAdmits is PROPOSED vocabulary (ROADMAP Phase 5)."
            } else {
                "\"play\" lets choose guess when the budget runs out and lets in pieces that trade the value for speed (none is built yet): every such filter and heuristic is refused as not built."
            })
            .cite(c_mode())
            .cite(c_exact_refuses()),
    );
    if let Some(e) = &err {
        c.rrect(
            xf,
            1008.0,
            42.0,
            132.0,
            22.0,
            11.0,
            Some(Color32::WHITE),
            Some((OP_LEAK, 1.5)),
            None,
        );
        c.txt(
            xf,
            1074.0,
            57.0,
            Al::M,
            "refused by plan.rs",
            tx(11.0, OP_LEAK).bold(),
        );
        c.hover(
            xf.rect(1008.0, 42.0, 132.0, 22.0),
            info("refused")
                .fact("plan.rs says", e.clone())
                .cite(c_check())
                .cite(c_solver()),
        );
    }

    // ---- the driver frames (fixed nesting) ----
    let mut present_flags: Vec<bool> = vec![];
    for (i, (id, present, is_default)) in frames.iter().enumerate() {
        let (fx, fy, fx1, fy1) = frame_rect(i);
        let (id, present, is_default) = (*id, *present, *is_default);
        present_flags.push(present);
        let an = anim_for("drivers", id);
        let xfd = match an {
            Some(t) => about(xf, (fx + fx1) / 2.0, (fy + fy1) / 2.0, 0.0, 1.08 - 0.08 * t),
            None => xf,
        };
        let saved = c.p.clone();
        if let Some(t) = an {
            c.p.set_opacity(t);
        }
        let (w, h) = (fx1 - fx, fy1 - fy);
        c.rrect(
            xfd,
            fx,
            fy,
            w,
            h,
            14.0,
            if present {
                Some(Color32::WHITE.gamma_multiply(0.6))
            } else {
                None
            },
            Some((
                if present { drv_col } else { hex("#a9b0b9") },
                if present { 1.8 } else { 1.2 },
            )),
            if present { None } else { Some((7.0, 5.0)) },
        );
        let strip_info = {
            let base = if present {
                piece_i(id)
            } else {
                slot_i("drivers")
            };
            let b = base
                .fact(
                    "state",
                    if present {
                        if is_default { "on by default (drivers absent)".to_string() } else { "on".to_string() }
                    } else {
                        "off".into()
                    },
                )
                .fact("operad reading", "a one-hole box: Search → Search, filled by everything inside it")
                .fact(
                    "nesting",
                    match id {
                        "budget" => "always the outer box: game.rs counts nodes across the whole null-window run".to_string(),
                        "aspiration" => "inside budget, around the value-finding search: this view's reading (plan.rs's normalized() lists null_window, aspiration, then budget, in the order the search runs them)".to_string(),
                        _ => "always inside budget, around the core".to_string(),
                    },
                )
                .cite(c_drivers())
                .cite(c_run())
                .cite(c_nesting());
            let b = match id {
                "budget" => b.cite(c_budget_check()),
                "mtd_f" => b.cite(c_mtd_f()),
                "aspiration" => b.cite(c_aspiration()),
                _ => b.cite(c_solve()),
            };
            if present {
                b
            } else {
                ext_note(b, "The dashed frame is where this driver would sit: the nesting is fixed by the code, not by the JSON order.")
            }
        };
        let lit = slot_zone(c, xfd.rect(fx, fy, w, 44.0), "drivers", strip_info);
        c.rrect(
            xfd,
            fx + 1.0,
            fy + 1.0,
            w - 2.0,
            44.0,
            13.0,
            Some(drv_col.gamma_multiply(if lit {
                0.22
            } else if present {
                0.07
            } else {
                0.025
            })),
            if lit { Some((LIT, 3.0)) } else { None },
            None,
        );
        let yb = fy + 28.0;
        let mut x = fx + 16.0;
        c.txt(
            xfd,
            x,
            yb,
            Al::L,
            id,
            tx(13.5, if present { drv_col } else { hex("#8a929c") })
                .mono()
                .bold(),
        );
        x += tw(id, 13.5, true) + 8.0;
        if present && id == "budget" {
            let t = budget_text(pv.budget.unwrap_or((None, None)));
            c.txt(xfd, x, yb, Al::L, &t, tx(12.0, OP_INK).mono());
            x += tw(&t, 12.0, true) + 12.0;
        }
        if is_default {
            c.rrect(
                xfd,
                x,
                yb - 13.0,
                56.0,
                17.0,
                8.5,
                Some(Color32::WHITE),
                Some((drv_col, 1.0)),
                Some((3.0, 2.0)),
            );
            c.txt(xfd, x + 28.0, yb - 1.0, Al::M, "default", tx(10.5, drv_col));
            x += 66.0;
        }
        if present {
            let contract = if id == "budget" {
                "ct:partial"
            } else {
                "ct:value-preserving"
            };
            let cw = tw(contract, 10.5, true) + 14.0;
            c.rrect(
                xfd,
                x,
                yb - 13.0,
                cw,
                17.0,
                4.0,
                Some(hex("#eef3fb")),
                Some((hex("#c6d6ea"), 1.0)),
                None,
            );
            c.txt(
                xfd,
                x + cw / 2.0,
                yb - 1.0,
                Al::M,
                contract,
                tx(10.5, hex("#2f6fb3")).mono(),
            );
            x += cw + 10.0;
        }
        let desc = match id {
            "budget" => {
                if present {
                    "the exact value, or None past the budget (plan.rs:35-38, game.rs:1057-1060)".to_string()
                } else {
                    "off: no budget, so plan.rs passes usize::MAX nodes (plan.rs:415). Drop budget here.".into()
                }
            }
            "aspiration" => "starts the value search from the root's estimate, which root.order must name (game.rs:697-706)".into(),
            "mtd_f" => "finds the value by yes/no searches that follow the bounds the last proved (game.rs:667-681)".into(),
            _ => {
                if present {
                    format!(
                        "one yes/no search per gap between adjacent rewards (game.rs:936-996){}",
                        if is_default { "; on because drivers is absent (plan.rs:206-214)" } else { "" }
                    )
                } else {
                    "off: drivers is given without it, so one search over the whole reward range (game.rs:947-959). Drop null_window here.".into()
                }
            }
        };
        let room = (fx1 - fx) - (x - fx) - 48.0;
        let mut dtext = desc;
        while tw(&dtext, 10.5, false) > room && dtext.chars().count() > 12 {
            let cut: String = dtext.chars().take(dtext.chars().count() - 6).collect();
            dtext = format!("{}…", cut.trim_end());
        }
        c.txt(
            xfd,
            x,
            yb,
            Al::L,
            &dtext,
            tx(
                10.5,
                if present {
                    OP_MUTED
                } else {
                    OP_MUTED.gamma_multiply(0.8)
                },
            ),
        );
        if present {
            let idx = pv_driver_index(&c.d.plan, id);
            button(
                c,
                xfd,
                fx1 - 20.0,
                fy + 22.0,
                Glyph::Cross,
                &if is_default {
                    "turn the default null_window off (drivers: [])".to_string()
                } else {
                    format!("remove {id} from drivers")
                },
                true,
                Some((
                    "plan.remove_piece",
                    json!({"slot": "drivers", "index": idx.unwrap_or(0)}),
                )),
            );
        }
        c.scene.emit_if(present, "frame", id);
        c.p = saved;
    }

    // ---- the core box ----
    let (kx, ky, kx1, ky1) = (core_x, core_y, core_x1, core_y1);
    c.rrect(
        xf,
        kx,
        ky,
        kx1 - kx,
        ky1 - ky,
        10.0,
        Some(Color32::WHITE),
        Some((OP_INK, 1.8)),
        None,
    );
    c.txt(
        xf,
        kx + 16.0,
        ky + 25.0,
        Al::L,
        "core",
        tx(13.5, OP_INK).bold(),
    );
    c.txt(
        xf,
        kx + 56.0,
        ky + 25.0,
        Al::L,
        "alpha-beta search over the net (Solver::search, game.rs:1046-1179)",
        tx(10.5, OP_MUTED),
    );
    c.txt(
        xf,
        kx1 - 16.0,
        ky + 25.0,
        Al::R,
        "Position → Value⊥",
        tx(12.0, SLATE).mono(),
    );
    c.hover(
        xf.rect(kx, ky, kx1 - kx, ky1 - ky),
        info("core: the alpha-beta search")
            .fact("holes", "order, filters, heuristics, memo (SearchPlan fields); bounds, engine (designed, never fillable)")
            .fact("operad reading", "an operation core(order, filters, heuristics, memo) → Search; filling the holes is composition")
            .fact("wires", "Position in, Value⊥ out; the holes are parameters, not wires (no piece has wires of its own)")
            .note("The search is fixed (alpha-beta over Game); a plan chooses what swaps in. As a box it is Position → Value⊥, and it has one typed hole per SearchPlan field (ns:Box is PROPOSED vocabulary, ROADMAP Phase 6).")
            .cite(c_core())
            .cite(c_composite())
            .cite(rs("plan.rs", 3, 4, "the search is fixed; a plan chooses what swaps in")),
    );

    // ---- typed wires: Position in, Value⊥ out, crossing every boundary ----
    let wire_y = ky + 46.0 + 160.0 + 7.0;
    let wire_info = |t: &str| -> Info {
        let i = info(format!("wire type {t}"));
        if t == "Position" {
            i.fact("enters", "each driver's box passes it inward, down to the core")
                .note("A marking of the net: what the search is asked about. Every box on the way in has the same input (ns:WireType is PROPOSED vocabulary, ROADMAP Phase 6).")
                .cite(c_composite())
        } else {
            i.fact("leaves", "core → the drivers, outward")
                .fact("⊥", "Solver::value returns None past the budget; with no budget plan.rs passes usize::MAX, so ⊥ is out of reach in practice but stays in the type")
                .note("The exact value of the position, or ⊥ (None) when the budget runs out. Every box on the way out has the same output.")
                .cite(c_composite())
                .cite(c_value())
                .cite(c_no_budget())
        }
    };
    let xs: Vec<f32> = frames
        .iter()
        .enumerate()
        .map(|(i, _)| frame_rect(i).0)
        .chain([kx])
        .collect();
    let xr: Vec<f32> = [kx1]
        .into_iter()
        .chain((0..nf).rev().map(|i| frame_rect(i).2))
        .collect();
    let pres_l: Vec<bool> = present_flags.iter().copied().chain([true]).collect();
    let pres_r: Vec<bool> = [true]
        .into_iter()
        .chain(present_flags.iter().rev().copied())
        .collect();
    c.line(xf.pt(8.0, wire_y), xf.pt(kx, wire_y), pos_c, xf.len(2.4));
    c.poly(
        &[
            xf.pt(kx - 7.0, wire_y - 4.5),
            xf.pt(kx, wire_y),
            xf.pt(kx - 7.0, wire_y + 4.5),
        ],
        pos_c,
        xf.len(2.0),
    );
    c.txt(
        xf,
        8.0,
        wire_y - 8.0,
        Al::L,
        "Position",
        tx(11.5, pos_c).bold(),
    );
    for (x, p) in xs.iter().zip(&pres_l) {
        c.p.circle(
            xf.pt(*x, wire_y),
            xf.len(4.5),
            if *p { pos_c } else { Color32::WHITE },
            Stroke::new(1.5, pos_c),
        );
    }
    c.hover_poly(
        &[xf.pt(8.0, wire_y), xf.pt(kx, wire_y)],
        6.0,
        wire_info("Position"),
    );
    c.line(
        xf.pt(kx1, wire_y),
        xf.pt(PW - 8.0, wire_y),
        val_c,
        xf.len(2.4),
    );
    c.poly(
        &[
            xf.pt(PW - 15.0, wire_y - 4.5),
            xf.pt(PW - 8.0, wire_y),
            xf.pt(PW - 15.0, wire_y + 4.5),
        ],
        val_c,
        xf.len(2.0),
    );
    c.txt(
        xf,
        PW - 8.0,
        wire_y - 8.0,
        Al::R,
        "Value⊥",
        tx(11.5, val_c).bold(),
    );
    for (x, p) in xr.iter().zip(&pres_r) {
        c.p.circle(
            xf.pt(*x, wire_y),
            xf.len(4.5),
            if *p { val_c } else { Color32::WHITE },
            Stroke::new(1.5, val_c),
        );
    }
    c.hover_poly(
        &[xf.pt(kx1, wire_y), xf.pt(PW - 8.0, wire_y)],
        6.0,
        wire_info("Value⊥"),
    );

    // ---- holes, iterated from the slots ----
    let ix0 = kx + 20.0;
    let iw = (kx1 - 20.0) - ix0;
    let oy = ky + 46.0;
    let oh = 160.0;
    let r2y = oy + oh + 14.0;
    let r2h = 88.0;
    let r3y = r2y + r2h + 14.0;
    let r3h = ky1 - 14.0 - r3y;
    let row2: Vec<&sp::Slot> = sp::SLOTS
        .iter()
        .filter(|s_| !s_.designed && s_.id != "drivers" && s_.id != "order")
        .collect();
    let row3: Vec<&sp::Slot> = sp::SLOTS.iter().filter(|s_| s_.designed).collect();
    let order_col = proto_piece_color("Order");
    let n_ord = pv.order.len();
    let chain_err = if pv.order_default {
        None
    } else {
        own_error(&mode, "order", &json!(pv.order))
    };

    // order hole: the chain box with its pieces as sub-boxes
    if let Some(sl) = sp::slot("order") {
        c.scene
            .emit("hole", "order", json!({"accepts": sl.accepts}));
        let hole = xf.rect(ix0, oy, iw, oh);
        let lit = slot_zone(
            c,
            hole,
            "order",
            slot_i("order")
                .fact("type", "Order pieces, chained (≤ 8, none twice)")
                .fact(
                    "filled by",
                    if let Some(e) = &chain_err {
                        format!("refused: {e}")
                    } else if pv.order_default {
                        "Chain::default (order absent)".into()
                    } else {
                        format!("[{}]", pv.order.join(", "))
                    },
                )
                .cite(c_chain())
                .cite(c_chain_new()),
        );
        hole_frame(
            c,
            xf,
            ix0,
            oy,
            iw,
            oh,
            order_col,
            "order : Order chain",
            false,
            lit,
        );
        let cb = (ix0 + 8.0, oy + 12.0, iw - 16.0, oh - 20.0);
        if let Some(e) = &chain_err {
            c.rrect(
                xf,
                cb.0,
                cb.1,
                cb.2,
                cb.3,
                7.0,
                Some(hex("#fdf0ee")),
                Some((OP_LEAK, 1.5)),
                None,
            );
            for (i, l) in wrap(&format!("refused: {e}"), cb.2 - 32.0, 12.0)
                .iter()
                .enumerate()
            {
                c.txt(
                    xf,
                    cb.0 + 16.0,
                    cb.1 + 30.0 + 17.0 * i as f32,
                    Al::L,
                    l,
                    tx(12.0, OP_LEAK),
                );
            }
        } else {
            c.rrect(
                xf,
                cb.0,
                cb.1,
                cb.2,
                cb.3,
                7.0,
                Some(hex("#fffaf2")),
                Some((order_col, 1.4)),
                None,
            );
            let hl = if pv.order_default {
                "chain · default: order is absent, so Chain::default (order.rs:205-216)"
            } else {
                "chain · a monoid under ++, [] the identity"
            };
            c.txt(
                xf,
                cb.0 + 14.0,
                cb.1 + 19.0,
                Al::L,
                hl,
                tx(11.0, hex("#7a4a08")),
            );
            c.txt(
                xf,
                cb.0 + cb.2 - 14.0,
                cb.1 + 19.0,
                Al::R,
                if pv.order_default {
                    "naming any piece replaces the default"
                } else {
                    "reorder with the arrow buttons: node counts change, never the value (order.rs:1-7)"
                },
                tx(10.5, OP_MUTED),
            );
            let (py, ph2, gap) = (cb.1 + 32.0, 70.0, 16.0);
            let (ix, iw2) = (cb.0 + 16.0, cb.2 - 32.0);
            let pwid = if n_ord > 0 {
                178.0_f32.min((iw2 - (n_ord as f32 - 1.0) * gap) / n_ord as f32)
            } else {
                0.0
            };
            let total = n_ord as f32 * pwid + (n_ord as f32 - 1.0).max(0.0) * gap;
            let px0 = ix + (iw2 - total) / 2.0;
            let x_of = |i: usize| px0 + i as f32 * (pwid + gap);
            // decisive runs
            let mut runs: Vec<(usize, usize)> = vec![];
            for (i, name) in pv.order.iter().enumerate() {
                let d = sp::piece_info(name).is_some_and(|p| p.decisive);
                if d {
                    match runs.last_mut() {
                        Some(r) if r.1 + 1 == i => r.1 = i,
                        _ => runs.push((i, i)),
                    }
                }
            }
            let in_lead = |i: usize| runs.first().is_some_and(|r| r.0 == 0 && i <= r.1);
            if n_ord == 0 {
                let mx = ix + iw2 / 2.0;
                c.rrect(
                    xf,
                    mx - 230.0,
                    py,
                    460.0,
                    ph2,
                    6.0,
                    Some(Color32::WHITE),
                    Some((order_col, 1.0)),
                    Some((5.0, 4.0)),
                );
                c.txt(
                    xf,
                    mx,
                    py + 30.0,
                    Al::M,
                    "[ ]",
                    tx(18.0, order_col).mono().bold(),
                );
                c.txt(xf, mx, py + 52.0, Al::M, "the empty chain: the identity; moves keep the net's own order (order.rs:236-238)", tx(10.5, OP_MUTED));
                c.hover(
                    xf.rect(mx - 230.0, py, 460.0, ph2),
                    info("the empty chain []")
                        .note("An order chain with no pieces: every key ties, and ties keep the net's own order of the moves. It is the monoid's identity, not the default chain (op:chain identity is PROPOSED vocabulary, ROADMAP Phase 6).")
                        .cite(c_chain_new())
                        .cite(c_chain_key())
                        .cite(c_monoid()),
                );
            }
            for (i, name) in pv.order.iter().enumerate() {
                let p = sp::piece_info(name);
                let decisive = p.is_some_and(|p| p.decisive);
                let x = x_of(i);
                let an = anim_for("order", name);
                let (xfp, saved) = match an {
                    Some(t) => {
                        let s_ = c.p.clone();
                        c.p.set_opacity(t);
                        (
                            about(
                                xf,
                                x + pwid / 2.0,
                                py + ph2 / 2.0,
                                -250.0 * (1.0 - t) + 8.0 * (std::f32::consts::PI * t).sin(),
                                0.55 + 0.45 * t,
                            ),
                            Some(s_),
                        )
                    }
                    None => (xf, None),
                };
                c.rrect(
                    xfp,
                    x,
                    py,
                    pwid,
                    ph2,
                    6.0,
                    Some(Color32::WHITE),
                    Some((order_col, if decisive { 2.2 } else { 1.3 })),
                    if pv.order_default {
                        Some((5.0, 3.0))
                    } else {
                        None
                    },
                );
                c.p.circle_filled(xfp.pt(x + 13.0, py + 13.0), xfp.len(8.0), order_col);
                c.txt(
                    xfp,
                    x + 13.0,
                    py + 16.5,
                    Al::M,
                    &(i + 1).to_string(),
                    tx(9.5, Color32::WHITE).bold(),
                );
                c.txt(
                    xfp,
                    x + pwid / 2.0 + 6.0,
                    py + 25.0,
                    Al::M,
                    name,
                    tx(if pwid < 150.0 { 12.0 } else { 13.0 }, OP_INK)
                        .mono()
                        .bold(),
                );
                let sub2 = [
                    decisive.then_some("decisive"),
                    p.is_some_and(|p| p.needs_goals).then_some("reads goals"),
                    Some("order-only"),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
                c.txt(
                    xfp,
                    x + pwid / 2.0,
                    py + 42.0,
                    Al::M,
                    &sub2,
                    tx(10.0, OP_MUTED),
                );
                let tier = if decisive {
                    if in_lead(i) {
                        "top tier: a nonzero key settles the rank, later pieces are not consulted"
                    } else {
                        "decisive: a nonzero key stops the comparison for the pieces after it"
                    }
                } else {
                    "consulted only when every piece before it ties"
                };
                c.hover(
                    xfp.rect(x, py, pwid, ph2),
                    piece_i(name)
                        .fact(
                            "position",
                            format!("{} of {n_ord} (highest priority first)", i + 1),
                        )
                        .fact("tier", tier)
                        .fact(
                            "operad reading",
                            "a sub-box of the chain box; the chain fills the core's order hole",
                        )
                        .cite(c_chain_key())
                        .cite(c_order_only()),
                );
                if !pv.order_default {
                    let by = py + ph2 - 13.0;
                    button(
                        c,
                        xfp,
                        x + 16.0,
                        by,
                        Glyph::Left,
                        &format!("move {name} up (sooner)"),
                        i > 0,
                        Some((
                            "plan.move_piece",
                            json!({"slot": "order", "from": i, "to": i.saturating_sub(1)}),
                        )),
                    );
                    button(
                        c,
                        xfp,
                        x + 38.0,
                        by,
                        Glyph::Right,
                        &format!("move {name} down (later)"),
                        i + 1 < n_ord,
                        Some((
                            "plan.move_piece",
                            json!({"slot": "order", "from": i, "to": i + 1}),
                        )),
                    );
                    button(
                        c,
                        xfp,
                        x + pwid - 16.0,
                        by,
                        Glyph::Cross,
                        &format!("remove {name} from the chain"),
                        true,
                        Some(("plan.remove_piece", json!({"slot": "order", "index": i}))),
                    );
                }
                if i + 1 < n_ord {
                    c.txt(
                        xf,
                        x + pwid + gap / 2.0,
                        py + ph2 / 2.0 + 4.0,
                        Al::M,
                        "·",
                        tx(18.0, order_col).bold(),
                    );
                }
                c.scene.emit("piece", &format!("order/{name}"), json!({}));
                if let Some(s_) = saved {
                    c.p = s_;
                }
            }
            for (k, r) in runs.iter().enumerate() {
                let (bx0, bx1, by) = (x_of(r.0) + 4.0, x_of(r.1) + pwid - 4.0, py + ph2 + 8.0);
                let lead = k == 0 && r.0 == 0;
                let label = if lead {
                    "decisive: the top tier (a nonzero key settles the rank)"
                } else {
                    "decisive"
                };
                let half = tw(label, 10.5, false) / 2.0;
                let lxc = (bx0 + bx1) / 2.0;
                let lxc = lxc.min(cb.0 + cb.2 - 12.0 - half).max(cb.0 + 12.0 + half);
                c.poly(
                    &[
                        xf.pt(bx0, by),
                        xf.pt(bx0, by + 6.0),
                        xf.pt(bx1, by + 6.0),
                        xf.pt(bx1, by),
                    ],
                    order_col,
                    xf.len(1.3),
                );
                c.txt(xf, lxc, by + 20.0, Al::M, label, tx(10.5, hex("#7a4a08")));
                c.hover(
                    xf.rect(bx0, by - 2.0, bx1 - bx0, 30.0),
                    info("decisive pieces")
                        .note("Whether a nonzero key settles the move's rank: the pieces after this one do not sort it further (it sits in a top tier of its own, as when the table names it, or when it wins). ns:decisive is PROPOSED vocabulary (ROADMAP Slice A).")
                        .cite(c_decisive())
                        .cite(c_chain_key()),
                );
            }
        }
    }

    // row 2: the fillable holes
    let n2 = row2.len().max(1);
    let r2w = (iw - 14.0 * (n2 as f32 - 1.0)) / n2 as f32;
    for (k, sl) in row2.iter().enumerate() {
        let x = ix0 + k as f32 * (r2w + 14.0);
        c.scene.emit("hole", sl.id, json!({"accepts": sl.accepts}));
        let col = proto_piece_color(sl.accepts);
        let rect = xf.rect(x, r2y, r2w, r2h);
        let label = format!(
            "{} : {}{}",
            sl.id,
            sl.accepts,
            if sl.id == "memo" { "" } else { "*" }
        );
        if sl.id == "memo" {
            let lit = slot_zone(
                c,
                rect,
                "memo",
                slot_i("memo")
                    .fact("type", "Memo {entries: N}, N ≥ 1")
                    .fact(
                        "filled by",
                        pv.memo
                            .map_or("nothing".to_string(), |n| format!("memo {{entries: {n}}}")),
                    )
                    .cite(c_memo())
                    .cite(c_memo_struct()),
            );
            hole_frame(c, xf, x, r2y, r2w, r2h, col, &label, false, lit);
            if let Some(n) = pv.memo {
                let an = anim_for("memo", "memo");
                let (xfp, saved) = match an {
                    Some(t) => {
                        let s_ = c.p.clone();
                        c.p.set_opacity(t);
                        (
                            about(
                                xf,
                                x + r2w / 2.0,
                                r2y + r2h / 2.0,
                                -320.0 * (1.0 - t) + 6.0 * (std::f32::consts::PI * t).sin(),
                                0.6 + 0.4 * t,
                            ),
                            Some(s_),
                        )
                    }
                    None => (xf, None),
                };
                c.rrect(
                    xfp,
                    x + 8.0,
                    r2y + 14.0,
                    r2w - 16.0,
                    r2h - 22.0,
                    6.0,
                    Some(hex("#f1faf8")),
                    Some((col, 1.5)),
                    None,
                );
                c.txt(
                    xfp,
                    x + 20.0,
                    r2y + 34.0,
                    Al::L,
                    &format!("memo {{entries: {}}}", fmt_n(n)),
                    tx(12.0, hex("#0f5f54")).mono().bold(),
                );
                for (i, l) in wrap(
                    "ct:value-preserving: the table is kept to N entries (game.rs:634-641)",
                    r2w - 64.0,
                    10.0,
                )
                .iter()
                .take(2)
                .enumerate()
                {
                    c.txt(
                        xfp,
                        x + 20.0,
                        r2y + 51.0 + 13.0 * i as f32,
                        Al::L,
                        l,
                        tx(10.0, OP_MUTED),
                    );
                }
                c.hover(
                    xfp.rect(x + 8.0, r2y + 14.0, r2w - 16.0, r2h - 22.0),
                    piece_i("memo")
                        .fact("entries", fmt_n(n))
                        .fact("operad reading", "fills the core's memo hole")
                        .cite(c_memo()),
                );
                button(
                    c,
                    xfp,
                    x + r2w - 22.0,
                    r2y + 30.0,
                    Glyph::Cross,
                    "remove the memo",
                    true,
                    Some(("plan.remove_piece", json!({"slot": "memo", "index": 0}))),
                );
                c.scene.emit("piece", "memo/memo", json!({}));
                if let Some(s_) = saved {
                    c.p = s_;
                }
            } else {
                c.txt(
                    xf,
                    x + 16.0,
                    r2y + 31.0,
                    Al::L,
                    "∅  no memo",
                    tx(12.5, hex("#8a929c")).mono(),
                );
                for (i, l) in wrap("the default table holds at most 2^23 entries (game.rs:589-594), inside a search and across calls", r2w - 30.0, 10.5).iter().take(3).enumerate() {
                    c.txt(xf, x + 16.0, r2y + 49.0 + 13.0 * i as f32, Al::L, l, tx(10.5, OP_MUTED));
                }
            }
            continue;
        }
        // a list hole: filters, heuristics (and any later list slot)
        let field = sl.field.unwrap_or(sl.id);
        let names: Vec<String> = match sl.id {
            "filters" => pv.filters.clone(),
            "heuristics" => pv.heur.clone(),
            _ => {
                let plan = c.d.plan.as_ref().map_or(Value::Null, |p| p["plan"].clone());
                // `root` is `{"order": [...]}`; field_or_default reads both shapes
                sp::field_or_default(&plan, field)
                    .iter()
                    .map(|x| s(x).to_string())
                    .collect()
            }
        };
        let is_default = sl.id == "filters" && pv.filters_default;
        let own = if names.is_empty() && !pv.filters_default || (!names.is_empty() && !is_default) {
            own_error(&mode, field, &json!(names))
        } else {
            None
        };
        let lit = slot_zone(
            c,
            rect,
            sl.id,
            slot_i(sl.id)
                .fact("type", format!("{} pieces", sl.accepts))
                .fact("filled by", if names.is_empty() { "nothing".to_string() } else { names.join(", ") })
                .fact(
                    "legal here",
                    match sl.id {
                        "filters" => "must_win and non_losing: they drop moves only where the value is already known, so either mode takes them; beam is reserved and refused in exact mode",
                        "heuristics" => "none: the one name known, static_eval, can change the value (refused in exact mode, not built in play mode)",
                        "root" => "the ode estimate (at most one; it orders the root's moves and sets the aspiration driver's start) and non_losing at the root: neither changes a value, so either mode takes them (pflow-rs plan.rs, root.rs)",
                        _ => "",
                    },
                )
                .fact("plan.rs reports", match (&own, &err) { (Some(o), Some(e)) if o != e => format!("{e} (check stops at the first error it meets, in another hole; this hole shows what check says about its own field)"), _ => String::new() })
                .cite(c_check())
                .cite(if sl.id == "filters" { c_filters() } else { c_exact_refuses() })
                .cite(c_unsound()),
        );
        hole_frame(c, xf, x, r2y, r2w, r2h, col, &label, false, lit);
        if names.is_empty() {
            c.txt(
                xf,
                x + 16.0,
                r2y + 31.0,
                Al::L,
                "∅  empty",
                tx(12.5, hex("#8a929c")).mono(),
            );
            let why = match sl.id {
                "filters" => {
                    "an explicit [] runs none: the default (must_win) is off (plan.rs:341-358)"
                        .to_string()
                }
                "heuristics" => {
                    if mode == "exact" {
                        "exact mode refuses every heuristic: static_eval, the one name known, can change the value (plan.rs:385-392)".to_string()
                    } else {
                        "play mode: static_eval is reserved, not built in this release (plan.rs:385-392)".to_string()
                    }
                }
                _ => String::new(),
            };
            for (i, l) in wrap(&why, r2w - 30.0, 10.5).iter().take(3).enumerate() {
                c.txt(
                    xf,
                    x + 16.0,
                    r2y + 49.0 + 13.0 * i as f32,
                    Al::L,
                    l,
                    tx(10.5, OP_MUTED),
                );
            }
        } else {
            // the pieces as chips, then what plan.rs says
            let mut cxp = x + 12.0;
            let mut refused = false;
            for (j, n) in names.iter().enumerate() {
                let bad = n == "beam"
                    || n == "static_eval"
                    || sp::piece_info(n).is_none_or(|p| p.status != "built");
                refused |= bad;
                let colp = if bad { OP_LEAK } else { col };
                let w = tw(n, 11.0, true) + if is_default { 16.0 } else { 38.0 };
                let an = anim_for(sl.id, n);
                let (xfp, saved) = match an {
                    Some(t) => {
                        let s_ = c.p.clone();
                        c.p.set_opacity(t);
                        (
                            about(
                                xf,
                                cxp + w / 2.0,
                                r2y + 29.0,
                                -200.0 * (1.0 - t),
                                0.6 + 0.4 * t,
                            ),
                            Some(s_),
                        )
                    }
                    None => (xf, None),
                };
                c.rrect(
                    xfp,
                    cxp,
                    r2y + 17.0,
                    w,
                    22.0,
                    5.0,
                    Some(Color32::WHITE),
                    Some((colp, 1.4)),
                    if is_default { Some((5.0, 3.0)) } else { None },
                );
                c.txt(
                    xfp,
                    cxp + 8.0,
                    r2y + 32.0,
                    Al::L,
                    n,
                    tx(11.0, colp).mono().bold(),
                );
                c.hover(
                    xfp.rect(cxp, r2y + 17.0, w, 22.0),
                    piece_i(n).fact("position", format!("{} of {}", j + 1, names.len())),
                );
                if !is_default {
                    button(
                        c,
                        xfp,
                        cxp + w - 13.0,
                        r2y + 28.0,
                        Glyph::Cross,
                        &format!("remove {n}"),
                        true,
                        Some(("plan.remove_piece", json!({"slot": sl.id, "index": j}))),
                    );
                }
                c.scene.emit("piece", &format!("{}/{n}", sl.id), json!({}));
                if let Some(s_) = saved {
                    c.p = s_;
                }
                cxp += w + 8.0;
            }
            let why = if let Some(o) = &own {
                format!("refused: {o}")
            } else if refused {
                format!(
                    "refused: {}",
                    err.clone().unwrap_or_else(|| "not allowed".into())
                )
            } else if is_default {
                "default (absent): sound-pruning, drops moves only where the value is already known (plan.rs:341-358)".to_string()
            } else if sl.id == "root" {
                "run once at the position searched: ode orders the root's moves, non_losing drops those that lose at once; neither changes a value".to_string()
            } else {
                "sound-pruning: drops moves only where the value is already known, so legal in either mode (plan.rs:41-43)".to_string()
            };
            let colw = if own.is_some() || refused {
                OP_LEAK
            } else {
                OP_MUTED
            };
            for (i, l) in wrap(&why, r2w - 30.0, 10.0).iter().take(3).enumerate() {
                c.txt(
                    xf,
                    x + 16.0,
                    r2y + 54.0 + 12.5 * i as f32,
                    Al::L,
                    l,
                    tx(10.0, colw),
                );
            }
        }
    }

    // row 3: the designed holes, never fillable
    let n3 = row3.len().max(1);
    let r3w = (iw - 14.0 * (n3 as f32 - 1.0)) / n3 as f32;
    for (k, sl) in row3.iter().enumerate() {
        let x = ix0 + k as f32 * (r3w + 14.0);
        c.scene.emit("hole", sl.id, json!({"accepts": sl.accepts}));
        let why = if sl.id == "bounds" {
            "designed, not built: plan.rs has no Bound kind, so nothing can fill this hole (ROADMAP:307)"
        } else {
            "designed, not built: no engine choice; game.rs picks BitModel or IndexedModel itself (ROADMAP:307)"
        };
        let lit = slot_zone(
            c,
            xf.rect(x, r3y, r3w, r3h),
            sl.id,
            slot_i(sl.id)
                .fact("fillable", "never: every drop is refused (designed in the ROADMAP, not a field of SearchPlan)")
                .cite(c_no_bound()),
        );
        hole_frame(
            c,
            xf,
            x,
            r3y,
            r3w,
            r3h,
            hex("#9aa1aa"),
            &format!("{} : {}", sl.id, sl.accepts),
            true,
            lit,
        );
        for (i, l) in wrap(why, r3w - 32.0, 10.5).iter().enumerate() {
            c.txt(
                xf,
                x + 16.0,
                r3y + 30.0 + 14.0 * i as f32,
                Al::L,
                l,
                tx(10.5, hex("#7d848e")),
            );
        }
    }

    // ---- captions and legend ----
    let cap = [
        ("Nesting is composition, and the code fixes the nesting: plan.rs reads drivers into flags (plan.rs:205-281) and game.rs counts nodes across the whole", OP_INK),
        ("null-window run (game.rs:902-935), so budget is always the outer box. The JSON order of drivers does not change the nesting (plan.rs:35-38).", OP_INK),
        ("A chain is a monoid under concatenation, [] the identity, and the composite is lexicographic (ROADMAP:420-423; order.rs:197-244, 259-275). A decisive piece's", hex("#7a4a08")),
        ("nonzero key settles the rank, so decisive pieces that lead the chain form a top tier (order.rs:28-31). Partial in code: at most 8 pieces (order.rs:13), none twice (plan.rs:284-298).", hex("#7a4a08")),
    ];
    for (i, (t, col)) in cap.iter().enumerate() {
        c.txt(
            xf,
            14.0,
            cap_y + [0.0, 16.0, 36.0, 52.0][i],
            Al::L,
            t,
            tx(11.5, *col),
        );
    }
    c.hover(
        xf.rect(14.0, cap_y - 12.0, 1120.0, 76.0),
        info("the laws behind the nesting")
            .note("Drivers are endomorphisms on search composed around the core; in general their order matters, but the drivers are flags and a budget and the nesting is fixed.")
            .cite(c_drivers())
            .cite(c_run())
            .cite(c_driver_order())
            .cite(c_monoid())
            .cite(c_chain_new())
            .cite(c_chain_key()),
    );
    let mut lx = 14.0;
    let sw = |c: &Ctx, lx: &mut f32, color: Color32, label: &str, dashed: bool, wire: bool| {
        if wire {
            c.line(
                xf.pt(*lx, leg_y - 4.0),
                xf.pt(*lx + 26.0, leg_y - 4.0),
                color,
                xf.len(2.4),
            );
            *lx += 32.0;
        } else {
            c.rrect(
                xf,
                *lx,
                leg_y - 11.0,
                22.0,
                13.0,
                3.0,
                Some(Color32::WHITE),
                Some((color, 1.5)),
                if dashed { Some((4.0, 3.0)) } else { None },
            );
            *lx += 28.0;
        }
        c.txt(xf, *lx, leg_y, Al::L, label, tx(11.0, SLATE));
        *lx += tw(label, 11.0, false) + 18.0;
    };
    sw(c, &mut lx, pos_c, "Position", false, true);
    sw(c, &mut lx, val_c, "Value⊥", false, true);
    sw(c, &mut lx, drv_col, "driver: a one-hole box", false, false);
    sw(c, &mut lx, order_col, "order piece", false, false);
    sw(c, &mut lx, proto_piece_color("Memo"), "memo", false, false);
    sw(
        c,
        &mut lx,
        proto_piece_color("Filter"),
        "filter hole",
        false,
        false,
    );
    sw(
        c,
        &mut lx,
        proto_piece_color("Heuristic"),
        "heuristic hole",
        false,
        false,
    );
    sw(
        c,
        &mut lx,
        hex("#9aa1aa"),
        "designed hole / driver off",
        true,
        false,
    );
    c.txt(
        xf,
        14.0,
        leg_y + 22.0,
        Al::L,
        "Drag a piece from the palette onto a hole: only the holes plan.rs's check accepts light up; hover a dark one for its refusal. Hover anything for its source line.",
        tx(10.5, OP_MUTED),
    );

    plan_warn(c);
    c.note("honesty", "Holes are typed by piece kind, not by wires: a piece has no wires of its own, so this is read as an operad in the sense of Fong-Spivak (Rough Def. 6.91: types, operations, substitution), not as Spivak's cospan operad of wires. The reading is partial: SearchPlan::check refuses some fillings of the right kind (an order piece listed twice, plan.rs:284-298; a heuristic, or the filter beam, plan.rs:385-392), as the ROADMAP's \"partial monoid\" says (ROADMAP:420-423). Only the search's interface, Position → Value⊥ (ROADMAP:428-430), is drawn as wires.");
}

/// The index of driver `id` in the plan's `drivers` list (a budget is the
/// object with that key), if listed.
fn pv_driver_index(plan: &Option<Value>, id: &str) -> Option<usize> {
    let list = plan.as_ref()?["plan"]["drivers"].as_array()?;
    list.iter().position(|d| driver_name(d) == id)
}

impl Scene {
    /// `emit(kind, id, {})` when `on`.
    fn emit_if(&mut self, on: bool, kind: &str, id: &str) {
        if on {
            self.emit(kind, id, json!({}));
        }
    }
}
