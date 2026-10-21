//! Composition: the component library (sim.pflow.xyz's registry plus
//! pflow-compose's templates and subnets) instantiated as positioned
//! fragments, and the structural diff of two documents with its
//! side-by-side drawing. The `compose.*` commands
//! (`crate::commands::compose`) put these into the session.
//!
//! Everything here is pure: no session, deterministic output.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::artifact::DrawShape;
use crate::doc::{Arc, Document, NodeKind, Place, Transition};
use crate::layout::{self, Algorithm};
use crate::{Error, Result};

/// Largest value a component parameter may take (token counts, rates).
pub const MAX_PARAM: f64 = 1.0e6;
/// Most stages the `workflow` template builds.
pub const MAX_STAGES: f64 = 50.0;

pub type Params = BTreeMap<String, f64>;

/// A place a component exposes for fusion with the surrounding net.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PortSpec {
    pub name: String,
    /// The fragment place the port is (before renaming).
    pub place: String,
    /// `in` (the component consumes from it), `out` (it produces into
    /// it), `inout`, `observe` (only tested) or `state` (untouched): taken
    /// from the declaring subnet where it has kinds, else from the arcs.
    pub role: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParamSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub default: f64,
}

/// One library entry, as `compose.components` lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ComponentInfo {
    pub name: &'static str,
    pub description: &'static str,
    /// `sim` (sim.pflow.xyz's registry, ported), `pflow-compose template`
    /// or `pflow-compose subnet`.
    pub source: &'static str,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub discipline: &'static [&'static str],
    pub ports: Vec<PortSpec>,
    pub params: Vec<ParamSpec>,
}

/// A component instance before it joins a net: single-color, positioned
/// with its bounding box at the origin.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fragment {
    pub places: Vec<FPlace>,
    pub transitions: Vec<FTransition>,
    pub arcs: Vec<FArc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FPlace {
    pub id: String,
    pub label: Option<String>,
    pub initial: f64,
    /// 0 = unbounded.
    pub capacity: f64,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FTransition {
    pub id: String,
    pub label: Option<String>,
    /// A rate the component declares (sim's per-hour rates; 0 = dormant).
    pub rate: Option<f64>,
    pub x: f64,
    pub y: f64,
}

/// In document orientation: `inhibit` on a place→transition arc is an
/// inhibitor, on a transition→place arc the editor's read arc.
#[derive(Debug, Clone, PartialEq)]
pub struct FArc {
    pub source: String,
    pub target: String,
    pub weight: f64,
    pub inhibit: bool,
    /// sim's `kinetic: false` (a prerequisite, not an accelerant). The
    /// pflow.xyz document cannot carry it; fusing reports it as a note.
    pub kinetic: bool,
}

impl Fragment {
    fn place(&mut self, id: &str, initial: f64, capacity: f64) -> &mut Self {
        self.places.push(FPlace {
            id: id.into(),
            label: None,
            initial,
            capacity,
            x: 0.0,
            y: 0.0,
        });
        self
    }

    fn transition(&mut self, id: &str, rate: Option<f64>) -> &mut Self {
        self.transitions.push(FTransition {
            id: id.into(),
            label: None,
            rate,
            x: 0.0,
            y: 0.0,
        });
        self
    }

    fn arc(&mut self, source: &str, target: &str, weight: f64) -> &mut Self {
        self.arcs.push(FArc {
            source: source.into(),
            target: target.into(),
            weight: weight.max(1.0),
            inhibit: false,
            kinetic: true,
        });
        self
    }

    fn inhibitor(&mut self, source: &str, target: &str, weight: f64) -> &mut Self {
        self.arc(source, target, weight);
        self.arcs.last_mut().expect("just pushed").inhibit = true;
        self
    }

    fn non_kinetic(&mut self, source: &str, target: &str) -> &mut Self {
        self.arc(source, target, 1.0);
        self.arcs.last_mut().expect("just pushed").kinetic = false;
        self
    }

    pub fn is_place(&self, id: &str) -> bool {
        self.places.iter().any(|p| p.id == id)
    }

    /// As a single-color document (for layout).
    fn to_document(&self) -> Document {
        let mut d = Document::default();
        for p in &self.places {
            d.places.push(Place {
                id: p.id.clone(),
                label: p.label.clone(),
                initial: vec![p.initial],
                capacity: vec![p.capacity],
                x: p.x,
                y: p.y,
                extra: Default::default(),
            });
        }
        for t in &self.transitions {
            d.transitions.push(Transition {
                id: t.id.clone(),
                label: t.label.clone(),
                role: None,
                x: t.x,
                y: t.y,
                extra: Default::default(),
            });
        }
        for a in &self.arcs {
            d.arcs.push(Arc {
                source: a.source.clone(),
                target: a.target.clone(),
                weight: vec![a.weight],
                inhibit: a.inhibit,
                extra: Default::default(),
            });
        }
        d
    }

    /// Positions a coordinate-less fragment: pflow.xyz's Sugiyama layout,
    /// turned to run left to right.
    fn auto_layout(&mut self) {
        let pos: BTreeMap<String, (f64, f64)> =
            layout::layout(&self.to_document(), Algorithm::Sugiyama)
                .into_iter()
                .map(|(id, x, y)| (id, (y, x)))
                .collect();
        for p in &mut self.places {
            (p.x, p.y) = pos.get(&p.id).copied().unwrap_or_default();
        }
        for t in &mut self.transitions {
            (t.x, t.y) = pos.get(&t.id).copied().unwrap_or_default();
        }
        self.normalize();
    }

    /// Scales positions by `k` and moves the bounding box to the origin.
    fn scale(&mut self, k: f64) {
        for p in &mut self.places {
            (p.x, p.y) = (p.x * k, p.y * k);
        }
        for t in &mut self.transitions {
            (t.x, t.y) = (t.x * k, t.y * k);
        }
        self.normalize();
    }

    fn normalize(&mut self) {
        let pts = self
            .places
            .iter()
            .map(|p| (p.x, p.y))
            .chain(self.transitions.iter().map(|t| (t.x, t.y)));
        let (mx, my) = pts.fold((f64::INFINITY, f64::INFINITY), |(a, b), (x, y)| {
            (a.min(x), b.min(y))
        });
        if !mx.is_finite() {
            return;
        }
        for p in &mut self.places {
            (p.x, p.y) = (p.x - mx, p.y - my);
        }
        for t in &mut self.transitions {
            (t.x, t.y) = (t.x - mx, t.y - my);
        }
    }

    /// The fragment without the given places: what an instantiation with
    /// those places attached actually contributes.
    fn without(&self, places: &BTreeSet<String>) -> Fragment {
        let mut f = self.clone();
        f.places.retain(|p| !places.contains(&p.id));
        f
    }

    /// Port roles from the arcs (see [`PortSpec::role`]).
    fn role_of(&self, place: &str) -> &'static str {
        let consumed = self.arcs.iter().any(|a| a.source == place && !a.inhibit);
        let produced = self.arcs.iter().any(|a| a.target == place && !a.inhibit);
        let tested = self
            .arcs
            .iter()
            .any(|a| a.inhibit && (a.source == place || a.target == place));
        match (consumed, produced) {
            (true, true) => "inout",
            (true, false) => "in",
            (false, true) => "out",
            _ if tested => "observe",
            _ => "state",
        }
    }
}

