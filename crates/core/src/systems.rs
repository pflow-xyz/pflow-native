//! Systems: declared ports on models, checked bindings between them, stored
//! systems (a named set of models and bindings) and pipeline runs. A port of
//! sim.pflow.xyz's `ports.go`, `bindcheck.go`, `pipeline.go` and `loop.go`
//! (FEATURES.md §D gap 5); nothing here touches a [`Session`], so it can move
//! upstream into pflow-rs as it stands.
//!
//! # Ports
//!
//! sim declares a port as an element tag (`port.output` / `port.input` /
//! `port.unit`). pflow.xyz documents have no element tags, so a port is an
//! entry of the document's top-level `"ports"` array instead:
//! `{id, direction: output|input, kind?, unit?, description?}`. It saves with
//! the net and, like sim's tags, is ordinary content: declaring one changes
//! the net's CID. An output port sits on a place (its trajectory is what a
//! binding resamples); an input port on a transition (its rate is what a
//! binding drives). The element kind is read off the document, never stored.
//!
//! # Bindings
//!
//! A [`Binding`] joins one model's output port to another's input port, with
//! an optional numeric `transform` (a scale factor; other text is recorded
//! and run 1:1), a declared unit conversion and a delay in hours. Its id is
//! the sha256 of its canonical JSON, first 24 hex characters, as sim's.
//! [`check_binding`] is binds-port-v1/v2/v3: the one copy of the rule, which
//! both `system.bind` and every run apply.
//!
//! # Pipeline runs
//!
//! [`plan`] orders a binding set (loops condensed, topological, ties by
//! model name) and refuses what no run can do: a loop with no delayed
//! binding (`closes-loop`), two bindings into one port (`port-taken`), more
//! schedule segments than one model takes (`segment-cap`), a loop over four
//! models (`loop-too-large`). [`run_pipeline`] runs every model as itself in
//! that order, resampling each binding's source place trajectory into a
//! piecewise-constant rate schedule on the target transition (the per-interval
//! rate of change, scaled by transform × conversion, negative rates clamped
//! to 0), a delayed binding shifted by its delay behind the target's own
//! declared rate. A loop is solved by sweeps exactly as sim's loop.go: one
//! delayed binding is the cut, each sweep runs the members along the cycle,
//! and the run stops when a sweep reproduces the previous one's cut schedule
//! bit for bit (`fixed-point`), or — an extension here — when the largest
//! difference is within `tolerance` (`converged`).
//!
//! Engines: `ode` (the default here) integrates each model's mass-action ODE
//! piecewise between schedule boundaries with pflow-solver, from one
//! unfolded net per model, so a run is deterministic and prefix-causal;
//! `ssa` runs sim's own scenario engine (`pflow_solver::stochastic`, through
//! [`crate::stochastic::run_scenario`]) with the schedule, as sim does.

use std::collections::{BTreeMap, BTreeSet};

use pflow_metamodel::RateSegment;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::doc::Document;
use crate::sim::{self, Marking, OdeOptions};
use crate::stochastic as st;
use crate::{Error, Result};

/// The document field ports are declared in.
pub const PORTS_KEY: &str = "ports";
pub const OUTPUT: &str = "output";
pub const INPUT: &str = "input";

/// Resample intervals one binding becomes (sim's default sample grid).
pub const SEGMENT_BUDGET: usize = 60;
/// Schedule segments one model may take across its input ports.
pub const MAX_SCHEDULE_SEGMENTS: usize = st::MAX_SCHEDULE_SEGMENTS;
/// The largest numeric transform a binding may carry.
pub const MAX_TRANSFORM_FACTOR: f64 = 1e6;
pub const MAX_CONVERSION_FACTOR: f64 = MAX_TRANSFORM_FACTOR;
pub const MIN_CONVERSION_FACTOR: f64 = 1.0 / MAX_CONVERSION_FACTOR;
/// The longest delay a binding may declare, in hours (the longest horizon).
pub const MAX_BINDING_DELAY: f64 = st::MAX_HORIZON;
pub const MAX_LOOP_MODELS: usize = 4;
/// Sweeps one loop may need, the final equality sweep included.
pub const MAX_LOOP_SWEEPS: usize = 24;
/// Model runs every loop of one call may need together.
pub const MAX_LOOP_RUNS: usize = 48;
pub const MAX_SYSTEM_MODELS: usize = 64;
pub const MAX_SYSTEM_BINDINGS: usize = 64;
pub const MAX_SYSTEM_NAME_BYTES: usize = 256;
pub const DEFAULT_HOURS: f64 = 8.0;

pub const DEF_BINDS_PORT_V1: &str = "binds-port-v1";
pub const DEF_BINDS_PORT_V2: &str = "binds-port-v2";
pub const DEF_BINDS_PORT_V3: &str = "binds-port-v3";
pub const DEF_PIPELINE_LOOP_V1: &str = "pipeline-loop-v1";

pub const BIND_OK: &str = "ok";
pub const BIND_REFUSED: &str = "refused";
pub const BIND_UNCHECKABLE: &str = "uncheckable";

pub const LOOP_FIXED_POINT: &str = "fixed-point";
pub const LOOP_CONVERGED: &str = "converged";
pub const LOOP_STOPPED: &str = "stopped";
pub const LOOP_NOT_SETTLED: &str = "not-settled";

// ------------------------------------------------------------------ ports

/// One port as stored in the document's `"ports"` array.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortDecl {
    pub id: String,
    pub direction: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// A declared port, with the kind of element it sits on.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Port {
    pub id: String,
    /// "place" or "transition".
    pub element: String,
    pub direction: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub unit: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// "output"/"out" → [`OUTPUT`], "input"/"in" → [`INPUT`].
pub fn parse_direction(s: &str) -> Result<&'static str> {
    match s {
        "output" | "out" => Ok(OUTPUT),
        "input" | "in" => Ok(INPUT),
        other => Err(Error::BadArgs(format!(
            "direction must be output (out) or input (in), got {other:?}"
        ))),
    }
}

/// The document's port declarations as stored (entries that do not read as
/// one are skipped).
pub fn declarations(doc: &Document) -> Vec<PortDecl> {
    doc.extra
        .get(PORTS_KEY)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| serde_json::from_value::<PortDecl>(v.clone()).ok())
                .filter(|d| d.direction == OUTPUT || d.direction == INPUT)
                .collect()
        })
        .unwrap_or_default()
}

/// Every declared port on an element the document has, in sim's order:
/// places before transitions, each in document order, output before input.
pub fn ports(doc: &Document) -> Vec<Port> {
    let decls = declarations(doc);
    let mut out = vec![];
    let mut push = |id: &str, element: &str| {
        for dir in [OUTPUT, INPUT] {
            if let Some(d) = decls.iter().find(|d| d.id == id && d.direction == dir) {
                out.push(Port {
                    id: d.id.clone(),
                    element: element.into(),
                    direction: dir.into(),
                    kind: d.kind.clone(),
                    unit: d.unit.clone(),
                    description: d.description.clone(),
                });
            }
        }
    };
    for p in &doc.places {
        push(&p.id, "place");
    }
    for t in &doc.transitions {
        push(&t.id, "transition");
    }
    out
}

/// Declarations naming an element the document no longer has (deleted or
/// renamed since); they are never bound.
pub fn stale_ports(doc: &Document) -> Vec<PortDecl> {
    declarations(doc)
        .into_iter()
        .filter(|d| doc.kind_of(&d.id).is_none())
        .collect()
}

fn find_port(doc: &Document, id: &str, direction: &str) -> Option<Port> {
    ports(doc)
        .into_iter()
        .find(|p| p.id == id && p.direction == direction)
}

fn write_ports(doc: &mut Document, decls: &[PortDecl]) {
    if decls.is_empty() {
        doc.extra.shift_remove(PORTS_KEY);
    } else {
        let v = decls
            .iter()
            .map(|d| serde_json::to_value(d).expect("serializable"))
            .collect();
        doc.extra.insert(PORTS_KEY.into(), Value::Array(v));
    }
}

/// Declares (or replaces) the port `decl.id`/`decl.direction`.
pub fn set_port(doc: &mut Document, decl: PortDecl) {
    let mut decls = declarations(doc);
    match decls
        .iter_mut()
        .find(|d| d.id == decl.id && d.direction == decl.direction)
    {
        Some(d) => *d = decl,
        None => decls.push(decl),
    }
    write_ports(doc, &decls);
}

/// Removes a declared port; false when there was none.
pub fn remove_port(doc: &mut Document, id: &str, direction: &str) -> bool {
    let mut decls = declarations(doc);
    let before = decls.len();
    decls.retain(|d| !(d.id == id && d.direction == direction));
    let removed = decls.len() != before;
    if removed {
        write_ports(doc, &decls);
    }
    removed
}

/// Rates a document declares for itself: a numeric `rate` on a transition
/// (pflow.xyz nets carry none, so this is usually empty; absent = 1.0).
pub fn declared_rates(doc: &Document) -> BTreeMap<String, f64> {
    doc.transitions
        .iter()
        .filter_map(|t| {
            t.extra
                .get("rate")
                .and_then(Value::as_f64)
                .filter(|r| r.is_finite() && *r >= 0.0)
                .map(|r| (t.id.clone(), r))
        })
        .collect()
}

// --------------------------------------------------------------- bindings

/// One end of a binding: a model (library name, or "current") and the
/// element id of its declared port.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortRef {
    pub model: String,
    pub port: String,
}

/// A declared unit conversion: 1 `from` = `factor` `to`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnitConversion {
    pub from: String,
    pub to: String,
    pub factor: f64,
}

/// A connection from one model's output port to another's input port.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub from: PortRef,
    pub to: PortRef,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub transform: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub convert: Option<UnitConversion>,
    /// Hours; 0 is none.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub delay: f64,
}

fn is_zero(x: &f64) -> bool {
    *x == 0.0
}

impl Binding {
    /// "from.port -> to.port", as sim's messages name a binding.
    pub fn head(&self) -> String {
        format!(
            "binding {}.{} -> {}.{}",
            self.from.model, self.from.port, self.to.model, self.to.port
        )
    }
}

/// The binding's content id: sha256 of its canonical JSON (sorted keys),
/// first 24 hex characters, as sim's `BindingID`.
pub fn binding_id(b: &Binding) -> String {
    content_id(&serde_json::to_value(b).expect("serializable"))
}

