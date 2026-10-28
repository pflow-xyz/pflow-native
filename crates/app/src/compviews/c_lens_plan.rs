//! The lens view's Connect-4 example: the search plan drawn as lenses, after
//! the prototype's `js/views/lens.js` (`renderPlan`), in its design
//! coordinates.
//!
//! An analogy of shape only. Every piece of the plan is a lens box: a
//! forward lane on top (positions and move lists down the tree) and a
//! backward lane underneath (values and bounds back up). Drivers are frames
//! around the alpha-beta core in the order game.rs runs them (budget
//! outermost), the order chain sits on the core's move-list lane, the memo is
//! a lens inside the core, and dashed loops are state carried from a
//! backward pass into a later forward pass.
//!
//! Every pflow-rs line cite is checked against the pinned rev (aacb7eb);
//! the prototype's are from c396d46, where `game.rs` and `plan.rs` have
//! since moved (the table is bounded, `must_win` is a default filter,
//! `mtd_f` and `aspiration` are drivers).

#![allow(clippy::too_many_arguments)]

use super::c_draw::*;
use super::*;

// --- pflow-rs cites at aacb7eb ---------------------------------------------

fn gs(a: u32, b: u32, note: &str) -> String {
    rs_cite("src/game.rs", a, b, note)
}
fn od(a: u32, b: u32, note: &str) -> String {
    rs_cite("src/order.rs", a, b, note)
}
fn pl(a: u32, b: u32, note: &str) -> String {
    rs_cite("src/plan.rs", a, b, note)
}
fn road(a: u32, b: u32, note: &str) -> String {
    let o = comp::ontology();
    let rev = o["repos"]["ontology"]["rev"].as_str().unwrap_or("");
    format!("ontology-pflow-xyz ROADMAP.md:{a}-{b} @{rev} ({note})")
}

struct C;
#[allow(non_snake_case)]
impl C {
    fn value() -> String {
        gs(
            809,
            815,
            "Solver::value: the exact value, or None past the budget",
        )
    }
    fn run() -> String {
        gs(
            918,
            934,
            "run: the bit engine when the marking fits, else counts; nodes reset to 0 (926, 933)",
        )
    }
    fn budget_count() -> String {
        gs(1057, 1060, "search: nodes += 1; past the budget, None")
    }
    fn up_solve() -> String {
        gs(982, 984, "solve: a None from a search is returned at once")
    }
    fn up_search() -> String {
        gs(
            1152,
            1154,
            "search: a None from a child is returned at once",
        )
    }
    fn solve() -> String {
        gs(
            937,
            996,
            "Solver::solve: yes/no searches, one per gap between adjacent rewards",
        )
    }
    fn window() -> String {
        gs(970, 977, "k: the gap nearest the guess, else the mover's worst gap first; window (vals[k], vals[k+1])")
    }
    fn narrow() -> String {
        gs(
            978,
            995,
            "v > (a+b)/2: i = k+1, else j = k; returns vals[i]",
        )
    }
    fn full_range() -> String {
        gs(947, 958, "null_window off (or under 3 rewards): one search over the whole range, or first a window around the guess")
    }
    fn mtd_f() -> String {
        gs(
            998,
            1043,
            "mtd_f_run: the next test follows the bound the last one proved",
        )
    }
    fn rewards() -> String {
        gs(
            121,
            138,
            "rewards(): the distinct sums of the reward weights",
        )
    }
    fn c4_reward() -> String {
        rs_cite(
            "tests/common/mod.rs",
            35,
            35,
            "Connect-4 rewards: win_x +1, win_o -1",
        )
    }
    fn search() -> String {
        gs(1046, 1181, "Solver::search: fail-soft alpha-beta")
    }
    fn tt_get() -> String {
        gs(
            1069,
            1079,
            "tt.get: a settled entry returns at once; else (a, b) narrowed and first = best",
        )
    }
    fn leaf() -> String {
        gs(1082, 1087, "no moves: the score, stored as exact")
    }
    fn children() -> String {
        gs(
            1089,
            1112,
            "children keyed by the chain (must_win may settle the node here)",
        )
    }
    fn loop_() -> String {
        gs(
            1142,
            1167,
            "each child searched with (a, b); max/min; a >= b: chain.cutoff, break",
        )
    }
    fn recurse() -> String {
        gs(
            1152,
            1152,
            "self.search(next, a, b, false): the core again, one level down",
        )
    }
    fn cutoff() -> String {
        gs(
            1163,
            1166,
            "a >= b: self.chain.cutoff(&mut self.history, t); break",
        )
    }
    fn bounds_store() -> String {
        gs(
            1168,
            1177,
            "the result as [lo, hi], merged with the old entry, stored",
        )
    }
    fn store() -> String {
        gs(1183, 1185, "store: Entry { lo, hi, best } into the table")
    }
    fn table_default() -> String {
        gs(587, 594, "Solver::MEMO = 2^23 entries by default")
    }
    fn with_memo() -> String {
        gs(635, 641, "with_memo: the table is kept to N entries")
    }
    fn age() -> String {
        gs(895, 899, "start: the table ages between calls")
    }
    fn memo_doc() -> String {
        rs_cite(
            "src/memo.rs",
            1,
            26,
            "bounded table: past N, a new key takes the weakest slot",
        )
    }
    fn order_only() -> String {
        od(
            1,
            7,
            "order never changes a value, only how soon a cutoff comes",
        )
    }
    fn ctx_read() -> String {
        od(15, 24, "Ctx: what a piece may read (first, history)")
    }
    fn cutoff_noop() -> String {
        od(
            36,
            37,
            "Orderer::cutoff: a no-op unless a piece overrides it",
        )
    }
    fn tt_best() -> String {
        od(52, 58, "TtBest: key 1 for cx.first; DECISIVE")
    }
    fn imm_win() -> String {
        od(
            60,
            66,
            "ImmediateWin: key 1 when next is over and the mover wins; DECISIVE",
        )
    }
    fn hist_key() -> String {
        od(68, 71, "History: key = history[t]")
    }
    fn hist_cut() -> String {
        od(73, 75, "History::cutoff: history[t] += 1")
    }
    fn centrality() -> String {
        od(90, 94, "Centrality: key = centrality[t]")
    }
    fn threats() -> String {
        od(96, 103, "Threats: goals t feeds one token short after t")
    }
    fn chain_key() -> String {
        od(
            259,
            269,
            "Chain::key: piece by piece, stopping at a decisive piece's nonzero key",
        )
    }
    fn chain_before() -> String {
        od(271, 275, "Chain::before: the first key to differ decides")
    }
    fn chain_cutoff() -> String {
        od(277, 281, "Chain::cutoff: every piece's cutoff")
    }
    fn chain_default() -> String {
        od(205, 216, "Chain::default: tt_best, immediate_win, history")
    }
    fn plan_chain() -> String {
        pl(
            283,
            298,
            "chain(): no order gives Chain::default; a repeated piece is refused",
        )
    }
    fn solver() -> String {
        pl(400, 440, "solver_with(): budget, chain, null_window / mtd_f, filters, aspiration, memo; mode only reaches choose")
    }
    fn drivers_doc() -> String {
        pl(
            29,
            38,
            "drivers: null_window on by default; budget gives a value up past N nodes",
        )
    }
    fn drivers_flags() -> String {
        pl(
            205,
            281,
            "drivers(): the list is read into flags, so its JSON order does not change the nesting",
        )
    }
    fn filters_doc() -> String {
        pl(
            341,
            358,
            "filters(): absent is Filter::DEFAULT = [must_win]; [] runs none",
        )
    }
    fn filter_default() -> String {
        rs_cite("src/filter.rs", 42, 42, "Filter::DEFAULT: [MustWin]")
    }
    fn aspiration() -> String {
        gs(697, 705, "with_aspiration: start from the root's estimate; a window around it, the whole range if the value falls outside")
    }
    fn core_type() -> String {
        road(
            428,
            430,
            "the core as a box Position -> Value⊥, drivers as nested boxes around it",
        )
    }
    fn wire_types() -> String {
        road(
            436,
            436,
            "ns:WireType: Position, Value, Value⊥, MoveList (PROPOSED)",
        )
    }
}

const ANALOGY: &str = "Lens reading: an analogy of shape only. No equilibrium is computed or checked, and no game-theoretic claim is made.";

fn lens_info(title: &str) -> Info {
    info(title).note(ANALOGY)
}

fn cites(mut i: Info, cs: &[String]) -> Info {
    for c in cs {
        if !i.cites.contains(c) {
            i.cites.push(c.clone());
        }
    }
    i
}

struct Lane {
    fwd: &'static str,
    bwd: &'static str,
}

fn lane(name: &str) -> Lane {
    match name {
        "budget" => Lane {
            fwd: "counts nodes, passes the position on",
            bwd: "the value, or ⊥ (None) past N nodes",
        },
        "null_window" => Lane {
            fwd: "picks the window (a, b) for the next gap between adjacent rewards, the mover's worst gap first",
            bwd: "v narrows (i, j), repeat while i < j; returns vals[i]",
        },
        "mtd_f" => Lane {
            fwd: "picks the next yes/no test from the bound the last one proved (MTD(f)), not a gap at a time",
            bwd: "v moves a bound, however far past the window; repeat while i < j; returns vals[i]",
        },
        "aspiration" => Lane {
            fwd: "starts from the root's estimate: the first test is the gap nearest it",
            bwd: "as the driver inside; a window that misses is widened to the whole range",
        },
        "memo" => Lane {
            fwd: "get(k): a settled entry returns at once; otherwise [lo, hi] narrows (a, b) and the remembered best move goes to tt_best",
            bwd: "store(k, lo, hi, best), kept to N entries; past N a new position takes the place of a weaker one",
        },
        "tt_best" => Lane {
            fwd: "key 1 for the move the table remembers as best (cx.first), else 0; decisive: a nonzero key settles the rank",
            bwd: "none of its own (cutoff is a no-op). Its forward reads what the backward pass stored: store, then tt.get, then first",
        },
        "immediate_win" => Lane {
            fwd: "key 1 for a move that ends the game in the mover's favour; decisive",
            bwd: "none (cutoff is a no-op)",
        },
        "history" => Lane {
            fwd: "key = history[t]: how often t caused a cutoff before",
            bwd: "a cutoff increments history[t] (order.rs:73-75, via chain.cutoff in search)",
        },
        "centrality" => Lane {
            fwd: "key = centrality[t]: the mover's goals that t's output places feed, fixed per transition",
            bwd: "none (cutoff is a no-op)",
        },
        "threats" => Lane {
            fwd: "key = the goals t feeds that are one token short of enabled after t",
            bwd: "none (cutoff is a no-op)",
        },
        _ => Lane { fwd: "", bwd: "" },
    }
}

