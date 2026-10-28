//! The event classes of the composition read as nets with boundaries
//! (`bundle.boundaries`): each fused group of `Flatten` is one port of the
//! shared boundary and one composite transition. Beside the composite's own
//! enablement at the flat initial marking, each member is asked on its own:
//! pflow-metamodel's `enabled_why_not` over a one-transition model holding
//! only that member's arcs (and any guard link that gates it, lowered to a
//! read arc as Flatten does), retargeted onto the flat places, at the flat
//! initial marking. So capacity and read arcs count exactly as they do for
//! the composite (the prototype's `js/views/boundaries.js`).

use pflow_compose::LinkKind;
use pflow_metamodel::{Arc, ArcType, Model, Transition};
use serde_json::{json, Value};

use super::{flatten, Composition};

/// `bundle.boundaries`' result: `{ok, classes: [{id, members: [{member,
/// enabled_alone, why}], enabled, why}]}`, classes in flat-id order.
pub fn boundaries_json(c: &Composition) -> Value {
    let flat = match flatten(&c.bundle) {
        Ok(f) => f,
        Err(e) => return json!({"ok": false, "error": e}),
    };
    let mk = flat.model.initial_marking();
    let member_of_end = |subnet: &str, port: &str| -> Option<String> {
        let s = c.bundle.subnet_by_id(subnet)?;
        let p = s.ports.iter().find(|p| p.id == port)?;
        (!p.transition.is_empty()).then(|| format!("{subnet}/{}", p.transition))
    };
    let mut groups: Vec<(&String, &Vec<String>)> = flat.map.fused_groups.iter().collect();
    groups.sort();
    let classes: Vec<Value> = groups
        .into_iter()
        .map(|(id, members)| {
            let composite = flat.model.enabled_why_not(id, &mk);
            let rows: Vec<Value> = members
                .iter()
                .map(|m| {
                    let (sid, tid) = m.split_once('/').unwrap_or(("", m));
                    let place = |local: &str| {
                        flat.place_of(&format!("{sid}/{local}"))
                            .map(str::to_string)
                            .unwrap_or_else(|| local.to_string())
                    };
                    let mut arcs: Vec<Arc> = c
                        .bundle
                        .subnet_by_id(sid)
                        .map(|s| {
                            s.model
                                .arcs
                                .iter()
                                .filter(|a| a.to == tid || a.from == tid)
                                .map(|a| {
                                    let mut a = a.clone();
                                    if a.to == tid {
                                        a.from = place(&a.from);
                                        a.to = m.clone();
                                    } else {
                                        a.from = m.clone();
                                        a.to = place(&a.to);
                                    }
                                    a
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    for l in &c.bundle.links {
                        if l.kind != LinkKind::Guard
                            || member_of_end(&l.from.subnet, &l.from.port).as_deref() != Some(m)
                        {
                            continue;
                        }
                        let Some(tp) = c
                            .bundle
                            .subnet_by_id(&l.to.subnet)
                            .and_then(|s| s.ports.iter().find(|p| p.id == l.to.port))
                        else {
                            continue;
                        };
                        if let Some(p) = flat.place_of(&format!("{}/{}", l.to.subnet, tp.place)) {
                            arcs.push(Arc {
                                from: p.to_string(),
                                to: m.clone(),
                                weight: 1,
                                typ: ArcType::Read,
                                ..Default::default()
                            });
                        }
                    }
                    let model = Model {
                        places: flat.model.places.clone(),
                        transitions: vec![Transition {
                            id: m.clone(),
                            ..Default::default()
                        }],
                        arcs,
                        ..Default::default()
                    };
                    match model.enabled_why_not(m, &mk) {
                        Ok(()) => json!({"member": m, "enabled_alone": true, "why": ""}),
                        Err(e) => {
                            let why = e.to_string();
                            let why = why.strip_prefix(&format!("{m} ")).unwrap_or(&why);
                            json!({"member": m, "enabled_alone": false, "why": why})
                        }
                    }
                })
                .collect();
            json!({
                "id": id,
                "members": rows,
                "enabled": composite.is_ok(),
                "why": composite.err().map(|e| e.to_string()).unwrap_or_default(),
            })
        })
        .collect();
    json!({"ok": true, "classes": classes})
}