/// sha256 of `v`'s canonical JSON, first 24 hex characters.
pub fn content_id(v: &Value) -> String {
    let mut s = String::new();
    canonical(v, &mut s);
    let sum = Sha256::digest(s.as_bytes());
    sum.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        .chars()
        .take(24)
        .collect()
}

/// JSON with object keys sorted and numbers printed shortest (as Go's
/// `json.Marshal` of a decoded `any` does for ordinary values).
fn canonical(v: &Value, out: &mut String) {
    match v {
        Value::Object(m) => {
            out.push('{');
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).expect("string"));
                out.push(':');
                canonical(&m[k.as_str()], out);
            }
            out.push('}');
        }
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canonical(x, out);
            }
            out.push(']');
        }
        Value::Number(n) => match n.as_i64() {
            Some(i) => out.push_str(&i.to_string()),
            None => out.push_str(&num(n.as_f64().unwrap_or(0.0))),
        },
        other => out.push_str(&other.to_string()),
    }
}

/// A number printed shortest (Go's `strconv.FormatFloat(x, 'g', -1, 64)`
/// for ordinary magnitudes).
pub fn num(x: f64) -> String {
    format!("{x}")
}

/// The rule that decides `b`: v3 with a delay, v2 with a conversion, else v1.
pub fn definition(b: &Binding) -> &'static str {
    if b.delay != 0.0 {
        DEF_BINDS_PORT_V3
    } else if b.convert.is_some() {
        DEF_BINDS_PORT_V2
    } else {
        DEF_BINDS_PORT_V1
    }
}

/// What an ok binding says about units.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct UnitCheck {
    pub verdict: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub from: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub to: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub factor: f64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// One binding's verdict, computed on read.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BindingCheck {
    pub definition: String,
    pub verdict: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub code: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reason: String,
    #[serde(rename = "fromPort", skip_serializing_if = "Option::is_none")]
    pub from_port: Option<Port>,
    #[serde(rename = "toPort", skip_serializing_if = "Option::is_none")]
    pub to_port: Option<Port>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<UnitCheck>,
}

impl BindingCheck {
    pub fn is_ok(&self) -> bool {
        self.verdict == BIND_OK
    }

    /// A model the binding names cannot be found.
    pub fn uncheckable(b: &Binding, reason: String) -> Self {
        Self {
            definition: definition(b).into(),
            verdict: BIND_UNCHECKABLE.into(),
            code: String::new(),
            reason,
            from_port: None,
            to_port: None,
            unit: None,
        }
    }

    fn refuse(mut self, code: &str, reason: String) -> Self {
        self.verdict = BIND_REFUSED.into();
        self.code = code.into();
        self.reason = reason;
        self
    }

    /// The check as an error: "<code>: <reason>".
    pub fn to_error(&self) -> Error {
        if self.code.is_empty() {
            Error::Invalid(self.reason.clone())
        } else {
            Error::Invalid(format!("{}: {}", self.code, self.reason))
        }
    }
}

/// binds-port-v1 (v2 with a conversion, v3 with a delay) on `b`, with
/// `from` and `to` the documents its two models name. The first failing
/// check is the one reported.
pub fn check_binding(from: &Document, to: &Document, b: &Binding) -> BindingCheck {
    let mut c = BindingCheck {
        definition: definition(b).into(),
        verdict: String::new(),
        code: String::new(),
        reason: String::new(),
        from_port: None,
        to_port: None,
        unit: None,
    };
    if b.from.model == b.to.model {
        return c.refuse("self-loop", format!(
            "from_model and to_model are both {}: a model driving its own input is a loop inside one net; build it into the net (a delay transition models the lag) — system.run runs loops only between different models, through a binding that declares a delay",
            b.from.model
        ));
    }
    let Some(fp) = find_port(from, &b.from.port, OUTPUT) else {
        return c.refuse("no-output-port", format!(
            "model {} declares no output port {:?} (see system.ports {{model: {:?}}}; declare one with system.set_port)",
            b.from.model, b.from.port, b.from.model
        ));
    };
    c.from_port = Some(fp.clone());
    if fp.element != "place" {
        return c.refuse("output-not-place", format!(
            "output port {:?} on model {} is a {}; a run can only resample a PLACE's trajectory (a transition has no time series, only an aggregate throughput)",
            fp.id, b.from.model, fp.element
        ));
    }
    let Some(tp) = find_port(to, &b.to.port, INPUT) else {
        return c.refuse("no-input-port", format!(
            "model {} declares no input port {:?} (see system.ports {{model: {:?}}}; declare one with system.set_port)",
            b.to.model, b.to.port, b.to.model
        ));
    };
    c.to_port = Some(tp.clone());
    if tp.element != "transition" {
        return c.refuse("input-not-transition", format!(
            "input port {:?} on model {} is a {}; a run can only drive a TRANSITION's rate (a place can only be set at t=0, not driven over time)",
            tp.id, b.to.model, tp.element
        ));
    }
    if fp.kind != tp.kind {
        return c.refuse("kind-differs", format!(
            "output port {:?} on model {} is kind {:?} but input port {:?} on model {} is kind {:?}; a binding joins ports of one declared kind, compared exactly — bind a port of kind {:?}, or redeclare one end with system.set_port",
            b.from.port, b.from.model, fp.kind, b.to.port, b.to.model, tp.kind, fp.kind
        ));
    }
    if let Some(cv) = &b.convert {
        if let Some((code, reason)) = convert_check(b, cv, &fp, &tp) {
            return c.refuse(code, reason);
        }
    } else if !fp.unit.is_empty() && !tp.unit.is_empty() && fp.unit != tp.unit {
        return c.refuse("unit-differs", format!(
            "output port {:?} on model {} counts {:?} but input port {:?} on model {} counts {:?}; units are compared exactly and never inferred from their names (a numeric transform scales a rate, it does not change its unit) — declare the conversion: pass convert_from {:?}, convert_to {:?} and convert_factor (how many {:?} one {:?} is) to system.bind; or, if both ends really count the same thing, set the same unit on both with system.set_port",
            b.from.port, b.from.model, fp.unit, b.to.port, b.to.model, tp.unit, fp.unit, tp.unit, tp.unit, fp.unit
        ));
    }
    let (f, note) = scale_factor(&b.transform);
    if note.is_empty() && f > MAX_TRANSFORM_FACTOR {
        return c.refuse("transform-over-cap", format!(
            "transform {:?} scales the rate by {}, over the {} a binding's numeric transform may take (past it a resampled rate can overflow to infinity); bind with a factor of at most {}",
            b.transform, num(f), num(MAX_TRANSFORM_FACTOR), num(MAX_TRANSFORM_FACTOR)
        ));
    }
    if let Some(cv) = &b.convert {
        let f = if note.is_empty() { f } else { 1.0 };
        let scale = f * cv.factor;
        if scale > MAX_TRANSFORM_FACTOR {
            return c.refuse("scale-over-cap", format!(
                "transform {:?} (a factor of {}) and convert factor {} together scale the rate by {}, over the {} a binding may scale a rate by in all (a run applies their product); lower the transform, or change what one token of a port counts",
                b.transform, num(f), num(cv.factor), num(scale), num(MAX_TRANSFORM_FACTOR)
            ));
        }
    }
    if let Some(reason) = delay_check(b) {
        return c.refuse("delay-invalid", reason);
    }
    c.verdict = BIND_OK.into();
    c.unit = Some(unit_check(b, &fp, &tp));
    c
}

/// binds-port-v3's check 12: a delay is finite, in (0, MAX_BINDING_DELAY].
pub fn delay_check(b: &Binding) -> Option<String> {
    const REMEDY: &str = "a delay is how many hours after the source's flow the target receives it; leave it unset for none";
    let d = b.delay;
    if d == 0.0 {
        None
    } else if !d.is_finite() {
        Some(format!(
            "delay {d} is not a finite number of hours; {REMEDY}"
        ))
    } else if d < 0.0 {
        Some(format!("delay {} is below 0 hours; {REMEDY}", num(d)))
    } else if d > MAX_BINDING_DELAY {
        Some(format!(
            "delay {} is outside (0, {}] hours: {REMEDY}",
            num(d),
            num(MAX_BINDING_DELAY)
        ))
    } else {
        None
    }
}

fn convert_check(
    b: &Binding,
    cv: &UnitConversion,
    fp: &Port,
    tp: &Port,
) -> Option<(&'static str, String)> {
    let mut missing = vec![];
    if fp.unit.is_empty() {
        missing.push(format!(
            "output port {:?} on model {}",
            b.from.port, b.from.model
        ));
    }
    if tp.unit.is_empty() {
        missing.push(format!(
            "input port {:?} on model {}",
            b.to.port, b.to.model
        ));
    }
    if !missing.is_empty() {
        return Some(("convert-unit-undeclared", format!(
            "the binding declares a conversion from {:?} to {:?}, but {} declares no unit; a conversion joins two declared units and is never applied to an undeclared one — declare the unit on that end with system.set_port, or leave the convert_* arguments out to bind with the unit unchecked",
            cv.from, cv.to, missing.join(" and ")
        )));
    }
    if fp.unit == tp.unit {
        return Some(("convert-same-unit", format!(
            "output port {:?} on model {} and input port {:?} on model {} both count {:?}, so their units agree without a conversion; a factor between equal units would only rescale the rate, which is what a numeric transform is for — leave the convert_* arguments out, and put a plain number in transform if the rate should be scaled",
            b.from.port, b.from.model, b.to.port, b.to.model, fp.unit
        )));
    }
    if cv.from != fp.unit || cv.to != tp.unit {
        return Some(("convert-unit-mismatch", format!(
            "convert declares {:?} to {:?}, but output port {:?} on model {} counts {:?} and input port {:?} on model {} counts {:?}; a conversion names exactly the units of the ports it joins (compared exactly) — pass convert_from {:?} and convert_to {:?}",
            cv.from, cv.to, b.from.port, b.from.model, fp.unit, b.to.port, b.to.model, tp.unit, fp.unit, tp.unit
        )));
    }
    if !(cv.factor.is_finite() && cv.factor > 0.0) {
        return Some(("convert-factor-invalid", format!(
            "convert factor {} is not a finite positive number; it says how many {:?} one {:?} is, so it must be greater than 0",
            cv.factor, cv.to, cv.from
        )));
    }
    if !(MIN_CONVERSION_FACTOR..=MAX_CONVERSION_FACTOR).contains(&cv.factor) {
        return Some(("convert-over-cap", format!(
            "convert factor {} is outside [{}, {}], the range a unit conversion may take (symmetric, because the binding the other way carries the reciprocal); change what one token of a port counts",
            num(cv.factor), num(MIN_CONVERSION_FACTOR), num(MAX_CONVERSION_FACTOR)
        )));
    }
    None
}

