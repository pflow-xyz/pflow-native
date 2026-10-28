//! The string view, pattern level, Connect-4 example: the prototype's
//! `c4Pattern` (js/views/string.js:1096-1377), the ROADMAP Slice B picture of
//! the plan. The alpha-beta core `Position → Value⊥`, the order chain as
//! `MoveList → MoveList` boxes on its parameter wire, the filters on the same
//! wire, the memo as a side wire (Table) and the drivers as nested frames,
//! every piece read from `plan.state`. Feedback (the null-window loop, the
//! recursion, the transposition table, cutoffs) is drawn as dashed red back
//! edges, each read from game.rs and cited to its lines.
//!
//! Every pflow-rs line cite here was checked against the pinned rev
//! (`search_plan::PLAN_RS_REV`, aacb7eb), where game.rs, order.rs and plan.rs
//! have all moved since the prototype's c396d46.

use super::b_draw::{dv, theme, *};
use super::b_orders::{hover_path, road_cite, R4};
use super::*;

const W: f32 = 1150.0;
const H: f32 = 778.0;
const MEMO: R4 = (636.0, 56.0, 196.0, 44.0);
const BUDGET: R4 = (40.0, 118.0, 888.0, 620.0);
const NW: R4 = (70.0, 160.0, 828.0, 548.0);
const CORE: R4 = (400.0, 272.0, 256.0, 262.0);
const COL_X: f32 = 130.0;
const COL_W: f32 = 160.0;
const COL_TOP: f32 = 244.0;
const COL_BH: f32 = 25.0;
const COL_GAP: f32 = 24.0;

/// "pflow-rs crates/pflow-metamodel/src/game.rs:593-693 @aacb7eb (note)".
fn rs(path: &str, from: u32, to: u32, note: &str) -> String {
    format!(
        "pflow-rs crates/pflow-metamodel/src/{path}:{from}-{to} @{} ({note})",
        sp::PLAN_RS_REV
    )
}

mod cites {
    // game.rs at aacb7eb
    pub const VALUE: (u32, u32, &str) = (
        809,
        814,
        "Solver::value: the exact value, or None past the budget",
    );
    pub const RUN: (u32, u32, &str) = (
        902,
        935,
        "run: nodes reset, the bit engine, then again by counts when it spills",
    );
    pub const SOLVE: (u32, u32, &str) = (
        936,
        996,
        "Solver::solve: yes/no searches, one per gap between adjacent rewards",
    );
    pub const SEARCH: (u32, u32, &str) = (
        1046,
        1179,
        "Solver::search: fail-soft alpha-beta with the transposition table",
    );
    pub const TT_READ: (u32, u32, &str) = (1069, 1079, "tt.get: an exact entry or a bound outside the window returns; else the window narrows and best goes first");
    pub const BUDGET: (u32, u32, &str) = (
        1057,
        1060,
        "nodes += 1, past the budget (or the clock) the search returns None",
    );
    pub const CHILDREN: (u32, u32, &str) = (
        1151,
        1166,
        "the child loop: v = search(next, a, b); the bound moves to v; a >= b is a cutoff",
    );
    pub const CUTOFF: (u32, u32, &str) =
        (1163, 1166, "a >= b: chain.cutoff(history, t), then break");
    pub const STORE: (u32, u32, &str) = (1181, 1185, "Solver::store: lo, hi, best into the table");
    pub const STORE_CALLS: (u32, u32, &str) = (
        1175,
        1177,
        "merged with what was known before the window narrowed, then stored",
    );
    pub const MEMO: (u32, u32, &str) = (
        588,
        598,
        "Solver::MEMO = 2^23 entries; the table is bounded inside a search and across calls",
    );
    pub const WITH_MEMO: (u32, u32, &str) = (
        635,
        641,
        "with_memo: at most `entries`; a weaker position is replaced",
    );
    pub const ENGINE: (u32, u32, &str) = (13, 21, "BitModel or IndexedModel, by the marking");
    pub const CHAIN: (u32, u32, &str) = (
        196,
        281,
        "Chain: keys compared piece by piece, a decisive piece's nonzero key settles the rank; cutoff reaches every piece",
    );
    pub const KEY: (u32, u32, &str) = (
        260,
        275,
        "Chain::key and before: the first key to differ decides",
    );
    pub const DECISIVE: (u32, u32, &str) = (
        26,
        37,
        "Orderer::DECISIVE (tt_best, immediate_win: true; history: false)",
    );
    pub const MAX8: (u32, u32, &str) = (220, 226, "Chain::new: at most 8 pieces");
    pub const DEFAULT_CHAIN: (u32, u32, &str) =
        (205, 216, "Chain::default: tt_best, immediate_win, history");
    pub const HISTORY: (u32, u32, &str) = (68, 76, "History: key = the count, cutoff adds one");
    pub const CHAIN_CUTOFF: (u32, u32, &str) = (277, 281, "Chain::cutoff: every piece is told");
    // plan.rs at aacb7eb
    pub const PLAN_DRIVERS: (u32, u32, &str) = (29, 38, "drivers: null_window (default on), mtd_f, aspiration, budget; each at most once; the search runs budget(null_window(core)) whatever the list's order");
    pub const PLAN_FLAGS: (u32, u32, &str) = (
        407,
        427,
        "solver_with: the plan's drivers become flags on the Solver",
    );
    pub const PLAN_FILTERS: (u32, u32, &str) = (41, 43, "filters: drop moves only where the value is known, legal in either mode; default Filter::DEFAULT");
    pub const PLAN_MEMO: (u32, u32, &str) =
        (39, 40, "memo: {entries: N} keeps the table to N entries");
    pub const PLAN_ORDER_TWICE: (u32, u32, &str) =
        (291, 295, "an order piece listed twice is refused");
}

fn c(x: (u32, u32, &str), path: &str) -> String {
    rs(path, x.0, x.1, x.2)
}
fn g(x: (u32, u32, &str)) -> String {
    c(x, "game.rs")
}
fn o(x: (u32, u32, &str)) -> String {
    c(x, "order.rs")
}
fn p(x: (u32, u32, &str)) -> String {
    c(x, "plan.rs")
}

fn wire_info(ty: &str, def: &str, cites: &[String], extra: &[(&str, &str)]) -> Info {
    let mut i = info(format!("wire {ty}"))
        .fact(
            "concept",
            "https://ontology.pflow.xyz/ns#WireType (PROPOSED, ROADMAP Phase 6)",
        )
        .fact("type", ty)
        .note(def);
    for (k, v) in extra {
        i = i.fact(k, *v);
    }
    for x in cites {
        i = i.cite(x.clone());
    }
    i.cite(road_cite(418, 430, "search composition: orderer monoid, drivers around the core, Solver = drivers ∘ core[…], core Position → Value⊥"))
}