fn lane_cites(name: &str) -> Vec<String> {
    match name {
        "budget" => vec![
            C::value(),
            C::budget_count(),
            C::up_solve(),
            C::up_search(),
            C::run(),
            C::drivers_doc(),
        ],
        "null_window" => vec![
            C::solve(),
            C::window(),
            C::narrow(),
            C::full_range(),
            C::drivers_doc(),
            C::rewards(),
            C::c4_reward(),
        ],
        "mtd_f" => vec![C::mtd_f(), C::solve(), C::drivers_doc()],
        "aspiration" => vec![C::aspiration(), C::full_range(), C::drivers_doc()],
        "memo" => vec![
            C::tt_get(),
            C::store(),
            C::bounds_store(),
            C::with_memo(),
            C::memo_doc(),
        ],
        "tt_best" => vec![
            C::tt_best(),
            C::ctx_read(),
            C::cutoff_noop(),
            C::store(),
            C::tt_get(),
        ],
        "immediate_win" => vec![C::imm_win(), C::cutoff_noop()],
        "history" => vec![C::hist_key(), C::hist_cut(), C::chain_cutoff(), C::cutoff()],
        "centrality" => vec![C::centrality(), C::cutoff_noop()],
        "threats" => vec![C::threats(), C::cutoff_noop()],
        _ => vec![],
    }
}

/// The inspector entry of a palette piece: its facts from `sp::piece_info`.
fn piece_base(name: &str) -> Info {
    let mut i = info(format!("piece {name}"));
    if let Some(p) = sp::piece_info(name) {
        i = i
            .fact("kind", p.kind)
            .fact("status", p.status)
            .fact("contract", p.contract)
            .note(p.doc)
            .cite(pl(1, 55, "the plan's module doc: every name a plan takes"));
    }
    i
}

// --- geometry -------------------------------------------------------------

const PW: f32 = 86.0;
const PG: f32 = 8.0;
const BAND: f32 = 120.0;
const NEST: f32 = 24.0;
const FX0: f32 = 92.0;
const YF: f32 = 220.0;
const YB: f32 = 384.0;
const CORE_TOP: f32 = 122.0;
const MEMO_TOP: f32 = YB + 26.0;
const YM1: f32 = YB + 60.0;
const YM2: f32 = YB + 114.0;
const MEMO_BOT: f32 = YB + 136.0;
const CORE_BOT: f32 = YB + 154.0;
const BUS: f32 = YF + 112.0;
const BULGE: f32 = 56.0;

struct Frame {
    id: String,
    x: f32,
    top: f32,
    right: f32,
    bottom: f32,
    /// Index in the plan's drivers list (for removal).
    index: usize,
}

struct Piece {
    name: String,
    i: usize,
    x: f32,
}

struct G {
    budget: Option<i64>,
    frames: Vec<Frame>,
    chain: Vec<String>,
    chain_default: bool,
    memo: Option<i64>,
    pieces: Vec<Piece>,
    cx: f32,
    cr: f32,
    x_k: f32,
    x_t: f32,
    x_w: f32,
    expand_x: f32,
    order_sock: f32,
    chain_x: f32,
    chain_right: f32,
    filters_x: f32,
    memo_x: f32,
    memo_right: f32,
    x_cut: f32,
    x_mm: f32,
    x_store: f32,
    ghost_x: f32,
    cap_x: f32,
    w: f32,
    h: f32,
    legend_y: f32,
    filters: Vec<String>,
}

const EXPAND_W: f32 = 60.0;
const ORDER_SOCK_W: f32 = 30.0;
const FILTERS_W: f32 = 58.0;
const GHOST_W: f32 = 80.0;

fn geometry(plan: &Value, order: &[String], drivers: &[Value], filters: &[String]) -> G {
    let names: Vec<String> = drivers.iter().map(driver_name).collect();
    let budget = drivers.iter().find_map(|d| {
        if driver_name(d) == "budget" {
            Some(d["budget"]["nodes"].as_i64().unwrap_or(0))
        } else {
            None
        }
    });
    let idx = |n: &str| names.iter().position(|x| x == n);
    // budget outermost, then the yes/no driver, then aspiration
    let mut frames: Vec<Frame> = vec![];
    for n in ["budget", "null_window", "mtd_f", "aspiration"] {
        if let Some(i) = idx(n) {
            frames.push(Frame {
                id: n.into(),
                x: 0.0,
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                index: i,
            });
        }
    }
    let depth = frames.len();
    let mut x = FX0;
    for (k, f) in frames.iter_mut().enumerate() {
        f.x = x;
        f.top = CORE_TOP - NEST * (depth - k) as f32;
        x += BAND;
    }
    let cx = x;
    let (x_k, x_t, x_w) = (cx + 14.0, cx + 31.0, cx + 48.0);
    let expand_x = cx + 56.0;
    let mut px = cx + 124.0;
    let pieces: Vec<Piece> = order
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let p = Piece {
                name: n.clone(),
                i,
                x: px,
            };
            px += PW + PG;
            p
        })
        .collect();
    let chain_end = if pieces.is_empty() {
        cx + 118.0
    } else {
        px - PG
    };
    let order_sock = chain_end + 8.0;
    let chain_x = pieces.first().map_or(order_sock, |p| p.x) - 6.0;
    let chain_right = order_sock + ORDER_SOCK_W + 6.0;
    let filters_x = order_sock + ORDER_SOCK_W + 12.0;
    // the filters sit on the move-list lane after the chain: a socket, or
    // one small card per filter the plan runs
    let fw = if filters.is_empty() {
        FILTERS_W
    } else {
        filters.len() as f32 * (FILTERS_W + 8.0) + 6.0
    };
    let memo_x = cx + 6.0;
    let memo_right = (order_sock + ORDER_SOCK_W).max(cx + 440.0);
    let core_right = (filters_x + fw + 40.0).max(memo_right + 104.0);
    let x_cut = core_right - 84.0;
    let x_mm = core_right - 34.0;
    let x_store = memo_right - 22.0;
    let ghost_x = core_right + 12.0;
    let cap_x = ghost_x + GHOST_W;
    for (k, f) in frames.iter_mut().enumerate() {
        let j = depth - 1 - k;
        f.right = cap_x + 60.0 + 12.0 * j as f32;
        f.bottom = CORE_BOT + 14.0 * (j as f32 + 1.0);
    }
    let right = frames.first().map_or(cap_x + 56.0, |f| f.right);
    let w = 1150f32.max(right + 14.0);
    let bottom = frames.first().map_or(CORE_BOT, |f| f.bottom);
    let legend_y = bottom + 30.0;
    G {
        budget,
        frames,
        chain: order.to_vec(),
        chain_default: plan.get("order").is_none(),
        memo: plan["memo"]["entries"].as_i64(),
        pieces,
        cx,
        cr: core_right,
        x_k,
        x_t,
        x_w,
        expand_x,
        order_sock,
        chain_x,
        chain_right,
        filters_x,
        memo_x,
        memo_right,
        x_cut,
        x_mm,
        x_store,
        ghost_x,
        cap_x,
        w,
        h: legend_y + 62.0,
        legend_y,
        filters: filters.to_vec(),
    }
}

fn has_driver(g: &G, id: &str) -> bool {
    g.frames.iter().any(|f| f.id == id)
}