fn unit_check(b: &Binding, from: &Port, to: &Port) -> UnitCheck {
    let mut u = UnitCheck {
        from: from.unit.clone(),
        to: to.unit.clone(),
        ..Default::default()
    };
    if let Some(cv) = &b.convert {
        u.verdict = "converted".into();
        u.factor = cv.factor;
        u.note = format!(
            "1 {} = {} {}, as declared on the binding (convert); applied exactly as declared, never inferred from the unit names",
            from.unit,
            num(cv.factor),
            to.unit
        );
        return u;
    }
    match (from.unit.is_empty(), to.unit.is_empty()) {
        (false, false) => u.verdict = "agree".into(),
        (true, true) => {
            u.verdict = "unchecked".into();
            u.missing = vec!["from".into(), "to".into()];
            u.note = "neither port declares a unit, so the unit is unchecked, not agreed".into();
        }
        (true, false) => {
            u.verdict = "unchecked".into();
            u.missing = vec!["from".into()];
            u.note = format!(
                "from_port {:?} on model {} declares no unit, so the unit is unchecked, not agreed",
                b.from.port, b.from.model
            );
        }
        (false, true) => {
            u.verdict = "unchecked".into();
            u.missing = vec!["to".into()];
            u.note = format!(
                "to_port {:?} on model {} declares no unit, so the unit is unchecked, not agreed",
                b.to.port, b.to.model
            );
        }
    }
    u
}

/// The assumption one binding contributes about units.
fn unit_line(b: &Binding, c: &BindingCheck) -> String {
    let head = format!(
        "binding {}.{} -> {}.{}: ",
        b.from.model, b.from.port, b.to.model, b.to.port
    );
    match &c.unit {
        Some(u) if u.verdict == "agree" => format!("{head}units agree ({})", u.from),
        Some(u) if u.verdict == "converted" => format!(
            "{head}units converted by the declared factor, {}; the resampled rate was multiplied by {} after any numeric transform",
            u.note,
            num(u.factor)
        ),
        Some(u) => format!("{head}{}", u.note),
        None => format!("{head}units not checked"),
    }
}

/// A transform as a scale factor: "" and "1:1" are 1 with no note; a plain
/// finite non-negative number is itself; anything else runs as 1 with a note.
pub fn scale_factor(transform: &str) -> (f64, String) {
    let t = transform.trim();
    if t.is_empty() || t == "1:1" {
        return (1.0, String::new());
    }
    match t.parse::<f64>() {
        Ok(f) if f.is_finite() && f >= 0.0 => (f, String::new()),
        Ok(_) => (1.0, format!(
            "transform {transform:?} is not a finite, non-negative scale factor; treated as 1:1 pass-through"
        )),
        Err(_) => (1.0, format!(
            "transform {transform:?} is not a plain numeric scale factor this runner interprets; treated as 1:1 pass-through"
        )),
    }
}

// ------------------------------------------------------------------- plans

/// A binding set no pipeline can run: a code and sim's text for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    pub code: &'static str,
    pub msg: String,
}

impl From<Refusal> for Error {
    fn from(r: Refusal) -> Self {
        Error::Invalid(format!("{}: {}", r.code, r.msg))
    }
}

/// One loop of a binding set: its models in sweep order (along the cycle
/// from the cut's target), the binding into each, the delayed ones, their
/// total delay and the cut (the one delayed binding a sweep drives from the
/// previous sweep).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanLoop {
    pub models: Vec<String>,
    pub bindings: Vec<String>,
    pub delayed: Vec<String>,
    #[serde(rename = "totalDelay")]
    pub total_delay: f64,
    pub cut: String,
    #[serde(skip)]
    cycle: String,
}

#[derive(Debug, Clone)]
struct Component {
    models: Vec<String>,
    lp: Option<usize>,
}

/// A binding set's run order and loops.
#[derive(Debug, Clone)]
pub struct Plan {
    pub order: Vec<String>,
    pub loops: Vec<PlanLoop>,
    /// (id, binding), sorted by id.
    pub bindings: Vec<(String, Binding)>,
    components: Vec<Component>,
    /// Target model → indices into `bindings`.
    incoming: BTreeMap<String, Vec<usize>>,
}

impl Plan {
    fn binding(&self, id: &str) -> Option<&Binding> {
        self.bindings.iter().find(|(i, _)| i == id).map(|(_, b)| b)
    }
}

fn segments_of(b: &Binding) -> usize {
    if b.delay != 0.0 {
        SEGMENT_BUDGET + 1
    } else {
        SEGMENT_BUDGET
    }
}

/// The set-level checks (closes-loop, port-taken, segment-cap,
/// loop-too-large) and the run order. Reads no model.
pub fn plan(mut bindings: Vec<(String, Binding)>) -> std::result::Result<Plan, Refusal> {
    // A run's answer must not depend on the order the ids were named in.
    bindings.sort_by(|a, b| a.0.cmp(&b.0));
    bindings.dedup_by(|a, b| a.0 == b.0);
    undelayed_cycle(&bindings)?;

    let mut incoming: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, (_, b)) in bindings.iter().enumerate() {
        incoming.entry(b.to.model.clone()).or_default().push(i);
    }
    for (model, ixs) in &incoming {
        let mut taken = BTreeSet::new();
        let mut total = 0;
        for &i in ixs {
            let b = &bindings[i].1;
            if !taken.insert(b.to.port.clone()) {
                return Err(Refusal {
                    code: "port-taken",
                    msg: format!(
                        "model {}'s input port {:?} is targeted by more than one binding in this set — a run refuses rather than silently letting one binding's schedule shadow another's",
                        b.to.model, b.to.port
                    ),
                });
            }
            total += segments_of(b);
        }
        if total > MAX_SCHEDULE_SEGMENTS {
            return Err(Refusal {
                code: "segment-cap",
                msg: format!(
                    "model {model} has {} incoming bindings; each is resampled into a {SEGMENT_BUDGET}-segment schedule ({} for a delayed binding), {total} segments in all, over the {MAX_SCHEDULE_SEGMENTS} cap — bind at most {} into one model",
                    ixs.len(),
                    SEGMENT_BUDGET + 1,
                    MAX_SCHEDULE_SEGMENTS / SEGMENT_BUDGET
                ),
            });
        }
    }

    // Models, sorted; each model's outgoing bindings by target, then id.
    let mut nodes: BTreeSet<String> = BTreeSet::new();
    let mut out: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, (_, b)) in bindings.iter().enumerate() {
        nodes.insert(b.from.model.clone());
        nodes.insert(b.to.model.clone());
        out.entry(b.from.model.clone()).or_default().push(i);
    }
    for e in out.values_mut() {
        e.sort_by(|&x, &y| {
            (&bindings[x].1.to.model, &bindings[x].0)
                .cmp(&(&bindings[y].1.to.model, &bindings[y].0))
        });
    }
    let nodes: Vec<String> = nodes.into_iter().collect();
    let sccs = tarjan(&nodes, &out, &bindings);
    let mut comp: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, c) in sccs.iter().enumerate() {
        for m in c {
            comp.insert(m, i);
        }
    }
    let mut indeg = vec![0usize; sccs.len()];
    let mut cout: Vec<Vec<usize>> = vec![vec![]; sccs.len()];
    for (_, b) in &bindings {
        let (f, t) = (comp[b.from.model.as_str()], comp[b.to.model.as_str()]);
        if f != t {
            cout[f].push(t);
            indeg[t] += 1;
        }
    }
    let key = |c: usize| sccs[c][0].clone();
    let mut ready: Vec<usize> = (0..sccs.len()).filter(|&c| indeg[c] == 0).collect();
    ready.sort_by_key(|&c| key(c));
    let mut plan = Plan {
        order: vec![],
        loops: vec![],
        bindings: vec![],
        components: vec![],
        incoming: incoming.clone(),
    };
    let mut queue = std::collections::VecDeque::from(ready);
    while let Some(c) = queue.pop_front() {
        let mut pc = Component {
            models: sccs[c].clone(),
            lp: None,
        };
        if sccs[c].len() > 1 {
            let lp = plan_loop(&sccs[c], &incoming, &bindings)?;
            pc.lp = Some(plan.loops.len());
            pc.models = lp.models.clone();
            plan.loops.push(lp);
        }
        plan.order.extend(pc.models.iter().cloned());
        plan.components.push(pc);
        let mut next = vec![];
        for &t in &cout[c] {
            indeg[t] -= 1;
            if indeg[t] == 0 {
                next.push(t);
            }
        }
        next.sort_by_key(|&t| key(t));
        queue.extend(next);
    }
    plan.bindings = bindings;
    Ok(plan)
}

fn tarjan(
    nodes: &[String],
    out: &BTreeMap<String, Vec<usize>>,
    bindings: &[(String, Binding)],
) -> Vec<Vec<String>> {
    struct T<'a> {
        out: &'a BTreeMap<String, Vec<usize>>,
        bindings: &'a [(String, Binding)],
        index: BTreeMap<String, usize>,
        low: BTreeMap<String, usize>,
        on: BTreeSet<String>,
        stack: Vec<String>,
        sccs: Vec<Vec<String>>,
        next: usize,
    }
    fn visit(t: &mut T, v: &str) {
        t.index.insert(v.into(), t.next);
        t.low.insert(v.into(), t.next);
        t.next += 1;
        t.stack.push(v.into());
        t.on.insert(v.into());
        let edges = t.out.get(v).cloned().unwrap_or_default();
        for i in edges {
            let w = t.bindings[i].1.to.model.clone();
            if !t.index.contains_key(&w) {
                visit(t, &w);
                let lw = t.low[&w];
                let lv = t.low.get_mut(v).expect("visited");
                *lv = (*lv).min(lw);
            } else if t.on.contains(&w) {
                let iw = t.index[&w];
                let lv = t.low.get_mut(v).expect("visited");
                *lv = (*lv).min(iw);
            }
        }
        if t.low[v] == t.index[v] {
            let mut c = vec![];
            loop {
                let w = t.stack.pop().expect("on stack");
                t.on.remove(&w);
                let done = w == v;
                c.push(w);
                if done {
                    break;
                }
            }
            c.sort();
            t.sccs.push(c);
        }
    }
    let mut t = T {
        out,
        bindings,
        index: BTreeMap::new(),
        low: BTreeMap::new(),
        on: BTreeSet::new(),
        stack: vec![],
        sccs: vec![],
        next: 0,
    };
    for v in nodes {
        if !t.index.contains_key(v) {
            visit(&mut t, v);
        }
    }
    t.sccs
}

