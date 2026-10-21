//! The document: a Petri net in the pflow.xyz editor JSON ("Shape A"),
//! held as ordered vectors so a load/save round trip keeps the file's own
//! order and any top-level fields this editor does not interpret.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    /// Per-color initial tokens (one entry per `Document::token`).
    pub initial: Vec<f64>,
    /// Per-color capacity; 0 means unbounded.
    pub capacity: Vec<f64>,
    pub x: f64,
    pub y: f64,
    /// Fields this editor does not model (`@type`, `offset`, ...), written
    /// back unchanged: they are part of the net's CID. Also the file's own
    /// `initial`/`capacity` arrays, verbatim, written back while they still
    /// read as the modeled vectors (see [`Document::to_value`]).
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    /// `None` when the file has no `role` (pflow.xyz's "default"). Kept
    /// apart from an explicit `"default"`, which is a triple of its own.
    #[serde(default)]
    pub role: Option<String>,
    pub x: f64,
    pub y: f64,
    /// Fields this editor does not model, written back unchanged.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

impl Transition {
    /// The role, `"default"` when unset.
    pub fn role(&self) -> &str {
        self.role.as_deref().unwrap_or("default")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arc {
    pub source: String,
    pub target: String,
    pub weight: Vec<f64>,
    /// Input side: an inhibitor arc. Output side: the editor's read arc.
    #[serde(default)]
    pub inhibit: bool,
    /// Fields this editor does not model (`@type`, ...), written back
    /// unchanged, and the file's own `weight` array (as for [`Place::extra`]).
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

impl Arc {
    pub fn weight_sum(&self) -> f64 {
        if self.weight.is_empty() {
            1.0
        } else {
            self.weight.iter().sum()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Place,
    Transition,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub token: Vec<String>,
    pub places: Vec<Place>,
    pub transitions: Vec<Transition>,
    pub arcs: Vec<Arc>,
    /// Top-level fields carried through untouched (`@context`, `name`, ...).
    #[serde(default)]
    pub extra: Map<String, Value>,
}

impl Document {
    /// Number of token colors: at least one, even when `token` is empty.
    pub fn colors(&self) -> usize {
        self.token.len().max(1)
    }

    pub fn place(&self, id: &str) -> Option<&Place> {
        self.places.iter().find(|p| p.id == id)
    }

    pub fn place_mut(&mut self, id: &str) -> Option<&mut Place> {
        self.places.iter_mut().find(|p| p.id == id)
    }

    pub fn transition(&self, id: &str) -> Option<&Transition> {
        self.transitions.iter().find(|t| t.id == id)
    }

    pub fn transition_mut(&mut self, id: &str) -> Option<&mut Transition> {
        self.transitions.iter_mut().find(|t| t.id == id)
    }

    pub fn kind_of(&self, id: &str) -> Option<NodeKind> {
        if self.place(id).is_some() {
            Some(NodeKind::Place)
        } else if self.transition(id).is_some() {
            Some(NodeKind::Transition)
        } else {
            None
        }
    }

    pub fn position(&self, id: &str) -> Option<(f64, f64)> {
        self.place(id)
            .map(|p| (p.x, p.y))
            .or_else(|| self.transition(id).map(|t| (t.x, t.y)))
    }

    pub fn arc_index(&self, source: &str, target: &str) -> Option<usize> {
        self.arcs
            .iter()
            .position(|a| a.source == source && a.target == target)
    }

    /// The first `{prefix}{n}` not already used by a node.
    pub fn fresh_id(&self, prefix: &str) -> String {
        (0..)
            .map(|n| format!("{prefix}{n}"))
            .find(|id| self.kind_of(id).is_none())
            .expect("unbounded")
    }

    /// Bounding box of all nodes: (min_x, min_y, max_x, max_y).
    pub fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let pts = self
            .places
            .iter()
            .map(|p| (p.x, p.y))
            .chain(self.transitions.iter().map(|t| (t.x, t.y)));
        pts.fold(None, |acc, (x, y)| match acc {
            None => Some((x, y, x, y)),
            Some((a, b, c, d)) => Some((a.min(x), b.min(y), c.max(x), d.max(y))),
        })
    }

    /// Reads a pflow.xyz editor document.
    pub fn from_json(data: &[u8]) -> Result<Self> {
        let v: Value = serde_json::from_slice(data).map_err(|e| Error::Parse(e.to_string()))?;
        Self::from_value(&v)
    }

    pub fn from_value(v: &Value) -> Result<Self> {
        let obj = v
            .as_object()
            .ok_or_else(|| Error::Parse("document is not a JSON object".into()))?;
        if !matches!(obj.get("places"), Some(Value::Object(_)) | None) {
            return Err(Error::Parse(
                "\"places\" must be an object keyed by id (pflow.xyz editor shape)".into(),
            ));
        }
        let mut doc = Document::default();
        for (k, val) in obj {
            match k.as_str() {
                "token" => {
                    doc.token = val
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .map(|t| t.as_str().unwrap_or_default().to_string())
                                .collect()
                        })
                        .unwrap_or_default()
                }
                "places" => {
                    for (id, p) in val.as_object().into_iter().flatten() {
                        doc.places.push(Place {
                            id: id.clone(),
                            label: str_field(p, "label"),
                            initial: float_vec(p.get("initial")),
                            capacity: float_vec(p.get("capacity")),
                            x: num_field(p, "x"),
                            y: num_field(p, "y"),
                            extra: rest(p, &["label", "x", "y"]),
                        });
                    }
                }
                "transitions" => {
                    for (id, t) in val.as_object().into_iter().flatten() {
                        doc.transitions.push(Transition {
                            id: id.clone(),
                            label: str_field(t, "label"),
                            role: str_field(t, "role"),
                            x: num_field(t, "x"),
                            y: num_field(t, "y"),
                            extra: rest(t, &["label", "role", "x", "y"]),
                        });
                    }
                }
                "arcs" => {
                    for a in val.as_array().into_iter().flatten() {
                        let source = str_field(a, "source")
                            .ok_or_else(|| Error::Parse("arc without source".into()))?;
                        let target = str_field(a, "target")
                            .ok_or_else(|| Error::Parse("arc without target".into()))?;
                        let weight = match a.get("weight") {
                            None | Some(Value::Null) => vec![1.0],
                            w => float_vec(w),
                        };
                        doc.arcs.push(Arc {
                            source,
                            target,
                            weight,
                            inhibit: a
                                .get("inhibitTransition")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            extra: rest(a, &["source", "target", "inhibitTransition"]),
                        });
                    }
                }
                // Always written by to_value. @type and @version are kept
                // (in `extra`): a net may be, say, a "ResourceNet".
                "@context" => {}
                _ => {
                    doc.extra.insert(k.clone(), val.clone());
                }
            }
        }
        Ok(doc)
    }

    /// Writes the pflow.xyz editor shape, readable by pflow.xyz, go-pflow
    /// and `pflow_core::from_json`.
    ///
    /// Loading and writing back keeps the net's CID ([`crate::cid`]): every
    /// field this editor does not model is carried in `extra` and written
    /// back (a place's `offset`, an element's own `@type`, a transition's
    /// explicit `"role": "default"`), because each one is an RDF triple of
    /// the preimage. Defaults are only filled in for elements that lack
    /// them, i.e. ones created here.
    ///
    /// Per-color vectors are written as the file had them while they still
    /// mean the same thing: `"capacity": [null, null]` is an empty RDF list
    /// (JSON-LD drops nulls from lists) where `[0, 0]` is a two-item one,
    /// and `[]` differs from the `[0]` that [`Document::normalize_colors`]
    /// pads it to. Trailing zeros are insignificant (no tokens, unbounded),
    /// as is `[]` versus `[1]` for a weight. Once an edit changes a vector,
    /// the modeled one is written. Number formatting does not matter (`1`
    /// and `1.0` are the same literal), nor does key order.
    pub fn to_value(&self) -> Value {
        let mut out = Map::new();
        out.insert("@context".into(), json!("https://pflow.xyz/schema"));
        out.insert("@type".into(), json!("PetriNet"));
        out.insert("@version".into(), json!("1.1"));
        for (k, v) in &self.extra {
            out.insert(k.clone(), v.clone());
        }
        out.insert("token".into(), json!(self.token));
        let mut places = Map::new();
        for (i, p) in self.places.iter().enumerate() {
            let mut m = Map::new();
            m.insert("@type".into(), json!("Place"));
            m.insert("offset".into(), json!(i));
            if let Some(l) = &p.label {
                m.insert("label".into(), json!(l));
            }
            m.insert("initial".into(), nums(&p.initial));
            m.insert("capacity".into(), nums(&p.capacity));
            m.insert("x".into(), num(p.x));
            m.insert("y".into(), num(p.y));
            overlay(&mut m, &p.extra, |k, v| match k {
                "initial" => same_vec(v, &p.initial, false),
                "capacity" => same_vec(v, &p.capacity, false),
                _ => true,
            });
            places.insert(p.id.clone(), Value::Object(m));
        }
        out.insert("places".into(), Value::Object(places));
        let mut transitions = Map::new();
        for t in &self.transitions {
            let mut m = Map::new();
            m.insert("@type".into(), json!("Transition"));
            if let Some(l) = &t.label {
                m.insert("label".into(), json!(l));
            }
            if let Some(r) = &t.role {
                m.insert("role".into(), json!(r));
            }
            m.insert("x".into(), num(t.x));
            m.insert("y".into(), num(t.y));
            overlay(&mut m, &t.extra, |_, _| true);
            transitions.insert(t.id.clone(), Value::Object(m));
        }
        out.insert("transitions".into(), Value::Object(transitions));
        let arcs: Vec<Value> = self
            .arcs
            .iter()
            .map(|a| {
                let mut v = json!({
                    "@type": "Arrow",
                    "source": a.source,
                    "target": a.target,
                    "weight": nums(&a.weight),
                    "inhibitTransition": a.inhibit,
                });
                overlay(v.as_object_mut().expect("object"), &a.extra, |k, v| {
                    k != "weight" || same_vec(v, &a.weight, true)
                });
                v
            })
            .collect();
        out.insert("arcs".into(), Value::Array(arcs));
        Value::Object(out)
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(&self.to_value()).expect("serializable")
    }

    /// Pads every per-color vector to the document's color count.
    pub fn normalize_colors(&mut self) {
        let n = self.colors();
        for p in &mut self.places {
            p.initial.resize(n, 0.0);
            p.capacity.resize(n, 0.0);
        }
        for a in &mut self.arcs {
            if a.weight.is_empty() {
                a.weight.push(1.0);
            }
            a.weight.resize(n, 0.0);
        }
    }
}

/// The fields of element `v` other than the modeled ones in `known`.
fn rest(v: &Value, known: &[&str]) -> Map<String, Value> {
    let mut m = v.as_object().cloned().unwrap_or_default();
    m.retain(|k, _| !known.contains(&k.as_str()));
    m
}

/// Writes carried-through fields over the defaults (a file's own `@type`
/// or `offset` wins, keeping its key position), those that `keep` allows.
fn overlay(
    m: &mut Map<String, Value>,
    extra: &Map<String, Value>,
    keep: impl Fn(&str, &Value) -> bool,
) {
    for (k, v) in extra {
        if keep(k, v) {
            m.insert(k.clone(), v.clone());
        }
    }
}

/// Whether a file's per-color array still reads as the modeled vector,
/// ignoring trailing zeros (and, for a weight, `[]` meaning `[1]`).
fn same_vec(raw: &Value, modeled: &[f64], weight: bool) -> bool {
    let canon = |mut v: Vec<f64>| {
        if weight && v.is_empty() {
            v.push(1.0);
        }
        while v.last() == Some(&0.0) {
            v.pop();
        }
        v
    };
    canon(float_vec(Some(raw))) == canon(modeled.to_vec())
}

fn str_field(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

fn num_field(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}

/// go-pflow's `toFloatSlice`: array (null/non-numeric slots -> 0), bare
/// scalar -> one component, absent/null -> empty.
fn float_vec(v: Option<&Value>) -> Vec<f64> {
    fn one(v: &Value) -> Option<f64> {
        match v {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.parse().ok(),
            _ => None,
        }
        .filter(|f: &f64| f.is_finite())
    }
    match v {
        None | Some(Value::Null) => vec![],
        Some(Value::Array(a)) => a.iter().map(|x| one(x).unwrap_or(0.0)).collect(),
        Some(x) => one(x).into_iter().collect(),
    }
}

/// Integral floats are written as JSON integers, so a saved file diffs
/// cleanly against one written by pflow.xyz.
fn num(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 9.0e15 {
        json!(f as i64)
    } else {
        json!(f)
    }
}

fn nums(v: &[f64]) -> Value {
    Value::Array(v.iter().map(|&f| num(f)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const COFFEE: &[u8] = include_bytes!("../../../examples/coffee-shop.json");

    #[test]
    fn round_trips_through_pflow_core() {
        let doc = Document::from_json(COFFEE).unwrap();
        assert_eq!(doc.places.len(), 4);
        assert_eq!(doc.transitions.len(), 5);
        assert_eq!(doc.arcs.len(), 8);
        assert_eq!(doc.colors(), 3);
        let out = doc.to_json_pretty();
        let again = Document::from_json(out.as_bytes()).unwrap();
        assert_eq!(doc, again);
        let net = pflow_core::from_json(out.as_bytes()).unwrap();
        assert_eq!(net.places["CoffeeBeans"].initial, vec![0.0, 5.0, 0.0]);
        assert_eq!(net.arcs.len(), 8);
    }

    #[test]
    fn keeps_unknown_top_level_fields() {
        let doc =
            Document::from_json(br#"{"name":"x","places":{},"transitions":{},"arcs":[]}"#).unwrap();
        assert_eq!(doc.to_value()["name"], json!("x"));
    }

    #[test]
    fn rejects_metamodel_shape() {
        assert!(Document::from_json(br#"{"places":[{"id":"p"}]}"#).is_err());
    }
}
