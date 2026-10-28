//! The Laws view's live card, after `laws.js`'s `liveCard`: the open state
//! drawn as the term the theory is about, next to the rewrites that cover it.
//!
//! - Connect-4, `orderer-monoid`: the current plan's order chain as the Chyp
//!   term Chyp brackets (`doc_left`'s shape, folded from the left), laid out
//!   by `c_hypergraph` (a port of `js/layout/hypergraph.js`) because Chyp has
//!   no coordinates for a term built from the live plan. Drop an order piece
//!   on the drawing to extend the chain.
//! - orders, `sync-spiders`: Flatten's event classes as spiders, next to the
//!   rewrite that covers the live link order.
//!
//! The other theories' cards (`filter-meet`, `driver-wrap`) are not ported:
//! see `docs/cv-notes/c.md`.

#![allow(clippy::too_many_arguments)]

use super::c_chyp::*;
use super::c_draw::*;
use super::c_hypergraph::*;
use super::laws_view::*;
use super::*;

const S: f32 = 30.0;

fn chain_term(pieces: &[String]) -> String {
    let n = pieces.len();
    match n {
        0 => "empty".into(),
        1 => pieces[0].clone(),
        _ => {
            let layers: Vec<String> = (1..n)
                .map(|k| {
                    let mut v = vec!["chain".to_string()];
                    v.extend(std::iter::repeat_n("id".to_string(), n - 1 - k));
                    v.join(" * ")
                })
                .collect();
            format!("{} ; {}", pieces.join(" * "), layers.join(" ; "))
        }
    }
}

/// The hypergraph of a chain: one box per piece, folded left by `chain`.
fn chain_graph(
    pieces: &[String],
    tw: &dyn Fn(&str, f32) -> f32,
) -> (HGraph, BTreeMap<String, String>) {
    let mut boxes = vec![];
    let mut labels = BTreeMap::new();
    let width = |l: &str| (1.1 * S as f64).max(tw(l, BOX_FONT) as f64 + 10.0);
    if pieces.is_empty() {
        boxes.push(HBox {
            id: "empty".into(),
            inputs: vec![],
            outputs: vec!["w0".into()],
            width: Some(width("empty")),
            height: None,
        });
        labels.insert("empty".to_string(), "empty".to_string());
        return (
            HGraph {
                boxes,
                inputs: vec![],
                outputs: vec!["w0".into()],
            },
            labels,
        );
    }
    for (i, p) in pieces.iter().enumerate() {
        boxes.push(HBox {
            id: format!("p{i}"),
            inputs: vec![],
            outputs: vec![format!("w{i}")],
            width: Some(width(p)),
            height: None,
        });
        labels.insert(format!("p{i}"), p.clone());
    }
    let mut acc = "w0".to_string();
    for i in 1..pieces.len() {
        boxes.push(HBox {
            id: format!("chain{i}"),
            inputs: vec![acc.clone(), format!("w{i}")],
            outputs: vec![format!("c{i}")],
            width: Some(width("chain")),
            height: None,
        });
        labels.insert(format!("chain{i}"), "chain".to_string());
        acc = format!("c{i}");
    }
    (
        HGraph {
            boxes,
            inputs: vec![],
            outputs: vec![acc],
        },
        labels,
    )
}

fn opts() -> Opts {
    Opts {
        box_min_height: 0.8 * S as f64,
        port_pitch: 0.8 * S as f64,
        row_gap: 0.2 * S as f64,
        wire_gap: S as f64,
        gap: 1.6 * S as f64,
        margin: 4.0,
        port_offsets: Some(Box::new(|k, n, _| port_off(k, n) as f64 * S as f64)),
        ..Opts::default()
    }
}

/// Draws the live card at `(x, y)` over `w` pixels; returns the height it took
/// (0: no card for this theory).
pub(super) fn live_card(
    c: &mut Ctx,
    x: f32,
    y: f32,
    w: f32,
    orders: bool,
    selected: &str,
    th: &'static Value,
) -> f32 {
    if !orders && selected == "orderer-monoid" {
        return chain_card(c, x, y, w, th);
    }
    if orders && selected == "sync-spiders" {
        return spider_card(c, x, y, w, th);
    }
    0.0
}