/// Refuses a set whose undelayed bindings alone form a cycle, naming one.
fn undelayed_cycle(bindings: &[(String, Binding)]) -> std::result::Result<(), Refusal> {
    let und: Vec<usize> = (0..bindings.len())
        .filter(|&i| bindings[i].1.delay == 0.0)
        .collect();
    let mut nodes: BTreeSet<&str> = BTreeSet::new();
    let mut indeg: BTreeMap<&str, usize> = BTreeMap::new();
    let mut out: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    let mut inn: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for &i in &und {
        let b = &bindings[i].1;
        nodes.insert(&b.from.model);
        nodes.insert(&b.to.model);
        *indeg.entry(&b.to.model).or_default() += 1;
        out.entry(&b.from.model).or_default().push(i);
        inn.entry(&b.to.model).or_default().push(i);
    }
    let mut queue: std::collections::VecDeque<&str> = nodes
        .iter()
        .copied()
        .filter(|n| indeg.get(n).copied().unwrap_or(0) == 0)
        .collect();
    let mut done: BTreeSet<&str> = BTreeSet::new();
    while let Some(n) = queue.pop_front() {
        done.insert(n);
        for &i in out.get(n).map(Vec::as_slice).unwrap_or(&[]) {
            let t = bindings[i].1.to.model.as_str();
            let d = indeg.get_mut(t).expect("counted");
            *d -= 1;
            if *d == 0 {
                queue.push_back(t);
            }
        }
    }
    let left: Vec<&str> = nodes
        .iter()
        .copied()
        .filter(|n| !done.contains(n))
        .collect();
    let Some(&start) = left.first() else {
        return Ok(());
    };
    // Walk incoming undelayed bindings backwards from the smallest model
    // left until a model repeats.
    let mut at: BTreeMap<&str, usize> = BTreeMap::new();
    let mut path: Vec<&str> = vec![];
    let mut via: Vec<usize> = vec![];
    let mut n = start;
    loop {
        if let Some(&k) = at.get(n) {
            path.drain(..k);
            via.drain(..k);
            break;
        }
        at.insert(n, path.len());
        let best = inn
            .get(n)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&i| !done.contains(bindings[i].1.from.model.as_str()))
            .min_by(|&x, &y| {
                (&bindings[x].1.from.model, &bindings[x].0)
                    .cmp(&(&bindings[y].1.from.model, &bindings[y].0))
            })
            .expect("every model left has an incoming binding from another left");
        path.push(n);
        via.push(best);
        n = &bindings[best].1.from.model;
    }
    let k = path.len();
    let mut models: Vec<&str> = (0..k).map(|j| path[k - 1 - j]).collect();
    let mut bids: Vec<&str> = (0..k)
        .map(|j| bindings[via[k - 1 - j]].0.as_str())
        .collect();
    let s = (0..k).min_by_key(|&j| models[j]).unwrap_or(0);
    models.rotate_left(s);
    bids.rotate_left(s);
    bids.rotate_left(1);
    Err(Refusal {
        code: "closes-loop",
        msg: format!(
            "this binding set is cyclic ({} -> {} through {}) and no binding on that loop declares a delay; a loop runs only when some binding on it carries one (system.bind delay, in hours), or build the loop into one net",
            models.join(" -> "),
            models[0],
            bids.join(", ")
        ),
    })
}

fn plan_loop(
    members: &[String],
    incoming: &BTreeMap<String, Vec<usize>>,
    bindings: &[(String, Binding)],
) -> std::result::Result<PlanLoop, Refusal> {
    let inside: BTreeSet<&str> = members.iter().map(String::as_str).collect();
    let mut into: BTreeMap<&str, usize> = BTreeMap::new();
    let mut next: BTreeMap<&str, &str> = BTreeMap::new();
    for m in members {
        for &i in incoming.get(m).map(Vec::as_slice).unwrap_or(&[]) {
            let b = &bindings[i].1;
            if inside.contains(b.from.model.as_str()) {
                into.insert(m, i);
                next.insert(&b.from.model, m);
            }
        }
    }
    let walk = |start: &str| -> Vec<String> {
        let mut cyc = vec![start.to_string()];
        let mut n = next.get(start).copied().unwrap_or("");
        while n != start && !n.is_empty() && cyc.len() <= members.len() {
            cyc.push(n.to_string());
            n = next.get(n).copied().unwrap_or("");
        }
        cyc
    };
    let mut c = walk(&members[0]);
    c.push(members[0].clone());
    let cycle = c.join(" -> ");
    if members.len() > MAX_LOOP_MODELS {
        return Err(Refusal {
            code: "loop-too-large",
            msg: format!(
                "the loop {cycle} has {} models, over the {MAX_LOOP_MODELS}-model limit for one loop; run part of it, or build it into one net",
                members.len()
            ),
        });
    }
    let first = members
        .iter()
        .find(|m| {
            into.get(m.as_str())
                .is_some_and(|&i| bindings[i].1.delay != 0.0)
        })
        .expect("a loop the plan did not refuse holds a delayed binding");
    let order = walk(first);
    let mut lp = PlanLoop {
        models: vec![],
        bindings: vec![],
        delayed: vec![],
        total_delay: 0.0,
        cut: bindings[into[first.as_str()]].0.clone(),
        cycle: {
            let mut c = order.clone();
            c.push(first.clone());
            c.join(" -> ")
        },
    };
    for m in &order {
        let (id, b) = &bindings[into[m.as_str()]];
        lp.models.push(m.clone());
        lp.bindings.push(id.clone());
        if b.delay != 0.0 {
            lp.delayed.push(id.clone());
            lp.total_delay += b.delay;
        }
    }
    Ok(lp)
}

/// The k-th boundary of the fixed resample grid over `horizon`.
pub fn grid_at(horizon: f64, k: usize, budget: usize) -> f64 {
    if k == budget {
        horizon
    } else {
        horizon * k as f64 / budget as f64
    }
}

/// A loop's resample interval, lag and N (sweeps that make every schedule
/// exact; 0 when the lag is not positive).
fn loop_sweeps(l: &PlanLoop, horizon: f64) -> (f64, f64, usize) {
    let delta = horizon / SEGMENT_BUDGET as f64;
    let lag = l.total_delay - l.bindings.len() as f64 * delta;
    if lag <= 0.0 {
        return (delta, lag, 0);
    }
    (delta, lag, ((horizon / lag).ceil() as usize).max(1))
}

fn ceil2(x: f64) -> f64 {
    (x * 100.0).ceil() / 100.0
}

/// The largest horizon in hundredths of an hour, at most `up_to`, for which
/// `ok` holds (`ok` must hold at 0 and be monotone).
fn largest_hours(up_to: f64, ok: impl Fn(f64) -> bool) -> f64 {
    let (mut lo, mut hi) = (0i64, (up_to * 100.0).floor() as i64);
    if ok(hi as f64 / 100.0) {
        return hi as f64 / 100.0;
    }
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if ok(mid as f64 / 100.0) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo as f64 / 100.0
}

fn loop_remedy_text(l: &PlanLoop, horizon: f64) -> String {
    let bound = l.bindings.len() as f64;
    let hmax = largest_hours(horizon, |h| {
        let (_, lg, k) = loop_sweeps(l, h);
        h == 0.0 || (lg > 0.0 && k < MAX_LOOP_SWEEPS)
    });
    let mut dmin =
        ceil2(horizon / (MAX_LOOP_SWEEPS - 1) as f64 + bound * horizon / SEGMENT_BUDGET as f64);
    for _ in 0..100 {
        let mut ld = l.clone();
        ld.total_delay = dmin;
        let (_, lg, m) = loop_sweeps(&ld, horizon);
        if lg > 0.0 && m < MAX_LOOP_SWEEPS {
            break;
        }
        dmin = ceil2(dmin + 0.01);
    }
    if hmax <= 0.0 {
        format!(
            "at {} hours bind with a total delay of at least {} h (no horizon of 0.01 h or more runs this delay)",
            num(horizon),
            num(dmin)
        )
    } else {
        format!(
            "at {} hours bind with a total delay of at least {} h, or run at most {} hours",
            num(horizon),
            num(dmin),
            num(hmax)
        )
    }
}

/// The checks that depend on the horizon (loop-lag, loop-horizon, run-cap);
/// returns each loop's N.
pub fn loop_hours_check(plan: &Plan, horizon: f64) -> std::result::Result<Vec<usize>, Refusal> {
    let mut sweeps = vec![];
    let mut runs = 0;
    for l in &plan.loops {
        let (delta, lag, n) = loop_sweeps(l, horizon);
        let nb = l.bindings.len();
        if lag <= 0.0 {
            return Err(Refusal {
                code: "loop-lag",
                msg: format!(
                    "the loop {} carries {} h of declared delay over {nb} bindings, but each binding reads its source up to one {:.4}-hour resample interval (hours/{SEGMENT_BUDGET}) ahead, so its lag {} − {nb}×{:.4} = {:.4} h is not positive and no number of sweeps settles it; {}",
                    l.cycle, num(l.total_delay), delta, num(l.total_delay), delta, lag, loop_remedy_text(l, horizon)
                ),
            });
        }
        if n + 1 > MAX_LOOP_SWEEPS {
            return Err(Refusal {
                code: "loop-horizon",
                msg: format!(
                    "hours {} needs {} sweeps for the loop {} ({} h of delay over {nb} bindings), over {MAX_LOOP_SWEEPS}; {}",
                    num(horizon), n + 1, l.cycle, num(l.total_delay), loop_remedy_text(l, horizon)
                ),
            });
        }
        sweeps.push(n);
        runs += l.models.len() * (n + 1);
    }
    if runs > MAX_LOOP_RUNS {
        let hmax = largest_hours(horizon, |h| {
            if h == 0.0 {
                return true;
            }
            let mut total = 0;
            for l in &plan.loops {
                let (_, lg, k) = loop_sweeps(l, h);
                if lg <= 0.0 {
                    return false;
                }
                total += l.models.len() * (k + 1);
            }
            total <= MAX_LOOP_RUNS
        });
        return Err(Refusal {
            code: "run-cap",
            msg: format!(
                "the loops in this set need up to {runs} model runs, over the {MAX_LOOP_RUNS}-run limit for one call; run at most {} hours or bind with more delay",
                num(hmax)
            ),
        });
    }
    Ok(sweeps)
}