/// A port declaration: (name, fragment place, role or "" to derive it,
/// description).
type PortDecl = (&'static str, &'static str, &'static str, &'static str);

struct Def {
    name: &'static str,
    description: &'static str,
    source: &'static str,
    discipline: &'static [&'static str],
    params: &'static [ParamSpec],
    /// Empty: every place is a port, described by its label.
    ports: &'static [PortDecl],
    build: fn(&Params) -> std::result::Result<Fragment, String>,
}

const fn param(name: &'static str, description: &'static str, default: f64) -> ParamSpec {
    ParamSpec {
        name,
        description,
        default,
    }
}

fn get(p: &Params, k: &str) -> f64 {
    p.get(k).copied().unwrap_or(0.0)
}

/// Token counts are whole.
fn count(p: &Params, k: &str) -> f64 {
    get(p, k).round()
}

const SIM: &str = "sim";
const TEMPLATE: &str = "pflow-compose template";
const SUBNET: &str = "pflow-compose subnet";

/// The library, sorted by name. sim's seven components are ported from
/// `sim-pflow-xyz/pkg/components/registry.go` (same ports, params,
/// defaults and discipline notes); templates and subnets come from
/// pflow-compose.
const LIBRARY: &[Def] = &[
    Def {
        name: "arrivals",
        description: "A source: work arriving from the world into a queue, at a rate per hour.",
        source: SIM,
        discipline: &[
            "a source transition has no inputs, so diagnose measures it as a demand knob (more demand ranks negative when losses are the objective)",
            "rates are per hour everywhere in sim; sim_calibrate learns this one from inter-arrival gaps",
        ],
        params: &[param("per_hour", "arrival rate", 10.0)],
        ports: &[("queue", "queue", "", "where arrivals land; attach to a service's queue")],
        build: |p| {
            let mut f = Fragment::default();
            f.place("queue", 0.0, 0.0)
                .transition("arrive", Some(get(p, "per_hour")))
                .arc("arrive", "queue", 1.0);
            Ok(f)
        },
    },
    Def {
        name: "bounded-queue",
        description: "A queue subnet (go-pflow metamodel/queue.go): items and free slots are complementary places, so a positive capacity is a structural P-invariant. Capacity 0 makes it unbounded (enqueue becomes a source).",
        source: SUBNET,
        discipline: &[],
        params: &[
            param("capacity", "bound on queued items; 0 = unbounded", 4.0),
            param("initial", "items queued at the start", 0.0),
        ],
        ports: &[
            ("in", "items", "in", "where items are queued"),
            ("out", "items", "out", "where items are released from"),
            ("depth", "items", "observe", "the queue length, for read arcs"),
        ],
        build: |p| {
            let sub = pflow_compose::new_queue(pflow_compose::QueueSpec {
                id: "queue".into(),
                capacity: count(p, "capacity") as i64,
                initial: count(p, "initial") as i64,
                description: String::new(),
            })?;
            Ok(from_model(&sub.model))
        },
    },
    Def {
        name: "datastore",
        description: "A capacity-bounded store — the database shape. Writers append records from a source place; the capacity is the disk.",
        source: SIM,
        discipline: &[
            "consumers that only look should use read arcs against the store, not consuming arcs — a query is not a delete",
            "the capacity is a post-firing bound the firing rule enforces; when the store is full, writes refuse rather than silently dropping",
        ],
        params: &[
            param("capacity", "records the store holds", 100.0),
            param("write_per_hour", "write throughput", 120.0),
        ],
        ports: &[
            ("source", "source", "", "records waiting to be written; attach to the producer"),
            ("store", "store", "", "the stored records; point read arcs here"),
        ],
        build: |p| {
            let mut f = Fragment::default();
            f.place("source", 0.0, 0.0)
                .place("store", 0.0, count(p, "capacity"))
                .transition("write", Some(get(p, "write_per_hour")))
                .arc("source", "write", 1.0)
                .arc("write", "store", 1.0);
            Ok(f)
        },
    },
    Def {
        name: "decision",
        description: "An operator decision: a budget pool spent through a dormant (rate 0) transition into a target place. Dormant to scenarios; fired only by choice.",
        source: SIM,
        discipline: &[
            "dormant by a rate of 0, the engine's true zero, so plain scenarios are untouched and the decision layer is invisible until an operator spends",
            "the budget is conserved, so every spend is priced against the alternatives",
        ],
        params: &[
            param("budget", "decision tokens available", 5.0),
            param("cost", "budget consumed per spend", 1.0),
            param("yield", "units minted into the target per spend", 1.0),
        ],
        ports: &[(
            "target",
            "target",
            "",
            "what the spend buys; attach to a staff pool, a spare shelf, a capacity place",
        )],
        build: |p| {
            let mut f = Fragment::default();
            f.place("budget", count(p, "budget"), 0.0)
                .place("target", 0.0, 0.0)
                .transition("spend", Some(0.0))
                .arc("budget", "spend", count(p, "cost"))
                .arc("spend", "target", count(p, "yield"));
            Ok(f)
        },
    },
    Def {
        name: "hazard",
        description: "Abandonment racing the service: each waiting item independently gives up at a patience rate, into a lost sink.",
        source: SIM,
        discipline: &[
            "the queue arc stays kinetic: each waiting item abandons independently, so loss = patience x mean queue length — which is what staffing moves",
            "the lost sink is fed by a bare hazard, which is exactly what makes diagnose classify it as loss",
            "sim_calibrate cannot learn a hazard racing an instant pickup from completion gaps alone (its observed gap reflects the queue's total drain); declare this rate from knowledge of the world",
        ],
        params: &[param("patience_per_hour", "abandonment rate per waiting item", 2.0)],
        ports: &[
            ("queue", "queue", "", "the queue being abandoned; attach to the service's queue"),
            ("lost", "lost", "", "the loss sink; attach to share one loss ledger"),
        ],
        build: |p| {
            let mut f = Fragment::default();
            f.place("queue", 0.0, 0.0)
                .place("lost", 0.0, 0.0)
                .transition("abandon", Some(get(p, "patience_per_hour")))
                .arc("queue", "abandon", 1.0)
                .arc("abandon", "lost", 1.0);
            Ok(f)
        },
    },
    Def {
        name: "inventory",
        description: "A reorder-point stock loop: when stock falls below the reorder point, an order goes out; delivery replenishes in batches after a lead time.",
        source: SIM,
        discipline: &[
            "the reorder trigger is an inhibitor arc (blocks at >= reorder_point), so the threshold is structural — reachability and verify can see it, and it costs nothing at runtime",
            "an inhibitor arc on on_order (weight 1) stops double-ordering while a delivery is in flight",
            "deliveries arrive as a batch (arc weight), which is what makes van/slot contention visible; per-unit trickle would hide it",
        ],
        params: &[
            param("initial", "opening stock", 8.0),
            param("reorder_point", "reorder when stock falls below this", 3.0),
            param("batch", "units per delivery", 5.0),
            param("lead_per_hour", "delivery completion rate (1/lead-time)", 1.0),
        ],
        ports: &[("stock", "stock", "", "the stock being managed; consumers draw from it")],
        build: |p| {
            let mut f = Fragment::default();
            f.place("stock", count(p, "initial"), 0.0)
                .place("on_order", 0.0, 0.0)
                .transition("reorder", Some(720.0))
                .transition("deliver", Some(get(p, "lead_per_hour")))
                .inhibitor("stock", "reorder", count(p, "reorder_point"))
                .inhibitor("on_order", "reorder", 1.0)
                .arc("reorder", "on_order", 1.0)
                .arc("on_order", "deliver", 1.0)
                .arc("deliver", "stock", count(p, "batch"));
            Ok(f)
        },
    },
    Def {
        name: "mailbox",
        description: "An asynchronous buffer between a producer and a reader — the email/message-queue shape. Messages land in an inbox; a single reader drains it.",
        source: SIM,
        discipline: &[
            "delivery is kinetic (each message travels independently); reading is non-kinetic (one reader does not read faster because the inbox is fuller) — the same prerequisite-not-accelerant rule as a service pickup",
            "an unbounded inbox is a genuine modeling statement: nothing pushes back on the producer; add a capacity if the real system drops or blocks",
        ],
        params: &[
            param("deliver_per_hour", "delivery rate per message", 60.0),
            param("read_per_hour", "messages the reader handles per hour", 6.0),
        ],
        ports: &[
            ("outbox", "outbox", "", "where messages are sent from; attach to the producer's output place"),
            ("handled", "handled", "", "messages dealt with; attach to feed downstream work"),
        ],
        build: |p| {
            let mut f = Fragment::default();
            f.place("outbox", 0.0, 0.0)
                .place("inbox", 0.0, 0.0)
                .place("handled", 0.0, 0.0)
                .transition("deliver", Some(get(p, "deliver_per_hour")))
                .transition("read", Some(get(p, "read_per_hour")))
                .arc("outbox", "deliver", 1.0)
                .arc("deliver", "inbox", 1.0)
                .non_kinetic("inbox", "read")
                .arc("read", "handled", 1.0);
            Ok(f)
        },
    },
    Def {
        name: "producer-consumer",
        description: "Producer-consumer with a bounded buffer (go-pflow templates).",
        source: TEMPLATE,
        discipline: &[],
        params: &[
            param("buffer_size", "buffer capacity", 10.0),
            param("initial_buffer", "items buffered at the start", 0.0),
            param("producers", "producer tokens", 1.0),
            param("consumers", "consumer tokens", 1.0),
        ],
        ports: &[],
        build: |p| template("producer-consumer", p),
    },
    Def {
        name: "queue",
        description: "A queueing system: arrivals, a waiting queue, servers and completion (go-pflow templates).",
        source: TEMPLATE,
        discipline: &[],
        params: &[
            param("servers", "available servers", 1.0),
            param("initial_queue", "items waiting at the start", 0.0),
            param("queue_capacity", "queue bound; 0 = unbounded", 0.0),
        ],
        ports: &[],
        build: |p| template("queue", p),
    },
    Def {
        name: "resource-pool",
        description: "A fixed-capacity acquire/release pool (go-pflow metamodel ResourcePool): available + in_use is conserved.",
        source: SUBNET,
        discipline: &[],
        params: &[param("capacity", "resources in the pool", 3.0)],
        ports: &[
            ("available", "available", "inout", "free resources"),
            ("in_use", "in_use", "out", "resources held"),
        ],
        build: |p| {
            let sub = pflow_compose::new_resource_pool("pool", count(p, "capacity") as i64);
            Ok(from_model(&sub.model))
        },
    },
    Def {
        name: "seir",
        description: "SEIR epidemic: Susceptible → Exposed → Infected → Recovered (go-pflow templates).",
        source: TEMPLATE,
        discipline: &[],
        params: &[
            param("population", "total population", 1000.0),
            param("initial_exposed", "exposed at the start", 5.0),
            param("initial_infected", "infected at the start", 5.0),
        ],
        ports: &[],
        build: |p| template("seir", p),
    },
    Def {
        name: "service",
        description: "A staffed service: a pool of servers picks work from a queue, holds a server while working, releases it on completion into a served sink.",
        source: SIM,
        discipline: &[
            "the queue->pickup arc is non-kinetic at rate 720: a free server notices waiting work in seconds, and a prerequisite is not an accelerant — without this, service speed scales with queue length and staffing questions have no answer",
            "the pool is held across the pickup/finish pair, so headcount is observably binding; seized-and-released-in-one-firing makes staffing irrelevant",
            "the served sink is fed by the pool-releasing transition, which is exactly what makes diagnose classify it as success",
            "sim_calibrate never overwrites the pickup's rate (declared >= 100 marks it): its observed gap is the queue wait, not the pickup",
        ],
        params: &[
            param("staff", "servers in the pool", 2.0),
            param("per_hour", "completions per busy server per hour", 4.0),
        ],
        ports: &[
            ("queue", "queue", "", "the work waiting; attach to an arrivals queue"),
            ("served", "served", "", "the success sink; attach to share one outcome across services"),
        ],
        build: |p| {
            let mut f = Fragment::default();
            f.place("queue", 0.0, 0.0)
                .place("staff_free", count(p, "staff"), 0.0)
                .place("busy", 0.0, 0.0)
                .place("served", 0.0, 0.0)
                .transition("pickup", Some(720.0))
                .transition("finish", Some(get(p, "per_hour")))
                .non_kinetic("queue", "pickup")
                .arc("staff_free", "pickup", 1.0)
                .arc("pickup", "busy", 1.0)
                .arc("busy", "finish", 1.0)
                .arc("finish", "served", 1.0)
                .arc("finish", "staff_free", 1.0);
            Ok(f)
        },
    },
    Def {
        name: "sir",
        description: "SIR epidemic: Susceptible → Infected → Recovered (go-pflow templates).",
        source: TEMPLATE,
        discipline: &[],
        params: &[
            param("population", "total population", 1000.0),
            param("initial_infected", "infected at the start", 10.0),
        ],
        ports: &[],
        build: |p| template("sir", p),
    },
    Def {
        name: "workflow",
        description: "A sequential workflow of `stages` steps, Stage0 → … → Stage<stages> (go-pflow templates).",
        source: TEMPLATE,
        discipline: &[],
        params: &[
            param("stages", "process steps (2..50)", 3.0),
            param("initial_items", "items at the start", 10.0),
        ],
        ports: &[],
        build: |p| {
            if get(p, "stages") > MAX_STAGES {
                return Err(format!("stages must be at most {MAX_STAGES}"));
            }
            template("workflow", p)
        },
    },
];

