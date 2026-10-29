//! A composition of pattern instances (a pflow-compose [`Bundle`]) and the
//! link rules, flattening and scenario runs the composition views draw
//! (milestone 1 of porting `pflow-views-prototype`'s ten views).
//!
//! Every decision is pflow-rs's: the subnets come from pflow-compose's own
//! constructors ([`new_queue`], [`new_resource_pool`], [`Saga`]'s
//! `to_meta_subnet`), a link's verdict is [`Bundle::validate`] (the
//! go-pflow `validateLink` port, codes and messages included), the flat net
//! is [`Bundle::flatten_with_map`], and a scenario fires through
//! `pflow_metamodel::Model::enabled_why_not` / `fire`. What is this crate's
//! own is labelled as such:
//!
//! - the instance parameters, the scenarios and their predicates are
//!   judgment, copied from the prototype (`js/model/orders.js`);
//! - two **page rules** (`rule: "page"` in a verdict): a link joins two
//!   different subnets (`PAGE_SAME_SUBNET`; `Validate` does not refuse a link
//!   inside one subnet), and the same link is not added twice
//!   (`PAGE_DUPLICATE`, an event link either way round);
//! - [`candidates`]: which ports a drag from a port may reach, the
//!   prototype's `rules.candidates` (first legal kind wins);
//! - the `passed` list of a legal link (which checks it met, with go-pflow
//!   cites), descriptive only: the verdict is `Validate`'s.
//!
//! Messages are pflow-compose's for a one-link bundle, so they carry its
//! label `links[0]`, as the go-pflow receipts do.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use pflow_compose::{
    new_queue, new_resource_pool, Bundle, Endpoint, FlattenMap, Link, LinkKind, PortKind,
    QueueSpec, Saga, Subnet,
};
use pflow_metamodel::{ArcType, Model};
use serde::Serialize;
use serde_json::{json, Value};

pub mod boundaries;
pub mod game;

/// The prototype's ontology facts (patterns, ports, link kinds, idioms,
/// caveats, IRIs) and its sources and bibliography, as one JSON document.
/// Provenance is in its `provenance` field: ontology-pflow-xyz
/// `ns/patterns.ttl`@bc4a996, via pflow-views-prototype `js/model/ontology.js`
/// and `sources.js`.
pub const ONTOLOGY_JSON: &str = include_str!("ontology.json");

pub fn ontology() -> &'static Value {
    static O: OnceLock<Value> = OnceLock::new();
    O.get_or_init(|| serde_json::from_str(ONTOLOGY_JSON).expect("ontology.json is JSON"))
}

pub mod checks;
pub mod content;
pub mod document;
pub mod receipt;

pub const PARAMS_NOTE: &str = "Parameters are this page's choice (judgment): the description fixes the patterns, not their sizes.";

// ------------------------------------------------------------ instances

/// One instance of a pattern, with the parameters it is built from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InstanceSpec {
    pub id: String,
    pub pattern: String,
    pub label: String,
    /// One of the extra instances the palette adds (not in the default
    /// composition).
    pub extra: bool,
    /// The constructor's parameters (judgment).
    pub params: Value,
    pub why: String,
}

fn spec(id: &str, pattern: &str, extra: bool, params: Value, why: &str) -> InstanceSpec {
    InstanceSpec {
        id: id.into(),
        pattern: pattern.into(),
        label: id.into(),
        extra,
        params,
        why: why.into(),
    }
}

fn saga_params(id: &str, customer: i64, shared: bool) -> Value {
    json!({
        "id": id,
        "accounts": {
            "customer": {"initial": customer, "shared": shared},
            "merchant": {"initial": 0, "shared": shared},
        },
        "steps": [
            {"id": "reserve", "name": "reserve stock", "compensation": "release stock", "moves": []},
            {"id": "charge", "name": "charge card", "compensation": "refund",
             "moves": [{"from": "customer", "to": "merchant", "amount": 1}]},
        ],
    })
}

/// The six instances: the default three (orders, workers, payment) and the
/// three extras, in the prototype's order (`INSTANCE_SPECS`, `EXTRA_SPECS`).
pub fn catalog() -> &'static [InstanceSpec] {
    static C: OnceLock<Vec<InstanceSpec>> = OnceLock::new();
    C.get_or_init(|| {
        vec![
            spec(
                "orders",
                "queue",
                false,
                json!({"ID": "orders", "Capacity": 3, "Initial": 2, "Payload": ""}),
                "Bounded (capacity 3) so the capacity invariant exists; two orders waiting so the one-saga-per-order caveat shows up in a run.",
            ),
            spec(
                "workers",
                "resource-pool",
                false,
                json!({"id": "workers", "name": "workers", "capacity": 2}),
                "\"a fixed number of workers\": two.",
            ),
            spec(
                "payment",
                "saga",
                false,
                saga_params("payment", 1, false),
                "Two steps so compensation runs in reverse; charge moves one token, so its compensation is the refund. \"A failed payment must refund\" is fail_commit after the charge.",
            ),
            spec(
                "payment2",
                "saga",
                true,
                saga_params("payment2", 0, true),
                "A second order needs a second saga (one saga is one case). Its accounts are shared, so a token link to payment's accounts makes one balance.",
            ),
            spec(
                "orders2",
                "queue",
                true,
                json!({"ID": "orders2", "Capacity": 3, "Initial": 0, "Payload": ""}),
                "A second queue: out of one into in of the other is a token link (a cospan composite).",
            ),
            spec(
                "workers2",
                "resource-pool",
                true,
                json!({"id": "workers2", "name": "workers2", "capacity": 1}),
                "A second pool: its available place can be token-linked to the first pool's.",
            ),
        ]
    })
}

pub fn instance(id: &str) -> Option<&'static InstanceSpec> {
    catalog().iter().find(|s| s.id == id)
}

/// The default composition's instances.
pub const DEFAULT_INSTANCES: [&str; 3] = ["orders", "workers", "payment"];

fn int(p: &Value, k: &str) -> Result<i64, String> {
    p.get(k)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("parameter {k:?} must be an integer"))
}

fn text(p: &Value, k: &str) -> Result<String, String> {
    p.get(k)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("parameter {k:?} must be a string"))
}

