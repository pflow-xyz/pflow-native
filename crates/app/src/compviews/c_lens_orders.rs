//! The lens view's orders example, after the prototype's `lens-orders.js`.
//!
//! Two modes (`view.composition {lens}`):
//! - strict: the lens test on the composition as go-pflow builds it, three
//!   checks computed by `bundle.lens` (core `lens.rs`), each with its verdict;
//! - assume: the fallback. Every transition pair with a backward pass, found
//!   mechanically, is drawn as a lens: the saga's steps as a chain of boxes
//!   (`do_` on the forward lane, `undo_` on the backward lane, the residual
//!   between them), the other instances' pairs above the step their forward
//!   transition is fused with. Fusion lines come from the flat net, so a
//!   link dragged in redraws the picture and re-runs the check.
//!
//! The manifest (`verdict`, `pair`, `step`, `fix`) is emitted the same in
//! both modes (spec D5, D13): the strict card draws every pair, every step
//! and every repair as an inspectable row, the assume drawing the same as
//! lens boxes, rings and tables.

#![allow(clippy::too_many_arguments)]

use super::c_draw::*;
use super::*;

const FWD: Color32 = Color32::from_rgb(0x2f, 0x6f, 0xb3);
const BWD: Color32 = Color32::from_rgb(0xb0, 0x41, 0x3e);
const FAIL: Color32 = Color32::from_rgb(0x88, 0x66, 0x00);
const EV: Color32 = Color32::from_rgb(0x22, 0x77, 0x44);
const OK: Color32 = Color32::from_rgb(0x22, 0x77, 0x44);
const GREY: Color32 = Color32::from_rgb(0x8a, 0x92, 0x9c);

fn flat_cites() -> Vec<String> {
    let o = comp::ontology();
    let rev = o["repos"]["go-pflow"]["rev"].as_str().unwrap_or("");
    vec![
        format!("go-pflow metamodel/compose_flatten.go @{rev} (Flatten: transition classes by union-find over event links)"),
        format!("go-pflow metamodel/compose_fuse.go @{rev} (fusion over equivalence classes)"),
    ]
}

fn go_rule_cites() -> Vec<String> {
    let o = comp::ontology();
    let rev = o["repos"]["go-pflow"]["rev"].as_str().unwrap_or("");
    vec![
        format!("go-pflow metamodel/compose.go:635-659 @{rev} (endpoint kinds must match the link kind)"),
        format!("go-pflow metamodel/compose_matrix.go:16-57 @{rev} (linkLegal: the net-type x link-kind matrix)"),
    ]
}

fn with_cites(mut i: Info, cs: &[String]) -> Info {
    for c in cs {
        if !i.cites.contains(c) {
            i.cites.push(c.clone());
        }
    }
    i
}

fn pair_id(p: &Value) -> String {
    format!("{}/{}->{}", s(&p["subnet"]), s(&p["t"]), s(&p["u"]))
}

fn fix_id(f: &Value) -> String {
    format!("{}:{}", s(&f["how"]), s(&f["u"]))
}

/// The whole example: the mode toggle, the manifest, then one of the two
/// drawings.
pub(super) fn lens_orders(c: &mut Ctx, area: Rect, mode: &str) {
    let Some(rep) = c.d.lens.clone() else { return };
    let a = rep["analysis"].clone();
    let assume = mode == "assume";
    // the mode toggle (a registry command)
    c.text(
        area.left_top() + Vec2::new(0.0, 4.0),
        Align2::LEFT_TOP,
        "Lens test:",
        12.0,
        MUTED,
    );
    let t1 = Rect::from_min_size(
        area.left_top() + Vec2::new(70.0, 0.0),
        Vec2::new(150.0, 22.0),
    );
    let t2 = Rect::from_min_size(
        area.left_top() + Vec2::new(226.0, 0.0),
        Vec2::new(210.0, 22.0),
    );
    c.button(
        t1,
        "strict (as composed)",
        !assume,
        "view.composition",
        json!({"lens": "strict"}),
    );
    c.button(
        t2,
        "assume a backward pass (fallback)",
        assume,
        "view.composition",
        json!({"lens": "assume"}),
    );
    c.text(
        area.left_top() + Vec2::new(446.0, 4.0),
        Align2::LEFT_TOP,
        if assume {
            "pairs with a backward pass, found mechanically; the composition is still checked"
        } else {
            "the composition as go-pflow builds it"
        },
        12.0,
        MUTED,
    );
    let holds = a["holds"] == true;
    let glued = a["glued"].as_i64().unwrap_or(0);
    c.scene.emit(
        "verdict",
        "strict",
        json!({"holds": holds, "glued": glued,
               "legal_token": a["legal"]["token"].as_i64().unwrap_or(0),
               "legal_data": a["legal"]["data"].as_i64().unwrap_or(0)}),
    );
    // the manifest is the same in both modes: what the drawings below show
    let pairs: Vec<Value> = arr(&a["pairs"]).to_vec();
    let steps: Vec<Value> = arr(&a["steps"]).to_vec();
    let fixes: Vec<Value> = arr(&rep["fixes"]).to_vec();
    for p in &pairs {
        c.scene.emit("pair", &pair_id(p), json!({}));
    }
    for st in &steps {
        c.scene.emit(
            "step",
            s(&st["flat"]),
            json!({"composes": st["composes"] == true, "split": st["split"] == true,
                   "dead_forward": s(&st["dead_forward"]["constraint"])}),
        );
    }
    for f in &fixes {
        c.scene.emit(
            "fix",
            &fix_id(f),
            json!({"holds": f["result"]["holds"] == true}),
        );
    }
    let body = Rect::from_min_max(area.left_top() + Vec2::new(0.0, 32.0), area.right_bottom());
    if a["ok"] != true {
        c.note(
            "warn",
            format!(
                "The composition does not flatten: {}",
                strs(&a["errors"]).join("; ")
            ),
        );
        return;
    }
    if assume {
        assumed(c, body, &a, &fixes);
        c.note("judgment", "Assumption (fallback): the pairs drawn as lenses were found mechanically (a residual only t produces and only u consumes; u returns what t moved on resource places), and the forward pass is the \"commit\" scenario's run (judgment, frozen). Inside the saga the chain is go-pflow's own construction; across instances only the flat net decides what fires together. The red rings are where that fails to be lens composition.");
        if subnets(c).iter().any(|x| x["extra"] == true) {
            c.note("na", "Added instances count in the check; only those fused with the first saga are drawn above it.");
        }
    } else {
        strict(c, body, &a, &steps, &pairs, &fixes);
        c.note("na", "Strict test: the composition as composed, no assumption. Switch to the fallback to see the pairs that do have a backward pass drawn as lenses.");
    }
    c.note("analogy", "Open games / lenses (Ghani, Hedges, Winschel, Zahn): forward play, backward values. Used here as a test of shape on the flat net, not as game theory.");
}

// ------------------------------------------------------------ strict

fn oriented(a: &Value, p: &Value) -> bool {
    let (t, u) = (
        format!("{}/{}", s(&p["subnet"]), s(&p["t"])),
        format!("{}/{}", s(&p["subnet"]), s(&p["u"])),
    );
    arr(&a["partner"])
        .iter()
        .any(|x| s(&x["t"]) == t && s(&x["u"]) == u)
        || arr(&a["round_trips"])
            .iter()
            .any(|x| s(&x["t"]) == t && s(&x["u"]) == u)
}