fn piece_info_b(name: &str) -> Info {
    let Some(pi) = sp::piece_info(name) else {
        return info(name);
    };
    let status = match pi.status {
        "built" => format!("built (in plan.rs / order.rs at {})", sp::PLAN_RS_REV),
        "reserved" => "reserved: named only to be refused".to_string(),
        _ => {
            "PROPOSED in the ontology ROADMAP (approved 2026-10-27); plan.rs refuses it as unknown"
                .to_string()
        }
    };
    let mut i = info(format!("{} piece {name}", pi.kind.to_lowercase()))
        .fact("concept", "https://ontology.pflow.xyz/ns#SearchPiece (PROPOSED vocabulary, ROADMAP Slice A)")
        .fact("kind", pi.kind)
        .fact("status", status)
        .fact("contract (ROADMAP ct:*)", pi.contract)
        .note(format!("{} ns:SearchPiece and the ct:* contracts are PROPOSED vocabulary (ontology ROADMAP Phase 5); the names, kinds and refusals are plan.rs's.", pi.doc))
        .cite(format!("pflow-rs crates/pflow-metamodel/src/plan.rs @{}", sp::PLAN_RS_REV));
    if pi.kind == "Order" {
        i = i.fact(
            "decisive",
            if pi.decisive {
                "yes: a nonzero key settles the rank (order.rs:26-37, 260-270)"
            } else {
                "no"
            },
        );
        if !pi.reads.is_empty() {
            i = i.fact("reads", pi.reads);
        }
    }
    if pi.needs_goals {
        i = i.fact("needs", "the net's goals (Connect-4 declares them)");
    }
    i
}

fn slot_info_b(id: &str) -> Info {
    let Some(sl) = sp::slot(id) else {
        return info(id);
    };
    let mut i = info(format!("slot: {}", sl.label))
        .fact(
            "concept",
            format!(
                "https://ontology.pflow.xyz/ns#SearchPieceKind ({})",
                if sl.designed {
                    "designed, not built".to_string()
                } else {
                    format!("takes {} pieces", sl.accepts)
                }
            ),
        )
        .note(if sl.designed {
            "A slot the design has and plan.rs does not: shown, never fillable.".to_string()
        } else {
            format!("SearchPlan field \"{}\".", sl.field.unwrap_or(""))
        });
    i = i.cite(road_cite(
        418,
        430,
        "search composition: Solver = drivers ∘ core[order chain, filters, bounds, memo]",
    ));
    i
}

/// A slot as a drop target: lit only where `plan.slot_candidates` says the
/// piece may go (the same rule as `Ctx::slot`); drawn by the caller.
fn snap_slot(c: &mut Ctx, r: Rect, slot: &str, i: Info) -> bool {
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
    let mut i = i;
    if let Some(x) = &v {
        i = i.fact(
            if x["ok"] == true { "drop" } else { "refused" },
            s(&x["why"]).to_string() + if x["ok"] == true { "legal" } else { "" },
        );
    }
    c.hover(r, i);
    if c.released && c.pointer.is_some_and(|q| r.contains(q)) {
        if let (Some(Drag::Piece { name, .. }), Some(x)) = (c.drag, &v) {
            if x["ok"] == true {
                let n = name.clone();
                c.command("plan.add_piece", json!({"piece": n, "slot": slot}));
            }
        }
    }
    lit
}

fn lit_ring(d: &D, r: R4) {
    d.rect(
        r.0 - 3.0,
        r.1 - 3.0,
        r.2 + 6.0,
        r.3 + 6.0,
        8.0,
        Color32::TRANSPARENT,
        LIT,
        3.0,
        None,
    );
}

/// The × that removes a piece (`plan.remove_piece`).
fn remove_x(c: &mut Ctx, d: &D, at: Pt, slot: &str, index: usize, label: &str) {
    d.disc(at, 7.0, Color32::WHITE, hex("#d4d8dd"), 1.0);
    d.text(
        (at.0, at.1 + 3.5),
        Align2::CENTER_BOTTOM,
        "×",
        11.0,
        hex("#8a9099"),
        false,
    );
    let r = d.rect_r(at.0 - 8.0, at.1 - 8.0, 16.0, 16.0);
    c.hover(r, info(label).fact("remove", "click: plan.remove_piece"));
    if c.clicked && c.pointer.is_some_and(|q| r.contains(q)) && c.drag.is_none() {
        c.command("plan.remove_piece", json!({"slot": slot, "index": index}));
    }
}

fn port(d: &D, at: Pt, ty: &str) {
    d.dot(at, dv::PORT_R, theme::search_type(ty));
}

/// The names in a plan's driver list, and its budget (`{"budget": {"nodes": N}}`).
fn driver_info(plan: &Value, drivers: &[Value]) -> (bool, bool, Option<Value>, String) {
    let names: Vec<String> = drivers.iter().map(driver_name).collect();
    let nw = names.iter().any(|n| n == "null_window");
    let mtd = names.iter().any(|n| n == "mtd_f");
    let budget = drivers.iter().find_map(|d| d.get("budget")).cloned();
    let _ = plan;
    (nw, mtd, budget, names.join(", "))
}