/// The subnet of an instance, from pflow-compose's constructor.
pub fn build(spec: &InstanceSpec) -> Result<Subnet, String> {
    let p = &spec.params;
    match spec.pattern.as_str() {
        "queue" => new_queue(QueueSpec {
            id: text(p, "ID")?,
            capacity: int(p, "Capacity")?,
            initial: int(p, "Initial")?,
            description: String::new(),
        }),
        "resource-pool" => Ok(new_resource_pool(text(p, "id")?, int(p, "capacity")?)),
        "saga" => build_saga(&SagaParams::parse(p)?),
        other => Err(format!("unknown pattern {other:?}")),
    }
}

/// A saga's parameters, as go-pflow's `saga.New(id)` builder takes them:
/// accounts (sorted by id) and steps in order, each with its moves.
#[derive(Debug, Clone, PartialEq)]
pub struct SagaParams {
    pub id: String,
    /// (account id, initial balance, shared).
    pub accounts: Vec<(String, i64, bool)>,
    pub steps: Vec<SagaStep>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SagaStep {
    pub id: String,
    pub name: String,
    pub compensation: String,
    /// (from account, to account, amount).
    pub moves: Vec<(String, String, i64)>,
}

impl SagaParams {
    pub fn parse(p: &Value) -> Result<Self, String> {
        let accounts = p
            .get("accounts")
            .and_then(Value::as_object)
            .ok_or("saga: accounts must be an object")?;
        let mut accts: Vec<(String, i64, bool)> = accounts
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.clone(),
                    int(v, "initial")?,
                    v.get("shared").and_then(Value::as_bool).unwrap_or(false),
                ))
            })
            .collect::<Result<_, String>>()?;
        accts.sort();
        let steps = p
            .get("steps")
            .and_then(Value::as_array)
            .ok_or("saga: steps must be an array")?
            .iter()
            .map(|s| {
                let moves = s
                    .get("moves")
                    .and_then(Value::as_array)
                    .map(|ms| {
                        ms.iter()
                            .map(|m| Ok((text(m, "from")?, text(m, "to")?, int(m, "amount")?)))
                            .collect::<Result<Vec<_>, String>>()
                    })
                    .transpose()?
                    .unwrap_or_default();
                Ok(SagaStep {
                    id: text(s, "id")?,
                    name: text(s, "name").unwrap_or_default(),
                    compensation: text(s, "compensation").unwrap_or_default(),
                    moves,
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(SagaParams {
            id: text(p, "id")?,
            accounts: accts,
            steps,
        })
    }
}

/// The saga subnet: pflow-compose's port of go-pflow's
/// `saga.ToMetaSubnet`, built with its builder (`Saga::builder`):
/// accounts (shared ones start at 0, their balance living in the linked
/// subnet), then the steps with their moves, validated.
pub fn build_saga(p: &SagaParams) -> Result<Subnet, String> {
    let mut b = Saga::builder(p.id.clone());
    for (id, initial, shared) in &p.accounts {
        b = if *shared {
            b.shared_account(id.clone())
        } else {
            b.account(id.clone(), *initial)
        };
    }
    for st in &p.steps {
        let mut s = b.step(st.id.clone());
        if !st.name.is_empty() {
            s = s.name(st.name.clone());
        }
        if !st.compensation.is_empty() {
            s = s.compensate(st.compensation.clone());
        }
        for (from, to, amount) in &st.moves {
            s = s.move_tokens(from.clone(), to.clone(), *amount);
        }
        b = s.done();
    }
    Ok(b.build_validated()?.to_meta_subnet())
}

/// The three idioms of ROADMAP 4.1, instantiated on the default instances
/// (`defaultLinks`).
pub fn default_links() -> Vec<Link> {
    vec![
        event("orders", "dequeue", "workers", "acquire"),
        event("orders", "dequeue", "payment", "do:reserve"),
        event("workers", "release", "payment", "commit"),
    ]
}

fn event(a: &str, ap: &str, b: &str, bp: &str) -> Link {
    link(LinkKind::Event, ep(a, ap), ep(b, bp))
}

pub fn ep(subnet: &str, port: &str) -> Endpoint {
    Endpoint {
        subnet: subnet.into(),
        port: port.into(),
        ..Default::default()
    }
}

pub fn link(kind: LinkKind, from: Endpoint, to: Endpoint) -> Link {
    Link {
        kind,
        from,
        to,
        ..Default::default()
    }
}

pub fn kind_name(k: LinkKind) -> &'static str {
    match k {
        LinkKind::Token => "token",
        LinkKind::Data => "data",
        LinkKind::Event => "event",
        LinkKind::Guard => "guard",
    }
}

pub fn parse_kind(s: &str) -> Result<LinkKind, String> {
    match s {
        "token" => Ok(LinkKind::Token),
        "data" => Ok(LinkKind::Data),
        "event" => Ok(LinkKind::Event),
        "guard" => Ok(LinkKind::Guard),
        other => Err(format!(
            "unknown link kind {other:?} (token, data, event, guard)"
        )),
    }
}

/// `"subnet:port"`, the prototype's port reference (port ids may contain
/// `:`, subnet ids do not).
pub fn port_key(e: &Endpoint) -> String {
    if !e.port.is_empty() || (e.place.is_empty() && e.transition.is_empty()) {
        format!("{}:{}", e.subnet, e.port)
    } else if !e.transition.is_empty() {
        format!("{}:@transition/{}", e.subnet, e.transition)
    } else {
        format!("{}:@place/{}", e.subnet, e.place)
    }
}

/// `"kind:subnet:port->subnet:port"`.
pub fn link_key(l: &Link) -> String {
    format!(
        "{}:{}->{}",
        kind_name(l.kind),
        port_key(&l.from),
        port_key(&l.to)
    )
}

/// An endpoint from `{"subnet", "port"}` or `"subnet:port"`.
pub fn parse_endpoint(v: &Value) -> Result<Endpoint, String> {
    match v {
        Value::String(s) => match s.split_once(':') {
            Some((a, b)) if !a.is_empty() && !b.is_empty() => {
                Ok(if let Some(t) = b.strip_prefix("@transition/") {
                    Endpoint {
                        subnet: a.into(),
                        transition: t.into(),
                        ..Default::default()
                    }
                } else if let Some(p) = b.strip_prefix("@place/") {
                    Endpoint {
                        subnet: a.into(),
                        place: p.into(),
                        ..Default::default()
                    }
                } else {
                    ep(a, b)
                })
            }
            _ => Err(format!("endpoint {s:?} is not \"subnet:port\"")),
        },
        Value::Object(o) => {
            let get = |k: &str| o.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());
            match (get("subnet"), get("port"), get("place"), get("transition")) {
                (Some(a), Some(b), None, None) => Ok(ep(a, b)),
                (Some(a), None, Some(p), None) => Ok(Endpoint {
                    subnet: a.into(),
                    place: p.into(),
                    ..Default::default()
                }),
                (Some(a), None, None, Some(t)) => Ok(Endpoint {
                    subnet: a.into(),
                    transition: t.into(),
                    ..Default::default()
                }),
                _ => Err(format!(
                    "endpoint {v} needs \"subnet\" and one of \"port\", \"place\", \"transition\""
                )),
            }
        }
        other => Err(format!(
            "an endpoint is {{\"subnet\", \"port\"}} or \"subnet:port\", not {other}"
        )),
    }
}