/// A pflow-compose template, its coordinates kept but spread ×2 (they are
/// 50 apart, closer than the editor's node size).
fn template(name: &str, p: &Params) -> std::result::Result<Fragment, String> {
    let params: pflow_compose::templates::Params = p.iter().map(|(k, v)| (k.clone(), *v)).collect();
    let net = pflow_compose::templates::generate(name, &params)?;
    let mut f = Fragment::default();
    let mut places: Vec<_> = net.places.iter().collect();
    places.sort_by(|(a, pa), (b, pb)| {
        (pa.x, pa.y, *a)
            .partial_cmp(&(pb.x, pb.y, *b))
            .expect("finite")
    });
    for (id, pl) in places {
        f.places.push(FPlace {
            id: id.clone(),
            label: pl.label_text.clone(),
            initial: pl.initial.first().copied().unwrap_or(0.0),
            capacity: pl.capacity.first().copied().unwrap_or(0.0),
            x: pl.x,
            y: pl.y,
        });
    }
    let mut ts: Vec<_> = net.transitions.iter().collect();
    ts.sort_by(|(a, ta), (b, tb)| {
        (ta.x, ta.y, *a)
            .partial_cmp(&(tb.x, tb.y, *b))
            .expect("finite")
    });
    for (id, t) in ts {
        f.transitions.push(FTransition {
            id: id.clone(),
            label: t.label_text.clone(),
            rate: None,
            x: t.x,
            y: t.y,
        });
    }
    for a in &net.arcs {
        f.arcs.push(FArc {
            source: a.source.clone(),
            target: a.target.clone(),
            weight: a.weight.first().copied().unwrap_or(1.0).max(1.0),
            inhibit: a.inhibit_transition,
            kinetic: true,
        });
    }
    f.scale(2.0);
    Ok(f)
}