/// The longest horizon (hundredths of an hour, ≤ the scenario cap) at which
/// every loop passes the horizon checks; 0 for a plan without loops, or when
/// no horizon of 0.01 h or more does. (sim also counts each delayed
/// binding's seed segments; documents declare no schedules, so that check
/// always passes here.)
pub fn max_hours(plan: &Plan) -> f64 {
    if plan.loops.is_empty() {
        return 0.0;
    }
    largest_hours(st::MAX_HORIZON, |h| {
        h == 0.0 || loop_hours_check(plan, h).is_ok()
    })
}

// ----------------------------------------------------------------- engines

/// What runs each model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// Mass-action ODE, piecewise between schedule boundaries.
    Ode,
    /// sim's scenario engine (`pflow_solver::stochastic`).
    Ssa,
}

impl Engine {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "ode" => Ok(Engine::Ode),
            "ssa" => Ok(Engine::Ssa),
            other => Err(Error::BadArgs(format!(
                "unknown engine {other:?} (ode or ssa)"
            ))),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Engine::Ode => "ode",
            Engine::Ssa => "ssa",
        }
    }
}

/// A model a run reads: its document, starting marking and rates.
#[derive(Debug, Clone)]
pub struct ModelInput {
    pub doc: Document,
    pub marking: Marking,
    /// By transition id; absent = 1.0, 0 disables.
    pub rates: BTreeMap<String, f64>,
}

impl ModelInput {
    /// A document at its initial marking with its declared rates.
    pub fn from_doc(doc: Document) -> Self {
        Self {
            marking: sim::initial_marking(&doc),
            rates: declared_rates(&doc),
            doc,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub hours: f64,
    pub engine: Engine,
    pub seed: u64,
    pub realizations: usize,
    /// Most sweeps per loop (default: the N+1 the lag guarantees).
    pub sweeps: Option<usize>,
    /// A loop also settles when its cut schedule moves by at most this much
    /// (rate per hour) between sweeps; 0 is sim's exact fixed point.
    pub tolerance: f64,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            hours: DEFAULT_HOURS,
            engine: Engine::Ode,
            seed: 1,
            realizations: 1,
            sweeps: None,
            tolerance: 0.0,
        }
    }
}

/// One model's trajectories.
#[derive(Debug, Clone)]
pub struct ModelRun {
    pub t: Vec<f64>,
    /// Display names (`place`, or `place.color`).
    pub names: Vec<String>,
    pub values: Vec<Vec<f64>>,
    /// Why the run stopped early (a non-finite state), if it did.
    pub stopped: Option<String>,
}

impl ModelRun {
    /// A port place's trajectory: the place, or the sum of its colors.
    pub fn series(&self, place: &str) -> Option<Vec<f64>> {
        let prefix = format!("{place}.");
        let cols: Vec<&Vec<f64>> = self
            .names
            .iter()
            .zip(&self.values)
            .filter(|(n, _)| *n == place || n.starts_with(&prefix))
            .map(|(_, v)| v)
            .collect();
        if cols.is_empty() {
            return None;
        }
        Some(
            (0..self.t.len())
                .map(|i| cols.iter().map(|c| c[i]).sum())
                .collect(),
        )
    }

    pub fn finals(&self) -> Map<String, Value> {
        self.names
            .iter()
            .zip(&self.values)
            .map(|(n, v)| (n.clone(), json!(v.last().copied().unwrap_or(0.0))))
            .collect()
    }
}

type Schedule = BTreeMap<String, Vec<RateSegment>>;

enum Runner {
    Ode {
        cn: sim::ContinuousNet,
        solver: pflow_solver::Solver,
        opts: pflow_solver::Options,
        horizon: f64,
    },
    Ssa {
        doc: Box<Document>,
        net: Box<st::Net>,
        input: ModelInput,
        sc: st::Scenario,
    },
}

impl Runner {
    fn new(name: &str, input: &ModelInput, opts: &RunOptions) -> Result<Self> {
        let net =
            st::Net::new(&input.doc).map_err(|e| Error::Invalid(format!("model {name}: {e}")))?;
        match opts.engine {
            Engine::Ode => {
                let gating = net.gating();
                if !gating.is_empty() {
                    return Err(Error::Invalid(format!(
                        "model {name}: the continuous engine cannot express this net's gating ({}); run with engine: ssa",
                        gating.join("; ")
                    )));
                }
                let cn = sim::continuous(&input.doc, &input.marking, &input.rates)?;
                Ok(Runner::Ode {
                    cn,
                    solver: sim::solver("tsit5")?,
                    opts: OdeOptions::default().solver_options(opts.hours)?,
                    horizon: opts.hours,
                })
            }
            Engine::Ssa => Ok(Runner::Ssa {
                doc: Box::new(input.doc.clone()),
                net: Box::new(net),
                input: input.clone(),
                sc: st::Scenario {
                    name: Some(name.into()),
                    marking: BTreeMap::new(),
                    rates: BTreeMap::new(),
                    schedule: BTreeMap::new(),
                    hours: opts.hours,
                    samples: SEGMENT_BUDGET + 1,
                    realizations: opts.realizations.max(1),
                    seed: opts.seed,
                    seed_set: true,
                    summary: false,
                    engine: st::Engine::Ssa,
                },
            }),
        }
    }

    fn has_transition(&self, id: &str) -> bool {
        match self {
            Runner::Ode { cn, .. } => cn.rates.contains_key(id),
            Runner::Ssa { net, .. } => net.has_transition(id),
        }
    }

    fn run(&self, schedule: &Schedule) -> Result<ModelRun> {
        match self {
            Runner::Ode {
                cn,
                solver,
                opts,
                horizon,
            } => Ok(ode_piecewise(cn, solver, opts, *horizon, schedule)),
            Runner::Ssa {
                doc,
                net,
                input,
                sc,
            } => {
                let mut sc = sc.clone();
                sc.schedule = schedule.clone();
                let r = st::run_scenario(doc, net, &input.marking, &input.rates, &sc)?;
                Ok(ModelRun {
                    t: r.series.t,
                    names: r.series.names,
                    values: r.series.mean,
                    stopped: None,
                })
            }
        }
    }
}

/// The rate a schedule gives at time `at` (the first segment ending after
/// it; the last holds past the end).
fn rate_at(segs: &[RateSegment], at: f64) -> f64 {
    segs.iter()
        .find(|s| at < s.until)
        .or(segs.last())
        .map(|s| s.value)
        .unwrap_or(0.0)
}

/// The mass-action ODE over `[0, horizon]`, restarted at every grid point and
/// every schedule boundary with the rates in force on that piece. One
/// unfolded net is reused (cloned, hasher and all), so the same inputs give
/// the same bits, and two schedules that agree up to t give runs that agree
/// up to t.
fn ode_piecewise(
    cn: &sim::ContinuousNet,
    solver: &pflow_solver::Solver,
    opts: &pflow_solver::Options,
    horizon: f64,
    schedule: &Schedule,
) -> ModelRun {
    use pflow_solver::{solve, Problem};
    let grid: Vec<f64> = (0..=SEGMENT_BUDGET)
        .map(|k| grid_at(horizon, k, SEGMENT_BUDGET))
        .collect();
    let eps = horizon * 1e-12;
    let mut breaks: Vec<(f64, Option<usize>)> = grid
        .iter()
        .enumerate()
        .map(|(k, &g)| (g, Some(k)))
        .collect();
    for segs in schedule.values() {
        for s in segs {
            if s.until > eps && s.until < horizon - eps {
                breaks.push((s.until, None));
            }
        }
    }
    breaks.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.1.is_some().cmp(&a.1.is_some())));
    breaks.dedup_by(|later, earlier| {
        if (later.0 - earlier.0).abs() <= eps {
            earlier.1 = earlier.1.or(later.1);
            true
        } else {
            false
        }
    });
    let mut state = cn.state.clone();
    let mut values: Vec<Vec<f64>> = cn
        .order
        .iter()
        .map(|(label, _)| {
            let mut v = Vec::with_capacity(grid.len());
            v.push(state.get(label).copied().unwrap_or(0.0));
            v
        })
        .collect();
    let mut t_out = vec![0.0];
    let mut stopped = None;
    let mut lo = 0.0;
    for &(b, k) in &breaks[1..] {
        let span = b - lo;
        if span > 0.0 && stopped.is_none() {
            let mid = lo + span / 2.0;
            let mut rates = cn.rates.clone();
            for (t, segs) in schedule {
                rates.insert(t.clone(), rate_at(segs, mid));
            }
            let prob = Problem::new(cn.net.clone(), state.clone(), [0.0, span], rates);
            let sol = solve(&prob, solver, opts);
            if let Some(end) = sol.u.last() {
                for (label, v) in state.iter_mut() {
                    *v = end.get(label).copied().unwrap_or(*v);
                }
            }
            if let Some((label, _)) = state.iter().find(|(_, v)| !v.is_finite()) {
                stopped = Some(format!(
                    "the state of {label} is not finite at t = {}",
                    num(b)
                ));
            }
        }
        if k.is_some() {
            t_out.push(b);
            for (i, (label, _)) in cn.order.iter().enumerate() {
                values[i].push(state.get(label).copied().unwrap_or(0.0));
            }
        }
        lo = b;
    }
    ModelRun {
        t: t_out,
        names: cn.order.iter().map(|(_, d)| d.clone()).collect(),
        values,
        stopped,
    }
}

// --------------------------------------------------------------- schedules

