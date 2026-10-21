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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default = "default_role")]
    pub role: String,
    pub x: f64,
    pub y: f64,
}

fn default_role() -> String {
    "default".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arc {
    pub source: String,
    pub target: String,
    pub weight: Vec<f64>,
    /// Input side: an inhibitor arc. Output side: the editor's read arc.
    #[serde(default)]
    pub inhibit: bool,
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
                        });
                    }
                }
                "transitions" => {
                    for (id, t) in val.as_object().into_iter().flatten() {
                        doc.transitions.push(Transition {
                            id: id.clone(),
                            label: str_field(t, "label"),
                            role: str_field(t, "role").unwrap_or_else(default_role),
                            x: num_field(t, "x"),
                            y: num_field(t, "y"),
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
                        });
                    }
                }
                // Always written by to_value.
                "@context" | "@type" | "@version" => {}
                _ => {
                    doc.extra.insert(k.clone(), val.clone());
                }
            }
        }
        Ok(doc)
    }

    /// Writes the pflow.xyz editor shape, readable by pflow.xyz, go-pflow
    /// and `pflow_core::from_json`.
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
            if t.role != "default" {
                m.insert("role".into(), json!(t.role));
            }
            m.insert("x".into(), num(t.x));
            m.insert("y".into(), num(t.y));
            transitions.insert(t.id.clone(), Value::Object(m));
        }
        out.insert("transitions".into(), Value::Object(transitions));
        let arcs: Vec<Value> = self
            .arcs
            .iter()
            .map(|a| {
                json!({
                    "@type": "Arrow",
                    "source": a.source,
                    "target": a.target,
                    "weight": nums(&a.weight),
                    "inhibitTransition": a.inhibit,
                })
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