fn break_text(a: &Value) -> String {
    let mut parts: Vec<String> = vec![];
    for st in arr(&a["steps"]) {
        if !st["dead_forward"].is_null() {
            let places = strs(&st["dead_forward"]["places"]).join(" and ");
            parts.push(format!(
                "{} can never fire: it needs {places}, and {} allows one token among them",
                s(&st["flat"]),
                s(&st["dead_forward"]["constraint"])
            ));
        }
        if st["split"] == true {
            let back: Vec<String> = arr(&st["back"])
                .iter()
                .map(|b| s(&b["u"]).to_string())
                .collect();
            parts.push(format!(
                "going forward {} fires as one, but its backward maps {} fire as {} separate transitions ({})",
                s(&st["flat"]),
                back.join(", "),
                arr(&st["back_flat"]).len(),
                strs(&st["back_flat"]).join("; ")
            ));
        }
        for d in arr(&st["dead"]) {
            parts.push(format!(
                "the backward rendezvous {} can never fire ({})",
                s(&d["flat"]),
                s(&d["dead"]["constraint"])
            ));
        }
    }
    for r in arr(&a["round_trips"]) {
        parts.push(format!(
            "{}'s backward map {} is fused into the forward pass ({}), so the way back has nothing to fire it",
            s(&r["t"]),
            s(&r["u"]),
            s(&r["u_flat"])
        ));
    }
    if parts.is_empty() {
        String::new()
    } else {
        parts.join("; ") + "."
    }
}

fn run_line(r: &Value) -> String {
    let bad: Vec<String> = arr(&r["expect"])
        .iter()
        .filter(|e| e["pass"] != true)
        .map(|e| format!("{} = {}, expected {}", s(&e["place"]), e["actual"], e["n"]))
        .collect();
    let stop = r["stopped_step"]
        .as_str()
        .map(|x| format!(" (stopped at {x})"))
        .unwrap_or_default();
    format!("{}: {}{stop}", s(&r["id"]), bad.join("; "))
}