/// A link from `{"kind", "from", "to"}`.
pub fn parse_link(v: &Value) -> Result<Link, String> {
    let kind = v
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("link {v} has no \"kind\""))?;
    Ok(link(
        parse_kind(kind)?,
        parse_endpoint(v.get("from").unwrap_or(&Value::Null))?,
        parse_endpoint(v.get("to").unwrap_or(&Value::Null))?,
    ))
}

pub fn link_json(l: &Link) -> Value {
    json!({
        "kind": kind_name(l.kind),
        "from": endpoint_json(&l.from),
        "to": endpoint_json(&l.to),
    })
}

/// `{subnet, port}`, plus `place` or `transition` when the link names those
/// instead of a port (a raw Bundle's links often do).
pub fn endpoint_json(e: &Endpoint) -> Value {
    let mut v = json!({"subnet": e.subnet, "port": e.port});
    if !e.place.is_empty() {
        v["place"] = json!(e.place);
    }
    if !e.transition.is_empty() {
        v["transition"] = json!(e.transition);
    }
    v
}

// ------------------------------------------------------------ the composition

/// An instance that was asked for and could not be built, or a link that
/// was dropped because one of its ends is such an instance.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Unavailable {
    pub instance: String,
    pub reason: String,
}

/// The open composition: instances (the bundle's subnets, in the order they
/// were added) and links.
#[derive(Debug, Clone)]
pub struct Composition {
    /// Where it came from and what is attached (document form, content).
    pub doc: document::DocState,
    pub specs: Vec<InstanceSpec>,
    pub bundle: Bundle,
    /// Instances asked for but not buildable at this pin (the saga).
    pub unavailable: Vec<Unavailable>,
    /// Links asked for whose ends are not in the composition.
    pub dropped_links: Vec<(Link, String)>,
    /// The token game on the flat net (`bundle.fire` …); part of undo.
    pub game: game::FlatGame,
}

impl Composition {
    /// Builds `instances` from the catalog and adds `links` whose ends exist
    /// (each still validated by [`Composition::add_link`] rules). An
    /// instance that cannot be built is recorded, not fatal.
    pub fn open(instances: &[String], links: &[Link]) -> Result<Self, String> {
        let mut c = Composition {
            doc: document::DocState::default(),
            specs: vec![],
            bundle: Bundle::new("composition"),
            unavailable: vec![],
            dropped_links: vec![],
            game: game::FlatGame::default(),
        };
        for id in instances {
            if let Err(e) = c.add_instance(id) {
                if c.specs.iter().any(|s| &s.id == id) || instance(id).is_none() {
                    return Err(e);
                }
                c.unavailable.push(Unavailable {
                    instance: id.clone(),
                    reason: e,
                });
            }
        }
        for l in links {
            let missing: Vec<&str> = [&l.from.subnet, &l.to.subnet]
                .into_iter()
                .filter(|s| c.bundle.subnet_by_id(s).is_none())
                .map(String::as_str)
                .collect();
            if !missing.is_empty() {
                c.dropped_links.push((
                    l.clone(),
                    format!("not in the composition: {}", missing.join(", ")),
                ));
                continue;
            }
            c.add_link(l.clone()).map_err(|v| v.refusal())?;
        }
        c.doc.attach_library_for(instances);
        Ok(c)
    }

    pub fn subnet(&self, id: &str) -> Option<&Subnet> {
        self.bundle.subnet_by_id(id)
    }

    pub fn spec(&self, id: &str) -> Option<&InstanceSpec> {
        self.specs.iter().find(|s| s.id == id)
    }

    pub fn subnets(&self) -> Vec<&Subnet> {
        self.bundle.subnets.iter().collect()
    }

    /// Adds a catalog instance.
    pub fn add_instance(&mut self, id: &str) -> Result<(), String> {
        let spec = instance(id).ok_or_else(|| {
            let ids: Vec<&str> = catalog().iter().map(|s| s.id.as_str()).collect();
            format!("no instance {id:?}; the catalog has {ids:?}")
        })?;
        if self.spec(id).is_some() {
            return Err(format!("instance {id:?} is already in the composition"));
        }
        let sub = build(spec)?;
        self.bundle.add_subnet(sub);
        self.specs.push(spec.clone());
        self.unavailable.retain(|u| u.instance != id);
        Ok(())
    }

    /// Removes an instance and every link touching it; returns those links.
    pub fn remove_instance(&mut self, id: &str) -> Result<Vec<Link>, String> {
        if self.spec(id).is_none() {
            return Err(format!("instance {id:?} is not in the composition"));
        }
        self.specs.retain(|s| s.id != id);
        self.bundle.subnets.retain(|s| s.id != id);
        let (gone, kept): (Vec<Link>, Vec<Link>) = std::mem::take(&mut self.bundle.links)
            .into_iter()
            .partition(|l| l.from.subnet == id || l.to.subnet == id);
        self.bundle.links = kept;
        Ok(gone)
    }

    /// The verdict on `l` against this composition, page rules included
    /// (duplicate: against the links already there).
    pub fn verdict(&self, l: &Link) -> Verdict {
        let mut v = validate_link(&self.subnets(), l);
        if v.ok && self.has_link(l) {
            v.ok = false;
            v.errors
                .push(Issue::page("PAGE_DUPLICATE", "already linked"));
        }
        v
    }

    fn has_link(&self, l: &Link) -> bool {
        let key = link_key(l);
        let rev = link_key(&link(l.kind, l.to.clone(), l.from.clone()));
        self.bundle
            .links
            .iter()
            .any(|x| link_key(x) == key || (l.kind == LinkKind::Event && link_key(x) == rev))
    }

    /// Validates, then adds; the refused verdict otherwise.
    pub fn add_link(&mut self, l: Link) -> Result<Verdict, Verdict> {
        let v = self.verdict(&l);
        if !v.ok {
            return Err(v);
        }
        self.bundle.add_link(l);
        Ok(v)
    }