/// A pflow-metamodel model (a pflow-compose subnet) as a fragment. Read
/// arcs (place → transition there) become the editor's output-side
/// inhibit arcs; positions come from [`Fragment::auto_layout`].
fn from_model(m: &pflow_metamodel::Model) -> Fragment {
    use pflow_metamodel::ArcType;
    let mut f = Fragment::default();
    for p in &m.places {
        f.place(&p.id, p.initial.max(0) as f64, p.capacity.max(0) as f64);
    }
    for t in &m.transitions {
        f.transition(&t.id, None);
    }
    for a in &m.arcs {
        let w = a.weight.max(1) as f64;
        match a.typ {
            ArcType::Normal => {
                f.arc(&a.from, &a.to, w);
                f.arcs.last_mut().expect("pushed").kinetic = a.kinetic.unwrap_or(true);
            }
            ArcType::Inhibitor => {
                f.inhibitor(&a.from, &a.to, w);
            }
            ArcType::Read => {
                f.inhibitor(&a.to, &a.from, w);
            }
        }
    }
    f
}

fn find(name: &str) -> Result<&'static Def> {
    LIBRARY.iter().find(|d| d.name == name).ok_or_else(|| {
        let names: Vec<&str> = LIBRARY.iter().map(|d| d.name).collect();
        Error::NotFound(format!(
            "no component {name:?}; the library has {}",
            names.join(", ")
        ))
    })
}

/// An instantiated component: its fragment (positioned, at the origin),
/// ports and the full parameter set used.
#[derive(Debug, Clone)]
pub struct Instance {
    pub name: &'static str,
    pub fragment: Fragment,
    pub ports: Vec<PortSpec>,
    pub params: Params,
}