/// The empirical per-interval rate of change of `(times, values)`, scaled,
/// as a schedule (sim's `scheduleFrom`): repeated times keep their last
/// value; more than `budget` intervals are read on a uniform grid.
pub fn schedule_from(
    times: &[f64],
    values: &[f64],
    factor: f64,
    budget: usize,
) -> (Vec<RateSegment>, bool, bool) {
    let (mut ts, mut vs) = fold_times(times, values);
    if budget > 0 && ts.len() > budget + 1 {
        let (start, span) = (ts[0], ts[ts.len() - 1] - ts[0]);
        let (mut gt, mut gv) = (vec![], vec![]);
        let mut j = 0;
        for k in 0..=budget {
            let g = if k == budget {
                ts[ts.len() - 1]
            } else {
                start + span * k as f64 / budget as f64
            };
            while j + 1 < ts.len() && ts[j + 1] <= g {
                j += 1;
            }
            gt.push(g);
            gv.push(vs[j]);
        }
        ts = gt;
        vs = gv;
    }
    let (mut clamped, mut overflow) = (false, false);
    let mut segs = vec![];
    for i in 1..ts.len() {
        let mut rate = (vs[i] - vs[i - 1]) / (ts[i] - ts[i - 1]) * factor;
        if !rate.is_finite() {
            overflow = true;
        }
        if rate < 0.0 {
            rate = 0.0;
            clamped = true;
        }
        segs.push(RateSegment {
            until: ts[i],
            value: rate,
        });
    }
    (segs, clamped, overflow)
}

/// [`schedule_from`] on the fixed grid of `budget` intervals over
/// `[0, horizon]` (sim's `scheduleOnGrid`), used by delayed and loop
/// bindings so every sweep's schedules share their boundaries.
pub fn schedule_on_grid(
    times: &[f64],
    values: &[f64],
    factor: f64,
    budget: usize,
    horizon: f64,
) -> (Vec<RateSegment>, bool, bool) {
    let (ts, vs) = fold_times(times, values);
    if ts.is_empty() {
        return (vec![], false, false);
    }
    let eps = horizon * 1e-9;
    let (mut clamped, mut overflow) = (false, false);
    let mut segs = vec![];
    let (mut prev, mut j) = (0.0, 0);
    for k in 0..=budget {
        let g = grid_at(horizon, k, budget);
        while j + 1 < ts.len() && ts[j + 1] <= g + eps {
            j += 1;
        }
        let v = vs[j];
        if k > 0 {
            let mut rate = (v - prev) / (g - grid_at(horizon, k - 1, budget)) * factor;
            if !rate.is_finite() {
                overflow = true;
            }
            if rate < 0.0 {
                rate = 0.0;
                clamped = true;
            }
            segs.push(RateSegment {
                until: g,
                value: rate,
            });
        }
        prev = v;
    }
    (segs, clamped, overflow)
}

fn fold_times(times: &[f64], values: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let (mut ts, mut vs): (Vec<f64>, Vec<f64>) = (vec![], vec![]);
    for (&t, &v) in times.iter().zip(values) {
        if let Some(&last) = ts.last() {
            if t <= last {
                *vs.last_mut().expect("parallel") = v;
                continue;
            }
        }
        ts.push(t);
        vs.push(v);
    }
    (ts, vs)
}

/// A schedule delivered `d` hours later behind `seed`; the flow carried past
/// the horizon is returned as in-transit.
pub fn shift_schedule(
    seed: &[RateSegment],
    segs: &[RateSegment],
    d: f64,
    horizon: f64,
) -> (Vec<RateSegment>, f64) {
    let mut out = seed.to_vec();
    let mut transit = 0.0;
    let mut lo = 0.0;
    for s in segs {
        let (from, to) = (lo + d, s.until + d);
        if from < horizon {
            out.push(RateSegment {
                until: to.min(horizon),
                value: s.value,
            });
        }
        if to > horizon {
            transit += s.value * (to - horizon.max(from));
        }
        lo = s.until;
    }
    (out, transit)
}

/// ∫ schedule over [0, horizon] (the last segment holds to the horizon).
pub fn delivered(segs: &[RateSegment], horizon: f64) -> f64 {
    let mut lo = 0.0;
    let mut total = 0.0;
    for s in segs {
        let hi = s.until.min(horizon);
        if hi > lo {
            total += s.value * (hi - lo);
        }
        lo = lo.max(hi);
    }
    if let Some(last) = segs.last() {
        if lo < horizon {
            total += last.value * (horizon - lo);
        }
    }
    total
}

// --------------------------------------------------------------------- runs

/// What a pipeline run produced.
#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub order: Vec<String>,
    /// One run per model, in the order they ran (empty when a loop did not
    /// settle).
    pub steps: Vec<(String, ModelRun)>,
    /// Per binding (by id), the schedule its target ran on last.
    pub schedules: BTreeMap<String, Vec<RateSegment>>,
    /// Per delayed binding, the flow its delay carried past the horizon.
    pub in_transit: BTreeMap<String, f64>,
    pub assumptions: Vec<String>,
    /// The loop reading (pipeline-loop-v1), when the set holds a loop.
    pub loop_reading: Option<Value>,
    pub hours: f64,
}

impl PipelineResult {
    /// fixed-point / converged / stopped / not-settled for a loop; "acyclic"
    /// otherwise.
    pub fn verdict(&self) -> String {
        self.loop_reading
            .as_ref()
            .and_then(|l| l["verdict"].as_str())
            .unwrap_or("acyclic")
            .to_string()
    }
}

enum SchedErr {
    Fatal(Error),
    Overflow(String),
}

struct Built {
    segs: Vec<RateSegment>,
    notes: Vec<String>,
    in_transit: f64,
}

struct Run<'a> {
    plan: &'a Plan,
    runners: BTreeMap<String, Runner>,
    inputs: &'a BTreeMap<String, ModelInput>,
    opts: &'a RunOptions,
}

impl Run<'_> {
    fn declared_rate(&self, b: &Binding) -> f64 {
        self.inputs
            .get(&b.to.model)
            .and_then(|m| m.rates.get(&b.to.port).copied())
            .unwrap_or(1.0)
    }

    /// Before a delay ends the target runs on its declared rate (documents
    /// declare no schedules, so sim's seed history is one segment).
    fn seed_history(&self, b: &Binding) -> Vec<RateSegment> {
        vec![RateSegment {
            until: b.delay,
            value: self.declared_rate(b),
        }]
    }

    /// Sweep 1's schedule for a delayed loop binding: the seed history, then
    /// the declared rate on every later grid segment shifted by the delay.
    fn seed_full(&self, b: &Binding) -> Vec<RateSegment> {
        let h = self.opts.hours;
        let mut segs = self.seed_history(b);
        for k in 1..=SEGMENT_BUDGET {
            let lo = grid_at(h, k - 1, SEGMENT_BUDGET) + b.delay;
            if lo >= h {
                break;
            }
            segs.push(RateSegment {
                until: (grid_at(h, k, SEGMENT_BUDGET) + b.delay).min(h),
                value: self.declared_rate(b),
            });
        }
        segs
    }

    fn delay_lines(&self, id: &str, b: &Binding) -> Vec<String> {
        let declared = match self
            .inputs
            .get(&b.to.model)
            .and_then(|m| m.rates.get(&b.to.port))
        {
            Some(r) => format!("declared rate {}/h", num(*r)),
            None => "default rate 1/h (it declares none)".into(),
        };
        vec![
            format!(
                "the delay {} h on binding {id} ({}.{} -> {}.{}) is declared, not measured",
                num(b.delay),
                b.from.model,
                b.from.port,
                b.to.model,
                b.to.port
            ),
            format!(
                "on 0–{} h {}.{} ran on its {declared}, not from {}",
                num(b.delay.min(self.opts.hours)),
                b.to.model,
                b.to.port,
                b.from.model
            ),
        ]
    }

    fn schedule(
        &self,
        id: &str,
        b: &Binding,
        driving: &ModelRun,
        in_loop: bool,
    ) -> std::result::Result<Built, SchedErr> {
        let values = driving.series(&b.from.port).ok_or_else(|| {
            SchedErr::Fatal(Error::Invalid(format!(
                "model {}'s own result carries no series for place {:?} — cannot build a schedule for {}.{}",
                b.from.model, b.from.port, b.to.model, b.to.port
            )))
        })?;
        let head = b.head();
        let mut notes = vec![];
        let (mut factor, note) = scale_factor(&b.transform);
        if !note.is_empty() {
            notes.push(format!("{head}: {note}"));
        }
        if let Some(cv) = &b.convert {
            factor *= cv.factor;
        }
        let (mut segs, clamped, overflow) = if b.delay != 0.0 || in_loop {
            schedule_on_grid(&driving.t, &values, factor, SEGMENT_BUDGET, self.opts.hours)
        } else {
            schedule_from(&driving.t, &values, factor, SEGMENT_BUDGET)
        };
        if overflow {
            return Err(SchedErr::Overflow(format!(
                "{head}: the source's resampled rate times the transform{} is not a finite number, so no schedule can be built for {} and it was not run; bind with a smaller factor, or run a longer horizon",
                if b.convert.is_some() { " and the declared conversion factor" } else { "" },
                b.to.model
            )));
        }
        if clamped {
            notes.push(format!(
                "{head}: the source series fell between some samples, which would have implied a negative rate; those segments were clamped to zero"
            ));
        }
        let mut resampled = format!(
            "{head}: {}'s schedule was resampled from {}'s own run at this pipeline's shared horizon{}, not measured independently",
            b.to.model,
            b.from.model,
            if self.opts.engine == Engine::Ssa { " and seed" } else { "" }
        );
        let mut in_transit = 0.0;
        if b.delay != 0.0 {
            let (shifted, transit) =
                shift_schedule(&self.seed_history(b), &segs, b.delay, self.opts.hours);
            segs = shifted;
            in_transit = transit;
            resampled.push_str(&format!(
                ", and delivered {} h after the source produced it",
                num(b.delay)
            ));
        }
        notes.push(resampled);
        if b.delay != 0.0 && !in_loop {
            notes.extend(self.delay_lines(id, b));
        }
        Ok(Built {
            segs,
            notes,
            in_transit,
        })
    }

    fn run_model(&self, model: &str, schedule: &Schedule) -> Result<ModelRun> {
        let r = &self.runners[model];
        for t in schedule.keys() {
            if !r.has_transition(t) {
                return Err(Error::Invalid(format!(
                    "model {model} has no transition {t:?} to drive"
                )));
            }
        }
        r.run(schedule)
            .map_err(|e| Error::Invalid(format!("running model {model}: {e}")))
    }
}

