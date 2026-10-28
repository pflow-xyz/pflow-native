//! The boundaries view's layout (a port of `layoutOrders` in the prototype's
//! `js/views/boundaries.js`): instances as nets with boundaries, one row per
//! transition wired to the outer (left) or shared (right) edge, and one card
//! per event class beyond the shared boundary. A pure function of the
//! subnets, the links and the flat net, in the prototype's design coordinates.

use super::*;

pub const W: f32 = 1150.0;
pub const OUTER_X: f32 = 88.0;
pub const LANE0: f32 = 16.0;
pub const LANE_STEP: f32 = 9.0;
pub const BOX_X: f32 = 114.0;
pub const BOX_W: f32 = 350.0;
pub const BOX_R: f32 = BOX_X + BOX_W;
pub const SHARED_X: f32 = 648.0;
pub const CARD_X: f32 = 680.0;
pub const CARD_W: f32 = W - CARD_X - 10.0;
pub const TOP: f32 = 64.0;
pub const HEAD: f32 = 28.0;
pub const TROW: f32 = 22.0;
pub const PROW: f32 = 17.0;
pub const CHIP_H: f32 = 15.0;
const GAP: f32 = 26.0;
pub const RECT_W: f32 = 112.0;
pub const RECT_H: f32 = 16.0;
pub const BPORT_R: f32 = 11.0;

fn greedy_wrap(text: &str, max: usize) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    for w in text.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + w.chars().count() > max {
            out.push(std::mem::take(&mut cur));
            cur = w.to_string();
        } else if cur.is_empty() {
            cur = w.to_string();
        } else {
            cur = format!("{cur} {w}");
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Wrap to at most `max` chars per line, balanced: the narrowest width that
/// needs no more lines (no orphans).
pub fn wrap(text: &str, max: usize) -> Vec<String> {
    let n = greedy_wrap(text, max).len();
    let mut w = max;
    while n > 1 && w > 12 && greedy_wrap(text, w - 1).len() == n {
        w -= 1;
    }
    greedy_wrap(text, w)
}

#[derive(Clone)]
pub struct PortPos {
    pub key: String,
    pub sid: String,
    pub port: Value,
    /// 'L' (outer boundary) or 'R' (shared boundary).
    pub side: char,
    pub x: f32,
    pub y: f32,
    /// The member (`subnet/transition`) of the row a transition port sits on.
    pub member: Option<String>,
}

#[derive(Clone)]
pub struct Row {
    pub sid: String,
    pub tid: String,
    pub member: String,
    pub cy: f32,
    pub cls: Option<String>,
    pub linked: bool,
    pub rx: f32,
    pub ports: Vec<usize>,
}

#[derive(Clone)]
pub struct BBox {
    pub sub: Value,
    pub y: f32,
    pub h: f32,
    pub rows: Vec<Row>,
    /// (index into `ports`, y) of the place ports, listed under the rows.
    pub pports: Vec<usize>,
    pub chips: Vec<(Value, f32, f32)>,
}

#[derive(Clone)]
pub struct Class {
    pub name: String,
    pub members: Vec<String>,
    /// (member, index of the port its wire starts at).
    pub mports: Vec<(String, usize)>,
    pub clinks: Vec<Value>,
    pub pre: Vec<Value>,
    pub reads: Vec<Value>,
    pub post: Vec<Value>,
    pub why: String,
    pub why_lines: Vec<String>,
    pub alone: Vec<(bool, Vec<String>)>,
    pub status_x: f32,
    pub leak: Option<(Value, String)>,
    pub leak_lines: Vec<String>,
    pub nrows: usize,
    pub h: f32,
    pub mean_y: f32,
    pub letter: char,
    pub top: f32,
    pub by: f32,
}

pub struct Ghost {
    pub l: Value,
    pub lane: f32,
    pub pts: Vec<(f32, f32)>,
    pub my: f32,
}

pub struct Lay {
    pub ports: Vec<PortPos>,
    pub port_ix: BTreeMap<String, usize>,
    pub rows: BTreeMap<String, (usize, usize)>,
    pub boxes: Vec<BBox>,
    pub classes: Vec<Class>,
    pub ghosts: Vec<Ghost>,
    pub boxes_bottom: f32,
    pub height: f32,
}

/// What the layout reads besides the subnets and links.
pub struct Input<'a> {
    pub subs: &'a [Value],
    pub links: &'a [Value],
    /// Flat event classes: (flat id, members).
    pub fused: Vec<(String, Vec<String>)>,
    pub flat: &'a Value,
    /// `bundle.boundaries` per class id: (composite why, per member (ok, why)).
    pub alone: &'a Value,
}