pub(super) fn lens_c4(c: &mut Ctx, area: Rect) {
    let (plan, order, drivers, filters, _heur) = plan_lists(c);
    // the piece palette lives along the bottom (drag onto a lit slot)
    let body = Rect::from_min_max(area.left_top(), area.right_bottom() - Vec2::new(0.0, 56.0));
    palette(c, area);
    let g = geometry(&plan, &order, &drivers, &filters);
    let xf = Xf::fit(body, g.w, g.h, 1.3);
    let xf = &xf;
    let col_pos = type_color("Position");
    let col_moves = type_color("MoveList");
    let col_val = type_color("Value");
    let col_win = type_color("Window");
    let col_tab = type_color("Table");
    let col_cut = type_color("Cutoff");
    let driver_col = kind_color_of("Driver");
    let order_col = kind_color_of("Order");
    let has_hist = g.pieces.iter().any(|p| p.name == "history");
    let tt = g.pieces.iter().find(|p| p.name == "tt_best");
    let hist = g.pieces.iter().find(|p| p.name == "history");
    let mode = plan["mode"].as_str().unwrap_or("exact").to_string();
    let present = g.memo.is_some();
    let t_ink = P_INK;
    let t_muted = P_MUTED;

    // merge animation: a piece slides into its slot, a frame scales in, the
    // memo rises (what the prototype's `animateEvent` does)
    let slide = |c: &Ctx, slot: &str, name: &str| -> (f32, f32) {
        match c.anim(|e| match e {
            Event::PieceAdded { name: n, slot: s } if s == slot && n == name => Some(()),
            _ => None,
        }) {
            Some((_, t)) => (0.0, -90.0 * (1.0 - t)),
            None => (0.0, 0.0),
        }
    };

    // ---- frames (outermost first)
    for f in &g.frames {
        let l = lane(&f.id);
        let is_b = f.id == "budget";
        let fill = match f.id.as_str() {
            "budget" => hex("#f3f6fa"),
            "null_window" | "mtd_f" => hex("#e9eff7"),
            _ => hex("#dfe8f4"),
        };
        let grow = match c.anim(|e| match e {
            Event::PieceAdded { name, slot } if slot == "drivers" && *name == f.id => Some(()),
            _ => None,
        }) {
            Some((_, t)) => 1.0 - 0.08 * (1.0 - t),
            None => 1.0,
        };
        let (fx, ftop, fw, fh) = (f.x, f.top, f.right - f.x, f.bottom - f.top);
        let (cxm, cym) = (fx + fw / 2.0, ftop + fh / 2.0);
        let (fx, ftop, fw, fh) = (
            cxm - fw * grow / 2.0,
            cym - fh * grow / 2.0,
            fw * grow,
            fh * grow,
        );
        let bx = f.x + 10.0;
        let box_r = dbox(c, xf, fx, ftop, fw, fh, 10.0, fill, driver_col, 1.5, false);
        let tab_txt = if is_b {
            format!("budget {{nodes: {}}}", fmt_n(g.budget.unwrap_or(0)))
        } else {
            f.id.clone()
        };
        let tab_w = tw(c, xf, &tab_txt, 10.5, true) + 56.0;
        let tab_r = dbox(
            c, xf, f.x, f.top, tab_w, 20.0, 10.0, driver_col, driver_col, 1.0, false,
        );
        dbold(
            c,
            xf,
            f.x + 10.0,
            f.top + 14.0,
            Align2::LEFT_CENTER,
            &tab_txt,
            10.5,
            Color32::WHITE,
            true,
        );
        dtext(
            c,
            xf,
            f.x + tab_w - 40.0,
            f.top + 14.0,
            Align2::LEFT_CENTER,
            "driver",
            9.5,
            hex("#dbe7f5"),
        );
        slot_target(c, tab_r, "drivers");
        let fwd_l = wrap_words(
            &format!(
                "fwd: {}",
                l.fwd.replace(", the mover's worst gap first", "")
            ),
            17,
        );
        let bwd_text = if is_b {
            format!(
                "bwd: the value, or ⊥ (None) past {} nodes",
                fmt_n(g.budget.unwrap_or(0))
            )
        } else {
            format!("bwd: {}", l.bwd.replace(" while i < j", ""))
        };
        let bwd_l = wrap_words(&bwd_text, 17);
        dlines(
            c,
            xf,
            bx,
            YF - 14.0 - (fwd_l.len() as f32 - 1.0) * 12.5,
            Align2::LEFT_BOTTOM,
            &fwd_l,
            10.0,
            12.5,
            t_ink,
            false,
        );
        dlines(
            c,
            xf,
            bx,
            YB + 18.0,
            Align2::LEFT_BOTTOM,
            &bwd_l,
            10.0,
            12.5,
            t_ink,
            false,
        );
        if is_b {
            dlines(
                c,
                xf,
                bx,
                (YF + YB) / 2.0 - 14.0,
                Align2::LEFT_BOTTOM,
                &[
                    "nodes += 1 at".into(),
                    "every search node".into(),
                    "below; ⊥ goes".into(),
                    "straight up".into(),
                ],
                9.5,
                12.0,
                t_muted,
                false,
            );
        } else if f.id == "null_window" || f.id == "mtd_f" {
            // the repeat loop: the backward lane feeds the next forward pass
            let lx = f.x + 16.0;
            let pts = dcubic(
                xf,
                [
                    (lx, YB - 10.0),
                    (lx, (YF + YB) / 2.0 + 30.0),
                    (lx, (YF + YB) / 2.0 - 30.0),
                    (lx, YF + 30.0),
                ],
            );
            wire(c, &pts, col_win, 1.6, true, true, xf.k);
            let wi = cites(
                lens_info(&format!("{}: repeat while i < j", f.id)).fact(
                    "Connect-4",
                    "vals = [-1, 0, 1]: at most two searches",
                ).fact("not a lens law", "a loop from the backward lane back into the forward lane: state between passes").note(
                    "Each yes/no search narrows the range of possible values; the next search tests the next gap.",
                ),
                &[if f.id == "mtd_f" { C::mtd_f() } else { C::narrow() }, C::window(), C::rewards(), C::c4_reward()],
            );
            hover_path(c, &pts, 6.0, wi);
            dlines(
                c,
                xf,
                lx + 10.0,
                (YF + YB) / 2.0 - 18.0,
                Align2::LEFT_BOTTOM,
                &["repeat".into(), "while i < j".into()],
                9.5,
                12.0,
                col_win,
                false,
            );
            dlines(
                c,
                xf,
                lx + 10.0,
                (YF + YB) / 2.0 + 14.0,
                Align2::LEFT_BOTTOM,
                &[
                    "Connect-4:".into(),
                    "vals = [-1, 0, 1]".into(),
                    "≤ 2 searches".into(),
                ],
                9.5,
                12.0,
                t_muted,
                false,
            );
        } else {
            dlines(
                c,
                xf,
                bx,
                (YF + YB) / 2.0 - 14.0,
                Align2::LEFT_BOTTOM,
                &["needs a root".into(), "estimate (root.order)".into()],
                9.5,
                12.0,
                t_muted,
                false,
            );
        }
        remove_x(
            c,
            xf,
            f.x + tab_w + 9.0,
            f.top + 10.0,
            "drivers",
            f.index,
            &f.id,
        );
        let mut fi = piece_base(&f.id)
            .fact("forward lane (top)", l.fwd)
            .fact("backward lane (bottom)", l.bwd);
        if is_b {
            fi = fi.fact(
                "scope",
                "nodes is reset only in value()'s run, before each engine run (game.rs:926, 933), and counted in every search() call (game.rs:1057): one budget covers every yes/no search of one solve() run, so it is the outermost frame. A run the bit engine cannot finish is redone over counts with the count reset again",
            ).fact("⊥ goes straight up", "a None from below is returned at once (game.rs:982-984, 1152-1154)").fact("absent", "no budget: usize::MAX");
        }
        if f.id == "null_window" {
            fi = fi.fact("Connect-4", "rewards win_x +1, win_o -1 (tests/common/mod.rs:35), so vals = [-1, 0, 1]: two gaps, at most two yes/no searches").fact("off", "drivers given without it: one search over the whole range, (a, b) = bounds");
        }
        if f.id == "mtd_f" {
            fi = fi
                .fact(
                    "instead of null_window",
                    "plan.rs refuses the two together: both find the value by yes/no searches",
                )
                .fact("Connect-4", "vals = [-1, 0, 1]: at most two tests");
        }
        let n = format!("{} {ANALOGY}", fi.note);
        fi = fi.note(n);
        fi = cites(fi, &lane_cites(&f.id));
        c.hover(box_r, fi.clone());
        c.hover(tab_r, fi);
        c.scene.emit("frame", &f.id, json!({}));
    }

    // ---- core frame
    let (cx, cr) = (g.cx, g.cr);
    let core_r = dbox(
        c,
        xf,
        cx,
        CORE_TOP,
        cr - cx,
        CORE_BOT - CORE_TOP,
        10.0,
        Color32::WHITE,
        t_ink,
        1.5,
        false,
    );
    let core_tab = "core · alpha-beta (Solver::search)";
    let tab_w = tw(c, xf, core_tab, 10.5, false) + 16.0;
    dbox(
        c, xf, cx, CORE_TOP, tab_w, 20.0, 10.0, t_ink, t_ink, 1.0, false,
    );
    dbold(
        c,
        xf,
        cx + 9.0,
        CORE_TOP + 14.0,
        Align2::LEFT_CENTER,
        core_tab,
        10.5,
        Color32::WHITE,
        false,
    );
    dtext(
        c,
        xf,
        cx + 12.0,
        CORE_TOP + 38.0,
        Align2::LEFT_CENTER,
        "fwd: position → children, tried in the chain's order",
        10.5,
        t_ink,
    );
    halo(
        c,
        xf,
        g.x_cut - 14.0,
        YB - 9.0,
        Align2::RIGHT_CENTER,
        "bwd: child values → max/min, cutoffs",
        10.5,
        t_ink,
        Color32::WHITE,
        false,
    );
    let core_i = cites(
        lens_info("core: alpha-beta (Solver::search)")
            .fact("forward lane (top)", "position → children, keyed by the chain and tried best first")
            .fact("backward lane (bottom)", "child values → max/min; a ≥ b is a cutoff: chain.cutoff, and the remaining children are not tried")
            .fact("interleaving", "each child's value comes back before the next child is tried, so forward and backward passes alternate: a lens runs one, then the other")
            .fact("wire types", "Position (+ window) → Value: a fail-soft bound inside (a, b)")
            .fact("filters", format!("{} drop moves only where the value is already known (must_win settles a node at a winning move, game.rs:1103-1111)", if g.filters.is_empty() { "none run here".to_string() } else { g.filters.join(", ") }))
            .fact("⊥ inside", "the budget check is at the top of every search() (game.rs:1057-1060), and a None from a child is returned at once (game.rs:1152-1154): the ROADMAP's PROPOSED reading types the core Position → Value⊥ (ROADMAP:428-430). This drawing puts ⊥ on the budget frame, where it is decided")
            .note("Fail-soft alpha-beta in the +1 side's view: inside the bounds every result is a bound on the true value; with the window the whole range, it is the value."),
        &[C::search(), C::children(), C::loop_(), C::bounds_store(), C::order_only(), C::budget_count(), C::up_search(), C::core_type()],
    );
    c.hover(core_r, core_i);

    // bounds: designed slot
    {
        let (x, y) = (g.expand_x, YF + 50.0);
        let r = dbox(
            c,
            xf,
            x,
            y,
            EXPAND_W,
            40.0,
            6.0,
            Color32::from_white_alpha(140),
            hex("#9aa3ad"),
            1.3,
            true,
        );
        dtext(
            c,
            xf,
            x + EXPAND_W / 2.0,
            y + 17.0,
            Align2::CENTER_CENTER,
            "bounds",
            10.0,
            t_muted,
        );
        dtext(
            c,
            xf,
            x + EXPAND_W / 2.0,
            y + 30.0,
            Align2::CENTER_CENTER,
            "designed",
            9.0,
            t_muted,
        );
        slot_target(c, r, "bounds");
        let i = slot_info(c, "bounds")
            .fact(
                "ROADMAP",
                "bounds compose by interval meet; not built (ROADMAP:424-425)",
            )
            .fact(
                "placement",
                "beside the window (a, b) it would narrow: this page's judgment",
            )
            .cite(road(424, 425, "Bounds: interval meet. Neither is built."));
        c.hover(r, i);
    }
    // the recursion ghost
    {
        let (gx, gw) = (g.ghost_x, GHOST_W);
        dbox(
            c,
            xf,
            gx + 4.0,
            YF - 22.0,
            gw,
            YB - YF + 48.0,
            9.0,
            Color32::TRANSPARENT,
            hex("#9aa3ad"),
            1.0,
            true,
        );
        let r = dbox(
            c,
            xf,
            gx,
            YF - 26.0,
            gw,
            YB - YF + 52.0,
            9.0,
            Color32::from_white_alpha(180),
            hex("#8a929c"),
            1.3,
            true,
        );
        dtext(
            c,
            xf,
            gx + gw / 2.0,
            YF - 9.0,
            Align2::CENTER_CENTER,
            "child, (a, b)",
            9.5,
            t_muted,
        );
        dlines(
            c,
            xf,
            gx + gw / 2.0,
            YF + 26.0,
            Align2::CENTER_BOTTOM,
            &["⟲ search(".into(), "child, a, b)".into()],
            9.5,
            12.0,
            t_ink,
            true,
        );
        dlines(
            c,
            xf,
            gx + gw / 2.0,
            YF + 56.0,
            Align2::CENTER_BOTTOM,
            &["the core".into(), "again, one".into(), "level down".into()],
            9.5,
            11.5,
            t_muted,
            false,
        );
        dtext(
            c,
            xf,
            gx + gw / 2.0,
            YB + 18.0,
            Align2::CENTER_CENTER,
            "v (child)",
            9.5,
            t_muted,
        );
        let i = cites(
            lens_info("the recursion: the core again, one level down")
                .fact("forward", "child position + window")
                .fact("backward", "the child's value v")
                .fact("bottom", "a leaf: no moves, the score (game.rs:1082-1087)")
                .note("self.search(next, a, b, false) for each child, best first (game.rs:1152). Budget counts these nodes too (game.rs:1057)."),
            &[C::recurse(), C::budget_count(), C::leaf()],
        );
        c.hover(r, i);
    }
    // ---- memo: a lens inside the core
    let (mx, mr) = (g.memo_x, g.memo_right);
    let memo_rise = match c.anim(|e| match e {
        Event::PieceAdded { slot, .. } if slot == "memo" => Some(()),
        _ => None,
    }) {
        Some((_, t)) => 70.0 * (1.0 - t),
        None => 0.0,
    };
    {
        let my = |y: f32| y + memo_rise;
        let box_r = dbox(
            c,
            xf,
            mx,
            my(MEMO_TOP),
            mr - mx,
            MEMO_BOT - MEMO_TOP,
            8.0,
            if present {
                hex("#eff7f5")
            } else {
                Color32::from_white_alpha(150)
            },
            if present { col_tab } else { hex("#9aa3ad") },
            1.4,
            !present,
        );
        slot_target(c, box_r, "memo");
        let tab = if present {
            format!("memo {{entries: {}}}", fmt_n(g.memo.unwrap_or(0)))
        } else {
            "memo slot: empty, the core keeps its default table".to_string()
        };
        let tab_w = tw(c, xf, &tab, if present { 10.5 } else { 10.0 }, present) + 20.0;
        let pill = dbox(
            c,
            xf,
            mx + 8.0,
            my(MEMO_BOT) - 10.0,
            tab_w,
            20.0,
            10.0,
            if present { col_tab } else { Color32::WHITE },
            if present { col_tab } else { hex("#9aa3ad") },
            1.0,
            !present,
        );
        let _ = pill;
        if present {
            dbold(
                c,
                xf,
                mx + 18.0,
                my(MEMO_BOT),
                Align2::LEFT_CENTER,
                &tab,
                10.5,
                Color32::WHITE,
                true,
            );
        } else {
            dtext(
                c,
                xf,
                mx + 18.0,
                my(MEMO_BOT),
                Align2::LEFT_CENTER,
                &tab,
                10.0,
                t_muted,
            );
        }
        let fwd_t = format!(
            "fwd: get(k): [lo, hi] narrows (a, b); {}",
            if tt.is_some() {
                "best move → tt_best"
            } else {
                "best move: no piece reads it"
            }
        );
        let bwd_t = if present {
            format!(
                "bwd: store(k, lo, hi, best), kept to {} entries",
                fmt_n(g.memo.unwrap_or(0))
            )
        } else {
            "bwd: store (default table: 2^23 entries)".to_string()
        };
        dtext(
            c,
            xf,
            g.x_w + 12.0,
            my(YM1) + 12.0,
            Align2::LEFT_CENTER,
            &fwd_t,
            10.0,
            t_ink,
        );
        dtext(
            c,
            xf,
            g.x_w + 12.0,
            my(YM2) - 11.0,
            Align2::LEFT_CENTER,
            &bwd_t,
            10.0,
            t_ink,
        );
        // table icon (state)
        let (tx, ty) = (g.x_t, (my(YM1) + my(YM2)) / 2.0);
        let ctr = xf.pt(tx, ty - 9.0);
        let (rx, ry) = (13.0 * xf.k, 4.0 * xf.k);
        let body = Rect::from_min_max(
            Pos2::new(ctr.x - rx, ctr.y),
            Pos2::new(ctr.x + rx, ctr.y + 18.0 * xf.k),
        );
        c.p.rect(
            body,
            2.0,
            Color32::WHITE,
            Stroke::new(1.3, col_tab),
            StrokeKind::Middle,
        );
        c.p.add(Shape::ellipse_filled(
            ctr,
            Vec2::new(rx, ry),
            hex("#d9efea"),
        ));
        c.p.add(Shape::ellipse_stroke(
            ctr,
            Vec2::new(rx, ry),
            Stroke::new(1.3, col_tab),
        ));
        dmono(
            c,
            xf,
            tx,
            ty + 6.0,
            Align2::CENTER_CENTER,
            "tt",
            9.0,
            col_tab,
        );
        let ti = cites(
            lens_info("the transposition table (state)")
                .fact("cap", if present { format!("{} entries (with_memo)", fmt_n(g.memo.unwrap_or(0))) } else { "2^23 entries by default (Solver::MEMO)".to_string() })
                .fact("not a lens", "shared state between passes, drawn as a dashed loop")
                .note("Solver.tt: what earlier searches found. It persists across calls (aged at each, game.rs:895-899), so the backward pass of one search feeds the forward pass of later ones. Bounded: past its cap a new position takes the place of a weaker one, and a position forgotten is searched again."),
            &[C::store(), C::with_memo(), C::table_default(), C::age(), C::memo_doc()],
        );
        c.hover(
            Rect::from_center_size(xf.pt(tx, ty), Vec2::new(30.0, 30.0) * xf.k),
            ti,
        );
        let base = piece_base("memo");
        let mi = if present {
            let mut i = base
                .clone()
                .fact("forward lane (top)", lane("memo").fwd)
                .fact("backward lane (bottom)", lane("memo").bwd);
            i.note = format!("{} {ANALOGY}", base.note);
            cites(i, &lane_cites("memo"))
        } else {
            cites(
                slot_info(c, "memo")
                    .fact("empty", "no memo in the plan: the core still keeps its default table, 2^23 entries, bounded")
                    .fact("drop", "memo, to keep the table to N entries")
                    .note(ANALOGY),
                &[C::table_default(), C::with_memo(), C::memo_doc()],
            )
        };
        c.hover(box_r, mi);
        if present {
            remove_x(
                c,
                xf,
                mx + 8.0 + tab_w + 10.0,
                my(MEMO_BOT),
                "memo",
                0,
                "memo",
            );
        }
        c.scene.emit("memo", "memo", json!({"present": present}));
    }
    // the order chain's backing and its sockets
    {
        let under = dbox(
            c,
            xf,
            g.chain_x,
            YF - 50.0,
            g.chain_right - g.chain_x,
            148.0,
            9.0,
            hex("#fcf7ef"),
            hex("#ecd9bb"),
            1.0,
            false,
        );
        slot_target(c, under, "order");
        let cap = if g.chain_default {
            "order chain · Chain::default (no \"order\" in the plan)"
        } else if g.pieces.is_empty() {
            "order chain · [] (identity)"
        } else {
            "order chain · the first key to differ decides"
        };
        let cap = if tw(c, xf, cap, 9.5, false) < g.chain_right - g.chain_x - 12.0 {
            cap
        } else {
            "order chain"
        };
        dbold(
            c,
            xf,
            g.chain_x + 7.0,
            YF - 41.0,
            Align2::LEFT_CENTER,
            cap,
            9.5,
            t_muted,
            false,
        );
        let ci = cites(
            slot_info(c, "order")
                .fact("now", if g.chain.is_empty() { "[] (the net's own move order)".to_string() } else { g.chain.join(", ") })
                .fact("composition", "keys compared piece by piece, the first to differ decides; a decisive piece's nonzero key settles the rank (order.rs:259-275)")
                .fact("ROADMAP", "a monoid under chaining with [] the identity, and a partial one in code: a chain holds at most 8 pieces and refuses a repeat (the Laws view)")
                .note(format!("the order chain (pieces drawn as lenses: analogy). {ANALOGY}")),
            &[C::chain_key(), C::chain_before(), C::plan_chain(), C::chain_default(), C::order_only()],
        );
        c.hover(under, ci);
    }

    // ---- lanes
    let lane_i = |ty: &str, carries: &str, cs: &[String], extra: &[(&str, String)]| -> Info {
        let fwd = ty == "Position" || ty == "MoveList";
        let mut i = lens_info(&format!(
            "{} lane: {ty}",
            if fwd { "forward" } else { "backward" }
        ))
        .fact("carries", carries)
        .fact(
            "wire type",
            format!(
                "{ty} ({})",
                if ["Position", "MoveList", "Value", "Value⊥"].contains(&ty) {
                    "ROADMAP ns:WireType, PROPOSED"
                } else {
                    "this page's label"
                }
            ),
        );
        for (k, v) in extra {
            i = i.fact(k, v.clone());
        }
        let mut all = cs.to_vec();
        all.push(C::wire_types());
        cites(i, &all)
    };
    let hseg = |c: &mut Ctx,
                x0: f32,
                x1: f32,
                y: f32,
                col: Color32,
                w: f32,
                dash: bool,
                head: bool,
                i: Info| {
        let pts = vec![xf.pt(x0, y), xf.pt(x1, y)];
        wire(c, &pts, col, w * xf.k.max(0.8), dash, head, xf.k);
        hover_path(c, &pts, 6.0, i);
    };
    let mut fx = vec![8.0];
    fx.extend(g.frames.iter().map(|f| f.x));
    fx.push(cx);
    for i in 0..fx.len() - 1 {
        hseg(
            c,
            fx[i],
            fx[i + 1] - 1.0,
            YF,
            col_pos,
            2.2,
            false,
            true,
            lane_i(
                "Position",
                if i == 0 {
                    "the position mk, into the search"
                } else {
                    "the position, passed on"
                },
                &[C::value()],
                &[],
            ),
        );
    }
    hseg(
        c,
        cx,
        g.expand_x - 1.0,
        YF,
        col_pos,
        2.2,
        false,
        true,
        lane_i(
            "Position",
            "the position (with its window)",
            &[C::tt_get()],
            &[],
        ),
    );
    hseg(
        c,
        g.expand_x + EXPAND_W,
        cr - 1.0,
        YF,
        col_moves,
        2.2,
        false,
        true,
        lane_i(
            "MoveList",
            "the children, ranked by the chain; filters drop some where the value is already known",
            &[C::children(), C::order_only()],
            &[],
        ),
    );
    hseg(
        c,
        cr,
        g.ghost_x - 1.0,
        YF,
        col_pos,
        2.2,
        false,
        true,
        lane_i(
            "Position",
            "each child, best first, with the window (a, b)",
            &[C::recurse()],
            &[],
        ),
    );
    hseg(
        c,
        g.ghost_x,
        g.cap_x,
        YF,
        col_pos,
        2.2,
        true,
        false,
        lane_i(
            "Position",
            "down the tree, level after level",
            &[C::recurse()],
            &[],
        ),
    );
    // the leaf cap: forward turns into backward
    {
        let pts = dcubic(
            xf,
            [
                (g.cap_x, YF),
                (g.cap_x + BULGE, YF),
                (g.cap_x + BULGE, YB),
                (g.cap_x + 2.0, YB),
            ],
        );
        // a gradient from Position to Value, by segments
        let n = pts.len() - 1;
        for k in 0..n {
            let t = k as f32 / n as f32;
            let a = col_pos;
            let b = col_val;
            let m = |u: u8, v: u8| (u as f32 + (v as f32 - u as f32) * t) as u8;
            poly(
                c,
                &pts[k..=k + 1],
                Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b())),
                2.4 * xf.k.max(0.8),
            );
        }
        arrow(c, pts[n - 1], pts[n], col_val, 8.5 * xf.k.max(0.7));
        dlines(
            c,
            xf,
            g.cap_x + 19.0,
            (YF + YB) / 2.0 - 4.0,
            Align2::CENTER_BOTTOM,
            &["leaf".into(), "score".into()],
            9.5,
            12.0,
            t_ink,
            false,
        );
        let i = cites(
            lens_info("leaf: the forward lane turns back")
                .fact("Connect-4 rewards", "win_x +1, win_o -1 (tests/common/mod.rs:35)")
                .fact("analogy", "only the turning-around shape is borrowed")
                .note("Where values come from: a position with no moves is scored and stored as exact (game.rs:1082-1087). In an open game, this end is where the utility comes in; here it is the reward of a final marking."),
            &[C::leaf(), C::c4_reward()],
        );
        hover_path(c, &pts, 8.0, i);
    }
    let bot_fact: Vec<(&str, String)> = if g.budget.is_some() {
        vec![("with the budget", "a ⊥ (None) from below passes through unchanged (game.rs:982-984, 1152-1154); it is drawn on the budget frame, where it is decided (game.rs:1057-1060)".to_string())]
    } else {
        vec![]
    };
    let bot_cites: Vec<String> = if g.budget.is_some() {
        vec![C::up_search(), C::up_solve(), C::budget_count()]
    } else {
        vec![]
    };
    let with = |a: &[String], b: &[String]| -> Vec<String> { a.iter().chain(b).cloned().collect() };
    hseg(
        c,
        g.cap_x,
        g.ghost_x,
        YB,
        col_val,
        2.2,
        true,
        false,
        lane_i(
            "Value",
            "values coming back up, level after level",
            &with(&[C::loop_()], &bot_cites),
            &bot_fact,
        ),
    );
    hseg(
        c,
        g.ghost_x,
        cr + 1.0,
        YB,
        col_val,
        2.2,
        false,
        true,
        lane_i(
            "Value",
            "v: one child's value, back into the max/min",
            &with(&[C::loop_()], &bot_cites),
            &bot_fact,
        ),
    );
    hseg(
        c,
        cr,
        cx + 1.0,
        YB,
        col_val,
        2.2,
        false,
        true,
        lane_i(
            "Value",
            "v: a fail-soft bound inside (a, b), or the value when the window is the whole range",
            &with(&[C::loop_(), C::bounds_store()], &bot_cites),
            &bot_fact,
        ),
    );
    // backward segments: region 0 is the caller, region k >= 1 the band of frames[k-1]
    for i in (1..fx.len()).rev() {
        let f = if i >= 2 { g.frames.get(i - 2) } else { None };
        let (ty, carries) = match f {
            None => ("Value⊥", "Option<f64>: the value, or ⊥ (None)".to_string()),
            Some(f) if f.id == "budget" => (
                "Value⊥",
                "the value, or ⊥ (None) past the budget".to_string(),
            ),
            Some(_) => (
                "Value",
                "vals[i]: the value the yes/no searches settle on".to_string(),
            ),
        };
        let inner = f.is_some_and(|f| f.id != "budget");
        let mut cs: Vec<String> = match f {
            Some(f) => lane_cites(&f.id).into_iter().take(3).collect(),
            None => vec![C::value()],
        };
        if inner {
            cs.extend(bot_cites.clone());
        }
        let extra: Vec<(&str, String)> = if inner { bot_fact.clone() } else { vec![] };
        hseg(
            c,
            fx[i],
            fx[i - 1] + 1.0,
            YB,
            col_val,
            2.2,
            false,
            true,
            lane_i(ty, &carries, &cs, &extra),
        );
    }
    // window (a, b): from where it is chosen to the core entry
    {
        let nw = g
            .frames
            .iter()
            .find(|f| f.id == "null_window" || f.id == "mtd_f");
        let x0 = nw.map_or(g.frames.last().map_or(34.0, |f| f.x + 62.0), |f| f.x + 62.0);
        let lbl = if nw.is_some() {
            "(a, b)"
        } else {
            "(a, b) = bounds"
        };
        let wi = cites(
            lens_info(&format!("window {lbl}"))
                .fact("wire type", "Window (this page's label)")
                .fact("carried", "with the position, into every child's search (game.rs:1152)")
                .note(if nw.is_some() {
                    "The yes/no driver picks the window for the next gap between adjacent rewards (game.rs:970-977)."
                } else {
                    "Without null_window or mtd_f, solve() runs one search over the whole range: (a, b) = the reward bounds (game.rs:947-958)."
                }),
            &if nw.is_some() { vec![C::window(), C::solve()] } else { vec![C::full_range()] },
        );
        hseg(
            c,
            x0,
            g.x_w - 3.0,
            YF + 9.0,
            col_win,
            1.6,
            false,
            false,
            wi.clone(),
        );
        c.p.circle_filled(xf.pt(x0, YF + 9.0), 2.6 * xf.k, col_win);
        let lines: Vec<String> = if nw.is_some() {
            vec![lbl.into()]
        } else {
            vec!["(a, b) =".into(), "bounds".into()]
        };
        let r = dlines(
            c,
            xf,
            x0 + 2.0,
            YF + 23.0,
            Align2::LEFT_BOTTOM,
            &lines,
            9.5,
            11.5,
            col_win,
            false,
        );
        c.hover(r, wi);
    }
    // memo lanes
    {
        let my = |y: f32| y + memo_rise;
        let best_x = tt.map(|t| t.x - 4.0);
        let f_end = best_x.unwrap_or(g.x_w);
        let (tx, ty) = (g.x_t, (my(YM1) + my(YM2)) / 2.0);
        let mut mi = lane_i(
            "Table",
            "get(k): the entry for this position, if any",
            &[C::tt_get()],
            &[("forward lane of the memo", lane("memo").fwd.to_string())],
        );
        let _ = &mut mi;
        hseg(c, g.x_k, f_end, my(YM1), col_tab, 1.8, false, false, mi);
        hseg(
            c,
            g.x_store,
            tx + 15.0,
            my(YM2),
            col_tab,
            1.8,
            false,
            true,
            lane_i(
                "Table",
                "store(k, lo, hi, best) into the table",
                &[C::store(), C::bounds_store()],
                &[("backward lane of the memo", lane("memo").bwd.to_string())],
            ),
        );
        // get reads the table
        let gp = vec![xf.pt(tx, ty - 14.0), xf.pt(tx, my(YM1) + 4.0)];
        wire(c, &gp, col_tab, 1.4, true, false, xf.k);
        hover_path(c, &gp, 5.0, info("get reads the table").cite(C::tt_get()));
        // k down, window up
        let kp = vec![xf.pt(g.x_k, YF + 2.0), xf.pt(g.x_k, my(YM1) - 3.0)];
        poly(c, &kp, Color32::WHITE, 6.0 * xf.k);
        wire(c, &kp, col_tab, 1.6, false, true, xf.k);
        hover_path(c, &kp, 5.0, cites(info("k: the position's key, to the table").note("w.key(): the table's key: the marking itself over the bit engine, a 128-bit hash over counts (memo.rs:1-26, game.rs:1069)"), &[C::tt_get()]));
        let wp = vec![xf.pt(g.x_w, my(YM1)), xf.pt(g.x_w, YF + 12.0)];
        poly(c, &wp, Color32::WHITE, 6.0 * xf.k);
        wire(c, &wp, col_tab, 1.6, false, true, xf.k);
        hover_path(c, &wp, 5.0, cites(info("[lo, hi] narrows the window").note("a = max(a, lo), b = min(b, hi); a settled entry (lo = hi, lo ≥ b or hi ≤ a) returns at once without searching (game.rs:1069-1079)."), &[C::tt_get()]));
        c.p.circle(
            xf.pt(g.x_w, YF + 9.0),
            3.2 * xf.k,
            Color32::WHITE,
            Stroke::new(1.6, col_win),
        );
        if let (Some(bx), Some(t)) = (best_x, tt) {
            let pts = vec![
                xf.pt(bx, my(YM1)),
                xf.pt(bx, YF + 14.0),
                xf.pt(t.x + 7.0, YF + 14.0),
            ];
            poly(c, &pts, Color32::WHITE, 6.0 * xf.k);
            wire(c, &pts, col_tab, 1.6, true, true, xf.k);
            hover_path(
                c,
                &pts,
                5.0,
                cites(
                    lens_info("state: the table's best move → tt_best")
                        .fact("not a lens", "a loop through state: backward pass, table, a later forward pass")
                        .note("tt_best's forward reads what an earlier backward pass stored: store writes Entry.best, tt.get reads it into first (game.rs:1069-1078), and Ctx carries it to the chain (order.rs:15-24, 52-58)."),
                    &[C::store(), C::tt_get(), C::ctx_read(), C::tt_best()],
                ),
            );
        }
        let sp_ = vec![xf.pt(g.x_store, YB + 2.0), xf.pt(g.x_store, my(YM2) - 3.0)];
        poly(
            c,
            &sp_,
            if present {
                hex("#eff7f5")
            } else {
                Color32::WHITE
            },
            6.0 * xf.k,
        );
        wire(c, &sp_, col_tab, 1.6, false, true, xf.k);
        let si = cites(
            info("[lo, hi], best: stored").note("After the children: the result as [lo, hi] (fail-soft), merged with what the table knew, stored with the best move (game.rs:1168-1185). A leaf is stored as exact (game.rs:1082-1087)."),
            &[C::bounds_store(), C::store(), C::leaf()],
        );
        hover_path(c, &sp_, 5.0, si.clone());
        let r = halo(
            c,
            xf,
            g.x_store - 6.0,
            YB + 18.0,
            Align2::RIGHT_CENTER,
            "lo, hi, best",
            9.5,
            col_tab,
            Color32::WHITE,
            false,
        );
        c.hover(r, si);
    }
    // ---- pieces layer
    {
        // expand: position → children
        let (x, w) = (g.expand_x, EXPAND_W);
        let r = dbox(
            c,
            xf,
            x,
            YF - 32.0,
            w,
            60.0,
            6.0,
            hex("#f4f7fb"),
            col_pos,
            1.2,
            false,
        );
        dmono(
            c,
            xf,
            x + w / 2.0,
            YF - 15.0,
            Align2::CENTER_CENTER,
            "moves(w)",
            10.0,
            t_ink,
        );
        dtext(
            c,
            xf,
            x + w / 2.0,
            YF + 15.0,
            Align2::CENTER_CENTER,
            "children",
            9.5,
            t_muted,
        );
        c.hover(
            r,
            cites(
                lens_info("position → children")
                    .fact("forward", "Position → MoveList")
                    .fact("leaf", "no moves: the score comes back at once and is stored as exact (game.rs:1082-1087)")
                    .note("The moves enabled at the position; each is played and settled into a child, then keyed by the order chain (game.rs:1089-1112)."),
                &[C::children(), C::leaf()],
            ),
        );
        // order socket
        let os = g.order_sock;
        let sr = dbox(
            c,
            xf,
            os,
            YF - 32.0,
            ORDER_SOCK_W,
            60.0,
            6.0,
            Color32::from_white_alpha(140),
            hex("#9aa3ad"),
            1.3,
            true,
        );
        slot_target(c, sr, "order");
        dbold(
            c,
            xf,
            os + ORDER_SOCK_W / 2.0,
            YF - 9.0,
            Align2::CENTER_CENTER,
            "+",
            15.0,
            t_muted,
            false,
        );
        dtext(
            c,
            xf,
            os + ORDER_SOCK_W / 2.0,
            YF + 17.0,
            Align2::CENTER_CENTER,
            "order",
            8.5,
            t_muted,
        );
        c.hover(
            sr,
            slot_info(c, "order")
                .fact(
                    "drop",
                    "an order piece: it joins the end of the chain (plan.rs check decides)",
                )
                .note(ANALOGY),
        );
        // filters: the plan's filters, or a socket
        let fxs = g.filters_x;
        if g.filters.is_empty() {
            let fr = dbox(
                c,
                xf,
                fxs,
                YF - 32.0,
                FILTERS_W,
                60.0,
                6.0,
                Color32::from_white_alpha(140),
                hex("#9aa3ad"),
                1.3,
                true,
            );
            slot_target(c, fr, "filters");
            dbold(
                c,
                xf,
                fxs + FILTERS_W / 2.0,
                YF - 10.0,
                Align2::CENTER_CENTER,
                "filters",
                10.0,
                t_muted,
                false,
            );
            dtext(
                c,
                xf,
                fxs + FILTERS_W / 2.0,
                YF + 17.0,
                Align2::CENTER_CENTER,
                "none run",
                8.5,
                t_muted,
            );
            c.hover(
                fr,
                cites(
                    slot_info(c, "filters")
                        .fact(
                            "plan.rs",
                            "filters: [] runs none; absent runs Filter::DEFAULT = [must_win]",
                        )
                        .fact(
                            "placement",
                            "on the move-list lane, after the chain: this page's judgment",
                        ),
                    &[C::filters_doc(), C::filter_default()],
                ),
            );
        } else {
            for (k, name) in g.filters.iter().enumerate() {
                let x = fxs + k as f32 * (FILTERS_W + 8.0);
                let col = kind_color_of("Filter");
                let r = dbox(
                    c,
                    xf,
                    x,
                    YF - 32.0,
                    FILTERS_W,
                    60.0,
                    6.0,
                    Color32::WHITE,
                    col,
                    1.3,
                    false,
                );
                dbold(
                    c,
                    xf,
                    x + FILTERS_W / 2.0,
                    YF - 12.0,
                    Align2::CENTER_CENTER,
                    &name.replace('_', "_\u{200b}"),
                    8.5,
                    col,
                    true,
                );
                let sub: &[&str] = if name == "must_win" {
                    &["settles at", "a win"]
                } else {
                    &["drops losing", "moves"]
                };
                dlines(
                    c,
                    xf,
                    x + FILTERS_W / 2.0,
                    YF + 6.0,
                    Align2::CENTER_BOTTOM,
                    &sub.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                    8.5,
                    10.5,
                    t_muted,
                    false,
                );
                slot_target(c, r, "filters");
                let (a, b) = if name == "must_win" {
                    (1103, 1111)
                } else {
                    (1112, 1129)
                };
                let fi = cites(
                    piece_base(name)
                        .fact(
                            "placement",
                            "on the move-list lane, after the chain: this page's judgment",
                        )
                        .fact(
                            "default",
                            if name == "must_win" {
                                "yes: Filter::DEFAULT"
                            } else {
                                "no: named in the plan"
                            },
                        )
                        .note(format!(
                            "{} {ANALOGY}",
                            sp::piece_info(name).map_or("", |p| p.doc)
                        )),
                    &[
                        gs(a, b, &format!("{name} in search")),
                        C::filters_doc(),
                        C::filter_default(),
                    ],
                );
                c.hover(r, fi);
                if plan.get("filters").is_some() {
                    remove_x(c, xf, x + FILTERS_W - 2.0, YF - 31.0, "filters", k, name);
                }
            }
        }
        // the pieces
        for p in &g.pieces {
            let l = lane(&p.name);
            let is_hist = p.name == "history";
            let (dx, dy) = slide(c, "order", &p.name);
            let (px, pyo) = (p.x + dx, dy);
            let yf = YF + pyo;
            let box_r = dbox(
                c,
                xf,
                px,
                yf - 32.0,
                PW,
                122.0,
                7.0,
                Color32::WHITE,
                order_col,
                1.4,
                false,
            );
            // the shaded head
            let head = xf.rect(px + 1.0, yf - 31.0, PW - 2.0, 58.0);
            c.p.rect_filled(head, 5.0, hex("#fbf1e2"));
            dpoly(
                c,
                &[xf.pt(px, yf + 28.0), xf.pt(px + PW, yf + 28.0)],
                hex("#e3c79c"),
                1.0,
                (3.0 * xf.k, 2.0 * xf.k),
            );
            poly(
                c,
                &[xf.pt(px, yf), xf.pt(px + PW, yf)],
                col_moves,
                2.2 * xf.k.max(0.8),
            );
            dbold(
                c,
                xf,
                px + PW / 2.0,
                yf - 15.0,
                Align2::CENTER_CENTER,
                &p.name,
                10.0,
                order_col,
                true,
            );
            let key: Vec<String> = match p.name.as_str() {
                "tt_best" => vec!["t = table best".into(), "decisive".into()],
                "immediate_win" => vec!["t wins now".into(), "decisive".into()],
                "history" => vec!["history[t]".into(), "past cutoffs".into()],
                "centrality" => vec!["goals t feeds".into(), "fixed per t".into()],
                "threats" => vec!["goals 1 short".into(), "after t".into()],
                _ => vec![],
            };
            dlines(
                c,
                xf,
                px + PW / 2.0,
                yf + 12.0,
                Align2::CENTER_BOTTOM,
                &key,
                9.5,
                11.0,
                t_ink,
                false,
            );
            let by = yf + 56.0;
            if is_hist {
                let pts = vec![
                    xf.pt(px + PW - 12.0, yf + 90.0),
                    xf.pt(px + PW - 12.0, by),
                    xf.pt(px + 12.0, by),
                ];
                wire(c, &pts, col_cut, 2.0 * xf.k.max(0.8), false, true, xf.k);
                hover_path(
                    c,
                    &pts,
                    5.0,
                    cites(
                        lens_info("history: backward lane").note(l.bwd),
                        &[C::hist_cut(), C::chain_cutoff(), C::cutoff()],
                    ),
                );
                let sp2 = dcubic(
                    xf,
                    [
                        (px + 9.0, by - 4.0),
                        (px + 2.0, by - 16.0),
                        (px + 4.0, yf + 30.0),
                        (px + 10.0, yf + 26.0),
                    ],
                );
                wire(c, &sp2, col_cut, 1.4, true, true, xf.k);
                hover_path(c, &sp2, 5.0, cites(lens_info("state: history[t], read by the next forward pass").note("history[t] += 1 on a cutoff (order.rs:73-75); the key of t is history[t] next time (order.rs:68-71). A loop through state, not a lens law."), &[C::hist_cut(), C::hist_key()]));
                dlines(
                    c,
                    xf,
                    px + PW / 2.0 - 5.0,
                    by + 15.0,
                    Align2::CENTER_BOTTOM,
                    &["history[t]".into(), "+= 1".into()],
                    9.5,
                    11.0,
                    col_cut,
                    true,
                );
            } else {
                dpoly(
                    c,
                    &[xf.pt(px + PW - 12.0, by), xf.pt(px + 12.0, by)],
                    hex("#c3c8cf"),
                    1.2,
                    (1.5 * xf.k, 3.0 * xf.k),
                );
                dtext(
                    c,
                    xf,
                    px + PW / 2.0,
                    by + 16.0,
                    Align2::CENTER_CENTER,
                    "no backward",
                    9.5,
                    t_muted,
                );
            }
            remove_x(c, xf, px + PW - 2.0, yf - 31.0, "order", p.i, &p.name);
            let mut pi = piece_base(&p.name);
            pi.title = format!("{} (a lens on the move-list lane)", pi.title);
            pi = pi
                .fact(
                    "position in the chain",
                    format!("{} of {}", p.i + 1, g.pieces.len()),
                )
                .fact("forward lane (top)", l.fwd)
                .fact("backward lane (bottom)", l.bwd)
                .fact(
                    "lane types",
                    "MoveList → MoveList; order-only, so the value cannot change (order.rs:1-7)",
                );
            pi.note = format!("{} {ANALOGY}", pi.note);
            c.hover(box_r, cites(pi, &lane_cites(&p.name)));
            c.scene.emit("lens", &p.name, json!({}));
        }
        // max/min, the cutoff diamond, the bus
        let mm = xf.rect(g.x_mm - 24.0, YB - 11.0, 48.0, 22.0);
        c.p.rect(
            mm,
            11.0 * xf.k,
            Color32::WHITE,
            Stroke::new(1.4, col_val),
            StrokeKind::Middle,
        );
        dbold(
            c,
            xf,
            g.x_mm,
            YB,
            Align2::CENTER_CENTER,
            "max/min",
            9.5,
            col_val,
            false,
        );
        c.hover(mm, cites(info("max / min over the children").note("The mover maximizes (s > 0) or minimizes; a = max(a, v) or b = min(b, v) (game.rs:1142-1162)."), &[C::loop_()]));
        let dc = xf.pt(g.x_cut, YB);
        let rr = 10.0 * xf.k;
        c.p.add(Shape::convex_polygon(
            vec![
                dc + Vec2::new(0.0, -rr),
                dc + Vec2::new(rr, 0.0),
                dc + Vec2::new(0.0, rr),
                dc + Vec2::new(-rr, 0.0),
            ],
            Color32::WHITE,
            Stroke::new(1.4, col_cut),
        ));
        dbold(
            c,
            xf,
            g.x_cut,
            YB,
            Align2::CENTER_CENTER,
            "≥",
            10.0,
            col_cut,
            false,
        );
        let cut_i = cites(
            lens_info("a ≥ b: a cutoff").fact("forward effect", "stops the forward pass: alpha-beta interleaves the two passes").note("The window closed: the remaining children are not tried (break), and every chain piece is told (chain.cutoff, game.rs:1163-1166)."),
            &[C::cutoff(), C::chain_cutoff()],
        );
        c.hover(
            Rect::from_center_size(dc, Vec2::splat(2.0 * rr)),
            cut_i.clone(),
        );
        let cr_ = halo(
            c,
            xf,
            g.x_cut + 8.0,
            YB - 26.0,
            Align2::LEFT_CENTER,
            "a ≥ b: cutoff",
            9.5,
            col_cut,
            PAPER,
            false,
        );
        c.hover(cr_, cut_i);
        if let Some(first) = g.pieces.first() {
            let left = first.x + PW - 12.0;
            let bus = vec![
                xf.pt(g.x_cut, YB - 11.0),
                xf.pt(g.x_cut, BUS),
                xf.pt(left, BUS),
            ];
            poly(c, &bus, Color32::WHITE, 5.0 * xf.k);
            wire(c, &bus, col_cut, 1.6, false, false, xf.k);
            let bi = cites(lens_info("chain.cutoff(t): the cutoff, sent to every piece").note("Chain::cutoff calls each piece's cutoff (order.rs:277-281); only history overrides the no-op (order.rs:36-37, 73-75)."), &[C::chain_cutoff(), C::cutoff_noop(), C::hist_cut(), C::cutoff()]);
            hover_path(c, &bus, 5.0, bi.clone());
            let r = halo(
                c,
                xf,
                g.x_cut - 6.0,
                BUS + 14.0,
                Align2::RIGHT_CENTER,
                "chain.cutoff(t)",
                9.5,
                col_cut,
                PAPER,
                false,
            );
            c.hover(r, bi);
            for p in &g.pieces {
                let tx = p.x + PW - 12.0;
                if p.name == "history" {
                    let pts = vec![xf.pt(tx, BUS), xf.pt(tx, YF + 92.0)];
                    wire(c, &pts, col_cut, 1.6, false, false, xf.k);
                    hover_path(
                        c,
                        &pts,
                        5.0,
                        cites(info("cutoff → history"), &[C::hist_cut()]),
                    );
                } else {
                    let pts = vec![xf.pt(tx, BUS), xf.pt(tx, YF + 92.0)];
                    dpoly(
                        c,
                        &pts,
                        col_cut.gamma_multiply(0.7),
                        1.2,
                        (1.5 * xf.k, 2.5 * xf.k),
                    );
                    hover_path(c, &pts, 5.0, cites(info(format!("cutoff → {}: a no-op", p.name)).note("Chain::cutoff calls this piece's cutoff too; it does not override the default, which does nothing (order.rs:36-37)."), &[C::cutoff_noop(), C::chain_cutoff()]));
                }
            }
        }
        // heuristics socket
        let (hx, hy) = (g.ghost_x + 8.0, YB - 70.0);
        let hr = dbox(
            c,
            xf,
            hx,
            hy,
            GHOST_W - 16.0,
            38.0,
            6.0,
            Color32::from_white_alpha(140),
            hex("#9aa3ad"),
            1.3,
            true,
        );
        slot_target(c, hr, "heuristics");
        dbold(
            c,
            xf,
            g.ghost_x + GHOST_W / 2.0,
            hy + 16.0,
            Align2::CENTER_CENTER,
            "heuristics",
            9.5,
            t_muted,
            false,
        );
        dtext(
            c,
            xf,
            g.ghost_x + GHOST_W / 2.0,
            hy + 29.0,
            Align2::CENTER_CENTER,
            "none built",
            8.5,
            t_muted,
        );
        c.hover(
            hr,
            cites(
                slot_info(c, "heuristics")
                    .fact("plan.rs", "names of pieces that can change the value; the reserved name static_eval is refused in exact mode")
                    .fact("placement", "by the recursion, where a static evaluation would stand in for searching further down: this page's judgment")
                    .fact("today", "none built: exact mode refuses a heuristic (it can change the value); play mode: not built in this release"),
                &[pl(44, 47, "pieces that can change the value: none built yet")],
            ),
        );
    }
    let _ = has_hist;
    let _ = hist;

    // ---- the formula strip, the caller, the engine, the drivers socket
    {
        let chain_txt = if g.chain.is_empty() {
            "[] (identity: the net's own move order)".to_string()
        } else {
            g.chain.join(" ▹ ")
        };
        let mut parts: Vec<String> = vec![];
        if let Some(n) = g.budget {
            parts.push(format!("budget({})", fmt_n(n)));
        }
        for f in &g.frames {
            if f.id != "budget" {
                parts.push(f.id.clone());
            }
        }
        parts.push(format!(
            "core[{}{chain_txt}; {}{}]",
            if g.chain_default {
                "Chain::default = "
            } else {
                ""
            },
            if g.filters.is_empty() {
                String::new()
            } else {
                format!("filters {}; ", g.filters.join(", "))
            },
            match g.memo {
                Some(n) => format!("memo({})", fmt_n(n)),
                None => "table: default".into(),
            },
        ));
        dtext(
            c,
            xf,
            8.0,
            24.0,
            Align2::LEFT_CENTER,
            "plan as a composite",
            10.5,
            t_muted,
        );
        let fr = dbold(
            c,
            xf,
            118.0,
            24.0,
            Align2::LEFT_CENTER,
            &parts.join(" ∘ "),
            12.0,
            t_ink,
            true,
        );
        let m = format!(
            "mode: {mode}{}",
            if mode == "play" {
                " (the same search: solver() does not read the mode)"
            } else {
                ""
            }
        );
        dtext(
            c,
            xf,
            g.w - 10.0,
            24.0,
            Align2::RIGHT_CENTER,
            &m,
            10.5,
            t_muted,
        );
        let strip = Rect::from_min_max(xf.pt(0.0, 8.0), Pos2::new(fr.right(), xf.pt(0.0, 36.0).y));
        c.hover(
            strip,
            cites(
                lens_info("the plan as a composite")
                    .fact("JSON", c.d.plan.as_ref().map_or(String::new(), |p| s(&p["json"]).to_string()))
                    .fact("nesting", "fixed by pflow-rs; the drivers' order in the JSON does not change it (plan.rs:205-281)")
                    .note("Drivers nested around the core, as pflow-rs runs them; the core holds the order chain, the filters and the memo. The ROADMAP (Phase 6) proposes reading this as Solver = drivers ∘ core[order chain, filters, bounds, memo]."),
                &[C::drivers_flags(), C::solver(), road(428, 430, "the core as a box Position → Value⊥, drivers nested around it")],
            ),
        );
        // the caller
        let cr_ = dlines(
            c,
            xf,
            10.0,
            YF - 44.0,
            Align2::LEFT_BOTTOM,
            &["Solver::".into(), "value(mk)".into()],
            10.5,
            13.0,
            t_ink,
            true,
        );
        dtext(
            c,
            xf,
            10.0,
            YF - 9.0,
            Align2::LEFT_CENTER,
            "position",
            9.5,
            t_muted,
        );
        dmono(
            c,
            xf,
            10.0,
            YB + 17.0,
            Align2::LEFT_CENTER,
            "Option<f64>",
            9.5,
            t_muted,
        );
        dtext(
            c,
            xf,
            10.0,
            YB + 29.0,
            Align2::LEFT_CENTER,
            "value or ⊥",
            9.5,
            t_muted,
        );
        let caller = cr_.union(Rect::from_min_max(
            xf.pt(8.0, YF - 50.0),
            xf.pt(84.0, YB + 36.0),
        ));
        c.hover(
            caller,
            cites(
                lens_info("caller: Solver::value(mk)")
                    .fact("forward in", "the marking mk (a position)")
                    .fact("backward out", "Result<Option<f64>>: Some(value), or ⊥ (None) past the budget")
                    .fact("not drawn", "the Err case: e.g. automatic transitions that never stop")
                    .note("The exact value (+1 side's view) of a settled position with both sides playing perfectly, or None past the budget (game.rs:809-815)."),
                &[C::value()],
            ),
        );
        // engine: a designed slot
        let y = (YF + YB) / 2.0 - 20.0;
        let er = dbox(
            c,
            xf,
            8.0,
            y,
            74.0,
            40.0,
            6.0,
            Color32::from_white_alpha(140),
            hex("#9aa3ad"),
            1.3,
            true,
        );
        dbold(
            c,
            xf,
            45.0,
            y + 17.0,
            Align2::CENTER_CENTER,
            "engine",
            10.0,
            t_muted,
            false,
        );
        dtext(
            c,
            xf,
            45.0,
            y + 30.0,
            Align2::CENTER_CENTER,
            "designed",
            9.0,
            t_muted,
        );
        slot_target(c, er, "engine");
        c.hover(er, cites(slot_info(c, "engine").fact("today", "game.rs picks the engine itself: the bit engine when the marking fits, else counts (game.rs:918-934)"), &[C::run()]));
        // drivers socket
        let dr = dbox(
            c,
            xf,
            FX0,
            40.0,
            116.0,
            20.0,
            10.0,
            Color32::from_white_alpha(140),
            hex("#9aa3ad"),
            1.3,
            true,
        );
        dbold(
            c,
            xf,
            FX0 + 58.0,
            54.0,
            Align2::CENTER_CENTER,
            "⊕ drivers slot",
            10.0,
            t_muted,
            false,
        );
        slot_target(c, dr, "drivers");
        c.hover(
            dr,
            cites(
                slot_info(c, "drivers")
                    .fact("drop", "budget, null_window, mtd_f or aspiration; pflow-rs nests them budget outside the yes/no driver whatever their JSON order")
                    .fact("now", if g.frames.is_empty() { "none (drivers given empty: one search over the whole range)".to_string() } else { g.frames.iter().map(|f| f.id.clone()).collect::<Vec<_>>().join(" ∘ ") })
                    .note(ANALOGY),
                &[C::drivers_flags(), C::drivers_doc()],
            ),
        );
        let _ = has_driver(&g, "budget");
    }

    // ---- legend
    {
        let y = g.legend_y;
        let item = |c: &Ctx, x: f32, col: Color32, dash: bool, rev: bool, label: &str| {
            let (a, b) = if rev { (x + 32.0, x) } else { (x, x + 32.0) };
            let pts = vec![xf.pt(a, y), xf.pt(b, y)];
            wire(c, &pts, col, 2.0 * xf.k.max(0.8), dash, !dash, xf.k);
            dtext(c, xf, x + 42.0, y, Align2::LEFT_CENTER, label, 10.5, t_ink);
        };
        item(
            c,
            10.0,
            col_pos,
            false,
            false,
            "forward lane (top): positions and move lists, down the tree",
        );
        item(
            c,
            380.0,
            col_val,
            false,
            true,
            "backward lane (bottom): values and bounds, up the tree",
        );
        item(
            c,
            740.0,
            col_tab,
            true,
            false,
            "dashed: state left for a later pass (not a lens law)",
        );
        let types = [
            ("Position", "Position"),
            ("MoveList", "MoveList"),
            ("Value / Value⊥", "Value"),
            ("Window", "Window"),
            ("Table", "Table"),
            ("Cutoff", "Cutoff"),
        ];
        let mut x = 10.0;
        for (name, k) in types {
            let r = xf.rect(x, y + 12.0, 12.0, 12.0);
            c.p.rect_filled(r, 3.0 * xf.k, type_color(k));
            dtext(
                c,
                xf,
                x + 17.0,
                y + 18.0,
                Align2::LEFT_CENTER,
                name,
                10.5,
                t_ink,
            );
            x += 30.0 + tw(c, xf, name, 10.5, false);
        }
        dtext(c, xf, x + 6.0, y + 18.0, Align2::LEFT_CENTER, "Position, MoveList, Value, Value⊥: ROADMAP ns:WireType (PROPOSED). Window, Table, Cutoff: this page's labels.", 10.0, t_muted);
        dpoly(
            c,
            &[xf.pt(10.0, y + 34.0), xf.pt(34.0, y + 34.0)],
            hex("#9aa3ad"),
            1.3,
            (4.0 * xf.k, 3.0 * xf.k),
        );
        dtext(
            c,
            xf,
            42.0,
            y + 38.0,
            Align2::LEFT_CENTER,
            "dashed outline: an empty or designed slot (drop target)",
            10.0,
            t_muted,
        );
        dpoly(
            c,
            &[xf.pt(380.0, y + 34.0), xf.pt(412.0, y + 34.0)],
            hex("#c3c8cf"),
            1.2,
            (1.5 * xf.k, 3.0 * xf.k),
        );
        dtext(
            c,
            xf,
            422.0,
            y + 38.0,
            Align2::LEFT_CENTER,
            "dotted: no backward pass (the cutoff is a no-op for that piece)",
            10.0,
            t_muted,
        );
        let lr = Rect::from_min_max(xf.pt(0.0, y - 8.0), xf.pt(g.w, y + 44.0));
        c.hover(lr, cites(info("legend").fact("wire types", "Position, Value, Value⊥, MoveList are the ROADMAP's PROPOSED ns:WireType; the rest are this page's labels"), &[C::wire_types()]));
    }
    plan_warn(c);
    c.note("analogy", "Analogy of shape only. Open games (Ghani, Hedges, Winschel, Zahn) characterise equilibria: a best-response condition on strategy profiles. This plan computes a value: minimax with alpha-beta under a node budget. The lens drawing borrows only the shape (moves flow forward, down the tree; values flow back, up); no equilibrium is computed or checked, and no game-theoretic claim is made.");
    c.note("honesty", "Not a lens in the strict sense: alpha-beta interleaves the passes (each child's value comes back before the next child is tried, and a cutoff stops the forward pass), and the table and history carry state from backward passes into later forward passes (dashed loops). The drawing shows the shape of one round trip, not a schedule.");
    c.note("honesty", "The nesting is pflow-rs's, not the JSON's: drivers are read into flags (plan.rs:205-281). budget counts nodes across every yes/no search of one solve() run (nodes is reset only before each engine run, game.rs:926, 933), so it is outermost; the yes/no driver wraps the root search. Filters run inside the core: must_win is on by default (a winning move settles a node at once), and drops moves only where the value is already known. Lane labels paraphrase the cited lines; Position, Value, Value⊥ and MoveList are the ROADMAP's PROPOSED wire types; Window, Table and Cutoff are this page's labels. No search runs here, so there is no play dot (the prototype's is illustrative).");
}