pub(super) fn draw(c: &mut Ctx, area: Rect) {
    let pal_area = Rect::from_min_max(
        Pos2::new(area.left(), area.bottom() - 40.0),
        area.right_bottom(),
    );
    let area = Rect::from_min_max(
        area.left_top(),
        Pos2::new(area.right(), area.bottom() - 44.0),
    );
    let (plan, order, drivers, filters, heuristics) = plan_lists(c);
    let explicit = |f: &str| plan.get(f).is_some_and(|v| v.is_array());
    let (null_window, mtd_f, budget, _) = driver_info(&plan, &drivers);
    let has_memo = plan.get("memo").is_some_and(|m| !m.is_null());
    let memo_n = plan["memo"]["entries"].as_u64().unwrap_or(0);
    let k = (area.width() / W).min(area.height() / H).min(1.4);
    let d = D {
        p: c.p.clone(),
        xf: Xf {
            origin: area.left_top(),
            k,
        },
    };
    d.rect(
        0.0,
        0.0,
        W,
        H,
        4.0,
        dv::PAPER,
        Color32::TRANSPARENT,
        0.0,
        None,
    );

    // the merge animation: a piece slides into its slot, the chain re-flows
    let ev = c.anim(|e| match e {
        Event::PieceAdded { name, slot } => Some((name.clone(), slot.clone())),
        _ => None,
    });
    let added: Option<(String, String)> = ev.as_ref().map(|((n, s_), _)| (n.clone(), s_.clone()));
    let at = ev.as_ref().map_or(1.0, |(_, t)| *t);

    // head: the composite, as the ROADMAP writes it
    let mut expr = format!(
        "alpha-beta[{}{}]",
        if order.is_empty() {
            "[]".to_string()
        } else {
            order.join(" · ")
        },
        if has_memo {
            format!(", memo {}", fmt_n(memo_n as f64))
        } else {
            String::new()
        }
    );
    if null_window || mtd_f {
        expr = format!("{}({expr})", if mtd_f { "mtd_f" } else { "null_window" });
    }
    if let Some(b) = &budget {
        expr = format!("budget({expr})");
        let _ = b;
    }
    d.text(
        (40.0, 28.0),
        Align2::LEFT_BOTTOM,
        "Solver =",
        15.0,
        theme::PAPER_INK,
        false,
    );
    d.text(
        (40.0 + d.tw("Solver = ", 15.0), 28.0),
        Align2::LEFT_BOTTOM,
        &expr,
        13.5,
        theme::PAPER_INK,
        true,
    );
    let mode = s(&plan["mode"]);
    let mode = if mode.is_empty() { "exact" } else { mode };
    let check_ok =
        c.d.plan
            .as_ref()
            .is_some_and(|pl| pl["check"]["ok"] == true);
    d.text(
        (40.0, 48.0),
        Align2::LEFT_BOTTOM,
        &format!(
            "mode {mode} · {} · {}",
            if check_ok { "SearchPlan::check and solver(connect-4) accept it" } else { "refused by SearchPlan::check" },
            if mode == "play" { "play lets choose guess when the budget runs out (plan.rs:15-19, 421); value() is the same search" } else { "drivers ∘ core[order chain, filters, bounds, memo] (ROADMAP Phase 6)" }
        ),
        11.0,
        theme::MUTED,
        false,
    );

    // ------------------------------------------------ frames: drivers, nested
    let frame = |c: &mut Ctx,
                 r: R4,
                 name: &str,
                 sub: &str,
                 empty: Option<&str>,
                 remove: Option<usize>,
                 piece: &str,
                 pop: f32| {
        let (x, y, w, h) = r;
        let fill = if empty.is_some() {
            Color32::TRANSPARENT
        } else {
            hex("#2f6fb3").gamma_multiply(if name == "budget" { 0.035 } else { 0.05 })
        };
        d.rect(
            x,
            y,
            w,
            h,
            12.0,
            fill,
            theme::piece_kind("Driver"),
            if empty.is_some() { 1.2 } else { 1.5 },
            empty.map(|_| (6.0, 4.0)),
        );
        let label = name.to_string();
        let tab_w = d.twm(&label, 11.5)
            + if sub.is_empty() {
                0.0
            } else {
                d.twm(&format!(" {sub}"), 10.0) * 1.05
            }
            + 22.0
            + if remove.is_some() { 18.0 } else { 0.0 };
        let (tx, ty) = (x + 12.0, y - 11.0);
        let sc = pop.clamp(0.0, 1.0);
        d.rect(
            tx,
            ty,
            tab_w,
            22.0,
            11.0,
            Color32::WHITE,
            theme::piece_kind("Driver"),
            1.2 + if sc < 1.0 { 1.5 } else { 0.0 },
            empty.map(|_| (4.0, 3.0)),
        );
        d.text(
            (x + 23.0, y + 4.0 + 3.0),
            Align2::LEFT_BOTTOM,
            name,
            11.5,
            theme::piece_kind("Driver"),
            true,
        );
        if !sub.is_empty() {
            d.text(
                (x + 23.0 + d.twm(&label, 11.5), y + 4.0 + 3.0),
                Align2::LEFT_BOTTOM,
                &format!(" {sub}"),
                10.0,
                hex("#4a6f99"),
                true,
            );
        }
        let tab = d.rect_r(tx, ty, tab_w, 22.0);
        let pi = if piece.is_empty() {
            slot_info_b("drivers")
        } else {
            piece_info_b(piece)
        };
        let i = pi
            .fact("drawn as", if empty.is_some() { "an empty frame: no driver here" } else { "a functor box (frame) around the search (ROADMAP: drivers are endomorphisms on search)" })
            .fact("ROADMAP op", "op:wrap")
            .fact("nesting", "fixed by plan.rs/game.rs: the search runs budget(null_window(core)); the node count is reset in run() and checked at every node")
            .cite(p(cites::PLAN_DRIVERS))
            .cite(p(cites::PLAN_FLAGS))
            .cite(g(cites::BUDGET))
            .cite(g(cites::RUN))
            .cite(g(cites::SOLVE));
        let lit = snap_slot(c, tab, "drivers", i);
        if lit {
            lit_ring(&d, (tx, ty, tab_w, 22.0));
        }
        if let Some(idx) = remove {
            remove_x(
                c,
                &d,
                (tx + tab_w - 12.0, y),
                "drivers",
                idx,
                &format!("remove {name}"),
            );
        }
        if let Some(e) = empty {
            d.text(
                (x + 24.0 + tab_w, y + 18.0 + 3.0),
                Align2::LEFT_BOTTOM,
                e,
                10.0,
                theme::MUTED,
                false,
            );
        }
    };
    let d_index = |name: &str| drivers.iter().position(|x| driver_name(x) == name);
    let pop_for = |name: &str| -> f32 {
        match &added {
            Some((n, sl)) if n == name && sl == "drivers" => at,
            _ => 1.0,
        }
    };
    if let Some(b) = &budget {
        let nodes = b["nodes"].as_u64();
        let sub = match (nodes, b["ms"].as_u64()) {
            (Some(n), Some(ms)) => format!("{{\"nodes\": {n}, \"ms\": {ms}}}"),
            (Some(n), None) => format!("{{\"nodes\": {n}}}"),
            (None, Some(ms)) => format!("{{\"ms\": {ms}}}"),
            _ => String::new(),
        };
        frame(
            c,
            BUDGET,
            "budget",
            &sub,
            None,
            d_index("budget"),
            "budget",
            pop_for("budget"),
        );
        c.scene.emit("frame", "budget", json!({}));
    }
    if null_window {
        frame(
            c,
            NW,
            "null_window",
            if explicit("drivers") {
                ""
            } else {
                "(default: on when drivers is not given)"
            },
            None,
            Some(d_index("null_window").unwrap_or(0)),
            "null_window",
            pop_for("null_window"),
        );
        c.scene.emit("frame", "null_window", json!({}));
    } else if mtd_f {
        frame(
            c,
            NW,
            "mtd_f",
            "",
            None,
            d_index("mtd_f"),
            "mtd_f",
            pop_for("mtd_f"),
        );
    } else {
        frame(
            c,
            NW,
            "drivers",
            "",
            Some(if budget.is_some() {
                "null_window off: drivers given without it, so one full-window search (plan.rs:29-31, game.rs:947-959)"
            } else {
                "none: one full-window search, no budget (game.rs:947-959)"
            }),
            None,
            "",
            1.0,
        );
    }
    if let Some(i) = d_index("aspiration") {
        // aspiration: a tab on the inner frame (it starts the search from the root's estimate)
        let (x, y) = (NW.0 + 300.0, NW.1 - 11.0);
        d.rect(
            x,
            y,
            90.0,
            22.0,
            11.0,
            Color32::WHITE,
            theme::piece_kind("Driver"),
            1.2,
            None,
        );
        d.text(
            (x + 12.0, y + 15.0),
            Align2::LEFT_BOTTOM,
            "aspiration",
            11.0,
            theme::piece_kind("Driver"),
            true,
        );
        let r = d.rect_r(x, y, 90.0, 22.0);
        c.hover(r, piece_info_b("aspiration").cite(p(cites::PLAN_DRIVERS)));
        remove_x(c, &d, (x + 82.0, y), "drivers", i, "remove aspiration");
    }

    // ------------------------------------------------ the core
    let (cx, cy, cw, ch) = CORE;
    let (cr, cb) = (cx + cw, cy + ch);
    let pp = |name: &str| -> Pt {
        match name {
            "pos" => (cx + 50.0, cy),
            "tt" => (cx + 180.0, cy),
            "store" => (cx + 230.0, cy),
            "ranked" => (cx, cb - 16.0),
            "cutoff" => (cx, cb - 50.0),
            "window" => (cr, cy + 40.0),
            "child_in" => (cr, cy + 102.0),
            "value" => (cx + 100.0, cb),
            _ => (cx + 220.0, cb),
        }
    };
    d.rect(
        cx,
        cy,
        cw,
        ch,
        dv::RX,
        Color32::WHITE,
        dv::STROKE,
        dv::SW,
        None,
    );
    d.text(
        (cx + cw / 2.0, cy + 118.0 + 4.0),
        Align2::CENTER_BOTTOM,
        "alpha-beta",
        15.0,
        dv::STROKE,
        false,
    );
    let ty = cy + 136.0 + 3.0;
    let (w1, w2, w3) = (
        d.tw("Position", 11.5),
        d.tw(" → ", 11.5),
        d.tw("Value⊥", 11.5),
    );
    let x0 = cx + cw / 2.0 - (w1 + w2 + w3) / 2.0;
    d.text(
        (x0, ty),
        Align2::LEFT_BOTTOM,
        "Position",
        11.5,
        theme::search_type("Position"),
        false,
    );
    d.text(
        (x0 + w1, ty),
        Align2::LEFT_BOTTOM,
        " → ",
        11.5,
        dv::STROKE,
        false,
    );
    d.text(
        (x0 + w1 + w2, ty),
        Align2::LEFT_BOTTOM,
        "Value⊥",
        11.5,
        theme::search_type("Value⊥"),
        false,
    );
    d.text(
        (cx + cw / 2.0, cy + 154.0 + 3.0),
        Align2::CENTER_BOTTOM,
        "Solver::search (game.rs:1046-1179)",
        10.0,
        dv::LABEL_FILL,
        true,
    );
    d.text(
        (cx + cw / 2.0, cy + 168.0 + 3.0),
        Align2::CENTER_BOTTOM,
        "fail-soft, with the transposition table",
        9.5,
        Color32::from_gray(0x88),
        false,
    );
    let lab = |x: f32, y: f32, t: &str, a: Align2| {
        d.text((x, y + 3.0), a, t, 9.0, Color32::from_gray(0x77), false);
    };
    lab(pp("pos").0, cy + 14.0, "Position", Align2::CENTER_BOTTOM);
    lab(pp("tt").0, cy + 14.0, "tt.get", Align2::CENTER_BOTTOM);
    lab(
        pp("store").0 + 4.0,
        cy + 26.0,
        "store",
        Align2::CENTER_BOTTOM,
    );
    lab(
        cx + 9.0,
        pp("ranked").1 + 3.0,
        "kids, best first",
        Align2::LEFT_BOTTOM,
    );
    lab(
        cx + 9.0,
        pp("cutoff").1 + 3.0,
        "cutoff",
        Align2::LEFT_BOTTOM,
    );
    lab(
        cr - 9.0,
        pp("window").1 + 3.0,
        "window (a, b)",
        Align2::RIGHT_BOTTOM,
    );
    lab(
        cr - 9.0,
        pp("child_in").1 + 3.0,
        "child values",
        Align2::RIGHT_BOTTOM,
    );
    lab(pp("value").0, cb - 8.0, "Value⊥", Align2::CENTER_BOTTOM);
    lab(pp("child_out").0, cb - 8.0, "v", Align2::CENTER_BOTTOM);
    for (n, ty) in [
        ("pos", "Position"),
        ("tt", "Table"),
        ("store", "Table"),
        ("ranked", "MoveList"),
        ("cutoff", "Cutoff"),
        ("window", "Window"),
        ("child_in", "Value⊥"),
        ("value", "Value⊥"),
        ("child_out", "Value⊥"),
    ] {
        port(&d, pp(n), ty);
    }
    c.scene.emit("box", "core", json!({}));
    c.hover(
        d.rect_r(cx, cy, cw, ch),
        info("core: alpha-beta (Solver::search)")
            .fact("concept", "https://ontology.pflow.xyz/ns#Box (PROPOSED, ROADMAP Phase 6)")
            .fact("dom → cod", "Position → Value⊥ (ROADMAP Phase 6)")
            .fact("⊥", "None: past the budget (game.rs:1057-1060), or the bit engine spilled (game.rs:1100, 1117) and run() retries by counts (game.rs:926-935)")
            .fact("parameters", "the order chain (MoveList), the filters, the memo (Table)")
            .fact("ROADMAP op", "op:fill: core[order chain, filters, bounds, memo]")
            .note("Fail-soft alpha-beta in the +1 side's view. Inside the bounds every result is a bound on the true value; with the window the whole range, it is the value. (game.rs:1046-1049)")
            .cite(g(cites::SEARCH))
            .cite(g(cites::VALUE))
            .cite(g(cites::RUN))
            .cite(road_cite(418, 430, "search composition: core Position → Value⊥")),
    );

    // Position in, Value⊥ out
    let v_tap = (pp("value").0, cb + 50.0);
    let pos_pts: Vec<Pt> = flatten(
        [
            (pp("pos").0, 60.0),
            (pp("pos").0, 120.0),
            (pp("pos").0, cy - 60.0),
            (pp("pos").0, cy),
        ],
        12,
    );
    d.path(&pos_pts, dv::WIRE, 1.5, None);
    d.halo(
        (pp("pos").0 + 6.0, 92.0 + 3.0),
        Align2::LEFT_BOTTOM,
        "Position",
        dv::LABEL,
        dv::LABEL_FILL,
    );
    hover_path(c, &d, &pos_pts, 6.0, wire_info("Position", "A settled marking of the game net: the position whose value is asked for (Solver::value).", &[g(cites::VALUE)], &[]));
    let val_pts = [(pp("value").0, cb), (pp("value").0, H - 6.0)];
    d.path(&val_pts, dv::WIRE, 1.5, None);
    d.halo(
        (pp("value").0 + 6.0, H - 22.0 + 3.0),
        Align2::LEFT_BOTTOM,
        "Value⊥",
        dv::LABEL,
        dv::LABEL_FILL,
    );
    if null_window {
        d.dot(v_tap, 3.0, dv::STROKE);
    }
    hover_path(
        c,
        &d,
        &val_pts,
        6.0,
        wire_info(
            "Value⊥",
            "The exact value (+1 side's view) of a settled position with both sides playing perfectly, or None past the budget (game.rs:809-810).",
            &[g(cites::VALUE), g(cites::BUDGET)],
            if null_window { &[("the dot", "each yes/no search's v also feeds the null-window loop (the dashed red back edge)")] } else { &[("one search", "no null_window: one search over the whole reward range gives the value (game.rs:947-959)")] },
        ),
    );

    // ------------------------------------------------ the order chain
    let col_x = COL_X + COL_W / 2.0;
    let tops_for = |n: usize| -> Vec<f32> {
        (0..n)
            .map(|i| COL_TOP + i as f32 * (COL_BH + COL_GAP))
            .collect()
    };
    let tops = tops_for(order.len());
    // previous positions (chain without the piece just added) for the re-flow
    let prev_idx: Vec<Option<usize>> = match &added {
        Some((n, sl)) if sl == "order" => {
            let mut j = 0;
            order
                .iter()
                .map(|x| {
                    if x == n {
                        None
                    } else {
                        j += 1;
                        Some(j - 1)
                    }
                })
                .collect()
        }
        _ => order.iter().enumerate().map(|(i, _)| Some(i)).collect(),
    };
    let prev_tops = tops_for(prev_idx.iter().flatten().count());
    let e = ease(at);
    let chain_end = if order.is_empty() {
        COL_TOP
    } else {
        tops[order.len() - 1] + COL_BH
    };
    let filters_y = chain_end + COL_GAP;
    // the filters on the same wire: pieces, or an empty slot (the identity)
    let n_f = filters.len().max(1);
    let filters_b = filters_y + n_f as f32 * COL_BH + (n_f as f32 - 1.0) * COL_GAP;
    // region
    let region: R4 = (
        COL_X - 22.0,
        COL_TOP - 52.0,
        COL_W + 44.0,
        chain_end + 14.0 - (COL_TOP - 52.0),
    );
    d.rect(
        region.0,
        region.1,
        region.2,
        region.3,
        10.0,
        hex("#c47a12").gamma_multiply(0.045),
        theme::piece_kind("Order"),
        1.0,
        Some((3.0, 3.0)),
    );
    let is_default = !explicit("order");
    d.text(
        (COL_X - 12.0, COL_TOP - 38.0 + 3.0),
        Align2::LEFT_BOTTOM,
        if is_default {
            "order chain (default)"
        } else {
            "order chain"
        },
        10.5,
        theme::piece_kind("Order"),
        false,
    );
    d.text(
        (COL_X - 12.0, COL_TOP - 26.0 + 3.0),
        Align2::LEFT_BOTTOM,
        "a monoid: [] is the identity",
        9.0,
        hex("#8a6d3b"),
        false,
    );
    let chain_pieces = if order.is_empty() {
        "[] (the identity)".to_string()
    } else {
        order.join(", ")
    };
    let mut si = slot_info_b("order")
        .fact("pieces", chain_pieces)
        .fact("law (ROADMAP)", "a monoid under chaining: [] is the identity (Chain::new(&[])), concatenation is associative (Chain::then), every composite stays ct:order-only")
        .fact("partial in code (ROADMAP)", "a repeated piece is refused (plan.rs:291-295) and a chain holds at most 8 pieces (order.rs:220-226)")
        .fact("in code", "one comparator over a key per piece, compared first-to-differ; a decisive piece's nonzero key settles the rank (order.rs:260-275)")
        .fact("ROADMAP op", "op:chain")
        .cite(o(cites::CHAIN))
        .cite(o(cites::KEY))
        .cite(o(cites::DECISIVE))
        .cite(o(cites::MAX8))
        .cite(p(cites::PLAN_ORDER_TWICE));
    if is_default {
        si = si.fact("default", "no \"order\" field: Chain::default, tt_best · immediate_win · history (order.rs:205-216)").cite(o(cites::DEFAULT_CHAIN));
    }
    let rr = d.rect_r(region.0, region.1, region.2, region.3);
    if snap_slot(c, rr, "order", si) {
        lit_ring(&d, region);
    }
    // the wire through every box
    let mut segs: Vec<(f32, f32)> = vec![(
        COL_TOP - 16.0,
        if order.is_empty() { filters_y } else { tops[0] },
    )];
    for i in 0..order.len().saturating_sub(1) {
        segs.push((tops[i] + COL_BH, tops[i + 1]));
    }
    if !order.is_empty() {
        segs.push((chain_end, filters_y));
    }
    let n_seg = segs.len();
    for (i, (a, b)) in segs.iter().enumerate() {
        d.line((col_x, *a), (col_x, *b), dv::WIRE, 1.5);
        if !(!order.is_empty() && i == n_seg - 1) {
            d.halo(
                (col_x + 7.0, (a + b) / 2.0 + 3.5 + 3.0),
                Align2::LEFT_BOTTOM,
                "MoveList",
                dv::LABEL,
                dv::LABEL_FILL,
            );
        }
    }
    // between filters, and from the last to the core
    for j in 0..n_f.saturating_sub(1) {
        let a = filters_y + j as f32 * (COL_BH + COL_GAP) + COL_BH;
        d.line((col_x, a), (col_x, a + COL_GAP), dv::WIRE, 1.5);
    }
    let ranked = pp("ranked");
    let chain_wire = flatten(
        [
            (col_x, filters_b),
            (col_x, ranked.1),
            ((col_x + cx) / 2.0, ranked.1),
            (cx, ranked.1),
        ],
        20,
    );
    d.path(&chain_wire, dv::WIRE, 1.5, None);
    hover_path(
        c,
        &d,
        &chain_wire,
        6.0,
        wire_info("MoveList", "A node's moves, ranked by the chain's keys and tried best first (game.rs:1089-1150). Drawn as the core's parameter wire (ROADMAP Phase 6), not as a loop.", &[rs("game.rs", 1089, 1150, "moves, keyed by the chain, sorted")], &[]),
    );
    for (i, name) in order.iter().enumerate() {
        let y = match (prev_idx[i], &added) {
            (Some(pi), Some(_)) => prev_tops[pi] + (tops[i] - prev_tops[pi]) * e,
            _ => tops[i],
        };
        let slide = matches!(&added, Some((n, sl)) if n == name && sl == "order");
        let x = COL_X + if slide { -90.0 * (1.0 - e) } else { 0.0 };
        let kind = sp::piece_info(name).map_or("Order", |x| x.kind);
        let flash = if slide {
            (e * std::f32::consts::PI).sin()
        } else {
            0.0
        };
        d.rect(
            x,
            y,
            COL_W,
            COL_BH,
            dv::RX,
            Color32::WHITE,
            lerp_col(dv::STROKE, theme::piece_kind(kind), flash),
            dv::SW + 1.5 * flash,
            None,
        );
        d.rect(
            x + 1.0,
            y + 4.0,
            3.5,
            COL_BH - 8.0,
            1.5,
            theme::piece_kind("Order"),
            Color32::TRANSPARENT,
            0.0,
            None,
        );
        d.text(
            (x + 12.0, y + COL_BH / 2.0 + 4.0 + 2.0),
            Align2::LEFT_BOTTOM,
            name,
            11.5,
            dv::STROKE,
            true,
        );
        if sp::piece_info(name).is_some_and(|p| p.decisive) {
            d.text(
                (x + COL_W - 8.0, y + COL_BH / 2.0 + 3.0 + 3.0),
                Align2::RIGHT_BOTTOM,
                "decisive",
                8.5,
                Color32::from_gray(0x99),
                false,
            );
        }
        port(&d, (col_x, y), "MoveList");
        port(&d, (col_x, y + COL_BH), "MoveList");
        let pi = piece_info_b(name)
            .fact(
                "place in the chain",
                format!(
                    "{} of {}: compared {} (highest priority first, plan.rs:15-16)",
                    i + 1,
                    order.len(),
                    if i == 0 {
                        "first"
                    } else {
                        "after the ones above"
                    }
                ),
            )
            .fact(
                "drawn as",
                "a box MoveList → MoveList on the core's parameter wire (ROADMAP Slice B)",
            )
            .cite(o(cites::KEY));
        c.hover(d.rect_r(x, y, COL_W, COL_BH), pi);
        remove_x(
            c,
            &d,
            (x - 12.0, y + COL_BH / 2.0),
            "order",
            i,
            &format!("remove {name} from the chain"),
        );
        c.scene.emit("chain", name, json!({"index": i}));
    }

    // filters: pieces on the wire (default must_win), or an empty slot
    let f_expl = explicit("filters");
    if filters.is_empty() {
        slot_box(
            c,
            &d,
            "filters",
            (COL_X, filters_y, COL_W, COL_BH),
            "filters",
            "empty",
            Some(col_x),
            &filters,
            f_expl,
        );
    } else {
        for (j, name) in filters.iter().enumerate() {
            let y = filters_y + j as f32 * (COL_BH + COL_GAP);
            let kind = "Filter";
            d.rect(
                COL_X,
                y,
                COL_W,
                COL_BH,
                dv::RX,
                Color32::WHITE,
                dv::STROKE,
                dv::SW,
                None,
            );
            d.rect(
                COL_X + 1.0,
                y + 4.0,
                3.5,
                COL_BH - 8.0,
                1.5,
                theme::piece_kind(kind),
                Color32::TRANSPARENT,
                0.0,
                None,
            );
            d.text(
                (COL_X + 12.0, y + COL_BH / 2.0 + 4.0 + 2.0),
                Align2::LEFT_BOTTOM,
                name,
                11.5,
                dv::STROKE,
                true,
            );
            d.text(
                (COL_X + COL_W - 8.0, y + COL_BH / 2.0 + 3.0 + 3.0),
                Align2::RIGHT_BOTTOM,
                if f_expl { "filter" } else { "default" },
                8.5,
                Color32::from_gray(0x99),
                false,
            );
            port(&d, (col_x, y), "MoveList");
            port(&d, (col_x, y + COL_BH), "MoveList");
            let r = d.rect_r(COL_X, y, COL_W, COL_BH);
            let mut pi = piece_info_b(name)
                .fact(
                    "drawn as",
                    "a box MoveList → MoveList on the core's parameter wire, after the order chain",
                )
                .cite(p(cites::PLAN_FILTERS));
            if !f_expl {
                pi = pi.fact(
                    "default",
                    "no \"filters\" field: Filter::DEFAULT (plan.rs:41-43)",
                );
            }
            if snap_slot(c, r, "filters", pi) {
                lit_ring(&d, (COL_X, y, COL_W, COL_BH));
            }
            if f_expl {
                remove_x(
                    c,
                    &d,
                    (COL_X - 12.0, y + COL_BH / 2.0),
                    "filters",
                    j,
                    &format!("remove {name}"),
                );
            }
        }
    }
    slot_box(
        c,
        &d,
        "heuristics",
        (COL_X, cb + 22.0, COL_W, COL_BH),
        "heuristics",
        "empty",
        None,
        &heuristics,
        true,
    );

    // designed, never fillable: bounds and the engine
    ghost(
        c,
        &d,
        "bounds",
        (712.0, cy - 44.0, 132.0, 36.0),
        "bounds",
        "designed: interval meet",
        &[road_cite(
            418,
            430,
            "core[order chain, filters, bounds, memo]",
        )],
    );
    ghost(
        c,
        &d,
        "engine",
        (COL_X, cb + 74.0, COL_W, 36.0),
        "engine",
        "designed: game.rs picks it itself",
        &[g(cites::ENGINE), g(cites::RUN)],
    );
    let dotted = hex("#aab0b8");
    d.path(
        &flatten(
            [
                (COL_X + COL_W, cb + 92.0),
                (cx - 30.0, cb + 92.0),
                (cx - 10.0, cb + 10.0),
                (cx + 8.0, cb),
            ],
            16,
        ),
        dotted,
        1.0,
        Some((1.5, 3.0)),
    );
    d.path(
        &flatten(
            [
                (712.0, cy - 20.0),
                (cr + 20.0, cy - 20.0),
                (cr - 12.0, cy - 14.0),
                (cr - 12.0, cy),
            ],
            16,
        ),
        dotted,
        1.0,
        Some((1.5, 3.0)),
    );

    // ------------------------------------------------ memo: a side wire into the core
    let memo_slide = matches!(&added, Some((_, sl)) if sl == "memo");
    let my = if memo_slide { -40.0 * (1.0 - e) } else { 0.0 };
    let (mx, mmy, mw, mh) = (MEMO.0, MEMO.1 + my, MEMO.2, MEMO.3);
    if has_memo {
        let flash = if memo_slide {
            (e * std::f32::consts::PI).sin()
        } else {
            0.0
        };
        d.rect(
            mx,
            mmy,
            mw,
            mh,
            dv::RX,
            Color32::WHITE,
            lerp_col(dv::STROKE, theme::piece_kind("Memo"), flash),
            dv::SW + 1.5 * flash,
            None,
        );
        d.rect(
            mx + 1.0,
            mmy + 4.0,
            3.5,
            mh - 8.0,
            1.5,
            theme::piece_kind("Memo"),
            Color32::TRANSPARENT,
            0.0,
            None,
        );
        d.text(
            (mx + 14.0, mmy + 18.0 + 3.0),
            Align2::LEFT_BOTTOM,
            "memo",
            12.0,
            dv::STROKE,
            true,
        );
        d.text(
            (mx + 14.0, mmy + 33.0 + 3.0),
            Align2::LEFT_BOTTOM,
            &format!("{{\"entries\": {memo_n}}}"),
            10.0,
            Color32::from_gray(0x55),
            true,
        );
        remove_x(
            c,
            &d,
            (mx + mw + 12.0, mmy + mh / 2.0),
            "memo",
            0,
            "remove the memo",
        );
    } else {
        d.rect(
            mx,
            mmy,
            mw,
            mh,
            dv::RX,
            Color32::WHITE,
            theme::piece_kind("Memo"),
            1.2,
            Some((5.0, 3.0)),
        );
        d.text(
            (mx + 12.0, mmy + 18.0 + 3.0),
            Align2::LEFT_BOTTOM,
            "memo: empty",
            11.0,
            theme::piece_kind("Memo"),
            true,
        );
        d.text(
            (mx + 12.0, mmy + 32.0 + 3.0),
            Align2::LEFT_BOTTOM,
            "default table: 2^23 entries (Solver::MEMO)",
            8.5,
            Color32::from_gray(0x88),
            false,
        );
    }
    port(&d, (mx + 40.0, mmy + mh), "Table");
    port(&d, (mx + mw - 40.0, mmy + mh), "Table");
    let mut mi = if has_memo {
        piece_info_b("memo")
    } else {
        slot_info_b("memo")
    };
    mi = mi
        .fact("drawn as", "a side wire (Table) into the core, outside both frames: the table persists across calls and is shared by every yes/no search")
        .cite(g(cites::MEMO))
        .cite(g(cites::WITH_MEMO))
        .cite(p(cites::PLAN_MEMO));
    mi = if has_memo {
        mi.fact("bound", format!("{} entries: the table holds at most that many, inside a search and across calls; a weaker position is replaced (game.rs:635-641)", fmt_n(memo_n as f64)))
    } else {
        mi.fact("without a memo", "the solver's own table: Solver::MEMO = 2^23 entries, bounded inside a search and across calls (game.rs:588-598)")
    };
    let mr = d.rect_r(mx, mmy, mw, mh);
    if snap_slot(c, mr, "memo", mi) {
        lit_ring(&d, (mx, mmy, mw, mh));
    }
    let table_pts = flatten(
        [
            (mx + 40.0, mmy + mh),
            (mx + 40.0, mmy + mh + 80.0),
            (pp("tt").0, cy - 90.0),
            (pp("tt").0, cy),
        ],
        20,
    );
    d.path(&table_pts, dv::WIRE, 1.5, None);
    d.halo(
        (pp("tt").0 + 34.0, cy - 70.0 + 3.0),
        Align2::LEFT_BOTTOM,
        "Table",
        dv::LABEL,
        dv::LABEL_FILL,
    );
    hover_path(c, &d, &table_pts, 6.0, wire_info("Table", "The transposition table read at every node: an exact entry or a bound outside the window returns at once, otherwise the window narrows and the remembered best move goes first (game.rs:1069-1079).", &[g(cites::TT_READ)], &[("side wire", "the memo is a parameter of the core, not inside the drivers (ROADMAP Slice B)")]));

    // ------------------------------------------------ back edges (dashed red)
    let fade = if ev.is_some() {
        ((at - 0.5) / 0.5).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let backc = dv::BACK.gamma_multiply(fade);
    let back = |c: &mut Ctx,
                pts: Vec<Pt>,
                label: &[&str],
                lpos: Pt,
                anchor: Align2,
                i: Info,
                key: &str| {
        d.path(&pts, backc, 1.5, Some(dv::BACK_DASH));
        let n = pts.len();
        d.arrow(pts[n - 1], pts[n - 2], backc);
        for (j, t) in label.iter().enumerate() {
            d.halo(
                (lpos.0, lpos.1 + 12.0 * j as f32 + 3.0),
                anchor,
                t,
                9.5,
                backc,
            );
        }
        let i = i
            .fact("drawn as", "a back edge: dashed #c44 (6,3) with an arrowhead, round the right side (diagram-viewer.js:490-501)")
            .cite(super::b_orders::dv_cite(490, 501, "back edges: dashed #c44 (6,3), routed around the right side, arrowhead"));
        hover_path(c, &d, &pts, 6.0, i);
        c.scene.emit("backedge", key, json!({}));
    };
    if null_window {
        back(
            c,
            ortho(&[v_tap, (850.0, v_tap.1), (850.0, pp("window").1), (pp("window").0 + 2.0, pp("window").1)], 14.0),
            &["null-window loop: each yes/no search", "narrows the window (game.rs:936-996)"],
            (v_tap.0 + 14.0, v_tap.1 + 16.0),
            Align2::LEFT_BOTTOM,
            info("back edge: the null-window loop")
                .note("The value as a run of yes/no searches, one per gap between adjacent rewards: if v > (a + b) / 2 then i = k + 1 else j = k, and the next search gets the window (vals[k], vals[k+1]) (game.rs:978-995).")
                .fact("driver", "null_window")
                .fact("carries", "Window (a, b), from this search's v")
                .cite(g(cites::SOLVE)),
            "nullwindow",
        );
    } else {
        d.text(
            (cr + 12.0, pp("window").1 - 2.0 + 3.0),
            Align2::LEFT_BOTTOM,
            "window: the whole reward range,",
            9.5,
            Color32::from_gray(0x88),
            false,
        );
        d.text(
            (cr + 12.0, pp("window").1 + 10.0 + 3.0),
            Align2::LEFT_BOTTOM,
            "once (game.rs:947-959)",
            9.5,
            Color32::from_gray(0x88),
            false,
        );
    }
    // recursion
    back(
        c,
        ortho(&[pp("child_out"), (pp("child_out").0, cb + 24.0), (692.0, cb + 24.0), (692.0, pp("child_in").1), (pp("child_in").0 + 2.0, pp("child_in").1)], 12.0),
        &["recursion: each child's", "value comes back", "(game.rs:1151-1166)"],
        (700.0, pp("child_in").1 + 40.0),
        Align2::LEFT_BOTTOM,
        info("back edge: alpha-beta recursion")
            .note("For each child, best first: v = search(next, a, b); the mover's bound moves to v, and a >= b is a cutoff (game.rs:1151-1166).")
            .fact("carries", "Value⊥ of a child position")
            .cite(g(cites::SEARCH))
            .cite(g(cites::CHILDREN)),
        "recursion",
    );
    // the transposition-table feedback
    let st = pp("store");
    back(
        c,
        flatten([st, (st.0, cy - 110.0), (mx + mw - 40.0, mmy + mh + 70.0), (mx + mw - 40.0, mmy + mh + 3.0)], 24),
        &["store (game.rs:1181-1185)"],
        (st.0 + 50.0, cy - 60.0),
        Align2::LEFT_BOTTOM,
        info("back edge: the transposition-table feedback")
            .note("What a search finds is stored (space allowing) and read back by tt.get at the next visit of the position, in this search or a later one (store: game.rs:1181-1185, called at 1084, 1107, 1126, 1177; read: 1069-1079).")
            .fact("carries", "Entry {lo, hi, best}")
            .cite(g(cites::STORE))
            .cite(g(cites::TT_READ))
            .cite(g(cites::STORE_CALLS)),
        "tt",
    );
    // cutoff → history (chain.cutoff tells every piece; only History records it)
    let cut = pp("cutoff");
    let hist = order.iter().position(|n| n == "history");
    let cut_info = info("back edge: cutoff → history")
        .note("a >= b: chain.cutoff(history, t) tells every piece; History adds one to the move's count (order.rs:73-75), which its key reads next time (order.rs:69-71).")
        .fact("carries", "the move that caused the cutoff")
        .cite(g(cites::CUTOFF))
        .cite(o(cites::CHAIN_CUTOFF))
        .cite(o(cites::HISTORY));
    match hist {
        Some(hi) => {
            let hy = tops[hi] + COL_BH / 2.0;
            back(
                c,
                flatten(
                    [
                        cut,
                        (cx - 50.0, cut.1),
                        (COL_X + COL_W + 44.0, hy),
                        (COL_X + COL_W + 3.0, hy),
                    ],
                    24,
                ),
                &["game.rs:1164"],
                (cx - 6.0, cut.1 - 7.0),
                Align2::RIGHT_BOTTOM,
                cut_info,
                "cutoff",
            );
        }
        None => {
            // no piece records cutoffs: the edge still runs (game.rs tells every piece), to the chain's border
            let end = (region.0 + region.2 + 3.0, filters_y - 8.0);
            back(
                c,
                flatten([cut, (cx - 50.0, cut.1), (end.0 + 44.0, end.1), end], 24),
                &["game.rs:1164"],
                (cx - 6.0, cut.1 - 7.0),
                Align2::RIGHT_BOTTOM,
                cut_info.fact(
                    "recorded by",
                    "no piece in this chain (only history keeps cutoffs)",
                ),
                "cutoff",
            );
            d.text(
                (cx - 8.0, cut.1 - 20.0 + 3.0),
                Align2::RIGHT_BOTTOM,
                "cutoffs: no piece records them",
                9.0,
                Color32::from_gray(0x88),
                false,
            );
        }
    }

    legend(&d, c);
    palette(c, pal_area);
    plan_warn(c);
    c.note("honesty", "Reading a pattern instance as a box dom → cod is the ROADMAP Phase 6 proposal. Connect-4: the core box, the chain as MoveList → MoveList boxes, the drivers as frames and the memo as a side wire are the ROADMAP design. In code the chain is one comparator over a key per piece (order.rs:196-281), and the drivers are flags (plan.rs:407-427). The nesting budget(null_window(core)) is fixed by game.rs, whatever order the JSON lists them in.");
    c.note("honesty", "A back edge is feedback, which a symmetric monoidal category does not have on its own (it needs a trace). Each one is drawn because game.rs does it, and each is cited to its lines at the pinned pflow-rs rev.");
    c.note("honesty", "Not drawn: the plan's `root` field (estimate, non_losing at the root: plan.rs:17-25) is not a slot here yet; mtd_f and aspiration are drawn as an inner frame and a tab.");
}

fn lerp_col(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

/// An empty slot (the identity), or one listing its entries.
#[allow(clippy::too_many_arguments)]
fn slot_box(
    c: &mut Ctx,
    d: &D,
    id: &str,
    r: R4,
    label: &str,
    sub: &str,
    on_wire: Option<f32>,
    list: &[String],
    explicit: bool,
) {
    let col = theme::piece_kind(if id == "filters" {
        "Filter"
    } else {
        "Heuristic"
    });
    d.rect(
        r.0,
        r.1,
        r.2,
        r.3,
        dv::RX,
        Color32::WHITE,
        col,
        1.2,
        Some((5.0, 3.0)),
    );
    d.text(
        (r.0 + 10.0, r.1 + r.3 / 2.0 + 3.5 + 3.0),
        Align2::LEFT_BOTTOM,
        &if list.is_empty() {
            label.to_string()
        } else {
            list.join(", ")
        },
        10.5,
        col,
        true,
    );
    d.text(
        (r.0 + r.2 - 8.0, r.1 + r.3 / 2.0 + 3.0 + 3.0),
        Align2::RIGHT_BOTTOM,
        sub,
        8.5,
        Color32::from_gray(0x99),
        false,
    );
    if let Some(wx) = on_wire {
        port(d, (wx, r.1), "MoveList");
        port(d, (wx, r.1 + r.3), "MoveList");
    }
    let rr = d.rect_r(r.0, r.1, r.2, r.3);
    let i = slot_info_b(id)
        .fact(
            "now",
            if list.is_empty() {
                "empty".to_string()
            } else {
                list.join(", ")
            },
        )
        .fact(
            "exact mode",
            if id == "filters" {
                "filters keep the value, so they are legal (plan.rs:41-43); `[]` runs none"
            } else {
                "refuses every heuristic: they can change the value (plan.rs:44-49)"
            },
        )
        .fact(
            "play mode",
            "parses, but every heuristic named here is \"not built in this release\"",
        )
        .fact(
            "drawn as",
            if on_wire.is_some() {
                "an empty position on the MoveList wire: an empty slot is the identity"
            } else {
                "an empty position, wired to nothing: the ROADMAP gives heuristics no type"
            },
        )
        .cite(p(cites::PLAN_FILTERS));
    let _ = explicit;
    if snap_slot(c, rr, id, i) {
        lit_ring(d, r);
    }
}

fn ghost(c: &mut Ctx, d: &D, id: &str, r: R4, l1: &str, l2: &str, cites_: &[String]) {
    d.rect(
        r.0,
        r.1,
        r.2,
        r.3,
        dv::RX,
        Color32::from_rgba_unmultiplied(255, 255, 255, 150),
        hex("#aab0b8"),
        1.0,
        Some((1.5, 3.0)),
    );
    d.text(
        (r.0 + 9.0, r.1 + 15.0 + 3.0),
        Align2::LEFT_BOTTOM,
        l1,
        10.5,
        hex("#7a828c"),
        true,
    );
    d.text(
        (r.0 + 9.0, r.1 + 28.0 + 3.0),
        Align2::LEFT_BOTTOM,
        l2,
        8.5,
        hex("#9aa1aa"),
        false,
    );
    let rr = d.rect_r(r.0, r.1, r.2, r.3);
    let mut i = slot_info_b(id)
        .fact(
            "status",
            format!(
                "designed in the ROADMAP; not a field of SearchPlan at {}",
                sp::PLAN_RS_REV
            ),
        )
        .fact("drawn as", "a ghost: shown, never fillable");
    for x in cites_ {
        i = i.cite(x.clone());
    }
    if snap_slot(c, rr, id, i) {
        lit_ring(d, r);
    }
}

fn legend(d: &D, c: &mut Ctx) {
    let lx = 948.0;
    let mut ly = 146.0;
    let ink = Color32::from_gray(0x44);
    d.text(
        (lx, ly - 18.0 + 3.0),
        Align2::LEFT_BOTTOM,
        "reading the drawing",
        10.5,
        theme::PAPER_INK,
        false,
    );
    let mut item = |draw: &dyn Fn(f32, f32), lines: &[&str]| {
        draw(lx, ly);
        for (i, t) in lines.iter().enumerate() {
            d.text(
                (lx + 34.0, ly + 4.0 + 12.0 * i as f32 + 3.0),
                Align2::LEFT_BOTTOM,
                t,
                10.0,
                ink,
                false,
            );
        }
        ly += 16.0 + lines.len() as f32 * 12.0;
    };
    item(
        &|x, y| {
            d.rect(
                x,
                y - 8.0,
                26.0,
                16.0,
                dv::RX,
                Color32::WHITE,
                dv::STROKE,
                dv::SW,
                None,
            )
        },
        &["box: a morphism,", "dom on top, cod below"],
    );
    item(
        &|x, y| {
            d.rect(
                x,
                y - 9.0,
                26.0,
                18.0,
                7.0,
                hex("#2f6fb3").gamma_multiply(0.06),
                theme::piece_kind("Driver"),
                1.4,
                None,
            )
        },
        &["frame: a driver, a functor", "box around the search"],
    );
    item(
        &|x, y| {
            d.rect(
                x,
                y - 8.0,
                26.0,
                16.0,
                dv::RX,
                Color32::WHITE,
                theme::piece_kind("Filter"),
                1.2,
                Some((5.0, 3.0)),
            )
        },
        &["dashed: an empty slot", "(the identity)"],
    );
    item(
        &|x, y| {
            d.rect(
                x,
                y - 8.0,
                26.0,
                16.0,
                dv::RX,
                Color32::WHITE,
                hex("#aab0b8"),
                1.0,
                Some((1.5, 3.0)),
            )
        },
        &["dotted ghost: designed,", "not in plan.rs"],
    );
    item(
        &|x, y| {
            d.cubic(
                [
                    (x, y + 6.0),
                    (x + 18.0, y + 6.0),
                    (x + 22.0, y - 6.0),
                    (x + 26.0, y - 6.0),
                ],
                dv::BACK,
                1.5,
                Some(dv::BACK_DASH),
            );
            d.arrow((x + 26.0, y - 6.0), (x + 22.0, y - 6.0), dv::BACK);
        },
        &["back edge: feedback", "read from game.rs"],
    );
    item(
        &|x, y| {
            d.line((x + 13.0, y - 10.0), (x + 13.0, y + 10.0), dv::WIRE, 1.5);
            port(d, (x + 13.0, y), "MoveList");
        },
        &["wire: its type is", "the label (ns:WireType)"],
    );
    for (i, t) in [
        "Position", "Value⊥", "MoveList", "Table", "Window", "Cutoff",
    ]
    .iter()
    .enumerate()
    {
        port(d, (lx + 6.0, ly + i as f32 * 15.0), t);
        d.text(
            (lx + 16.0, ly + 3.5 + i as f32 * 15.0 + 3.0),
            Align2::LEFT_BOTTOM,
            t,
            9.5,
            Color32::from_gray(0x55),
            true,
        );
    }
    ly += 6.0 * 15.0 + 8.0;
    d.text(
        (lx, ly + 8.0 + 3.0),
        Align2::LEFT_BOTTOM,
        "port dots: by type; × removes a piece",
        9.0,
        Color32::from_gray(0x88),
        false,
    );
    c.hover(
        d.rect_r(lx - 4.0, 120.0, 190.0, ly - 110.0),
        info("legend")
            .note("diagram-viewer.js boxes, wires and back edges; frames, slots and ghosts for the ROADMAP's plan drawing.")
            .cite(super::b_orders::dv_cite(503, 526, "boxes: rx 6, white, #333 1.5; port dots r 4 #666"))
            .cite(super::b_orders::dv_cite(490, 501, "back edges: dashed #c44 (6,3), routed around the right side, arrowhead"))
            .cite(road_cite(418, 430, "search composition: drivers around the core"))
            .cite(road_cite(476, 482, "Slice B: diagram-viewer.js conventions")),
    );
}