fn build(def: &Def, params: &Params) -> Result<Fragment> {
    let mut f =
        (def.build)(params).map_err(|e| Error::BadArgs(format!("component {}: {e}", def.name)))?;
    if def.source != TEMPLATE {
        f.auto_layout();
    }
    Ok(f)
}

fn ports(def: &Def, f: &Fragment) -> Vec<PortSpec> {
    if def.ports.is_empty() {
        return f
            .places
            .iter()
            .map(|p| PortSpec {
                name: p.id.clone(),
                place: p.id.clone(),
                role: f.role_of(&p.id).into(),
                description: p.label.clone().unwrap_or_default(),
            })
            .collect();
    }
    def.ports
        .iter()
        .map(|(name, place, role, description)| PortSpec {
            name: (*name).into(),
            place: (*place).into(),
            role: if role.is_empty() {
                f.role_of(place).into()
            } else {
                (*role).into()
            },
            description: (*description).into(),
        })
        .collect()
}

/// Builds component `name` with `explicit` parameters over its defaults.
/// Unknown parameters, and values that are negative, non-finite or above
/// [`MAX_PARAM`], are refused.
pub fn instantiate(name: &str, explicit: &Params) -> Result<Instance> {
    let def = find(name)?;
    let mut params: Params = def
        .params
        .iter()
        .map(|p| (p.name.to_string(), p.default))
        .collect();
    for (k, v) in explicit {
        if !params.contains_key(k) {
            let known: Vec<&str> = def.params.iter().map(|p| p.name).collect();
            return Err(Error::BadArgs(format!(
                "component {name:?} has no param {k:?}; it takes {known:?}"
            )));
        }
        if !v.is_finite() || *v < 0.0 || *v > MAX_PARAM {
            return Err(Error::BadArgs(format!(
                "param {k:?} = {v} must be a number in 0..={MAX_PARAM}"
            )));
        }
        params.insert(k.clone(), *v);
    }
    let fragment = build(def, &params)?;
    let ports = ports(def, &fragment);
    Ok(Instance {
        name: def.name,
        fragment,
        ports,
        params,
    })
}

/// sim's refusal of a parameter whose only effect is on an attached port's
/// place (which keeps the existing place as it is, so the value would be
/// dropped without a word): each explicit parameter is nudged by one and
/// the unattached rest of the fragment compared.
pub fn check_attached_params(
    inst: &Instance,
    explicit: &Params,
    attached: &BTreeSet<String>,
) -> Result<()> {
    if attached.is_empty() {
        return Ok(());
    }
    let def = find(inst.name)?;
    let base = inst.fragment.without(attached);
    for k in explicit.keys() {
        let mut nudged = inst.params.clone();
        *nudged.get_mut(k).expect("validated") += 1.0;
        let same = build(def, &nudged)
            .map(|f| f.without(attached) == base)
            .unwrap_or(false);
        if same {
            return Err(Error::BadArgs(format!(
                "param {k:?} of component {:?} only shapes the attached port place(s) {:?}, \
                 which keep the existing place as it is; set it on that place instead",
                inst.name,
                attached.iter().collect::<Vec<_>>()
            )));
        }
    }
    Ok(())
}

/// Every library entry with its ports at default parameters.
pub fn components() -> Vec<ComponentInfo> {
    LIBRARY
        .iter()
        .map(|def| {
            let params: Params = def
                .params
                .iter()
                .map(|p| (p.name.to_string(), p.default))
                .collect();
            let ports = build(def, &params)
                .map(|f| ports(def, &f))
                .unwrap_or_default();
            ComponentInfo {
                name: def.name,
                description: def.description,
                source: def.source,
                discipline: def.discipline,
                ports,
                params: def.params.to_vec(),
            }
        })
        .collect()
}

// ---------------------------------------------------------------- diff

/// One structural difference between two documents.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Change {
    /// `place`, `transition`, `arc` or `net`.
    pub kind: &'static str,
    /// Node id; for an arc `source->target`, with ` (inhibitor)` or
    /// ` (read)` for the inhibit kinds and ` #n` for a further parallel arc.
    pub id: String,
    /// `added`, `removed` or `modified`.
    pub change: &'static str,
    pub before: String,
    pub after: String,
    /// The nodes it touches: the node itself, an arc's endpoints.
    #[serde(skip)]
    pub nodes: Vec<String>,
}

fn trimmed(v: &[f64], weight: bool) -> Vec<f64> {
    let mut v = v.to_vec();
    if weight && v.is_empty() {
        v.push(1.0);
    }
    while v.last() == Some(&0.0) {
        v.pop();
    }
    v
}