fn card_frame(c: &Ctx, x: f32, y: f32, w: f32, h: f32, title: &str) {
    let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
    c.p.rect(
        r,
        6.0,
        Color32::WHITE,
        Stroke::new(1.0, hex("#d8dde3")),
        StrokeKind::Middle,
    );
    c.p.rect_filled(
        Rect::from_min_size(r.left_top(), Vec2::new(4.0, h)),
        2.0,
        hex("#2f6fb3"),
    );
    c.p.text(
        r.left_top() + Vec2::new(12.0, 6.0),
        Align2::LEFT_TOP,
        "Live ·",
        FontId::proportional(12.0),
        INK,
    );
    c.p.text(
        r.left_top() + Vec2::new(52.0, 6.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(12.0),
        INK,
    );
}

fn chain_card(c: &mut Ctx, x: f32, y: f32, w: f32, th: &'static Value) -> f32 {
    let (plan, order, _, _, _) = plan_lists(c);
    let title = "the current plan's order chain, as the term Chyp brackets";
    if let Some(e) =
        c.d.plan
            .as_ref()
            .and_then(|p| p["check"]["error"].as_str().map(String::from))
    {
        card_frame(c, x, y, w, 40.0, title);
        c.text(
            Pos2::new(x + 12.0, y + 24.0),
            Align2::LEFT_TOP,
            &format!("pflow-rs refuses this plan: {e}"),
            11.5,
            LEAK,
        );
        return 48.0;
    }
    let tw = |t: &str, sz: f32| {
        c.p.layout_no_wrap(t.to_string(), FontId::proportional(sz), Color32::WHITE)
            .size()
            .x
    };
    let (g, labels) = chain_graph(&order, &tw);
    let lay = layout_hypergraph(&g, &opts());
    let dgm = from_layout(
        &lay,
        &g,
        &|id| labels.get(id).cloned().unwrap_or_default(),
        &th["diagrams"]["styles"],
        Some("Order"),
    );
    let term = chain_term(&order);
    let is_default = plan.get("order").is_none();
    // text, to the right of the drawing
    let tx = x + 14.0 + dgm.w + 16.0;
    let tw_ = (x + w - tx - 8.0).max(200.0);
    let doc_left = arr(&th["theory"]["lets"])
        .iter()
        .find(|l| s(&l["name"]) == "doc_left")
        .cloned();
    let def = arr(&th["theory"]["rules"])
        .iter()
        .find(|r| s(&r["name"]) == "default_order_def")
        .cloned();
    let order_json = plan
        .get("order")
        .map_or("(absent)".to_string(), |o| o.to_string());
    let mut paras: Vec<(String, Color32)> = vec![
        (
            format!(
                "\"order\": {order_json} → {}",
                if is_default {
                    "Chain::default(): tt_best, immediate_win, history"
                } else {
                    "Chain::new keeps the list as given"
                }
            ),
            INK,
        ),
        (
            format!("Folded from the left, as a JSON parser would, it is the Chyp term {term}"),
            INK,
        ),
    ];
    let mut links: Vec<&str> = vec![];
    if is_default && def.as_ref().is_some_and(|d| s(&d["rhs"]) == term) {
        paras.push((
            format!(
                "= default_order (def, line {}): appending [] to it changes nothing:",
                def.as_ref().map_or(0, |d| d["line"].as_u64().unwrap_or(0))
            ),
            VALID,
        ));
        links = vec!["default_append_empty"];
    } else if doc_left.as_ref().is_some_and(|d| s(&d["term"]) == term) {
        paras.push((format!("= doc_left (line {}), the plan.rs doc chain: Chyp proved its bracketings agree and that [] on either end changes nothing:", doc_left.as_ref().map_or(0, |d| d["line"].as_u64().unwrap_or(0))), VALID));
        links = vec![
            "doc_chain_bracketing",
            "doc_chain_balanced",
            "doc_chain_append_empty",
            "doc_chain_prepend_empty",
        ];
    } else if order.is_empty() {
        paras.push(("\"order\": [] is empty, the unit of chain (rules unitL, unitR). It is not the absent key, which is Chain::default().".into(), INK));
    } else {
        paras.push((format!("Not the doc chain. assoc, unitL and unitR are stated for every chain, but Chyp checked bracketings of the plan.rs doc chain only, not of this {}-piece chain.", order.len()), Color32::from_rgb(0x8a, 0x5a, 0x0a)));
    }
    let h_paras: f32 = paras
        .iter()
        .map(|p| (p.0.chars().count() as f32 * 6.2 / tw_).ceil().max(1.0) * 14.0 + 3.0)
        .sum();
    let h_links = if links.is_empty() { 0.0 } else { 24.0 };
    let h = (dgm.h + 10.0).max(h_paras + h_links + 40.0) + 28.0;
    card_frame(c, x, y, w, h, title);
    // the drawing, a drop target for an order piece
    let (ox, oy) = (x + 14.0, y + 28.0);
    let rect = Rect::from_min_size(Pos2::new(ox, oy), Vec2::new(dgm.w, dgm.h));
    slot_target(c, rect, "order");
    let slide = c.anim(|e| match e {
        Event::PieceAdded { slot, name } if slot == "order" => Some(name.clone()),
        _ => None,
    });
    let slide = slide.and_then(|(name, t)| {
        order
            .iter()
            .rposition(|p| *p == name)
            .map(|i| (format!("p{i}"), t))
    });
    let marks = Marks {
        hl: None,
        ins: None,
        slide,
    };
    let gi = with_cites_info(
        info(format!("the live chain: {term}"))
            .fact("term", term.clone())
            .fact("plan.order", order_json.clone())
            .fact("layout", "c_hypergraph (a port of js/layout/hypergraph.js, held to Chyp's cvxpy optimum on the receipts): Chyp has no coordinates for a term built from the live plan")
            .fact("drop", "an order piece: it joins the end of the chain (plan.rs's check decides)")
            .note("Built from the plan's order (chain(), plan.rs), folded from the left like doc_left (orderer-monoid.chyp). Each chain box is Chyp's `chain` generator: Order × Order → Order."),
        &theory_cites("orderer-monoid"),
    );
    draw_diagram(
        c,
        th,
        &dgm,
        ox,
        oy,
        &marks,
        gi,
        "the live plan's chain",
        None,
    );
    // the text
    let mut ty = y + 28.0;
    for (t, col) in &paras {
        let r = para(
            c,
            Pos2::new(tx, ty),
            tw_,
            t,
            11.5,
            *col,
            None,
            t.starts_with("Folded"),
        );
        ty = r.bottom() + 3.0;
    }
    if !links.is_empty() {
        let mut lx = tx;
        for l in &links {
            let rc = chip(
                c,
                Pos2::new(lx, ty),
                l,
                true,
                false,
                hex("#c9d3df"),
                false,
                json!({"rewrite": l}),
                info(format!("rewrite {l}"))
                    .note("Chyp proved it (precomputed); click to show it."),
            );
            lx = rc.right() + 4.0;
        }
        ty += 24.0;
    }
    let _ = ty;
    let nc = ["NOT_chains_commute", "value_ignores_order"];
    let r = para(c, Pos2::new(tx, y + h - 34.0), tw_, "The chain does not commute (NOT_chains_commute fails, as it must); its value does (value_ignores_order, from the assumed rule order_only). Drop an order piece on the drawing to extend the chain.", 11.0, MUTED, None, false);
    let _ = (nc, r);
    h + 8.0
}

fn member_of(c: &Ctx, end: &Value) -> Option<String> {
    let sub = subnets(c)
        .into_iter()
        .find(|x| s(&x["id"]) == s(&end["subnet"]))?;
    let p = arr(&sub["ports"])
        .iter()
        .find(|p| s(&p["id"]) == s(&end["port"]))?
        .clone();
    (!s(&p["transition"]).is_empty())
        .then(|| format!("{}/{}", s(&end["subnet"]), s(&p["transition"])))
}

fn spider_card(c: &mut Ctx, x: f32, y: f32, w: f32, th: &'static Value) -> f32 {
    let title =
        "event links as spiders (the laws below are why the order of links does not matter)";
    let groups: Vec<(String, Vec<String>)> =
        c.d.flat
            .as_ref()
            .and_then(|f| f["fused_groups"].as_object())
            .map(|o| o.iter().map(|(k, v)| (k.clone(), strs(v))).collect())
            .unwrap_or_default();
    // L1..L3: the names sync-spiders.chyp:19-21 gives the default links
    let ends = |l: &Value| {
        let mut v = [
            format!("{}:{}", s(&l["from"]["subnet"]), s(&l["from"]["port"])),
            format!("{}:{}", s(&l["to"]["subnet"]), s(&l["to"]["port"])),
        ];
        v.sort();
        v.join("~")
    };
    let names = [
        ("orders:dequeue~workers:acquire", "L1"),
        ("orders:dequeue~payment:do:reserve", "L2"),
        ("payment:commit~workers:release", "L3"),
    ];
    let ev: Vec<(Value, Option<&str>)> = links(c)
        .into_iter()
        .filter(|l| s(&l["kind"]) == "event")
        .map(|l| {
            let e = ends(&l);
            let n = names.iter().find(|x| x.0 == e).map(|x| x.1);
            (l, n)
        })
        .collect();
    let row_h = 24.0;
    let blocks: Vec<f32> = groups
        .iter()
        .map(|g| g.1.len().max(1) as f32 * row_h + 8.0)
        .collect();
    let body_h = (blocks.iter().sum::<f32>() + 10.0 * groups.len() as f32).max(56.0);
    let h = body_h + 36.0;
    card_frame(c, x, y, w, h, title);
    let green = kind_color("event");
    let (bw, sx_off) = (136.0f32, 184.0f32);
    let pulse = c.anim(|e| match e {
        Event::LinkAdded { kind, .. } if kind == "event" => Some(()),
        _ => None,
    });
    let mut yy = y + 28.0;
    let styles = &th["diagrams"]["styles"];
    let fill = styles["orders_dequeue"]["bg"].as_str().unwrap_or("#fde2c4");
    for (gi, (name, members)) in groups.iter().enumerate() {
        let x0 = x + 14.0;
        let sx = x0 + sx_off;
        let sy = yy + blocks[gi] / 2.0;
        let links_in: Vec<&(Value, Option<&str>)> = ev
            .iter()
            .filter(|(l, _)| {
                member_of(c, &l["from"]).is_some_and(|m| members.contains(&m))
                    && member_of(c, &l["to"]).is_some_and(|m| members.contains(&m))
            })
            .collect();
        for (i, m) in members.iter().enumerate() {
            let my = sy + (i as f32 - (members.len() as f32 - 1.0) / 2.0) * row_h;
            let a = Pos2::new(x0 + bw, my);
            let b = Pos2::new(sx, sy);
            let t = pulse.as_ref().map_or(1.0, |(_, t)| *t);
            let pts = cubic(
                a,
                a + Vec2::new((b.x - a.x) * 0.5, 0.0),
                b - Vec2::new((b.x - a.x) * 0.5, 0.0),
                b,
            );
            poly(c, &pts, green.gamma_multiply(0.4 + 0.6 * t), 2.0);
            let via: Vec<String> = links_in
                .iter()
                .filter(|(l, _)| {
                    member_of(c, &l["from"]).as_deref() == Some(m)
                        || member_of(c, &l["to"]).as_deref() == Some(m)
                })
                .map(|(l, n)| {
                    format!(
                        "{} #{}: {}:{} ~ {}:{}",
                        n.unwrap_or("link"),
                        l["index"],
                        s(&l["from"]["subnet"]),
                        s(&l["from"]["port"]),
                        s(&l["to"]["subnet"]),
                        s(&l["to"]["port"])
                    )
                })
                .collect();
            hover_path(
                c,
                &pts,
                5.0,
                with_cites_info(
                    info(format!("leg: {m} in {name}"))
                        .fact("member", m.clone())
                        .fact("event links on it", via.join("; "))
                        .fact("class (flat name)", name.clone())
                        .note("A leg of the spider: this member transition fires as the class (sync-spiders.chyp: a Sync handle into m, out of n)."),
                    &theory_cites("sync-spiders"),
                ),
            );
            let r = Rect::from_min_size(Pos2::new(x0, my - 10.0), Vec2::new(bw, 20.0));
            c.p.rect(
                r,
                2.0,
                hex(fill),
                Stroke::new(1.0, hex("#333333")),
                StrokeKind::Middle,
            );
            c.text(r.center(), Align2::CENTER_CENTER, m, 10.5, INK);
            let (sid, tid) = m.split_once('/').unwrap_or(("", m));
            let mut i2 = info(format!("transition {m}")).fact("subnet", sid).fact("transition", tid).note("A member transition: a Sync handle None → Sync in sync-spiders.chyp (gens orders_dequeue, workers_acquire, payment_do_reserve for the default class).");
            if let Some(sb) = subnets(c).iter().find(|x| s(&x["id"]) == sid) {
                if let Some(t) = arr(&sb["transitions"]).iter().find(|t| s(&t["id"]) == tid) {
                    i2 = i2.fact("description", s(&t["description"]).to_string());
                }
            }
            c.hover(r, i2);
        }
        let dot = Pos2::new(sx, sy);
        let grow = pulse.as_ref().map_or(1.0, |(_, t)| 1.0 + 0.5 * (1.0 - *t));
        c.p.circle(dot, 9.0 * grow, green, Stroke::new(1.5, Color32::WHITE));
        let lbl = links_in
            .iter()
            .map(|(l, n)| format!("{} #{}", n.unwrap_or("link"), l["index"]))
            .collect::<Vec<_>>()
            .join(", ");
        c.p.text(
            dot + Vec2::new(15.0, -8.0),
            Align2::LEFT_CENTER,
            format!("{}-member class", members.len()),
            FontId::proportional(11.0),
            hex("#227744"),
        );
        c.p.text(
            dot + Vec2::new(15.0, 6.0),
            Align2::LEFT_CENTER,
            &lbl,
            FontId::proportional(10.5),
            hex("#555555"),
        );
        let ft = c.d.flat.as_ref().and_then(|f| {
            arr(&f["transitions"])
                .iter()
                .find(|t| s(&t["id"]) == name)
                .cloned()
        });
        c.hover(
            Rect::from_center_size(dot, Vec2::splat(22.0)),
            with_cites_info(
                info(format!("{name}: one event class = one spider"))
                    .fact("members", members.join(", "))
                    .fact("links", lbl.clone())
                    .fact("fused", ft.as_ref().map_or("?".to_string(), |t| t["fused"].to_string()))
                    .note("One event class = one spider: the links that fuse it are one m ; n each, and a connected set of links is one spider (special commutative Frobenius laws; Flatten's classes are a union-find)."),
                &theory_cites("sync-spiders"),
            ),
        );
        yy += blocks[gi] + 10.0;
    }
    if groups.is_empty() {
        c.text(
            Pos2::new(x + 14.0, y + 34.0),
            Align2::LEFT_TOP,
            "No event links: every transition is its own class.",
            11.5,
            MUTED,
        );
    }
    // the text: which rewrite covers the live link order
    let tx = x + 14.0 + sx_off + 170.0;
    let tw_ = (x + w - tx - 8.0).max(200.0);
    let mut ty = y + 28.0;
    let order_s = ev
        .iter()
        .map(|(l, n)| format!("{}=#{} {}", n.unwrap_or("?"), l["index"], ends(l)))
        .collect::<Vec<_>>()
        .join("  ");
    let r = para(
        c,
        Pos2::new(tx, ty),
        tw_,
        &format!(
            "Flatten's event classes now: {} fused, from {} event link{} {order_s}",
            groups.len(),
            ev.len(),
            if ev.len() == 1 { "" } else { "s" }
        ),
        11.0,
        INK,
        None,
        false,
    );
    ty = r.bottom() + 4.0;
    let find = |n: &str| ev.iter().position(|(_, x)| *x == Some(n));
    if let (Some(a), Some(b)) = (find("L1"), find("L2")) {
        let pick = if a < b { "fuse_L1_L2" } else { "fuse_L2_L1" };
        let r = para(c, Pos2::new(tx, ty), tw_, &format!("The 3-member class from L1 and L2 (in this order: {}) is spider3: Chyp proved it, and that the other order gives the same spider:", if a < b { "L1 then L2" } else { "L2 then L1" }), 11.0, VALID, None, false);
        ty = r.bottom() + 3.0;
        let mut lx = tx;
        for l in [pick, "link_order_irrelevant", "orders_example_fused"] {
            let rc = chip(
                c,
                Pos2::new(lx, ty),
                l,
                true,
                false,
                hex("#c9d3df"),
                false,
                json!({"rewrite": l}),
                info(format!("rewrite {l}"))
                    .note("Chyp proved it (precomputed); click to show it."),
            );
            lx = rc.right() + 4.0;
        }
        ty += 24.0;
    }
    if find("L3").is_some() && find("L1").is_some() && find("L2").is_some() {
        let r = para(c, Pos2::new(tx, ty), tw_, "L3 is on other handles, so its place in the link list does not matter (simp(): the interchange law):", 11.0, INK, None, false);
        let rc = chip(
            c,
            Pos2::new(tx, r.bottom() + 3.0),
            "L3_position_irrelevant",
            true,
            false,
            hex("#c9d3df"),
            false,
            json!({"rewrite": "L3_position_irrelevant"}),
            info("rewrite L3_position_irrelevant")
                .note("Chyp proved it (precomputed); click to show it."),
        );
        let _ = rc;
    }
    h + 8.0
}
