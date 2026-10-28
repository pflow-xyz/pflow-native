//! The laws view (see `docs/composition-views-scene.md`).

use super::*;

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

pub(super) fn draw_graph(c: &mut Ctx, g: &Value, r: Rect) {
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

pub(super) fn laws(c: &mut Ctx, area: Rect, orders: bool, rewrite: Option<&str>) {
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