/// Runs every model a binding set touches as one pipeline (sim's
/// `RunPipeline`). `models` must hold every model the bindings name.
pub fn run_pipeline(
    bindings: Vec<(String, Binding)>,
    models: &BTreeMap<String, ModelInput>,
    opts: &RunOptions,
) -> Result<PipelineResult> {
    if bindings.is_empty() {
        return Err(Error::BadArgs(
            "no bindings given: a pipeline needs at least one".into(),
        ));
    }
    if bindings.len() > MAX_SYSTEM_BINDINGS {
        return Err(Error::BadArgs(format!(
            "{} bindings, over the {MAX_SYSTEM_BINDINGS}-binding limit for one pipeline; run part of the set",
            bindings.len()
        )));
    }
    for (_, b) in &bindings {
        if let Some(reason) = delay_check(b) {
            return Err(Error::Invalid(format!("delay-invalid: {reason}")));
        }
    }
    let plan = plan(bindings)?;
    if opts.realizations > st::MAX_RUNS {
        return Err(Error::BadArgs(format!(
            "realizations {} is outside 0..{}",
            opts.realizations,
            st::MAX_RUNS
        )));
    }
    if !(opts.hours > 0.0 && opts.hours <= st::MAX_HORIZON) {
        return Err(Error::BadArgs(format!(
            "hours {} is outside (0, {}]",
            opts.hours,
            num(st::MAX_HORIZON)
        )));
    }
    if !(opts.tolerance >= 0.0 && opts.tolerance.is_finite()) {
        return Err(Error::BadArgs(format!(
            "tolerance must be a finite number ≥ 0, got {}",
            opts.tolerance
        )));
    }
    if let Some(s) = opts.sweeps {
        if !(2..=MAX_LOOP_SWEEPS).contains(&s) {
            return Err(Error::BadArgs(format!(
                "sweeps {s} is outside 2..{MAX_LOOP_SWEEPS} (the second sweep is the first that can confirm a fixed point)"
            )));
        }
    }
    let sweeps = loop_hours_check(&plan, opts.hours)?;

    let mut runners = BTreeMap::new();
    for m in &plan.order {
        let input = models.get(m).ok_or_else(|| {
            Error::NotFound(format!(
                "binding references model {m}, which was not resolvable"
            ))
        })?;
        runners.insert(m.clone(), Runner::new(m, input, opts)?);
    }
    let mut unit_lines = vec![];
    for (_, b) in &plan.bindings {
        let c = check_binding(&models[&b.from.model].doc, &models[&b.to.model].doc, b);
        if !c.is_ok() {
            return Err(c.to_error());
        }
        unit_lines.push(unit_line(b, &c));
    }
    let r = Run {
        plan: &plan,
        runners,
        inputs: models,
        opts,
    };

    let mut assumptions = vec![format!(
        "this run composes {} model(s) via {} binding(s); no model's own checks cover whether the JOIN between them is sound (units are compared only where both ports declare one; a resample window coarser than the downstream model's own dynamics)",
        plan.order.len(),
        plan.bindings.len()
    )];
    assumptions.extend(unit_lines);
    assumptions.push(match opts.engine {
        Engine::Ssa => format!("a binding couples means: every model a binding drives ran on its source's mean across {} realization(s), resampled into a schedule of up to {} segments, so a driven model's spread is its own spread given that mean, not the spread the joint system would show", opts.realizations.max(1), SEGMENT_BUDGET + 1),
        Engine::Ode => format!("each model ran as a mass-action ODE (token counts as real-valued concentrations); a binding resamples its source's trajectory into a piecewise-constant schedule of up to {} segments, which smooths anything shorter than one resample window (hours/{SEGMENT_BUDGET})", SEGMENT_BUDGET + 1),
    });

    let mut results: BTreeMap<String, ModelRun> = BTreeMap::new();
    let mut steps = vec![];
    let mut schedules = BTreeMap::new();
    let mut in_transit = BTreeMap::new();
    let mut reading: Option<Map<String, Value>> = None;
    if !plan.loops.is_empty() {
        let mut m = Map::new();
        m.insert("definition".into(), json!(DEF_PIPELINE_LOOP_V1));
        m.insert("verdict".into(), json!(LOOP_FIXED_POINT));
        m.insert("components".into(), json!([]));
        m.insert("sweepsRun".into(), json!(0));
        m.insert("residual".into(), json!([]));
        reading = Some(m);
    }
    let mut claims = vec![];
    let mut loop_lines = vec![];
    for (ci, pc) in plan.components.iter().enumerate() {
        let Some(li) = pc.lp else {
            let id = &pc.models[0];
            let mut sched = Schedule::new();
            for &bi in plan.incoming.get(id).map(Vec::as_slice).unwrap_or(&[]) {
                let (bid, b) = &plan.bindings[bi];
                let built = match r.schedule(bid, b, &results[&b.from.model], false) {
                    Ok(x) => x,
                    Err(SchedErr::Fatal(e)) => return Err(e),
                    Err(SchedErr::Overflow(m)) => return Err(Error::Invalid(m)),
                };
                assumptions.extend(built.notes);
                if b.delay != 0.0 {
                    in_transit.insert(bid.clone(), built.in_transit);
                }
                schedules.insert(bid.clone(), built.segs.clone());
                sched.insert(b.to.port.clone(), built.segs);
            }
            let res = r.run_model(id, &sched)?;
            if let Some(why) = &res.stopped {
                return Err(Error::Invalid(format!("model {id} stopped: {why}")));
            }
            results.insert(id.clone(), res.clone());
            steps.push((id.clone(), res));
            continue;
        };
        let pl = &plan.loops[li];
        let out = run_loop(&r, pl, sweeps[li])?;
        let rd = reading.as_mut().expect("a plan with loops has a reading");
        rd["components"]
            .as_array_mut()
            .expect("array")
            .push(out.component.clone());
        let total = rd["sweepsRun"].as_u64().unwrap_or(0) + out.sweeps_run as u64;
        rd["sweepsRun"] = json!(total);
        rd["residual"]
            .as_array_mut()
            .expect("array")
            .extend(out.residual.clone());
        if out.verdict != LOOP_FIXED_POINT && out.verdict != LOOP_CONVERGED {
            rd["verdict"] = json!(out.verdict);
            let mut reason = out.reason.clone();
            let rest: Vec<String> = plan.components[ci + 1..]
                .iter()
                .flat_map(|c| c.models.clone())
                .collect();
            if !rest.is_empty() {
                reason.push_str(&format!("; not run: {}", rest.join(", ")));
            }
            rd.insert("reason".into(), json!(reason));
            rd.insert("remedy".into(), json!(out.remedy));
            return Ok(PipelineResult {
                order: plan.order.clone(),
                steps: vec![],
                schedules,
                in_transit,
                assumptions,
                loop_reading: reading.map(Value::Object),
                hours: opts.hours,
            });
        }
        if out.verdict == LOOP_CONVERGED {
            rd["verdict"] = json!(LOOP_CONVERGED);
        }
        for m in &pl.models {
            let res = out.results[m].clone();
            results.insert(m.clone(), res.clone());
            steps.push((m.clone(), res));
        }
        for (id, segs) in out.schedules {
            schedules.insert(id, segs);
        }
        let mut transit = Map::new();
        for id in &pl.delayed {
            let v = out.in_transit.get(id).copied().unwrap_or(0.0);
            in_transit.insert(id.clone(), v);
            transit.insert(id.clone(), json!(v));
        }
        let entry = rd.entry("inTransit").or_insert_with(|| json!({}));
        if let Some(o) = entry.as_object_mut() {
            o.extend(transit);
        }
        assumptions.extend(out.notes);
        claims.push(out.claim);
        loop_lines.extend(out.limits);
    }
    if let Some(rd) = reading.as_mut() {
        let claim = claims.join("; and ");
        rd.insert("claim".into(), json!(claim));
        let mut head = vec![claim, match opts.engine {
            Engine::Ssa => format!("mean-field: each model was driven by the other's mean across {} realization(s), so nothing correlates across the seam; for joint dynamics put both in one net", opts.realizations.max(1)),
            Engine::Ode => "mean-field: each model was driven by the other's deterministic (ODE) trajectory; for joint dynamics put both in one net".into(),
        }];
        head.extend(loop_lines);
        head.push(format!(
            "nothing is claimed about the loop beyond {} h: a fixed point can be a loop that grows; read the series",
            num(opts.hours)
        ));
        head.append(&mut assumptions);
        assumptions = head;
    }
    Ok(PipelineResult {
        order: plan.order.clone(),
        steps,
        schedules,
        in_transit,
        assumptions,
        loop_reading: reading.map(Value::Object),
        hours: opts.hours,
    })
}

struct LoopOutcome {
    component: Value,
    residual: Vec<Value>,
    sweeps_run: usize,
    verdict: &'static str,
    reason: String,
    remedy: String,
    claim: String,
    results: BTreeMap<String, ModelRun>,
    schedules: BTreeMap<String, Vec<RateSegment>>,
    in_transit: BTreeMap<String, f64>,
    notes: Vec<String>,
    limits: Vec<String>,
}

/// Compares the cut schedule a sweep ran on with the one re-derived from it:
/// (largest |difference|, time through which they agree bit for bit, same).
fn residual(ran: &[RateSegment], next: &[RateSegment], horizon: f64) -> (f64, f64, bool) {
    let mut max_diff: f64 = 0.0;
    let mut first: Option<usize> = None;
    for (i, (a, b)) in ran.iter().zip(next).enumerate() {
        max_diff = max_diff.max((a.value - b.value).abs());
        if first.is_none()
            && (a.until.to_bits() != b.until.to_bits() || a.value.to_bits() != b.value.to_bits())
        {
            first = Some(i);
        }
    }
    if first.is_none() && ran.len() != next.len() {
        first = Some(ran.len().min(next.len()));
    }
    match first {
        None => (max_diff, horizon, true),
        Some(0) => (max_diff, 0.0, false),
        Some(i) => (max_diff, ran[i - 1].until, false),
    }
}