fn strict(c: &mut Ctx, area: Rect, a: &Value, steps: &[Value], pairs: &[Value], fixes: &[Value]) {
    let x0 = area.left() + 6.0;
    let w = (area.width() - 12.0).min(1000.0);
    let ids: Vec<String> = subnets(c).iter().map(|x| s(&x["id"]).to_string()).collect();
    let holds = a["holds"] == true;
    let glued = a["glued"].as_i64().unwrap_or(0);
    let lg = |k: &str| a["legal"][k].as_i64().unwrap_or(0);
    let mut y = area.top() + 4.0;
    let top = y;
    let head = if holds {
        "The composition is a lens composite.".to_string()
    } else {
        format!(
            "No: {} is not a lens composite as composed.",
            ids.join(" ⊗ ")
        )
    };
    let hr = para(c, Pos2::new(x0, y), w, &head, 16.0, INK, None, false);
    c.hover(
        hr,
        with_cites(
            info("the lens test (strict)")
                .fact("legal links", format!("token {}, data {}, event {}, guard {}", lg("token"), lg("data"), lg("event"), lg("guard")))
                .fact("place links used", glued.to_string())
                .fact("verdict", if holds { "a lens composite" } else { "not a lens composite" })
                .note("Computed by bundle.lens (core lens.rs) from the subnets, go-pflow's link rules (validateLink) and its Flatten. A lens composite needs a boundary to compose along, a backward map in every part, and the parts' backward maps to fire together the way their forward maps do."),
            &[go_rule_cites(), flat_cites()].concat(),
        ),
    );
    y = hr.bottom() + 8.0;
    let r = para(c, Pos2::new(x0, y), w, "A lens composite needs a boundary to compose along, a backward map in every part, and the parts' backward maps to fire together the way their forward maps do. Each check below is computed from go-pflow's link rules and its Flatten (as bundle.lens runs them).", 12.5, INK, None, false);
    y = r.bottom() + 10.0;
    let mark = |c: &Ctx, y: f32, ok: bool| {
        c.p.text(
            Pos2::new(x0, y),
            Align2::LEFT_TOP,
            if ok { "✓" } else { "✗" },
            FontId::proportional(13.0),
            if ok { OK } else { LEAK },
        );
    };
    // 1. a boundary
    mark(c, y, glued > 0);
    let t1 = format!(
        "Lens composition is sequential: one part's output boundary is the next part's input. Among {} go-pflow allows {} token and {} data links (and {} event, {} guard), and the composition uses {} place link{}, so {}.",
        ids.join(", "), lg("token"), lg("data"), lg("event"), lg("guard"), glued,
        if glued == 1 { "" } else { "s" },
        if glued > 0 { "a boundary is glued".to_string() } else { "no boundary is glued: the composite is ⊗ plus transition fusion (the cospan view shows no pushout). Event fusion is a rendezvous, not a composition along a boundary".to_string() }
    );
    let r = para(
        c,
        Pos2::new(x0 + 20.0, y),
        w - 20.0,
        &t1,
        12.5,
        INK,
        Some(("A boundary to compose along. ", INK)),
        false,
    );
    c.hover(
        r,
        with_cites(
            info("1. a boundary to compose along")
                .fact("token links legal", lg("token").to_string())
                .fact("data links legal", lg("data").to_string())
                .fact("place links used", glued.to_string())
                .note("Every ordered port pair is tried against validate_link; a token or data link glues a boundary, an event link only fuses transitions."),
            &go_rule_cites(),
        ),
    );
    y = r.bottom() + 8.0;
    // 2. a backward map in every part
    let nb = strs(&a["no_backward"]);
    mark(c, y, nb.is_empty());
    let t2 = format!(
        "{}Found mechanically (a residual place only t produces and only u consumes, and u returns what t moved on resource places):",
        if nb.is_empty() { String::new() } else { format!("None found in {}. ", nb.join(", ")) }
    );
    let r = para(
        c,
        Pos2::new(x0 + 20.0, y),
        w - 20.0,
        &t2,
        12.5,
        INK,
        Some(("A backward map in every part. ", INK)),
        false,
    );
    y = r.bottom() + 4.0;
    // chips: every pair found; the ones the commit run orients are drawn solid
    let mut cx = x0 + 20.0;
    let mut cy = y;
    for p in pairs {
        let id = pair_id(p);
        let label = format!("{}/{} → {}", s(&p["subnet"]), s(&p["t"]), s(&p["u"]));
        let wd = label.chars().count() as f32 * 6.6 + 12.0;
        if cx + wd > x0 + w {
            cx = x0 + 20.0;
            cy += 22.0;
        }
        let rc = Rect::from_min_size(Pos2::new(cx, cy), Vec2::new(wd, 18.0));
        let or = oriented(a, p);
        c.boxr(
            rc,
            if or {
                Color32::from_rgb(0xf1, 0xea, 0xf7)
            } else {
                Color32::from_rgb(0xf3, 0xf4, 0xf6)
            },
            if or {
                MUTED
            } else {
                Color32::from_rgb(0xd0, 0xd4, 0xda)
            },
            1.0,
        );
        c.mono(
            rc.center(),
            Align2::CENTER_CENTER,
            &label,
            10.5,
            if or { INK } else { MUTED },
        );
        c.hover(
            rc,
            info(format!("pair {id}"))
                .fact("residuals", strs(&p["residuals"]).join(", "))
                .fact("resources", strs(&p["resources"]).join(", "))
                .fact("exact inverse", p["exact"].to_string())
                .fact("on the commit run", if or { "yes: oriented forward → backward" } else { "no: only the other direction runs" })
                .note("A forward/backward pair: t's only product is the residual, which only u consumes; u gives back what t moved on resource places."),
        );
        cx += wd + 6.0;
    }
    y = cy + 24.0;
    let r = para(
        c,
        Pos2::new(x0 + 20.0, y),
        w - 20.0,
        "Only the saga's are compensations; the pool's and the queue's are plain inverses.",
        12.5,
        MUTED,
        None,
        false,
    );
    y = r.bottom() + 8.0;
    // 3. backward maps fire together
    let bad = !break_text(a).is_empty();
    mark(c, y, !bad);
    let t3 = if bad {
        break_text(a)
    } else {
        "Every forward rendezvous has one backward rendezvous, and it can fire.".into()
    };
    let r = para(
        c,
        Pos2::new(x0 + 20.0, y),
        w - 20.0,
        &t3,
        12.5,
        INK,
        Some(("Backward maps fire together as the forward maps do. ", INK)),
        false,
    );
    y = r.bottom() + 4.0;
    for st in steps {
        let composes = st["composes"] == true;
        let no_back = st["no_backward"] == true && st["dead_forward"].is_null();
        let r = c.mono(
            Pos2::new(x0 + 20.0, y),
            Align2::LEFT_TOP,
            &format!(
                "{} {}  {}",
                if composes {
                    "✓"
                } else if no_back {
                    "—"
                } else {
                    "✗"
                },
                s(&st["flat"]),
                if composes {
                    "composes".to_string()
                } else if no_back {
                    "no backward map (the turnaround)".to_string()
                } else if st["split"] == true {
                    format!("backward maps split: {}", strs(&st["back_flat"]).join("; "))
                } else {
                    "backward maps do not fire together".to_string()
                }
            ),
            10.5,
            if composes {
                OK
            } else if no_back {
                MUTED
            } else {
                LEAK
            },
        );
        let back: Vec<String> = arr(&st["back"])
            .iter()
            .map(|b| s(&b["u"]).to_string())
            .collect();
        c.hover(
            r,
            with_cites(
                info(format!("step {}", s(&st["flat"])))
                    .fact("members", strs(&st["members"]).join(", "))
                    .fact("backward maps", back.join(", "))
                    .fact("backward as flat transitions", strs(&st["back_flat"]).join("; "))
                    .fact("composes", composes.to_string())
                    .note("One forward rendezvous of the commit run (judgment, frozen), and whether its backward maps fire as one."),
                &flat_cites(),
            ),
        );
        y = r.bottom() + 3.0;
    }
    y += 6.0;
    // the runs agree
    let leak: Vec<String> = arr(&a["runs"])
        .iter()
        .filter(|r| arr(&r["expect"]).iter().any(|e| e["pass"] != true))
        .map(run_line)
        .collect();
    if !leak.is_empty() {
        let r = para(
            c,
            Pos2::new(x0, y),
            w,
            &leak.join(" · "),
            12.5,
            INK,
            Some((
                "The runs agree (go-pflow's own run gives the same markings): ",
                INK,
            )),
            false,
        );
        y = r.bottom() + 10.0;
    }
    // can one link fix it?
    if !fixes.is_empty() {
        let r = para(c, Pos2::new(x0, y), w, "Each repair below is validated by go-pflow's rules, flattened, and the scenarios re-run. An event link only adds a rendezvous (all members fire as one); a lens needs the pool released on either outcome, which is a choice no link kind expresses.", 12.0, INK, Some(("Can one link fix it? ", INK)), false);
        y = r.bottom() + 6.0;
        let cols = [x0, x0 + 300.0, x0 + w - 60.0];
        for (i, h) in ["repair", "scenarios after it", "lens?"].iter().enumerate() {
            c.text(Pos2::new(cols[i], y), Align2::LEFT_TOP, h, 11.5, MUTED);
        }
        y += 16.0;
        c.line(
            Pos2::new(x0, y - 1.0),
            Pos2::new(x0 + w, y - 1.0),
            Color32::from_rgb(0xe3, 0xe6, 0xea),
            1.0,
        );
        for f in fixes {
            let (what, runs, dead, fh, legal) = fix_texts(f);
            let r1 = para(
                c,
                Pos2::new(cols[0], y + 3.0),
                290.0,
                &what,
                10.5,
                INK,
                None,
                true,
            );
            let mut s2 = runs.clone();
            if !dead.is_empty() {
                s2.push_str(&format!("   {dead}"));
            }
            let r2 = para(
                c,
                Pos2::new(cols[1], y + 3.0),
                cols[2] - cols[1] - 10.0,
                &s2,
                11.0,
                if !(legal && fh) { LEAK } else { INK },
                None,
                false,
            );
            c.text(
                Pos2::new(cols[2], y + 3.0),
                Align2::LEFT_TOP,
                if fh { "✓ a lens" } else { "✗" },
                12.0,
                if fh { OK } else { LEAK },
            );
            let row = r1.union(r2).expand2(Vec2::new(4.0, 2.0));
            c.hover(row, fix_info(f));
            y = row.bottom() + 4.0;
            c.line(
                Pos2::new(x0, y - 2.0),
                Pos2::new(x0 + w, y - 2.0),
                Color32::from_rgb(0xe3, 0xe6, 0xea),
                1.0,
            );
        }
        y += 6.0;
    }
    let r = para(c, Pos2::new(x0, y), w, "The Connect-4 plan is drawn as lenses (an analogy of shape). For this composition, the fallback draws what does have a backward pass and shows where the composite stops being a lens.", 12.0, INK, None, false);
    y = r.bottom() + 8.0;
    let b = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(190.0, 26.0));
    c.button(
        b,
        "Assume a backward pass →",
        false,
        "view.composition",
        json!({"lens": "assume"}),
    );
    let all = Rect::from_min_max(Pos2::new(x0, top), Pos2::new(x0 + w, y + 28.0));
    let _ = all;
}

fn fix_texts(f: &Value) -> (String, String, String, bool, bool) {
    let l = &f["link"];
    let removed: Vec<String> = arr(&f["removed"])
        .iter()
        .map(|r| format!("{} ⊗ {}", s(&r["from"]["port"]), s(&r["to"]["port"])))
        .collect();
    let what = format!(
        "{} {}:{} ⊗ {}:{}{}",
        if s(&f["how"]) == "add" {
            "add"
        } else {
            "move to"
        },
        s(&l["from"]["subnet"]),
        s(&l["from"]["port"]),
        s(&l["to"]["subnet"]),
        s(&l["to"]["port"]),
        if removed.is_empty() {
            String::new()
        } else {
            format!(" (drop {})", removed.join(", "))
        }
    );
    let legal = f["legal"] == true;
    let r = &f["result"];
    let runs = if !legal {
        "refused by go-pflow's rules".to_string()
    } else if r.is_null() {
        "does not flatten".to_string()
    } else {
        arr(&r["runs"])
            .iter()
            .map(|run| {
                let bad: Vec<&Value> = arr(&run["expect"])
                    .iter()
                    .filter(|e| e["pass"] != true)
                    .collect();
                if bad.is_empty() {
                    format!("{} ✓", s(&run["id"]))
                } else if let Some(st) = run["stopped_step"].as_str() {
                    format!("{} ✗ stops at {st}", s(&run["id"]))
                } else {
                    format!(
                        "{} ✗ {}",
                        s(&run["id"]),
                        bad.iter()
                            .map(|e| s(&e["says"]).to_string())
                            .collect::<Vec<_>>()
                            .join("; ")
                    )
                }
            })
            .collect::<Vec<_>>()
            .join(" · ")
    };
    let dead = arr(&r["steps"])
        .iter()
        .find(|st| !st["dead_forward"].is_null())
        .map(|st| {
            format!(
                "{} is dead: {} allows one of {}",
                s(&st["flat"]),
                s(&st["dead_forward"]["constraint"]),
                strs(&st["dead_forward"]["places"]).join(", ")
            )
        })
        .unwrap_or_default();
    (what, runs, dead, r["holds"] == true, legal)
}