    /// The scenarios this composition's content declares (none when bare):
    /// what the token game, `bundle.scenarios`, `bundle.run` and the views read.
    pub fn declared_scenarios(&self) -> Vec<content::DeclaredScenario> {
        self.doc
            .content
            .as_ref()
            .map(content::ModelContent::scenarios)
            .unwrap_or_default()
    }

    pub fn pattern_of(&self, subnet: &str) -> Option<&str> {
        self.spec(subnet).map(|s| s.pattern.as_str())
    }

    /// The idiom (`IDIOMS`) a link instantiates, if any: same kind and the
    /// same pattern ports (`do:<first>` is the first step's), either way
    /// round for an event link.
    pub fn idiom_of(&self, l: &Link) -> Option<&'static Value> {
        let (fs, ts) = (self.spec(&l.from.subnet)?, self.spec(&l.to.subnet)?);
        let first_do = |s: &InstanceSpec| -> Option<String> {
            let id = s.params.get("steps")?.get(0)?.get("id")?.as_str()?;
            Some(format!("do:{id}"))
        };
        let matches = |end: &Value, s: &InstanceSpec, port: &str| -> bool {
            let pat = end.get("pattern").and_then(Value::as_str);
            let p = end.get("port").and_then(Value::as_str).unwrap_or("");
            pat == Some(s.pattern.as_str())
                && (p == port || (p == "do:<first>" && first_do(s).as_deref() == Some(port)))
        };
        ontology()["idioms"].as_array()?.iter().find(|idm| {
            if idm["kind"].as_str() != Some(kind_name(l.kind)) {
                return false;
            }
            (matches(&idm["from"], fs, &l.from.port) && matches(&idm["to"], ts, &l.to.port))
                || (l.kind == LinkKind::Event
                    && matches(&idm["from"], ts, &l.to.port)
                    && matches(&idm["to"], fs, &l.from.port))
        })
    }

    /// Links with their index, key and idiom.
    pub fn links_json(&self) -> Vec<Value> {
        self.bundle
            .links
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let idm = self.idiom_of(l);
                let mut v = link_json(l);
                v["index"] = json!(i);
                v["key"] = json!(link_key(l));
                v["idiom"] = idm.map_or(Value::Null, |x| x["key"].clone());
                v["leak"] = json!(idm.is_some_and(|x| x["leak"].as_bool() == Some(true)));
                v
            })
            .collect()
    }

    /// The whole state, as `bundle.state` returns it.
    pub fn to_json(&self) -> Value {
        let subnets: Vec<Value> = self
            .specs
            .iter()
            .filter_map(|s| self.subnet(&s.id).map(|sub| subnet_json(s, sub)))
            .collect();
        let mut out = json!({
            "instances": self.specs.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
            "subnets": subnets,
            "links": self.links_json(),
            "unavailable": self.unavailable,
            "dropped_links": self.dropped_links.iter().map(|(l, why)| json!({"link": link_json(l), "reason": why})).collect::<Vec<_>>(),
            "document": document::document_json(self),
            "capabilities": document::capabilities(self),
            "missing": document::missing(self),
            "content": document::content_json(self),
        });
        // "This page's choice": true of the catalog's parameters only, not of a loaded document's.
        if self.doc.origin == document::Origin::Catalog {
            out["params_note"] = json!(PARAMS_NOTE);
        }
        out
    }
}

/// The ontology port key (`queue-port-in`, `saga-port-do`, …) a subnet port
/// instantiates.
pub fn pattern_port(pattern: &str, port: &str) -> Option<String> {
    let p = &ontology()["patterns"][pattern];
    let all = p["ports"]
        .as_array()
        .into_iter()
        .chain(p["portTemplates"].as_array())
        .flatten();
    for q in all {
        if q["id"].as_str() == Some(port) {
            return q["key"].as_str().map(str::to_string);
        }
        if let Some(t) = q["idTemplate"].as_str() {
            if let (Some(i), Some(j)) = (t.find('<'), t.find('>')) {
                let (pre, post) = (&t[..i], &t[j + 1..]);
                if port.len() > pre.len() + post.len()
                    && port.starts_with(pre)
                    && port.ends_with(post)
                {
                    return q["key"].as_str().map(str::to_string);
                }
            }
        }
    }
    None
}

fn port_kind(k: Option<PortKind>) -> &'static str {
    match k {
        Some(PortKind::In) => "in",
        Some(PortKind::Out) => "out",
        Some(PortKind::Inout) => "inout",
        Some(PortKind::Observe) => "observe",
        None => "",
    }
}

pub fn arc_type(t: ArcType) -> &'static str {
    match t {
        ArcType::Normal => "",
        ArcType::Inhibitor => "inhibitor",
        ArcType::Read => "read",
    }
}

fn place_json(p: &pflow_metamodel::Place) -> Value {
    json!({
        "id": p.id,
        "kind": if p.is_token() { "token" } else { "data" },
        "initial": p.initial,
        "capacity": p.capacity,
        "exported": p.exported,
        "resource": p.resource,
        "description": p.description,
    })
}

fn arcs_json(m: &Model) -> Vec<Value> {
    m.arcs
        .iter()
        .map(|a| json!({"from": a.from, "to": a.to, "weight": a.weight, "type": arc_type(a.typ)}))
        .collect()
}

fn constraints_json(m: &Model) -> Vec<Value> {
    m.constraints
        .iter()
        .map(|c| json!({"id": c.id, "expr": c.expr}))
        .collect()
}

/// A subnet as the views draw it: the receipt's shape (`tools/gocheck`)
/// plus the instance's label, pattern and parameters, and each port's
/// ontology key.
pub fn subnet_json(spec: &InstanceSpec, s: &Subnet) -> Value {
    let ports: Vec<Value> = s
        .ports
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "kind": port_kind(p.kind),
                "target": if p.is_transition() { "transition" } else { "place" },
                "place": p.place,
                "transition": p.transition,
                "schema": p.schema,
                "pattern_port": pattern_port(&spec.pattern, &p.id),
            })
        })
        .collect();
    json!({
        "id": s.id,
        "label": spec.label,
        "pattern": spec.pattern,
        "extra": spec.extra,
        "params": spec.params,
        "why": spec.why,
        "net_type": s.net_type.to_string(),
        "model_name": s.model.name,
        "ports": ports,
        "places": s.model.places.iter().map(place_json).collect::<Vec<_>>(),
        "transitions": s.model.transitions.iter().map(|t| json!({"id": t.id, "description": t.description})).collect::<Vec<_>>(),
        "arcs": arcs_json(&s.model),
        "constraints": constraints_json(&s.model),
    })
}