fn fmt_vec(v: &[f64]) -> String {
    match v {
        [] => "0".into(),
        [x] => format!("{x}"),
        _ => format!(
            "[{}]",
            v.iter()
                .map(|x| format!("{x}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn fmt_label(l: &Option<String>) -> String {
    match l {
        Some(l) => format!("{l:?}"),
        None => "none".into(),
    }
}

/// Keys of `doc`'s arcs, in arc order (see [`Change::id`]).
pub fn arc_keys(doc: &Document) -> Vec<String> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    doc.arcs
        .iter()
        .map(|a| {
            let mut k = format!("{}->{}", a.source, a.target);
            if a.inhibit {
                k += if doc.kind_of(&a.source) == Some(NodeKind::Transition) {
                    " (read)"
                } else {
                    " (inhibitor)"
                };
            }
            let n = seen.entry(k.clone()).or_default();
            *n += 1;
            if *n > 1 {
                k = format!("{k} #{n}");
            }
            k
        })
        .collect()
}

fn describe_place(p: &Place) -> String {
    let mut s = format!(
        "initial {}, capacity {}",
        fmt_vec(&trimmed(&p.initial, false)),
        fmt_vec(&trimmed(&p.capacity, false))
    );
    if p.label.is_some() {
        s += &format!(", label {}", fmt_label(&p.label));
    }
    s
}

fn describe_transition(t: &Transition) -> String {
    let mut parts = vec![];
    if t.label.is_some() {
        parts.push(format!("label {}", fmt_label(&t.label)));
    }
    if t.role.is_some() {
        parts.push(format!("role {}", t.role()));
    }
    if parts.is_empty() {
        "transition".into()
    } else {
        parts.join(", ")
    }
}

/// Field-wise (before, after) of the fields that differ.
fn fields(diffs: Vec<(&str, String, String)>) -> Option<(String, String)> {
    let diffs: Vec<_> = diffs.into_iter().filter(|(_, a, b)| a != b).collect();
    if diffs.is_empty() {
        return None;
    }
    let side = |after: bool| {
        diffs
            .iter()
            .map(|d| format!("{} {}", d.0, if after { &d.2 } else { &d.1 }))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Some((side(false), side(true)))
}

/// The structural difference from `a` (before) to `b` (after): places,
/// transitions and arcs added or removed, and surviving elements whose
/// initial tokens, capacity, label, role or arc weight changed (per color,
/// trailing zeros ignored), plus a change of token colors. Positions are
/// not compared. Sorted by kind, then id.
pub fn diff(a: &Document, b: &Document) -> Vec<Change> {
    let mut out = vec![];
    if a.token != b.token {
        out.push(Change {
            kind: "net",
            id: "token".into(),
            change: "modified",
            before: format!("{:?}", a.token),
            after: format!("{:?}", b.token),
            nodes: vec![],
        });
    }
    let node = |kind, id: &str, change, before, after| Change {
        kind,
        id: id.to_string(),
        change,
        before,
        after,
        nodes: vec![id.to_string()],
    };
    let mut places = vec![];
    for p in &b.places {
        match a.place(&p.id) {
            None => places.push(node(
                "place",
                &p.id,
                "added",
                String::new(),
                describe_place(p),
            )),
            Some(q) => {
                let n = a.colors().max(b.colors());
                let v = |x: &[f64]| {
                    let mut t = trimmed(x, false);
                    if n > 1 {
                        t.resize(n, 0.0);
                    }
                    fmt_vec(&t)
                };
                if let Some((before, after)) = fields(vec![
                    ("initial", v(&q.initial), v(&p.initial)),
                    ("capacity", v(&q.capacity), v(&p.capacity)),
                    ("label", fmt_label(&q.label), fmt_label(&p.label)),
                ]) {
                    places.push(node("place", &p.id, "modified", before, after));
                }
            }
        }
    }
    for p in &a.places {
        if b.place(&p.id).is_none() {
            places.push(node(
                "place",
                &p.id,
                "removed",
                describe_place(p),
                String::new(),
            ));
        }
    }
    let mut transitions = vec![];
    for t in &b.transitions {
        match a.transition(&t.id) {
            None => transitions.push(node(
                "transition",
                &t.id,
                "added",
                String::new(),
                describe_transition(t),
            )),
            Some(u) => {
                if let Some((before, after)) = fields(vec![
                    ("label", fmt_label(&u.label), fmt_label(&t.label)),
                    ("role", u.role().to_string(), t.role().to_string()),
                ]) {
                    transitions.push(node("transition", &t.id, "modified", before, after));
                }
            }
        }
    }
    for t in &a.transitions {
        if b.transition(&t.id).is_none() {
            transitions.push(node(
                "transition",
                &t.id,
                "removed",
                describe_transition(t),
                String::new(),
            ));
        }
    }
    let ka: BTreeMap<String, &Arc> = arc_keys(a).into_iter().zip(&a.arcs).collect();
    let kb: BTreeMap<String, &Arc> = arc_keys(b).into_iter().zip(&b.arcs).collect();
    let w = |x: &Arc| format!("weight {}", fmt_vec(&trimmed(&x.weight, true)));
    let arc = |k: &str, x: &Arc, change, before, after| Change {
        kind: "arc",
        id: k.to_string(),
        change,
        before,
        after,
        nodes: vec![x.source.clone(), x.target.clone()],
    };
    let mut arcs = vec![];
    for (k, x) in &kb {
        match ka.get(k) {
            None => arcs.push(arc(k, x, "added", String::new(), w(x))),
            Some(y) if w(x) != w(y) => arcs.push(arc(k, x, "modified", w(y), w(x))),
            _ => {}
        }
    }
    for (k, y) in &ka {
        if !kb.contains_key(k) {
            arcs.push(arc(k, y, "removed", w(y), String::new()));
        }
    }
    for group in [&mut places, &mut transitions, &mut arcs] {
        group.sort_by(|x, y| x.id.cmp(&y.id).then(x.change.cmp(y.change)));
    }
    out.extend(places);
    out.extend(transitions);
    out.extend(arcs);
    out
}

/// The diff in sim_diff's shape (`places_added`, …, `has_changes`), the
/// modified entries as `"id: before → after"`.
pub fn sim_shape(changes: &[Change]) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for kind in ["place", "transition", "arc"] {
        for change in ["added", "removed", "modified"] {
            let list: Vec<String> = changes
                .iter()
                .filter(|c| c.kind == kind && c.change == change)
                .map(|c| {
                    if change == "modified" {
                        format!("{}: {} → {}", c.id, c.before, c.after)
                    } else {
                        c.id.clone()
                    }
                })
                .collect();
            m.insert(format!("{kind}s_{change}"), serde_json::json!(list));
        }
    }
    if let Some(c) = changes.iter().find(|c| c.kind == "net") {
        m.insert(
            "token_modified".into(),
            serde_json::json!(format!("{} → {}", c.before, c.after)),
        );
    }
    m.insert("has_changes".into(), serde_json::json!(!changes.is_empty()));
    serde_json::Value::Object(m)
}

// ------------------------------------------------------------- drawing

pub const DRAW_W: f64 = 1000.0;
pub const DRAW_H: f64 = 560.0;
const MARGIN: f64 = 20.0;
const HEADER: f64 = 40.0;
const FOOTER: f64 = 30.0;
const PAD: f64 = 24.0;
const NODE_R: f64 = 10.0;
/// Above this many nodes a side drops its labels, and the drawing its
/// id-matching lines.
const DETAIL_LIMIT: usize = 150;
/// Horizontal room a label needs beside a node on the same row.
const LABEL_GAP: f64 = 70.0;

const ADDED: &str = "#2e9d4f";
const REMOVED: &str = "#d64545";
const MODIFIED: &str = "#e08a00";
const PLAIN: &str = "#777777";
const FAINT: &str = "#bbbbbb";

fn status_color(change: Option<&str>) -> &'static str {
    match change {
        Some("added") => ADDED,
        Some("removed") => REMOVED,
        Some("modified") => MODIFIED,
        _ => PLAIN,
    }
}

/// Maps a document's coordinates into a pane: one scale shared by both
/// sides, each side centered in its own pane.
struct Pane {
    x0: f64,
    y0: f64,
    w: f64,
    h: f64,
    scale: f64,
    cx: f64,
    cy: f64,
}

impl Pane {
    fn map(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.x0 + self.w / 2.0 + (x - self.cx) * self.scale,
            self.y0 + self.h / 2.0 + (y - self.cy) * self.scale,
        )
    }
}

fn span(doc: &Document) -> (f64, f64, f64, f64) {
    doc.bounds().unwrap_or((0.0, 0.0, 0.0, 0.0))
}

/// Shortens segment p→q by `r0` at p and `r1` at q.
fn trim_segment(
    p: (f64, f64),
    q: (f64, f64),
    r0: f64,
    r1: f64,
) -> Option<((f64, f64), (f64, f64))> {
    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len.is_nan() || len <= r0 + r1 {
        return None;
    }
    let (ux, uy) = (dx / len, dy / len);
    Some((
        (p.0 + ux * r0, p.1 + uy * r0),
        (q.0 - ux * r1, q.1 - uy * r1),
    ))
}

fn draw_side(
    shapes: &mut Vec<DrawShape>,
    doc: &Document,
    pane: &Pane,
    status: &BTreeMap<(&str, String), &'static str>,
    detailed: bool,
) {
    // Every node is labeled when they are far enough apart to read;
    // changed ones always are.
    let pts: Vec<(f64, f64)> = doc
        .places
        .iter()
        .map(|p| pane.map(p.x, p.y))
        .chain(doc.transitions.iter().map(|t| pane.map(t.x, t.y)))
        .collect();
    let labels = detailed
        && pts.iter().enumerate().all(|(i, p)| {
            pts[i + 1..]
                .iter()
                .all(|q| (p.0 - q.0).abs() >= LABEL_GAP || (p.1 - q.1).abs() >= 2.5 * NODE_R + 12.0)
        });
    let pos = |id: &str| doc.position(id).map(|(x, y)| pane.map(x, y));
    for (k, a) in arc_keys(doc).iter().zip(&doc.arcs) {
        let (Some(p), Some(q)) = (pos(&a.source), pos(&a.target)) else {
            continue;
        };
        let change = status.get(&("arc", k.clone())).copied();
        let color = status_color(change).to_string();
        let width = if change.is_some() { 2.0 } else { 1.0 };
        let Some((s, e)) = trim_segment(p, q, NODE_R, NODE_R + 1.0) else {
            continue;
        };
        shapes.push(DrawShape::Line {
            points: vec![s, e],
            color: color.clone(),
            width,
            dashed: a.inhibit,
            dash: None,
        });
        let len = ((e.0 - s.0).powi(2) + (e.1 - s.1).powi(2)).sqrt();
        let (ux, uy) = ((e.0 - s.0) / len, (e.1 - s.1) / len);
        if a.inhibit {
            shapes.push(DrawShape::Circle {
                x: e.0 - ux * 3.0,
                y: e.1 - uy * 3.0,
                r: 3.0,
                fill: None,
                stroke: Some(color),
                stroke_width: 1.0,
            });
        } else {
            let (bx, by) = (e.0 - ux * 7.0, e.1 - uy * 7.0);
            shapes.push(DrawShape::Polygon {
                points: vec![
                    e,
                    (bx - uy * 3.5, by + ux * 3.5),
                    (bx + uy * 3.5, by - ux * 3.5),
                ],
                fill: Some(color),
                stroke: None,
                stroke_width: 1.0,
            });
        }
    }
    let mut lone = 0;
    let mut node = |id: &str, kind: &'static str, x: f64, y: f64| {
        let (x, y) = pane.map(x, y);
        let change = status.get(&(kind, id.to_string())).copied();
        let stroke = Some(status_color(change).to_string());
        let stroke_width = if change.is_some() { 3.0 } else { 1.5 };
        if kind == "place" {
            shapes.push(DrawShape::Circle {
                x,
                y,
                r: NODE_R,
                fill: None,
                stroke,
                stroke_width,
            });
        } else {
            shapes.push(DrawShape::Rect {
                x: x - NODE_R * 0.8,
                y: y - NODE_R,
                w: NODE_R * 1.6,
                h: NODE_R * 2.0,
                fill: None,
                stroke,
                radius: 2.0,
                stroke_width,
            });
        }
        if labels || change.is_some() {
            // Crowded: only changed nodes are labeled, alternately below
            // and above so neighbours do not collide.
            let above = !labels && lone % 2 == 1;
            lone += 1;
            let text: String = id.chars().take(18).collect();
            shapes.push(DrawShape::Text {
                x,
                y: if above {
                    y - NODE_R - 5.0
                } else {
                    y + NODE_R + 11.0
                },
                text,
                size: 9.0,
                anchor: "middle".into(),
                color: status_color(change).into(),
                bold: false,
                central: false,
            });
        }
    };
    for p in &doc.places {
        node(&p.id, "place", p.x, p.y);
    }
    for t in &doc.transitions {
        node(&t.id, "transition", t.x, t.y);
    }
}