fn fix_info(f: &Value) -> Info {
    let (what, runs, dead, fh, legal) = fix_texts(f);
    let mut i = info(format!("repair {}", fix_id(f)))
        .fact("link", f["link"].to_string())
        .fact("how", s(&f["how"]))
        .fact("legal (go-pflow's rules)", legal.to_string())
        .fact("scenarios after it", runs)
        .fact("a lens?", fh.to_string())
        .note(format!("{what}. Each repair is validated, flattened and the scenarios re-run; an event link only adds a rendezvous."));
    if !dead.is_empty() {
        i = i.fact("dead", dead);
    }
    with_cites(i, &[go_rule_cites(), flat_cites()].concat())
}

// ------------------------------------------------------------ assumed

const X0: f32 = 130.0;
const SW: f32 = 230.0;
const GAP: f32 = 96.0;
const FY: f32 = 330.0;
const BY: f32 = 490.0;
const MY: f32 = 410.0;

struct Sq {
    sub: Value,
    tid: String,
    x: f32,
    y: f32,
    col: Color32,
    role: String,
    facts: Vec<(String, String)>,
}

struct Ci {
    sub: Value,
    pid: String,
    x: f32,
    y: f32,
    col: Color32,
    role: String,
    facts: Vec<(String, String)>,
}

fn find<'a>(subs: &'a [Value], id: &str) -> Option<&'a Value> {
    subs.iter().find(|x| s(&x["id"]) == id)
}

fn flat_transition(c: &Ctx, member: &str) -> Option<String> {
    let (sid, tid) = member.split_once('/')?;
    c.d.flat
        .as_ref()?
        .get("transition_map")?
        .get(sid)?
        .get(tid)?
        .as_str()
        .map(str::to_string)
}