// ------------------------------------------------------------ verdicts

/// A refusal or warning, with whose rule it is.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Issue {
    pub code: String,
    pub message: String,
    /// `"pflow-compose"` (go-pflow's `validateLink`, ported) or `"page"`.
    pub rule: &'static str,
}

impl Issue {
    fn page(code: &str, message: impl Into<String>) -> Self {
        Issue {
            code: code.into(),
            message: message.into(),
            rule: "page",
        }
    }
}

/// A cite into go-pflow, as the prototype's `rules.js` gives them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Cite {
    pub repo: &'static str,
    pub rev: &'static str,
    pub path: &'static str,
    pub from: u32,
    pub to: u32,
    pub note: &'static str,
}

const fn go(path: &'static str, from: u32, to: u32, note: &'static str) -> Cite {
    Cite {
        repo: "go-pflow",
        rev: "69257b3",
        path,
        from,
        to,
        note,
    }
}

const CITE_ENDPOINT: Cite = go(
    "metamodel/compose.go",
    635,
    659,
    "endpoint kinds must match the link kind",
);
const CITE_LEGAL: Cite = go(
    "metamodel/compose_matrix.go",
    16,
    57,
    "linkLegal: the net-type x link-kind matrix",
);
const CITE_SCHEMA: Cite = go(
    "metamodel/compose.go",
    667,
    674,
    "port schema tags must agree when both are set",
);
const CITE_DIRECTION: Cite = go(
    "metamodel/compose.go",
    676,
    683,
    "token link: from not an in-port, to not an out-port",
);
const CITE_OBSERVER: Cite = go(
    "metamodel/compose.go",
    734,
    748,
    "validateDataLinkObserver: the observer moves nothing on the place",
);
const CITE_GUARD: Cite = go(
    "metamodel/compose.go",
    687,
    706,
    "guard link: condition, lowering, W_RESTRICTIVE_LINK",
);
pub const CITE_TWO_SUBNETS: Cite = go(
    "metamodel/compose.go",
    175,
    176,
    "Link connects two subnets (page rule; Validate does not check it)",
);

/// A check a legal link met (descriptive: the verdict is `Validate`'s).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Passed {
    pub text: String,
    pub cite: Cite,
}

/// pflow-compose's verdict on one link, plus the page rules.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Verdict {
    pub ok: bool,
    pub kind: &'static str,
    pub errors: Vec<Issue>,
    pub warnings: Vec<Issue>,
    pub passed: Vec<Passed>,
}

impl Verdict {
    pub fn codes(&self) -> Vec<&str> {
        self.errors.iter().map(|e| e.code.as_str()).collect()
    }

    /// `[CODE] message; …`, the error a refused edit returns.
    pub fn refusal(&self) -> String {
        self.errors
            .iter()
            .map(|e| format!("[{}] {}", e.code, e.message))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// The label pflow-compose gives the one link of the bundle a verdict is
/// computed on.
pub const LABEL: &str = "links[0]";

/// The verdict on `l` over `subnets`: the same-subnet page rule, then
/// pflow-compose's `Bundle::validate` on a bundle of the subnets `l` names
/// and `l` alone. Errors are all of `Validate`'s; warnings, the link's own.
pub fn validate_link(subnets: &[&Subnet], l: &Link) -> Verdict {
    let mut v = Verdict {
        ok: true,
        kind: kind_name(l.kind),
        errors: vec![],
        warnings: vec![],
        passed: vec![],
    };
    if l.from.subnet == l.to.subnet {
        v.ok = false;
        v.errors.push(Issue::page(
            "PAGE_SAME_SUBNET",
            format!(
                "{LABEL}: both ends are on subnet {:?}; a link connects two subnets",
                l.from.subnet
            ),
        ));
        return v;
    }
    let mut b = Bundle::new("verdict");
    for s in subnets {
        if s.id == l.from.subnet || s.id == l.to.subnet {
            b.add_subnet((*s).clone());
        }
    }
    b.add_link(l.clone());
    let res = b.validate();
    let rule = "pflow-compose";
    v.errors = res
        .errors
        .into_iter()
        .map(|e| Issue {
            code: e.code,
            message: e.message,
            rule,
        })
        .collect();
    v.warnings = res
        .warnings
        .into_iter()
        .filter(|w| w.element == LABEL)
        .map(|e| Issue {
            code: e.code,
            message: e.message,
            rule,
        })
        .collect();
    v.ok = v.errors.is_empty();
    if v.ok {
        v.passed = passed(&b, l);
    }
    v
}

/// What a legal link met, by kind (`rules.js`'s `passed`).
fn passed(b: &Bundle, l: &Link) -> Vec<Passed> {
    let (Some(fs), Some(ts)) = (b.subnet_by_id(&l.from.subnet), b.subnet_by_id(&l.to.subnet))
    else {
        return vec![];
    };
    let (fp, tp) = (fs.port_by_id(&l.from.port), ts.port_by_id(&l.to.port));
    let p = |text: String, cite: Cite| Passed { text, cite };
    let mut out = vec![p(
        match l.kind {
            LinkKind::Token | LinkKind::Data => "both ends are place ports".into(),
            LinkKind::Event => "both ends are transition ports".into(),
            LinkKind::Guard => "from is the gated transition, to is the observed place".into(),
        },
        CITE_ENDPOINT,
    )];
    out.push(p(
        format!(
            "net kinds allow a {} link: {} → {}",
            kind_name(l.kind),
            fs.net_type,
            ts.net_type
        ),
        CITE_LEGAL,
    ));
    if let (Some(a), Some(c)) = (fp, tp) {
        if !a.schema.is_empty() && !c.schema.is_empty() {
            out.push(p(format!("schemas agree: {}", a.schema), CITE_SCHEMA));
        }
        match l.kind {
            LinkKind::Token => out.push(p(
                format!(
                    "direction: from {}, to {}",
                    port_kind(a.kind),
                    port_kind(c.kind)
                ),
                CITE_DIRECTION,
            )),
            LinkKind::Data => out.push(p(
                "the observer moves nothing on the observed place".into(),
                CITE_OBSERVER,
            )),
            LinkKind::Guard => out.push(p(
                "condition \"> 0\" lowers to a read arc (structural)".into(),
                CITE_GUARD,
            )),
            LinkKind::Event => {}
        }
    }
    out
}

/// One row of [`all_verdicts`], the shape `tools/gocheck` writes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VerdictRow {
    pub kind: &'static str,
    pub from: String,
    pub to: String,
    pub ok: bool,
    pub codes: Vec<String>,
    pub warnings: Vec<String>,
    /// The errors' then the link warnings' messages.
    pub messages: Vec<String>,
}

pub const KINDS: [LinkKind; 4] = [
    LinkKind::Token,
    LinkKind::Data,
    LinkKind::Event,
    LinkKind::Guard,
];

/// Every ordered pair of ports on different subnets × every link kind,
/// subnets by id (`allVerdicts`). No page rule applies (pairs are on two
/// subnets and nothing is linked yet).
pub fn all_verdicts(subnets: &[&Subnet]) -> Vec<VerdictRow> {
    let mut all: Vec<&Subnet> = subnets.to_vec();
    all.sort_by(|a, b| a.id.cmp(&b.id));
    let mut rows = vec![];
    for a in &all {
        for pa in &a.ports {
            for b in &all {
                for pb in &b.ports {
                    if a.id == b.id {
                        continue;
                    }
                    for kind in KINDS {
                        let l = link(kind, ep(&a.id, &pa.id), ep(&b.id, &pb.id));
                        let v = validate_link(subnets, &l);
                        rows.push(VerdictRow {
                            kind: kind_name(kind),
                            from: port_key(&l.from),
                            to: port_key(&l.to),
                            ok: v.ok,
                            codes: v.errors.iter().map(|e| e.code.clone()).collect(),
                            warnings: v.warnings.iter().map(|e| e.code.clone()).collect(),
                            messages: v
                                .errors
                                .iter()
                                .chain(&v.warnings)
                                .map(|e| e.message.clone())
                                .collect(),
                        });
                    }
                }
            }
        }
    }
    rows
}

/// Legal links among `subnets` by kind, over every ordered port pair.
pub fn legal_link_counts(subnets: &[&Subnet]) -> BTreeMap<&'static str, usize> {
    let mut n: BTreeMap<&'static str, usize> = KINDS.iter().map(|k| (kind_name(*k), 0)).collect();
    for r in all_verdicts(subnets) {
        if r.ok {
            *n.get_mut(r.kind).expect("a kind") += 1;
        }
    }
    n
}