/// One loop by sweeps (sim's `runLoop`), with `n` the loop's N.
fn run_loop(r: &Run, pl: &PlanLoop, n: usize) -> Result<LoopOutcome> {
    let h = r.opts.hours;
    let needed = r.opts.sweeps.unwrap_or(n + 1).min(MAX_LOOP_SWEEPS);
    let (delta, lag, _) = loop_sweeps(pl, h);
    let mut out = LoopOutcome {
        component: json!({
            "models": pl.models,
            "bindings": pl.bindings,
            "delayed": pl.delayed,
            "totalDelay": pl.total_delay,
            "cut": pl.cut,
            "lag": lag,
            "sweepsNeeded": n + 1,
            "sweepsAllowed": needed,
            "sweepsRun": 0,
        }),
        residual: vec![],
        sweeps_run: 0,
        verdict: LOOP_NOT_SETTLED,
        reason: String::new(),
        remedy: String::new(),
        claim: String::new(),
        results: BTreeMap::new(),
        schedules: BTreeMap::new(),
        in_transit: BTreeMap::new(),
        notes: vec![],
        limits: vec![],
    };
    let into: BTreeMap<&str, (&str, &Binding)> = pl
        .models
        .iter()
        .zip(&pl.bindings)
        .map(|(m, id)| {
            (
                m.as_str(),
                (id.as_str(), r.plan.binding(id).expect("planned")),
            )
        })
        .collect();
    let cut = r.plan.binding(&pl.cut).expect("planned");
    let mut input = r.seed_full(cut);
    let mut input_notes: Vec<String> = vec![];
    let stop = |out: &mut LoopOutcome, k: usize, reason: String| {
        out.verdict = LOOP_STOPPED;
        out.reason = format!("sweep {k}: {reason}");
        out.remedy = if k == 1 {
            "this stopped in sweep 1, before any feedback went round the loop, so the loop's gain did not cause it: check the stopped model's own rates and the transform or conversion factor on the binding into it".into()
        } else {
            "a loop whose gain is above 1 grows every sweep until a run stops; lower a transform or conversion factor on the loop, or ask about fewer hours".into()
        };
    };
    for k in 1..=needed {
        let mut sweep: BTreeMap<String, ModelRun> = BTreeMap::new();
        let mut notes = vec![];
        let mut transit = BTreeMap::new();
        let mut scheds = BTreeMap::new();
        for m in &pl.models {
            let (id, b) = into[m.as_str()];
            let segs = if id == pl.cut {
                notes.extend(input_notes.iter().cloned());
                input.clone()
            } else {
                match r.schedule(id, b, &sweep[&b.from.model], true) {
                    Ok(built) => {
                        notes.extend(built.notes);
                        if b.delay != 0.0 {
                            transit.insert(id.to_string(), built.in_transit);
                        }
                        built.segs
                    }
                    Err(SchedErr::Overflow(msg)) => {
                        stop(&mut out, k, msg);
                        return Ok(out);
                    }
                    Err(SchedErr::Fatal(e)) => return Err(e),
                }
            };
            scheds.insert(id.to_string(), segs.clone());
            let sched: Schedule = [(b.to.port.clone(), segs)].into_iter().collect();
            let res = r
                .run_model(m, &sched)
                .map_err(|e| Error::Invalid(format!("sweep {k} of the loop {}: {e}", pl.cycle)))?;
            out.sweeps_run = k;
            out.component["sweepsRun"] = json!(k);
            if let Some(why) = &res.stopped {
                stop(&mut out, k, format!("{m} stopped: {why}"));
                return Ok(out);
            }
            sweep.insert(m.clone(), res);
        }
        let built = match r.schedule(&pl.cut, cut, &sweep[&cut.from.model], true) {
            Ok(b) => b,
            Err(SchedErr::Overflow(msg)) => {
                stop(&mut out, k, msg);
                return Ok(out);
            }
            Err(SchedErr::Fatal(e)) => return Err(e),
        };
        transit.insert(pl.cut.clone(), built.in_transit);
        if k >= 2 {
            let (max_diff, exact, same) = residual(&input, &built.segs, h);
            out.residual.push(json!({
                "sweep": k,
                "binding": pl.cut,
                "maxDiff": max_diff,
                "exactThrough": exact,
            }));
            let within = r.opts.tolerance > 0.0 && max_diff <= r.opts.tolerance;
            if same || within {
                out.verdict = if same {
                    LOOP_FIXED_POINT
                } else {
                    LOOP_CONVERGED
                };
                out.results = sweep;
                out.notes = notes;
                out.in_transit = transit;
                out.schedules = scheds;
                out.claim = if same {
                    format!(
                        "the loop {} through {} settled exactly: sweep {k} reproduced sweep {}'s schedules bit for bit, which, with {} h of declared delay against {} resample intervals of {:.4} h, is the unique fixed point of this coupling over 0–{} h{}",
                        pl.cycle, pl.bindings.join(", "), k - 1, num(pl.total_delay), pl.bindings.len(), delta, num(h),
                        if r.opts.engine == Engine::Ssa { format!(" at seed {}", r.opts.seed) } else { String::new() }
                    )
                } else {
                    format!(
                        "the loop {} through {} converged within tolerance {}: sweep {k}'s cut schedule differs from sweep {}'s by at most {} (exact through {} h); not the exact fixed point, which needs up to {} sweeps",
                        pl.cycle, pl.bindings.join(", "), num(r.opts.tolerance), k - 1, num(max_diff), num(exact), n + 1
                    )
                };
                for id in &pl.delayed {
                    out.limits
                        .extend(r.delay_lines(id, r.plan.binding(id).expect("planned")));
                }
                return Ok(out);
            }
        }
        input = built.segs;
        input_notes = built.notes;
    }
    out.verdict = LOOP_NOT_SETTLED;
    if needed < n + 1 {
        out.reason = format!(
            "sweep {needed} did not reproduce sweep {}'s schedules for the loop {}; the {} h of delay over {} bindings guarantees a fixed point only by sweep {}",
            needed - 1, pl.cycle, num(pl.total_delay), pl.bindings.len(), n + 1
        );
        out.remedy = format!(
            "allow more sweeps (sweeps ≥ {}), or pass a tolerance",
            n + 1
        );
    } else {
        out.reason = format!(
            "sweep {needed} did not reproduce sweep {}'s schedules for the loop {}, though its {} h of delay over {} bindings guarantees it by then",
            needed - 1, pl.cycle, num(pl.total_delay), pl.bindings.len()
        );
        out.remedy = format!(
            "this is a defect in the runner or engine, not a property of these models: sweep {needed} should have reproduced sweep {}; report it with these binding ids: {}",
            needed - 1,
            pl.bindings.join(", ")
        );
    }
    Ok(out)
}

// ------------------------------------------------------------ stored state

/// A stored system: a named set of models and the bindings between them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct System {
    pub name: String,
    /// Content id of the sorted lists and the name.
    pub id: String,
    /// Sorted, unique.
    pub models: Vec<String>,
    /// Sorted, unique binding ids.
    #[serde(default)]
    pub bindings: Vec<String>,
}

impl System {
    pub fn new(name: &str, mut models: Vec<String>, mut bindings: Vec<String>) -> Self {
        models.sort();
        models.dedup();
        bindings.sort();
        bindings.dedup();
        let id = content_id(&json!({"name": name, "models": models, "bindings": bindings}));
        Self {
            name: name.into(),
            id,
            models,
            bindings,
        }
    }
}

/// Bindings and systems: the state the `system.*` commands keep.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SystemsState {
    #[serde(default)]
    pub bindings: BTreeMap<String, Binding>,
    #[serde(default)]
    pub systems: BTreeMap<String, System>,
}

impl SystemsState {
    /// The systems naming binding `id`.
    pub fn systems_naming(&self, id: &str) -> Vec<String> {
        self.systems
            .values()
            .filter(|s| s.bindings.iter().any(|b| b == id) || s.models.iter().any(|m| m == id))
            .map(|s| s.name.clone())
            .collect()
    }
}

/// `base` with `over` on top (declared rates, then session rates).
pub fn rates_with(
    base: &BTreeMap<String, f64>,
    over: &BTreeMap<String, f64>,
) -> BTreeMap<String, f64> {
    let mut r = base.clone();
    r.extend(over.iter().map(|(k, v)| (k.clone(), *v)));
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bind(from: &str, to: &str, delay: f64) -> (String, Binding) {
        let b = Binding {
            from: PortRef {
                model: from.into(),
                port: "out".into(),
            },
            to: PortRef {
                model: to.into(),
                port: "in".into(),
            },
            transform: String::new(),
            convert: None,
            delay,
        };
        (binding_id(&b), b)
    }

    #[test]
    fn id_is_canonical() {
        let (id, b) = bind("a", "b", 0.0);
        assert_eq!(id.len(), 24);
        let mut b2 = b.clone();
        b2.delay = 2.0;
        assert_ne!(binding_id(&b2), id);
        let mut s = String::new();
        canonical(&serde_json::to_value(&b2).unwrap(), &mut s);
        assert_eq!(
            s,
            r#"{"delay":2,"from":{"model":"a","port":"out"},"to":{"model":"b","port":"in"}}"#
        );
    }

    #[test]
    fn plan_orders_and_refuses() {
        let p = plan(vec![bind("b", "c", 0.0), bind("a", "b", 0.0)]).unwrap();
        assert_eq!(p.order, vec!["a", "b", "c"]);
        let e = plan(vec![bind("a", "b", 0.0), bind("b", "a", 0.0)]).unwrap_err();
        assert_eq!(e.code, "closes-loop");
        let p = plan(vec![bind("a", "b", 0.0), bind("b", "a", 2.0)]).unwrap();
        assert_eq!(p.loops.len(), 1);
        assert_eq!(
            p.loops[0].models,
            vec!["a", "b"],
            "sweeps start at the cut's target"
        );
        assert!(loop_hours_check(&p, 8.0).is_ok());
        assert_eq!(
            loop_hours_check(&p, 120.0).unwrap_err().code,
            "loop-lag",
            "2 h over two 2-hour intervals"
        );
        assert!(max_hours(&p) > 8.0);
    }

    #[test]
    fn schedules() {
        let (segs, clamped, _) = schedule_from(&[0.0, 1.0, 2.0], &[0.0, 2.0, 1.0], 1.0, 60);
        assert_eq!(segs[0].value, 2.0);
        assert_eq!(segs[1].value, 0.0);
        assert!(clamped);
        let (shifted, transit) = shift_schedule(
            &[RateSegment {
                until: 1.0,
                value: 5.0,
            }],
            &[RateSegment {
                until: 2.0,
                value: 3.0,
            }],
            1.0,
            2.0,
        );
        assert_eq!(shifted.len(), 2);
        assert_eq!(transit, 3.0);
        assert_eq!(delivered(&shifted, 2.0), 8.0);
    }
}