fn flat_members(c: &Ctx, flat: &str) -> Vec<String> {
    c.d.flat
        .as_ref()
        .map(|f| {
            arr(&f["transitions"])
                .iter()
                .find(|t| s(&t["id"]) == flat)
                .map(|t| strs(&t["members"]))
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

fn t_info(c: &Ctx, sq: &Sq) -> Info {
    let member = format!("{}/{}", s(&sq.sub["id"]), sq.tid);
    let t = arr(&sq.sub["transitions"])
        .iter()
        .find(|x| s(&x["id"]) == sq.tid);
    let ports: Vec<String> = arr(&sq.sub["ports"])
        .iter()
        .filter(|p| s(&p["transition"]) == sq.tid)
        .map(|p| s(&p["id"]).to_string())
        .collect();
    let f = flat_transition(c, &member);
    let mut i = info(format!("transition {member}"))
        .fact(
            "ports",
            if ports.is_empty() {
                "none: inside the boundary".into()
            } else {
                ports.join(", ")
            },
        )
        .fact("lens role", sq.role.clone())
        .fact("flat transition", f.clone().unwrap_or_else(|| "?".into()));
    if let Some(f) = &f {
        let others: Vec<String> = flat_members(c, f)
            .into_iter()
            .filter(|m| *m != member)
            .collect();
        if !others.is_empty() {
            i = i.fact("fires with", others.join(", "));
        }
    }
    for (k, v) in &sq.facts {
        i = i.fact(k, v.clone());
    }
    if let Some(t) = t {
        i = i.note(s(&t["description"]));
    }
    with_cites(i, &flat_cites())
}

fn draw_sq(c: &mut Ctx, xf: &Xf, sq: &Sq) {
    let at = xf.pt(sq.x, sq.y);
    let r = Rect::from_center_size(at, Vec2::splat(18.0 * xf.k));
    c.p.rect(
        r,
        2.0,
        Color32::WHITE,
        Stroke::new(1.8, sq.col),
        StrokeKind::Middle,
    );
    halo(
        c,
        xf,
        sq.x,
        sq.y + 22.0,
        Align2::CENTER_CENTER,
        &sq.tid,
        10.5,
        INK,
        PAPER,
        true,
    );
    c.hover(r.expand(3.0), t_info(c, sq));
    if let Some(p) = arr(&sq.sub["ports"])
        .iter()
        .find(|p| s(&p["target"]) == "transition" && s(&p["transition"]) == sq.tid)
        .cloned()
    {
        c.port(at, s(&sq.sub["id"]), &p, false, true);
    }
}

fn draw_ci(c: &mut Ctx, xf: &Xf, ci: &Ci) {
    let at = xf.pt(ci.x, ci.y);
    c.p.circle(at, 8.0 * xf.k, Color32::WHITE, Stroke::new(1.6, ci.col));
    let pl = arr(&ci.sub["places"])
        .iter()
        .find(|p| s(&p["id"]) == ci.pid)
        .cloned();
    if pl
        .as_ref()
        .is_some_and(|p| p["initial"].as_i64().unwrap_or(0) > 0)
    {
        c.p.circle_filled(at, 2.6 * xf.k, INK);
    }
    halo(
        c,
        xf,
        ci.x,
        ci.y + 21.0,
        Align2::CENTER_CENTER,
        &ci.pid,
        10.0,
        MUTED,
        PAPER,
        true,
    );
    let sid = s(&ci.sub["id"]);
    let mut i = info(format!("place {sid}/{}", ci.pid))
        .fact(
            "initial",
            pl.as_ref().map_or("0".into(), |p| p["initial"].to_string()),
        )
        .fact("lens role", ci.role.clone());
    for (k, v) in &ci.facts {
        i = i.fact(k, v.clone());
    }
    if let Some(p) = &pl {
        i = i.note(s(&p["description"]));
    }
    c.hover(Rect::from_center_size(at, Vec2::splat(20.0 * xf.k)), i);
    if let Some(p) = arr(&ci.sub["ports"])
        .iter()
        .find(|p| s(&p["target"]) == "place" && s(&p["place"]) == ci.pid)
        .cloned()
    {
        c.port(at, sid, &p, false, true);
    }
}

fn lane(c: &Ctx, xf: &Xf, pts: &[(f32, f32)], col: Color32, w: f32, dash: bool, head: bool) {
    let p: Vec<Pos2> = pts.iter().map(|q| xf.pt(q.0, q.1)).collect();
    wire(c, &p, col, w * xf.k.max(0.8), dash, head, xf.k);
}

fn assumed(c: &mut Ctx, area: Rect, a: &Value, fixes: &[Value]) {
    let subs = subnets(c);
    let saga = subs
        .iter()
        .find(|x| s(&x["pattern"]) == "saga" && x["extra"] != true)
        .or_else(|| subs.iter().find(|x| s(&x["pattern"]) == "saga"))
        .cloned();
    let Some(saga) = saga else {
        c.note("na", "No saga instance: nothing to chain.");
        return;
    };
    let sid = s(&saga["id"]).to_string();
    let steps: Vec<String> = arr(&saga["params"]["steps"])
        .iter()
        .map(|x| s(&x["id"]).to_string())
        .collect();
    let n = steps.len();
    let cap_x = X0 + n as f32 * (SW + GAP) - GAP / 2.0;
    let (w, h) = (cap_x + 330.0, BY + 100.0);
    let svg_area = Rect::from_min_max(
        area.left_top(),
        Pos2::new(area.right(), area.top() + (area.height() * 0.66).max(300.0)),
    );
    let xf = Xf::fit(svg_area, w, h, 1.25);
    let xf = &xf;
    let pair_of = |tid: &str| {
        arr(&a["pairs"])
            .iter()
            .find(|p| s(&p["subnet"]) == sid && s(&p["t"]) == tid)
            .cloned()
    };
    let mut sqs: Vec<Sq> = vec![];
    let mut cis: Vec<Ci> = vec![];
    let mut anchors: Vec<(String, (f32, f32))> = vec![];
    let anchor_of = |anchors: &[(String, (f32, f32))], m: &str| {
        anchors.iter().find(|(k, _)| k == m).map(|(_, v)| *v)
    };
    let outcome = schema_color("saga:outcome");

    // ---- boxes (first: they sit under everything)
    dtext(
        c,
        xf,
        X0 - 40.0,
        FY - 78.0,
        Align2::LEFT_CENTER,
        &format!("{sid} (Saga): each step as a lens, chained by the saga's own control places"),
        12.5,
        INK,
    );
    let tint = pattern_tint("saga");
    for (i, st) in steps.iter().enumerate() {
        let bx = X0 + i as f32 * (SW + GAP);
        let cx = bx + SW / 2.0 + 20.0;
        let r = dbox(
            c,
            xf,
            bx,
            FY - 50.0,
            SW,
            BY - FY + 100.0,
            10.0,
            tint,
            GREY,
            1.3,
            false,
        );
        dbold(
            c,
            xf,
            bx + 10.0,
            FY - 34.0,
            Align2::LEFT_CENTER,
            &format!("step {st}"),
            11.5,
            INK,
            false,
        );
        let i_ = with_cites(
            info(format!("saga step {st} as a lens"))
                .fact("forward", format!("do_{st} takes next_{st} on the forward lane"))
                .fact("backward", format!("undo_{st} takes unwind_{st} on the backward lane"))
                .fact("between them", format!("the residual do_{st} leaves for undo_{st}"))
                .fact("found by", "bundle.lens: a residual place only do_ produces and only undo_ consumes; undo_ returns what do_ moved on the accounts")
                .fact("assumption", "this pair is read as a lens; go-pflow builds it inside one subnet (Saga.ToMetaSubnet), so no link rule checks it"),
            &subnet_info(&saga).cites,
        );
        c.hover(r, i_);
        let _ = cx;
    }
    // other instances' boxes come after the placement below

    // ---- chain geometry
    for (i, st) in steps.iter().enumerate() {
        let bx = X0 + i as f32 * (SW + GAP);
        let cx = bx + SW / 2.0 + 20.0;
        sqs.push(Sq {
            sub: saga.clone(),
            tid: format!("do_{st}"),
            x: cx,
            y: FY,
            col: FWD,
            role: "forward map (get)".into(),
            facts: vec![],
        });
        sqs.push(Sq {
            sub: saga.clone(),
            tid: format!("undo_{st}"),
            x: cx,
            y: BY,
            col: BWD,
            role: "backward map (put)".into(),
            facts: vec![],
        });
        anchors.push((format!("{sid}/do_{st}"), (cx, FY)));
        anchors.push((format!("{sid}/undo_{st}"), (cx, BY)));
        let res = pair_of(&format!("do_{st}"))
            .map(|p| strs(&p["residuals"]))
            .unwrap_or_default();
        for (k, r) in res.iter().enumerate() {
            let rx = cx + (k as f32 - (res.len() as f32 - 1.0) / 2.0) * 92.0;
            lane(
                c,
                xf,
                &cubic_design(cx, FY + 10.0, rx, MY - 9.0, true),
                GREY,
                1.0,
                true,
                true,
            );
            lane(
                c,
                xf,
                &cubic_design(rx, MY + 9.0, cx, BY - 10.0, false),
                GREY,
                1.0,
                true,
                true,
            );
            cis.push(Ci {
                sub: saga.clone(),
                pid: r.clone(),
                x: rx,
                y: MY,
                col: GREY,
                role: "residual: what the forward map leaves for the backward map".into(),
                facts: vec![
                    ("produced only by".into(), format!("do_{st}")),
                    ("consumed only by".into(), format!("undo_{st}")),
                ],
            });
        }
        let fx = bx + 26.0;
        lane(c, xf, &[(fx, FY), (fx, BY)], FAIL, 1.4, true, false);
        sqs.push(Sq { sub: saga.clone(), tid: format!("fail_{st}"), x: fx, y: MY, col: FAIL, role: "turnaround: takes the forward input, gives the backward output (no lens map does this)".into(), facts: vec![("why it matters".into(), "a lens's forward map always gives a forward output; fail_ turns back instead, so the saga is a lens only on runs where no step fails".into())] });
        anchors.push((format!("{sid}/fail_{st}"), (fx, MY)));
        let next = if i + 1 < n {
            format!("next_{}", steps[i + 1])
        } else {
            "committing".into()
        };
        halo(
            c,
            xf,
            bx + SW + GAP / 2.0,
            FY - 8.0,
            Align2::CENTER_CENTER,
            &next,
            10.5,
            FWD,
            PAPER,
            true,
        );
        halo(
            c,
            xf,
            bx + SW + GAP / 2.0,
            BY - 8.0,
            Align2::CENTER_CENTER,
            &format!("unwind_{st}"),
            10.5,
            BWD,
            PAPER,
            true,
        );
    }
    // lanes
    lane(
        c,
        xf,
        &[(X0 - 70.0, FY), (cap_x + 40.0, FY)],
        FWD,
        2.2,
        false,
        true,
    );
    lane(
        c,
        xf,
        &[(cap_x + 40.0, BY), (X0 - 62.0, BY)],
        BWD,
        2.2,
        false,
        true,
    );
    if let Some(f0) = steps.first() {
        halo(
            c,
            xf,
            X0 - 60.0,
            FY - 8.0,
            Align2::LEFT_CENTER,
            &format!("next_{f0}"),
            10.5,
            FWD,
            PAPER,
            true,
        );
    }
    dbold(
        c,
        xf,
        X0 - 70.0,
        FY + 34.0,
        Align2::LEFT_CENTER,
        "forward →",
        10.5,
        FWD,
        false,
    );
    dbold(
        c,
        xf,
        X0 - 70.0,
        BY + 34.0,
        Align2::LEFT_CENTER,
        "← backward",
        10.5,
        BWD,
        false,
    );
    cis.push(Ci {
        sub: saga.clone(),
        pid: "rolled_back".into(),
        x: X0 - 78.0,
        y: BY,
        col: outcome,
        role: "the backward lane's output".into(),
        facts: vec![],
    });
    // the turnaround: commit leaves forward, fail_commit turns back
    let tx = cap_x + 80.0;
    lane(
        c,
        xf,
        &[(cap_x + 40.0, FY), (tx - 10.0, FY)],
        FWD,
        2.2,
        false,
        false,
    );
    let p1 = dcubic(
        xf,
        [
            (cap_x + 40.0, FY),
            (cap_x + 60.0, FY),
            (tx - 20.0, MY),
            (tx - 10.0, MY),
        ],
    );
    wire(c, &p1, FAIL, 1.6 * xf.k.max(0.8), true, false, xf.k);
    let p2 = dcubic(
        xf,
        [
            (tx, MY + 10.0),
            (tx, BY),
            (cap_x + 60.0, BY),
            (cap_x + 40.0, BY),
        ],
    );
    wire(c, &p2, BWD, 2.2 * xf.k.max(0.8), false, false, xf.k);
    sqs.push(Sq {
        sub: saga.clone(),
        tid: "commit".into(),
        x: tx,
        y: FY,
        col: FWD,
        role: "the forward pass ends: final acknowledgement".into(),
        facts: vec![],
    });
    sqs.push(Sq {
        sub: saga.clone(),
        tid: "fail_commit".into(),
        x: tx,
        y: MY,
        col: FAIL,
        role: "turnaround: the forward pass is sent back (the refund begins)".into(),
        facts: vec![],
    });
    anchors.push((format!("{sid}/commit"), (tx, FY)));
    anchors.push((format!("{sid}/fail_commit"), (tx, MY)));
    cis.push(Ci {
        sub: saga.clone(),
        pid: "completed".into(),
        x: tx + 70.0,
        y: FY,
        col: outcome,
        role: "the forward lane's output".into(),
        facts: vec![],
    });
    lane(
        c,
        xf,
        &[(tx + 10.0, FY), (tx + 60.0, FY)],
        FWD,
        2.2,
        false,
        true,
    );

    // ---- the other instances' pairs, above the step their forward map fires with
    let saga_anchor_of =
        |c: &Ctx, anchors: &[(String, (f32, f32))], member: &str| -> Option<(String, f32, f32)> {
            let f = flat_transition(c, member)?;
            let m = flat_members(c, &f)
                .into_iter()
                .find(|x| x.starts_with(&format!("{sid}/")) && anchor_of(anchors, x).is_some())?;
            let (x, y) = anchor_of(anchors, &m)?;
            Some((m, x, y))
        };
    struct Other {
        t: String,
        u: String,
        residuals: Vec<String>,
        trip: bool,
    }
    let mut others: Vec<Other> = vec![];
    for p in arr(&a["partner"]) {
        if !s(&p["t"]).starts_with(&format!("{sid}/")) {
            others.push(Other {
                t: s(&p["t"]).into(),
                u: s(&p["u"]).into(),
                residuals: strs(&p["residuals"]),
                trip: false,
            });
        }
    }
    for r in arr(&a["round_trips"]) {
        if !s(&r["t"]).starts_with(&format!("{sid}/")) {
            others.push(Other {
                t: s(&r["t"]).into(),
                u: s(&r["u"]).into(),
                residuals: strs(&r["residuals"]),
                trip: true,
            });
        }
    }
    type Group = (String, Option<(f32, f32)>, Vec<usize>);
    let mut groups: Vec<Group> = vec![];
    for (i, o) in others.iter().enumerate() {
        let an = saga_anchor_of(c, &anchors, &o.t);
        let key = an.as_ref().map_or("_none".to_string(), |x| x.0.clone());
        match groups.iter_mut().find(|g| g.0 == key) {
            Some(g) => g.2.push(i),
            None => groups.push((key, an.map(|x| (x.1, x.2)), vec![i])),
        }
    }
    let (bw, bt, bh) = (150.0f32, 70.0f32, 150.0f32);
    let mut free_x = X0;
    let mut boxes: Vec<(f32, Value, String)> = vec![];
    for (_, an, items) in &groups {
        let total = items.len() as f32 * bw + (items.len() as f32 - 1.0) * 16.0;
        let mut x = an.map_or(free_x, |a| a.0 - total / 2.0);
        if an.is_none() {
            free_x += total + 20.0;
        }
        for &i in items {
            let o = &others[i];
            let (os, ot) = o.t.split_once('/').unwrap_or(("", &o.t));
            let ou = o.u.split_once('/').map_or(o.u.as_str(), |x| x.1);
            let Some(sub) = find(&subs, os).cloned() else {
                continue;
            };
            let cx = x + bw / 2.0;
            let (ty, uy) = (bt + 42.0, bt + bh - 30.0);
            let ry = (ty + uy) / 2.0;
            boxes.push((x, sub.clone(), format!("{os} ({})", s(&sub["pattern"]))));
            sqs.push(Sq {
                sub: sub.clone(),
                tid: ot.into(),
                x: cx - 30.0,
                y: ty,
                col: FWD,
                role: "forward map".into(),
                facts: vec![("backward map".into(), ou.into())],
            });
            sqs.push(Sq {
                sub: sub.clone(),
                tid: ou.into(),
                x: cx - 30.0,
                y: uy,
                col: BWD,
                role: if o.trip {
                    "backward map, but it fires on the forward pass".into()
                } else {
                    "backward map".into()
                },
                facts: vec![("forward map".into(), ot.into())],
            });
            anchors.push((o.t.clone(), (cx - 30.0, ty)));
            anchors.push((o.u.clone(), (cx - 30.0, uy)));
            for r in &o.residuals {
                cis.push(Ci {
                    sub: sub.clone(),
                    pid: r.clone(),
                    x: cx + 2.0,
                    y: ry,
                    col: GREY,
                    role: "residual".into(),
                    facts: vec![
                        ("produced only by".into(), ot.into()),
                        ("consumed only by".into(), ou.into()),
                    ],
                });
            }
            lane(
                c,
                xf,
                &[(cx - 30.0, ty + 10.0), (cx - 30.0, uy - 10.0)],
                GREY,
                1.0,
                true,
                true,
            );
            x += bw + 16.0;
        }
    }
    for (x, sub, title) in &boxes {
        let r = dbox(
            c,
            xf,
            *x,
            bt,
            bw,
            bh,
            9.0,
            pattern_tint(s(&sub["pattern"])),
            GREY,
            1.2,
            false,
        );
        dbold(
            c,
            xf,
            x + 8.0,
            bt + 15.0,
            Align2::LEFT_CENTER,
            title,
            11.0,
            INK,
            false,
        );
        c.hover(r, subnet_info(sub));
    }

    // ---- fusion lines: every flat transition that joins a drawn non-saga transition to the saga
    let pulse = c
        .anim(|e| match e {
            Event::LinkAdded { .. } => Some(()),
            _ => None,
        })
        .map_or(1.0, |(_, t)| 0.4 + 0.6 * t);
    let drawn_other: Vec<(String, (f32, f32))> = anchors
        .iter()
        .filter(|(m, _)| !m.starts_with(&format!("{sid}/")))
        .cloned()
        .collect();
    let all_links = links(c);
    for (m, from) in &drawn_other {
        let f = flat_transition(c, m);
        let members = f.as_ref().map(|f| flat_members(c, f)).unwrap_or_default();
        let saga_members: Vec<String> = members
            .iter()
            .filter(|x| x.starts_with(&format!("{sid}/")) && anchor_of(&anchors, x).is_some())
            .cloned()
            .collect();
        if saga_members.is_empty() {
            let a0 = xf.pt(from.0 + 10.0, from.1);
            dpoly(
                c,
                &[a0, xf.pt(from.0 + 32.0, from.1)],
                hex("#9aa3ad"),
                1.0,
                (2.0, 3.0),
            );
            let r = halo(
                c,
                xf,
                from.0,
                from.1 + 36.0,
                Align2::CENTER_CENTER,
                "fires alone: no link",
                9.5,
                MUTED,
                PAPER,
                false,
            );
            c.hover(
                r,
                with_cites(
                    info(format!("{m} is linked to nothing"))
                        .fact("flat transition", f.clone().unwrap_or_default())
                        .note("No event link fuses it, so nothing in the saga fires it."),
                    &flat_cites(),
                ),
            );
            continue;
        }
        for sm in &saga_members {
            let to = anchor_of(&anchors, sm).unwrap();
            let pts = dcubic(
                xf,
                [
                    (from.0 + 9.0, from.1 + 4.0),
                    (from.0 + 60.0, from.1 + 60.0),
                    (to.0 - 40.0, to.1 - 70.0),
                    (to.0, to.1 - 10.0),
                ],
            );
            let col = EV.gamma_multiply(pulse);
            poly(c, &pts, col, 1.8 * xf.k.max(0.8));
            let mid = pts[pts.len() / 2];
            c.p.text(
                mid + Vec2::new(6.0, 0.0),
                Align2::LEFT_CENTER,
                "⊗",
                FontId::proportional(fsz(xf, 12.0)),
                col,
            );
            let ls: Vec<String> = all_links
                .iter()
                .filter(|l| {
                    s(&l["kind"]) == "event" && {
                        let end = |r: &Value| {
                            subs.iter()
                                .find(|x| s(&x["id"]) == s(&r["subnet"]))
                                .and_then(|su| {
                                    arr(&su["ports"])
                                        .iter()
                                        .find(|p| s(&p["id"]) == s(&r["port"]))
                                        .map(|p| {
                                            format!("{}/{}", s(&r["subnet"]), s(&p["transition"]))
                                        })
                                })
                        };
                        end(&l["from"]).is_some_and(|e| members.contains(&e))
                            && end(&l["to"]).is_some_and(|e| members.contains(&e))
                    }
                })
                .map(|l| {
                    format!(
                        "{}:{} ⊗ {}:{}",
                        s(&l["from"]["subnet"]),
                        s(&l["from"]["port"]),
                        s(&l["to"]["subnet"]),
                        s(&l["to"]["port"])
                    )
                })
                .collect();
            let mut i = info(format!("fused: {m} ⊗ {sm}"))
                .fact("flat transition", f.clone().unwrap_or_default())
                .fact("members", members.join(", "))
                .fact(
                    "event links in this class",
                    if ls.is_empty() {
                        "none directly; joined through the class (union-find)".into()
                    } else {
                        ls.join(", ")
                    },
                );
            i = i.note("Every flat transition that joins two members is one rendezvous: all of them fire as one.");
            hover_path(c, &pts, 6.0, with_cites(i, &flat_cites()));
        }
    }

    // ---- squares and circles on top
    for sq in &sqs {
        draw_sq(c, xf, sq);
    }
    for ci in &cis {
        draw_ci(c, xf, ci);
    }

    // ---- where the composite stops being a lens
    let tmpl = |c: &mut Ctx, at: (f32, f32), lines: Vec<String>, i: Info| {
        let ctr = xf.pt(at.0, at.1);
        c.p.add(Shape::dashed_line(
            &(0..=24)
                .map(|k| {
                    let t = k as f32 / 24.0 * std::f32::consts::TAU;
                    ctr + Vec2::new(t.cos(), t.sin()) * 17.0 * xf.k
                })
                .collect::<Vec<_>>(),
            Stroke::new(1.8, LEAK),
            4.0 * xf.k,
            3.0 * xf.k,
        ));
        let mut r = Rect::from_center_size(ctr, Vec2::splat(34.0 * xf.k));
        for (k, ln) in lines.iter().enumerate() {
            r = r.union(halo(
                c,
                xf,
                at.0 + 22.0,
                at.1 + 30.0 + k as f32 * 13.0,
                Align2::LEFT_CENTER,
                ln,
                10.0,
                LEAK,
                PAPER,
                false,
            ));
        }
        c.hover(r, i);
    };
    let step_rows: Vec<Value> = arr(&a["steps"]).to_vec();
    for st in &step_rows {
        let flat = s(&st["flat"]);
        let fwd_saga = flat_members(c, flat)
            .into_iter()
            .find(|x| x.starts_with(&format!("{sid}/")) && anchor_of(&anchors, x).is_some());
        if let (false, Some(fs_)) = (st["dead_forward"].is_null(), &fwd_saga) {
            let at = anchor_of(&anchors, fs_).unwrap();
            let dfw = &st["dead_forward"];
            let short = if flat.len() > 40 {
                "this rendezvous".to_string()
            } else {
                flat.to_string()
            };
            let places: Vec<String> = strs(&dfw["places"])
                .iter()
                .map(|p| p.split('/').nth(1).unwrap_or(p).to_string())
                .collect();
            tmpl(
                c,
                at,
                vec![format!("✗ {short} can never fire:"), format!("needs {};", places.join(" + ")), format!("{} allows one", s(&dfw["constraint"]).split('/').nth(1).unwrap_or(""))],
                with_cites(
                    info("dead rendezvous")
                        .fact("decided by", "bundle.lens: dead_by on the flat net (go-pflow's run stops here too)")
                        .note(format!("{flat} needs a token on each of {}; the declared invariant {} keeps exactly one token among them.", strs(&dfw["places"]).join(", "), s(&dfw["constraint"]))),
                    &flat_cites(),
                ),
            );
        }
        let mut back: Vec<(String, String, String)> = arr(&st["back"])
            .iter()
            .map(|b| {
                (
                    s(&b["u"]).to_string(),
                    s(&b["u_flat"]).to_string(),
                    "fires on its own".to_string(),
                )
            })
            .collect();
        for r in arr(&st["trips"]).iter().filter(|r| s(&r["t_flat"]) == flat) {
            back.push((
                s(&r["u"]).into(),
                s(&r["u_flat"]).into(),
                format!("fires on the forward pass ({})", s(&r["u_flat"])),
            ));
        }
        let Some(target) = back
            .iter()
            .find(|b| b.0.starts_with(&format!("{sid}/")))
            .cloned()
        else {
            continue;
        };
        if back.len() < 2 {
            continue;
        }
        let missing: Vec<&(String, String, String)> =
            back.iter().filter(|b| b.1 != target.1).collect();
        if missing.is_empty() {
            continue;
        }
        let Some(at) = anchor_of(&anchors, &target.0) else {
            continue;
        };
        let tl = target.0.split('/').nth(1).unwrap_or("");
        let mut lines = vec![
            format!(
                "✗ lens composition needs {} here,",
                missing
                    .iter()
                    .map(|b| b.0.split('/').nth(1).unwrap_or("").to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            format!("firing with {tl}, as their forward maps fire together;"),
        ];
        for b in &missing {
            lines.push(format!("{}: {}", b.0, b.2));
        }
        let uniq: Vec<String> = {
            let mut v: Vec<String> = vec![];
            for b in &back {
                if !v.contains(&b.1) {
                    v.push(b.1.clone());
                }
            }
            v
        };
        tmpl(
            c,
            at,
            lines,
            with_cites(
                info("where the composite stops being a lens")
                    .fact("decided by", "bundle.lens on the flat net (Flatten's classes)")
                    .fact("fix with a link?", "an event link only adds a rendezvous: fusing release with undo_reserve as well as commit makes one transition that needs both, and saga_control allows one (try it: drag undo:reserve to release)")
                    .note(format!("Going forward, {flat} fires {} as one. A lens composite would fire their backward maps as one on the way back; here they are {}.", strs(&st["members"]).join(", "), uniq.join("; "))),
                &flat_cites(),
            ),
        );
    }

    // ---- the tables under the drawing
    let ty0 = svg_area.bottom() + 6.0;
    let colw = (area.width() - 24.0) * 0.56;
    let lx = area.left() + 6.0;
    let holds = a["holds"] == true;
    c.text(
        Pos2::new(lx, ty0),
        Align2::LEFT_TOP,
        if holds {
            "The assumed lenses compose."
        } else {
            "The assumed lenses do not compose into one lens."
        },
        13.0,
        INK,
    );
    let mut y = ty0 + 20.0;
    let cols = [lx, lx + colw * 0.40, lx + colw * 0.78];
    for (i, h) in [
        "forward rendezvous (commit run)",
        "its backward maps, and where they fire",
        "lens composition",
    ]
    .iter()
    .enumerate()
    {
        c.text(Pos2::new(cols[i], y), Align2::LEFT_TOP, h, 10.5, MUTED);
    }
    y += 15.0;
    for st in &step_rows {
        let back: Vec<String> = arr(&st["back"])
            .iter()
            .map(|b| {
                format!(
                    "{} ({})",
                    s(&b["u"]),
                    if s(&b["u_flat"]) == s(&b["u"]) {
                        "alone".to_string()
                    } else {
                        s(&b["u_flat"]).to_string()
                    }
                )
            })
            .chain(
                arr(&st["trips"])
                    .iter()
                    .filter(|r| s(&r["t_flat"]) == s(&st["flat"]))
                    .map(|r| format!("{} (forward: {})", s(&r["u"]), s(&r["u_flat"]))),
            )
            .collect();
        let trip = arr(&st["trips"])
            .iter()
            .any(|r| s(&r["t_flat"]) == s(&st["flat"]));
        let (verdict, vc) = if !st["dead_forward"].is_null() {
            (
                format!("✗ dead ({})", s(&st["dead_forward"]["constraint"])),
                LEAK,
            )
        } else if st["composes"] == true && !trip {
            ("✓ composes".to_string(), OK)
        } else if st["no_backward"] == true && !trip {
            ("— no backward map (the turnaround)".to_string(), MUTED)
        } else {
            ("✗ backward maps do not fire together".to_string(), LEAK)
        };
        let r1 = para(
            c,
            Pos2::new(cols[0], y),
            cols[1] - cols[0] - 8.0,
            s(&st["flat"]),
            10.0,
            INK,
            None,
            true,
        );
        let r2 = para(
            c,
            Pos2::new(cols[1], y),
            cols[2] - cols[1] - 8.0,
            &if back.is_empty() {
                "—".into()
            } else {
                back.join(", ")
            },
            10.0,
            INK,
            None,
            true,
        );
        let r3 = para(
            c,
            Pos2::new(cols[2], y),
            colw - (cols[2] - lx),
            &verdict,
            10.5,
            vc,
            None,
            false,
        );
        let row = r1.union(r2).union(r3);
        c.hover(
            row,
            with_cites(
                info(format!("step {}", s(&st["flat"])))
                    .fact("members", strs(&st["members"]).join(", "))
                    .fact("verdict", verdict),
                &flat_cites(),
            ),
        );
        y = row.bottom() + 3.0;
    }
    y += 4.0;
    for r in arr(&a["runs"]) {
        let ok = arr(&r["expect"]).iter().all(|e| e["pass"] == true);
        let marking: Vec<String> = arr(&r["expect"])
            .iter()
            .map(|e| format!("{}={}", s(&e["place"]), e["actual"]))
            .collect();
        let t = format!(
            "{}   {}   {}",
            s(&r["title"]),
            marking.join(", "),
            if ok {
                "✓".to_string()
            } else {
                format!(
                    "✗ {}{}",
                    arr(&r["expect"])
                        .iter()
                        .filter(|e| e["pass"] != true)
                        .map(|e| s(&e["says"]).to_string())
                        .collect::<Vec<_>>()
                        .join("; "),
                    r["stopped_step"]
                        .as_str()
                        .map(|x| format!(" (stopped at {x})"))
                        .unwrap_or_default()
                )
            }
        );
        let rr = para(
            c,
            Pos2::new(lx, y),
            colw,
            &t,
            10.0,
            if ok { OK } else { LEAK },
            None,
            false,
        );
        c.hover(rr, info(format!("scenario {}", s(&r["id"]))).fact("end marking", marking.join(", ")).note("A scenario script fired on the flat net with pflow-metamodel's firing rule (bundle.run)."));
        y = rr.bottom() + 2.0;
    }
    // repairs, right-hand column
    let rx = lx + colw + 16.0;
    let rw = area.right() - rx - 6.0;
    if !fixes.is_empty() && rw > 200.0 {
        let mut y = ty0;
        c.text(
            Pos2::new(rx, y),
            Align2::LEFT_TOP,
            "Can one link fix it?",
            13.0,
            INK,
        );
        y += 20.0;
        for f in fixes {
            let (what, runs, dead, fh, legal) = fix_texts(f);
            let r1 = para(c, Pos2::new(rx, y), rw - 60.0, &what, 10.0, INK, None, true);
            let mut t = runs;
            if !dead.is_empty() {
                t.push_str(&format!("  {dead}"));
            }
            let r2 = para(
                c,
                Pos2::new(rx, r1.bottom() + 1.0),
                rw - 60.0,
                &t,
                10.0,
                if legal { MUTED } else { LEAK },
                None,
                false,
            );
            c.text(
                Pos2::new(rx + rw - 54.0, y),
                Align2::LEFT_TOP,
                if fh { "✓ a lens" } else { "✗" },
                11.0,
                if fh { OK } else { LEAK },
            );
            let row = r1.union(r2).expand2(Vec2::new(4.0, 1.0));
            c.hover(row, fix_info(f));
            y = row.bottom() + 5.0;
        }
    }
}

/// A forward-lane to residual curve (vertical tangents), as design points.
fn cubic_design(x0: f32, y0: f32, x1: f32, y1: f32, down: bool) -> Vec<(f32, f32)> {
    let steps = 16;
    let (c1, c2) = if down {
        ((x0, (y0 + y1) / 2.0 - 10.0), (x1, (y0 + y1) / 2.0 - 20.0))
    } else {
        ((x0, (y0 + y1) / 2.0 + 20.0), (x1, (y0 + y1) / 2.0 + 10.0))
    };
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let u = 1.0 - t;
            (
                u * u * u * x0 + 3.0 * u * u * t * c1.0 + 3.0 * u * t * t * c2.0 + t * t * t * x1,
                u * u * u * y0 + 3.0 * u * u * t * c1.1 + 3.0 * u * t * t * c2.1 + t * t * t * y1,
            )
        })
        .collect()
}