// ------------------------------------------------------------ candidates

/// The kinds worth trying from a port to a port, in the order "auto"
/// prefers; `Guard` with `true` is a guard oriented transition → place,
/// the drag having started at the place.
fn kinds_for(from_transition: bool, to_transition: bool) -> Vec<(LinkKind, bool)> {
    match (from_transition, to_transition) {
        (true, true) => vec![(LinkKind::Event, false)],
        (true, false) => vec![(LinkKind::Guard, false)],
        (false, true) => vec![(LinkKind::Guard, true)],
        (false, false) => vec![(LinkKind::Token, false), (LinkKind::Data, false)],
    }
}

/// Every other port of the composition, judged as the far end of a link
/// dragged from `from` (`rules.candidates`). `prefer` is `"auto"` or one
/// link kind. For each target the first legal kind wins; when none is
/// legal the first kind tried is kept, so a view can say why.
pub fn candidates(c: &Composition, from: &Endpoint, prefer: &str) -> Result<Vec<Value>, String> {
    let prefer_kind = match prefer {
        "auto" => None,
        k => Some(parse_kind(k)?),
    };
    let fs = c
        .subnet(&from.subnet)
        .ok_or_else(|| format!("no subnet {:?} in the composition", from.subnet))?;
    let fp = fs
        .port_by_id(&from.port)
        .ok_or_else(|| format!("subnet {:?} has no port {:?}", from.subnet, from.port))?;
    let mut out = vec![];
    for s in &c.bundle.subnets {
        for tp in &s.ports {
            if s.id == from.subnet && tp.id == from.port {
                continue;
            }
            let to = ep(&s.id, &tp.id);
            let mut kinds = kinds_for(fp.is_transition(), tp.is_transition());
            if let Some(k) = prefer_kind {
                kinds.retain(|(x, _)| *x == k);
                if kinds.is_empty() {
                    kinds = vec![(k, false)];
                }
            }
            let mut tried: Vec<(Link, Verdict)> = vec![];
            let mut best: Option<usize> = None;
            for (kind, swap) in kinds {
                let l = if swap {
                    link(kind, to.clone(), from.clone())
                } else {
                    link(kind, from.clone(), to.clone())
                };
                let v = c.verdict(&l);
                let ok = v.ok;
                tried.push((l, v));
                if ok {
                    best = Some(tried.len() - 1);
                    break;
                }
            }
            let (bl, bv) = &tried[best.unwrap_or(0)];
            out.push(json!({
                "ref": {"subnet": s.id, "port": tp.id},
                "key": port_key(&to),
                "ok": bv.ok,
                "link": link_json(bl),
                "verdict": bv,
                "tried": tried.iter().map(|(l, v)| json!({"link": link_json(l), "verdict": v})).collect::<Vec<_>>(),
            }));
        }
    }
    Ok(out)
}

/// The rule card for an accepted link (`ruleFor`): the link kind's
/// ontology entry and the checks it met.
pub fn rule_for(l: &Link, v: &Verdict) -> Value {
    let lk = &ontology()["link_kinds"][kind_name(l.kind)];
    json!({
        "kind": kind_name(l.kind),
        "iri": lk["iri"],
        "comment": lk["comment"],
        "passed": v.passed,
        "warnings": v.warnings,
        "cites": lk["cites"],
    })
}

// ------------------------------------------------------------ flatten

/// The flat net and its map, from [`Bundle::flatten_with_map`].
pub struct Flat {
    pub model: Model,
    pub map: FlattenMap,
}

pub fn flatten(b: &Bundle) -> Result<Flat, String> {
    b.flatten_with_map().map(|(model, map)| Flat { model, map })
}

impl Flat {
    /// The flat transition member `"<subnet>/<transition>"` ended up in.
    pub fn transition_of(&self, member: &str) -> Option<&str> {
        let (s, t) = member.split_once('/')?;
        self.map.transition.get(s)?.get(t).map(String::as_str)
    }

    pub fn place_of(&self, member: &str) -> Option<&str> {
        let (s, p) = member.split_once('/')?;
        self.map.place.get(s)?.get(p).map(String::as_str)
    }