fn pkey(sid: &str, port: &str) -> String {
    format!("{sid}|{port}")
}

pub fn layout(inp: &Input) -> Lay {
    let subs = inp.subs;
    let by_id: BTreeMap<&str, &Value> = subs.iter().map(|x| (s(&x["id"]), x)).collect();
    let mut event_ports: BTreeSet<String> = BTreeSet::new();
    for l in inp.links.iter().filter(|l| s(&l["kind"]) == "event") {
        event_ports.insert(pkey(s(&l["from"]["subnet"]), s(&l["from"]["port"])));
        event_ports.insert(pkey(s(&l["to"]["subnet"]), s(&l["to"]["port"])));
    }
    let mut class_of: BTreeMap<&str, &str> = BTreeMap::new();
    for (name, members) in &inp.fused {
        for m in members {
            class_of.insert(m, name);
        }
    }
    let member_of_ref = |r: &Value| -> Option<String> {
        let sub = by_id.get(s(&r["subnet"]))?;
        let p = arr(&sub["ports"])
            .iter()
            .find(|q| s(&q["id"]) == s(&r["port"]))?;
        let t = s(&p["transition"]);
        (!t.is_empty()).then(|| format!("{}/{t}", s(&sub["id"])))
    };

    let mut boxes: Vec<BBox> = vec![];
    let mut ports: Vec<PortPos> = vec![];
    let mut port_ix: BTreeMap<String, usize> = BTreeMap::new();
    let mut rows_ix: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut y = TOP;
    for sub in subs {
        let sid = s(&sub["id"]).to_string();
        let mut bx = BBox {
            sub: sub.clone(),
            y,
            h: 0.0,
            rows: vec![],
            pports: vec![],
            chips: vec![],
        };
        let mut ry = y + HEAD + 4.0;
        for t in arr(&sub["transitions"]) {
            let tid = s(&t["id"]).to_string();
            let cy = ry + TROW / 2.0;
            let member = format!("{sid}/{tid}");
            let cls = class_of.get(member.as_str()).map(|x| x.to_string());
            let mut row = Row {
                sid: sid.clone(),
                tid: tid.clone(),
                member: member.clone(),
                cy,
                cls: cls.clone(),
                linked: cls.is_some(),
                rx: if cls.is_some() {
                    BOX_R - 26.0 - RECT_W
                } else {
                    BOX_X + 26.0
                },
                ports: vec![],
            };
            for p in arr(&sub["ports"])
                .iter()
                .filter(|q| s(&q["target"]) == "transition" && s(&q["transition"]) == tid)
            {
                let key = pkey(&sid, s(&p["id"]));
                let side = if event_ports.contains(&key) { 'R' } else { 'L' };
                let pos = PortPos {
                    key: key.clone(),
                    sid: sid.clone(),
                    port: p.clone(),
                    side,
                    x: if side == 'R' { BOX_R } else { BOX_X },
                    y: cy,
                    member: Some(member.clone()),
                };
                port_ix.insert(key, ports.len());
                row.ports.push(ports.len());
                ports.push(pos);
            }
            rows_ix.insert(member, (boxes.len(), bx.rows.len()));
            bx.rows.push(row);
            ry += TROW;
        }
        ry += 4.0;
        for p in arr(&sub["ports"])
            .iter()
            .filter(|q| s(&q["target"]) == "place")
        {
            let key = pkey(&sid, s(&p["id"]));
            let pos = PortPos {
                key: key.clone(),
                sid: sid.clone(),
                port: p.clone(),
                side: 'L',
                x: BOX_X,
                y: ry + PROW / 2.0,
                member: None,
            };
            port_ix.insert(key, ports.len());
            bx.pports.push(ports.len());
            ports.push(pos);
            ry += PROW;
        }
        ry += 5.0;
        let (mut cx, mut cyc) = (BOX_X + 14.0, ry + CHIP_H / 2.0);
        for p in arr(&sub["places"]) {
            let w = 15.0 + s(&p["id"]).chars().count() as f32 * 9.0 * 0.6 + 12.0;
            if cx + w > BOX_R - 8.0 {
                cx = BOX_X + 14.0;
                cyc += CHIP_H;
            }
            bx.chips.push((p.clone(), cx, cyc));
            cx += w;
        }
        ry = cyc + CHIP_H / 2.0 + 8.0;
        bx.h = ry - y;
        boxes.push(bx);
        y = ry + GAP;
    }
    let boxes_bottom = y - GAP;

    // Event classes: one boundary port each, cards in order of their members' mean height.
    let arcs = arr(&inp.flat["arcs"]);
    let is_read = |a: &Value| matches!(s(&a["type"]), "read" | "inhibitor");
    let mut classes: Vec<Class> = inp
        .fused
        .iter()
        .map(|(name, members)| {
            let mports: Vec<(String, usize)> = members
                .iter()
                .filter_map(|m| {
                    let (bi, ri) = *rows_ix.get(m)?;
                    let row = &boxes[bi].rows[ri];
                    let pi = row
                        .ports
                        .iter()
                        .copied()
                        .find(|i| ports[*i].side == 'R')
                        .or_else(|| row.ports.first().copied())?;
                    Some((m.clone(), pi))
                })
                .collect();
            let clinks: Vec<Value> = inp
                .links
                .iter()
                .filter(|l| {
                    s(&l["kind"]) == "event"
                        && member_of_ref(&l["from"]).is_some_and(|m| members.contains(&m))
                })
                .cloned()
                .collect();
            let pre: Vec<Value> = arcs
                .iter()
                .filter(|a| s(&a["to"]) == name && !is_read(a))
                .cloned()
                .collect();
            let reads: Vec<Value> = arcs
                .iter()
                .filter(|a| s(&a["to"]) == name && is_read(a))
                .cloned()
                .collect();
            let post: Vec<Value> = arcs
                .iter()
                .filter(|a| s(&a["from"]) == name && !is_read(a))
                .cloned()
                .collect();
            let cls_info = arr(&inp.alone["classes"])
                .iter()
                .find(|c| s(&c["id"]) == name);
            let why = cls_info
                .map(|c| s(&c["why"]).to_string())
                .unwrap_or_default();
            let leak_link = clinks.iter().find(|l| l["leak"] == true).cloned();
            let leak_idiom = leak_link.as_ref().and_then(|l| {
                comp::ontology()["idioms"]
                    .as_array()?
                    .iter()
                    .find(|i| s(&i["key"]) == s(&l["idiom"]))
                    .map(|i| s(&i["caveat"]).to_string())
            });
            let leak_lines = leak_idiom
                .as_deref()
                .map(|t| wrap(t, 82))
                .unwrap_or_default();
            let nrows = (pre.len() + reads.len()).max(post.len()).max(1);
            let status_x = CARD_X
                + 24.0
                + members
                    .iter()
                    .map(|m| (m.chars().count() + 6) as f32 * 10.5 * 0.6)
                    .fold(0.0, f32::max)
                + 14.0;
            let status_chars =
                (((CARD_X + CARD_W - 8.0 - status_x) / (9.5 * 0.52)).floor() as usize).max(16);
            let alone: Vec<(bool, Vec<String>)> = members
                .iter()
                .map(|m| {
                    let a = cls_info
                        .and_then(|c| arr(&c["members"]).iter().find(|x| s(&x["member"]) == m));
                    let ok = a.is_none_or(|x| x["enabled_alone"] == true);
                    let why = a.map(|x| s(&x["why"]).to_string()).unwrap_or_default();
                    let t = if ok {
                        "enabled on its own".to_string()
                    } else {
                        format!("alone: {why}")
                    };
                    (ok, wrap(&t, status_chars))
                })
                .collect();
            let member_lines: usize = alone.iter().map(|a| a.1.len()).sum();
            let why_lines = if why.is_empty() {
                vec![]
            } else {
                wrap(
                    &format!("EnabledWhyNot: {}", why.replace(name.as_str(), "@")),
                    ((CARD_W - 36.0) / (9.5 * 0.52)).floor() as usize,
                )
            };
            let h = 24.0
                + member_lines as f32 * 13.0
                + 4.0
                + why_lines.len() as f32 * 13.0
                + 16.0
                + 13.0
                + 13.0
                + clinks.len() as f32 * 12.0
                + 6.0
                + 14.0
                + nrows as f32 * 18.0
                + 8.0
                + if leak_idiom.is_some() {
                    22.0 + leak_lines.len() as f32 * 12.0
                } else {
                    0.0
                }
                + 6.0;
            let mean_y =
                mports.iter().map(|(_, pi)| ports[*pi].y).sum::<f32>() / mports.len().max(1) as f32;
            Class {
                name: name.clone(),
                members: members.clone(),
                mports,
                clinks,
                pre,
                reads,
                post,
                why,
                why_lines,
                alone,
                status_x,
                leak: leak_link.zip(leak_idiom),
                leak_lines,
                nrows,
                h,
                mean_y,
                letter: 'A',
                top: 0.0,
                by: 0.0,
            }
        })
        .collect();
    classes.sort_by(|a, b| {
        a.mean_y
            .total_cmp(&b.mean_y)
            .then_with(|| a.name.cmp(&b.name))
    });
    let mut bottom = TOP - 14.0;
    for (i, c) in classes.iter_mut().enumerate() {
        c.letter = (b'A' + (i % 26) as u8) as char;
        c.top = (c.mean_y - 14.0).max(bottom + 14.0).max(TOP);
        c.by = c.top + 14.0;
        bottom = c.top + c.h;
    }

    let ghosts: Vec<Ghost> = inp
        .links
        .iter()
        .filter(|l| s(&l["kind"]) != "event")
        .enumerate()
        .filter_map(|(i, l)| {
            let lane = LANE0 + (i % 7) as f32 * LANE_STEP;
            // A place end meets its port circle; a transition end leaves the
            // transition's bottom edge and runs left in the gap between rows.
            let end = |r: &Value| -> Option<Vec<(f32, f32)>> {
                let pos = &ports[*port_ix.get(&pkey(s(&r["subnet"]), s(&r["port"])))?];
                if s(&pos.port["target"]) == "place" {
                    return Some(vec![(pos.x - 6.0, pos.y)]);
                }
                let m = pos.member.as_ref()?;
                let (bi, ri) = *rows_ix.get(m)?;
                let row = &boxes[bi].rows[ri];
                let x = row.rx + 8.0;
                Some(vec![(x, row.cy + RECT_H / 2.0), (x, row.cy + TROW / 2.0)])
            };
            let (a, b) = (end(&l["from"])?, end(&l["to"])?);
            let (ay, by) = (a.last()?.1, b.last()?.1);
            let mut pts = a.clone();
            pts.push((lane, ay));
            pts.push((lane, by));
            pts.extend(b.iter().rev().copied());
            Some(Ghost {
                l: l.clone(),
                lane,
                pts,
                my: (ay + by) / 2.0,
            })
        })
        .collect();
    let height = boxes_bottom.max(bottom) + 54.0;
    Lay {
        ports,
        port_ix,
        rows: rows_ix,
        boxes,
        classes,
        ghosts,
        boxes_bottom,
        height,
    }
}