/// The side-by-side picture of a diff: `a` in the left pane, `b` in the
/// right, each in its own coordinates scaled to fit its pane (never
/// enlarged). Removed
/// elements are red on the left, added green on the right, modified
/// orange on both; nodes with the same id on both sides are joined by a
/// faint line. Returns (width, height, shapes); every coordinate is
/// finite and inside the drawing.
pub fn side_by_side(
    a: (&str, &Document),
    b: (&str, &Document),
    changes: &[Change],
) -> (f64, f64, Vec<DrawShape>) {
    let (name_a, da) = a;
    let (name_b, db) = b;
    let pane_w = (DRAW_W - 3.0 * MARGIN) / 2.0;
    let pane_h = DRAW_H - HEADER - FOOTER - MARGIN;
    let inner = |v: f64| v - 2.0 * PAD - 2.0 * NODE_R;
    let (sa, sb) = (span(da), span(db));
    let fit = |s: (f64, f64, f64, f64)| {
        let fx = inner(pane_w) / (s.2 - s.0).max(1.0);
        let fy = inner(pane_h - 14.0) / (s.3 - s.1).max(1.0);
        fx.min(fy)
    };
    let pane = |i: f64, s: (f64, f64, f64, f64)| Pane {
        x0: MARGIN + i * (pane_w + MARGIN),
        y0: HEADER,
        w: pane_w,
        h: pane_h - 14.0,
        scale: fit(s).min(1.0),
        cx: (s.0 + s.2) / 2.0,
        cy: (s.1 + s.3) / 2.0,
    };
    let (pa, pb) = (pane(0.0, sa), pane(1.0, sb));

    let mut status_a: BTreeMap<(&str, String), &'static str> = BTreeMap::new();
    let mut status_b = status_a.clone();
    for c in changes {
        let key = (c.kind, c.id.clone());
        match c.change {
            "removed" => {
                status_a.insert(key, "removed");
            }
            "added" => {
                status_b.insert(key, "added");
            }
            _ => {
                status_a.insert(key.clone(), "modified");
                status_b.insert(key, "modified");
            }
        }
    }

    let mut shapes = vec![];
    for (p, name, d) in [(&pa, name_a, da), (&pb, name_b, db)] {
        shapes.push(DrawShape::Rect {
            x: p.x0,
            y: p.y0 - 4.0,
            w: p.w,
            h: pane_h,
            fill: None,
            stroke: Some(FAINT.into()),
            radius: 4.0,
            stroke_width: 1.0,
        });
        let title: String = name.chars().take(40).collect();
        shapes.push(DrawShape::Text {
            x: p.x0 + p.w / 2.0,
            y: 24.0,
            text: format!(
                "{title}: {} places, {} transitions, {} arcs",
                d.places.len(),
                d.transitions.len(),
                d.arcs.len()
            ),
            size: 13.0,
            anchor: "middle".into(),
            color: "#444444".into(),
            bold: true,
            central: false,
        });
    }
    let detailed = da.places.len() + da.transitions.len() <= DETAIL_LIMIT
        && db.places.len() + db.transitions.len() <= DETAIL_LIMIT;
    if detailed {
        let ids = da
            .places
            .iter()
            .map(|p| &p.id)
            .chain(da.transitions.iter().map(|t| &t.id));
        for id in ids {
            if da.kind_of(id) != db.kind_of(id) {
                continue;
            }
            let (Some((x0, y0)), Some((x1, y1))) = (da.position(id), db.position(id)) else {
                continue;
            };
            shapes.push(DrawShape::Line {
                points: vec![pa.map(x0, y0), pb.map(x1, y1)],
                color: FAINT.into(),
                width: 0.5,
                dashed: false,
                dash: Some((2.0, 4.0)),
            });
        }
    }
    draw_side(&mut shapes, da, &pa, &status_a, detailed);
    draw_side(&mut shapes, db, &pb, &status_b, detailed);
    for (p, d) in [(&pa, da), (&pb, db)] {
        if d.places.is_empty() && d.transitions.is_empty() {
            shapes.push(DrawShape::Text {
                x: p.x0 + p.w / 2.0,
                y: p.y0 + p.h / 2.0,
                text: "(empty)".into(),
                size: 12.0,
                anchor: "middle".into(),
                color: PLAIN.into(),
                bold: false,
                central: true,
            });
        }
    }
    let count = |c: &str| changes.iter().filter(|x| x.change == c).count();
    let legend = [
        (ADDED, format!("added {}", count("added"))),
        (REMOVED, format!("removed {}", count("removed"))),
        (MODIFIED, format!("modified {}", count("modified"))),
    ];
    for (i, (color, text)) in legend.into_iter().enumerate() {
        let x = MARGIN + 10.0 + i as f64 * 150.0;
        let y = DRAW_H - FOOTER / 2.0;
        shapes.push(DrawShape::Circle {
            x,
            y,
            r: 5.0,
            fill: None,
            stroke: Some(color.into()),
            stroke_width: 3.0,
        });
        shapes.push(DrawShape::Text {
            x: x + 12.0,
            y,
            text,
            size: 11.0,
            anchor: "start".into(),
            color: "#444444".into(),
            bold: false,
            central: true,
        });
    }
    (DRAW_W, DRAW_H, shapes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_component_builds_at_defaults() {
        let all = components();
        assert_eq!(all.len(), LIBRARY.len());
        for c in &all {
            let inst = instantiate(c.name, &Params::new()).unwrap();
            assert!(!inst.fragment.places.is_empty(), "{}", c.name);
            assert!(!c.ports.is_empty(), "{} has ports", c.name);
            for p in &c.ports {
                assert!(
                    inst.fragment.is_place(&p.place),
                    "{}: port {}",
                    c.name,
                    p.name
                );
            }
            for n in inst.fragment.places.iter().map(|p| (p.x, p.y)) {
                assert!(n.0 >= 0.0 && n.1 >= 0.0 && n.0.is_finite() && n.1.is_finite());
            }
        }
        let names: Vec<_> = all.iter().map(|c| c.name).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "library sorted by name");
    }

    #[test]
    fn port_roles_follow_the_arcs() {
        let svc = instantiate("service", &Params::new()).unwrap();
        let role = |n: &str| svc.ports.iter().find(|p| p.name == n).unwrap().role.clone();
        assert_eq!(role("queue"), "in");
        assert_eq!(role("served"), "out");
        let arr = instantiate("arrivals", &Params::new()).unwrap();
        assert_eq!(arr.ports[0].role, "out");
    }

    #[test]
    fn diff_of_identical_documents_is_empty() {
        let d = Document::from_json(include_bytes!("../../../examples/coffee-shop.json")).unwrap();
        assert!(diff(&d, &d).is_empty());
        assert_eq!(sim_shape(&[])["has_changes"], false);
    }
}