    /// Members (`subnet/local`) of each flat transition, sorted.
    pub fn transition_members(&self) -> BTreeMap<String, Vec<String>> {
        invert(&self.map.transition)
    }

    pub fn place_members(&self) -> BTreeMap<String, Vec<String>> {
        invert(&self.map.place)
    }

    pub fn to_json(&self) -> Value {
        let tm = self.transition_members();
        let pm = self.place_members();
        let places: Vec<Value> = self
            .model
            .places
            .iter()
            .map(|p| {
                let mut v = place_json(p);
                v["members"] = json!(pm.get(&p.id).cloned().unwrap_or_default());
                v
            })
            .collect();
        let transitions: Vec<Value> = self
            .model
            .transitions
            .iter()
            .map(|t| {
                json!({
                    "id": t.id,
                    "description": t.description,
                    "members": tm.get(&t.id).cloned().unwrap_or_default(),
                    "fused": self.map.fused_groups.contains_key(&t.id),
                })
            })
            .collect();
        let mk = self.model.initial_marking();
        let enablement: Vec<Value> = self
            .model
            .transitions
            .iter()
            .map(|t| match self.model.enabled_why_not(&t.id, &mk) {
                Ok(()) => json!({"transition": t.id, "enabled": true, "why": ""}),
                Err(e) => json!({"transition": t.id, "enabled": false, "why": e.to_string()}),
            })
            .collect();
        json!({
            "ok": true,
            "error": "",
            "places": places,
            "transitions": transitions,
            "arcs": arcs_json(&self.model),
            "constraints": constraints_json(&self.model),
            "wires": sorted(&self.map.wires),
            "fused_groups": sorted(&self.map.fused_groups),
            "place_map": nested(&self.map.place),
            "transition_map": nested(&self.map.transition),
            "warnings": self.map.warnings.iter().map(|w| json!({"code": w.code, "element": w.element, "message": w.message})).collect::<Vec<_>>(),
            "initial_enablement": enablement,
        })
    }
}

fn invert(
    m: &std::collections::HashMap<String, std::collections::HashMap<String, String>>,
) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (s, locals) in m {
        for (local, flat) in locals {
            out.entry(flat.clone())
                .or_default()
                .push(format!("{s}/{local}"));
        }
    }
    for v in out.values_mut() {
        v.sort();
    }
    out
}

fn sorted(m: &std::collections::HashMap<String, Vec<String>>) -> Value {
    let b: BTreeMap<&String, &Vec<String>> = m.iter().collect();
    json!(b)
}

fn nested(
    m: &std::collections::HashMap<String, std::collections::HashMap<String, String>>,
) -> Value {
    let b: BTreeMap<&String, BTreeMap<&String, &String>> =
        m.iter().map(|(k, v)| (k, v.iter().collect())).collect();
    json!(b)
}

/// `bundle.flatten`'s result: the flat net, or `{ok: false, error}` with
/// pflow-compose's refusal.
pub fn flatten_json(b: &Bundle) -> Value {
    match flatten(b) {
        Ok(f) => f.to_json(),
        Err(e) => json!({"ok": false, "error": e}),
    }
}

// ------------------------------------------------------------ scenarios

/// A predicate on the marking at the end of a scenario.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Expect {
    pub place: &'static str,
    pub op: &'static str,
    pub n: i64,
    pub says: &'static str,
}

/// A firing script in member transitions (`"<subnet>/<transition>"`), with
/// its expected predicates. Judgment, frozen (ROADMAP Phase 4).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scenario {
    pub id: &'static str,
    pub title: &'static str,
    pub script: Vec<&'static str>,
    pub expect: Vec<Expect>,
    #[serde(skip_serializing_if = "str::is_empty")]
    pub note: &'static str,
}

/// The orders library's four scenarios (`SCENARIOS`), derived from the
/// library content (`library/orders.content.json`, the one place they are
/// written) and parsed once.
pub fn scenarios() -> &'static [Scenario] {
    static S: OnceLock<Vec<Scenario>> = OnceLock::new();
    S.get_or_init(|| {
        // Leaked once: the static list lives as long as the process.
        let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
        content::library_entry("orders")
            .map(|l| l.content.scenarios())
            .unwrap_or_default()
            .into_iter()
            .map(|d| Scenario {
                id: leak(d.id),
                title: leak(d.title),
                script: d.script.into_iter().map(leak).collect(),
                expect: d
                    .expect
                    .into_iter()
                    .map(|e| Expect {
                        place: leak(e.place),
                        op: leak(e.op),
                        n: e.n,
                        says: leak(e.says),
                    })
                    .collect(),
                note: leak(d.note),
            })
            .collect()
    })
}

pub fn scenario(id: &str) -> Option<&'static Scenario> {
    scenarios().iter().find(|s| s.id == id)
}

/// One step of a run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunStep {
    pub member: String,
    /// The flat transition it resolved to (`null`: no such member).
    pub flat: Option<String>,
    pub enabled: bool,
    /// pflow-metamodel's reason when not enabled.
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExpectResult {
    pub place: String,
    pub flat: Option<String>,
    pub op: String,
    pub n: i64,
    pub says: String,
    pub actual: i64,
    pub pass: bool,
}

/// A script fired on the flat net (the receipt's scenario shape).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Run {
    pub id: String,
    pub title: String,
    pub steps: Vec<RunStep>,
    pub completed: bool,
    /// Index of the step that could not fire, or -1.
    pub stopped_at: i64,
    pub stopped_step: Option<String>,
    pub final_marking: BTreeMap<String, i64>,
    /// The marking before the first step, then after each step that fired
    /// (the Process view reads token counts off them).
    pub markings: Vec<BTreeMap<String, i64>>,
    pub expect: Vec<ExpectResult>,
}

/// Fires `script` from the flat net's initial marking through
/// `pflow_metamodel::Model::enabled_why_not` / `fire`, stopping at the
/// first member that is not in the net or cannot fire; then evaluates
/// `expect` on the marking reached.
pub fn run(flat: &Flat, id: &str, title: &str, script: &[String], expect: &[Expect]) -> Run {
    let mut mk = flat.model.initial_marking();
    let mut markings: Vec<BTreeMap<String, i64>> =
        vec![mk.iter().map(|(k, v)| (k.clone(), *v)).collect()];
    let mut steps = vec![];
    let mut stopped_at = -1;
    for (i, member) in script.iter().enumerate() {
        let t = flat.transition_of(member).map(str::to_string);
        let why = match &t {
            None => Some(format!("no member {member:?} in the composition")),
            Some(t) => flat
                .model
                .enabled_why_not(t, &mk)
                .err()
                .map(|e| e.to_string()),
        };
        steps.push(RunStep {
            member: member.clone(),
            flat: t.clone(),
            enabled: why.is_none(),
            why: why.clone().unwrap_or_default(),
        });
        if why.is_some() {
            stopped_at = i as i64;
            break;
        }
        mk = flat.model.fire(t.as_deref().expect("resolved"), &mk);
        markings.push(mk.iter().map(|(k, v)| (k.clone(), *v)).collect());
    }
    let expect = expect
        .iter()
        .map(|e| {
            let f = flat.place_of(e.place).map(str::to_string);
            let actual = f.as_ref().and_then(|p| mk.get(p)).copied().unwrap_or(0);
            ExpectResult {
                place: e.place.into(),
                flat: f,
                op: e.op.into(),
                n: e.n,
                says: e.says.into(),
                actual,
                pass: e.op == "==" && actual == e.n,
            }
        })
        .collect();
    Run {
        id: id.into(),
        title: title.into(),
        stopped_step: (stopped_at >= 0).then(|| script[stopped_at as usize].clone()),
        completed: stopped_at < 0,
        stopped_at,
        steps,
        final_marking: mk.into_iter().collect(),
        markings,
        expect,
    }
}

pub fn run_scenario(flat: &Flat, sc: &Scenario) -> Run {
    let script: Vec<String> = sc.script.iter().map(|s| s.to_string()).collect();
    run(flat, sc.id, sc.title, &script, &sc.expect)
}

// ------------------------------------------------------------ incidence

/// The incidence matrix C = post − pre of a model (rows: places, columns:
/// transitions, in model order). Read and inhibitor arcs move nothing.
pub fn incidence(m: &Model) -> Vec<Vec<i64>> {
    let mut c = vec![vec![0i64; m.transitions.len()]; m.places.len()];
    let pi = |id: &str| m.places.iter().position(|p| p.id == id);
    let ti = |id: &str| m.transitions.iter().position(|t| t.id == id);
    for a in &m.arcs {
        if a.is_read_only() {
            continue;
        }
        if let (Some(p), Some(t)) = (pi(&a.from), ti(&a.to)) {
            c[p][t] -= a.weight;
        } else if let (Some(t), Some(p)) = (ti(&a.from), pi(&a.to)) {
            c[p][t] += a.weight;
        }
    }
    c
}

/// A declared constraint read as a P-invariant, with the numbers behind its
/// check.
#[derive(Debug, Clone, PartialEq)]
pub struct InvariantCheck {
    /// y: the coefficient of each named place (a place named twice counts twice).
    pub y: Vec<(String, i64)>,
    /// The declared constant n.
    pub n: i64,
    /// (y·C)[t] for every transition (column order of C).
    pub y_c: Vec<i64>,
    /// y·M₀.
    pub y_m0: i64,
}

impl InvariantCheck {
    /// y·C = 0 and y·M₀ = n.
    pub fn holds(&self) -> bool {
        self.y_c.iter().all(|v| *v == 0) && self.y_m0 == self.n
    }
}

/// A declared constraint read as a P-invariant: `tokens("a") + … == n`
/// gives y (1 on each named place). `None` when the expression is not a
/// plain sum of tokens(…) equal to an integer, or names a place the net
/// lacks.
pub fn invariant_check(m: &Model, c: &[Vec<i64>], expr: &str) -> Option<InvariantCheck> {
    let (lhs, rhs) = expr.rsplit_once(" == ")?;
    let n: i64 = rhs.trim().parse().ok()?;
    let mut y = vec![0i64; m.places.len()];
    let mut named: Vec<(String, i64)> = vec![];
    for term in lhs.split(" + ") {
        let p = term.trim().strip_prefix("tokens(\"")?.strip_suffix("\")")?;
        y[m.places.iter().position(|q| q.id == p)?] += 1;
        match named.iter_mut().find(|(q, _)| q == p) {
            Some(e) => e.1 += 1,
            None => named.push((p.to_string(), 1)),
        }
    }
    let y_m0: i64 = m.places.iter().zip(&y).map(|(p, k)| k * p.initial).sum();
    let y_c = (0..m.transitions.len())
        .map(|t| (0..y.len()).map(|p| y[p] * c[p][t]).sum::<i64>())
        .collect();
    Some(InvariantCheck {
        y: named,
        n,
        y_c,
        y_m0,
    })
}

/// A declared constraint read as a P-invariant: it holds when y·C = 0 and
/// y·M₀ = n (together they prove it for every reachable marking: the
/// prototype's incidence.js check). `None` when the expression is not a
/// plain sum of tokens(…) equal to an integer.
pub fn invariant_holds(m: &Model, c: &[Vec<i64>], expr: &str) -> Option<bool> {
    invariant_check(m, c, expr).map(|k| k.holds())
}

/// `bundle.incidence`'s result: the flat net's matrix, each subnet's own
/// block, and each declared constraint checked as y·C = 0 and y·M₀ = n
/// (with y, n, the vector y·C per transition and y·M₀ beside `holds`).
pub fn incidence_json(b: &Bundle) -> Value {
    let flat = match flatten(b) {
        Ok(f) => f,
        Err(e) => return json!({"ok": false, "error": e}),
    };
    let c = incidence(&flat.model);
    let blocks: Vec<Value> = b
        .subnets
        .iter()
        .map(|s| {
            json!({
                "subnet": s.id,
                "rows": s.model.places.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
                "cols": s.model.transitions.iter().map(|t| t.id.clone()).collect::<Vec<_>>(),
                "matrix": incidence(&s.model),
            })
        })
        .collect();
    let invariants: Vec<Value> = flat
        .model
        .constraints
        .iter()
        .map(|k| match invariant_check(&flat.model, &c, &k.expr) {
            Some(v) => json!({"id": k.id, "expr": k.expr, "holds": v.holds(),
                              "y": v.y, "n": v.n, "y_c": v.y_c, "y_m0": v.y_m0}),
            None => json!({"id": k.id, "expr": k.expr, "holds": null}),
        })
        .collect();
    json!({
        "ok": true,
        "rows": flat.model.places.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
        "cols": flat.model.transitions.iter().map(|t| t.id.clone()).collect::<Vec<_>>(),
        "matrix": c,
        "blocks": blocks,
        "invariants": invariants,
    })
}
