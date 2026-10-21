//! Typed results. A data-producing command stores one of these in the
//! session and returns its JSON; the app's views render them by kind and
//! `export.artifact` writes them out. New analysis families add data here
//! (or reuse a kind), never a bespoke panel-only format.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::session::Step;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub title: String,
    /// The command that produced it, so it can be re-run or cited.
    pub source: Step,
    pub data: ArtifactData,
}

/// One named series, with an optional ± band (e.g. SSA standard deviation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Series {
    pub name: String,
    pub values: Vec<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub band: Option<Vec<f64>>,
    /// Overlay group (a scenario, a swept value): views color by series
    /// name and dash/label by group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

impl Series {
    pub fn new(name: impl Into<String>, values: Vec<f64>) -> Self {
        Self {
            name: name.into(),
            values,
            band: None,
            group: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ArtifactData {
    /// Values over a shared x axis (time, or a swept parameter).
    TimeSeries {
        x_label: String,
        x: Vec<f64>,
        series: Vec<Series>,
    },
    Heatmap {
        x_label: String,
        y_label: String,
        xs: Vec<f64>,
        ys: Vec<f64>,
        /// Row-major: `z[iy][ix]`.
        z: Vec<Vec<f64>>,
    },
    Matrix {
        rows: Vec<String>,
        cols: Vec<String>,
        values: Vec<Vec<f64>>,
    },
    Graph {
        nodes: Vec<GraphNode>,
        edges: Vec<GraphEdge>,
    },
    /// A firing sequence with the marking after each step.
    Trace {
        steps: Vec<TraceStep>,
    },
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<Value>>,
    },
    Scatter {
        x_label: String,
        y_label: String,
        points: Vec<(f64, f64)>,
        /// Indices into `points` on the Pareto front.
        #[serde(default)]
        highlight: Vec<usize>,
    },
    /// Node sets of the net (invariant supports, siphons, traps, orbits…).
    Sets {
        sets: Vec<NodeSet>,
    },
    /// A ranked bar chart (tornado when bars carry lo/hi).
    Bars {
        value_label: String,
        bars: Vec<Bar>,
    },
    Drawing {
        width: f64,
        height: f64,
        shapes: Vec<DrawShape>,
    },
    Report(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    /// Anything the producer wants to attach (a marking, flags such as
    /// `initial` / `deadlock`, a model id).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub data: Value,
}

/// A named set of nodes (places/transitions) of the net: an invariant's
/// support, a siphon, a trap, an orbit, a deadlock witness. Views list
/// them and highlight one on the canvas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeSet {
    pub label: String,
    pub nodes: Vec<String>,
    /// Per-node coefficients (an invariant's vector), if meaningful.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub weights: std::collections::BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// One bar of a ranked chart (tornado: `lo`/`hi` are the low/high-input
/// outcomes around `value`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    pub name: String,
    pub value: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lo: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hi: Option<f64>,
}

/// A vector picture in its own coordinate space (y down), for views no
/// other kind fits: string diagrams, side-by-side mappings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "lowercase")]
pub enum DrawShape {
    Line {
        points: Vec<(f64, f64)>,
        #[serde(default = "default_ink")]
        color: String,
        #[serde(default = "one")]
        width: f64,
        #[serde(default)]
        dashed: bool,
        /// Dash pattern (on, off) in drawing units; overrides `dashed`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dash: Option<(f64, f64)>,
    },
    /// A closed shape (arrowheads, regions).
    Polygon {
        points: Vec<(f64, f64)>,
        #[serde(default)]
        fill: Option<String>,
        #[serde(default)]
        stroke: Option<String>,
        #[serde(default = "one")]
        stroke_width: f64,
    },
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        #[serde(default)]
        fill: Option<String>,
        #[serde(default)]
        stroke: Option<String>,
        #[serde(default)]
        radius: f64,
        #[serde(default = "one")]
        stroke_width: f64,
    },
    Circle {
        x: f64,
        y: f64,
        r: f64,
        #[serde(default)]
        fill: Option<String>,
        #[serde(default)]
        stroke: Option<String>,
        #[serde(default = "one")]
        stroke_width: f64,
    },
    Text {
        x: f64,
        y: f64,
        text: String,
        #[serde(default = "default_size")]
        size: f64,
        /// "start", "middle" or "end".
        #[serde(default = "default_anchor")]
        anchor: String,
        #[serde(default = "default_ink")]
        color: String,
        #[serde(default)]
        bold: bool,
        /// Center the text vertically on `y` (SVG dominant-baseline
        /// central) instead of sitting on it as a baseline.
        #[serde(default)]
        central: bool,
    },
}

fn default_ink() -> String {
    "#444444".into()
}
fn one() -> f64 {
    1.0
}
fn default_size() -> f64 {
    12.0
}
fn default_anchor() -> String {
    "middle".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceStep {
    pub transition: String,
    pub marking: std::collections::BTreeMap<String, Vec<i64>>,
}

impl ArtifactData {
    pub fn kind(&self) -> &'static str {
        match self {
            ArtifactData::TimeSeries { .. } => "timeseries",
            ArtifactData::Heatmap { .. } => "heatmap",
            ArtifactData::Matrix { .. } => "matrix",
            ArtifactData::Graph { .. } => "graph",
            ArtifactData::Trace { .. } => "trace",
            ArtifactData::Table { .. } => "table",
            ArtifactData::Scatter { .. } => "scatter",
            ArtifactData::Sets { .. } => "sets",
            ArtifactData::Bars { .. } => "bars",
            ArtifactData::Drawing { .. } => "drawing",
            ArtifactData::Report(_) => "report",
        }
    }

    /// Export formats this kind supports (JSON always).
    pub fn formats(&self) -> &'static [&'static str] {
        match self {
            ArtifactData::TimeSeries { .. }
            | ArtifactData::Heatmap { .. }
            | ArtifactData::Scatter { .. } => &["json", "csv", "svg"],
            ArtifactData::Report(_) => &["json"],
            ArtifactData::Graph { .. } => &["json", "dot"],
            ArtifactData::Drawing { .. } => &["json", "svg"],
            _ => &["json", "csv"],
        }
    }

    /// Shape checks for data that did not come from our own commands.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            ArtifactData::TimeSeries { x, series, .. } => {
                for s in series {
                    if s.values.len() != x.len() {
                        return Err(format!(
                            "series {:?}: {} values for {} x",
                            s.name,
                            s.values.len(),
                            x.len()
                        ));
                    }
                    if s.band.as_ref().is_some_and(|b| b.len() != x.len()) {
                        return Err(format!("series {:?}: band length differs from x", s.name));
                    }
                }
            }
            ArtifactData::Heatmap { xs, ys, z, .. } => {
                if z.len() != ys.len() || z.iter().any(|r| r.len() != xs.len()) {
                    return Err("heatmap z must be ys.len() rows of xs.len() values".into());
                }
            }
            ArtifactData::Matrix { rows, cols, values } => {
                if values.len() != rows.len() || values.iter().any(|r| r.len() != cols.len()) {
                    return Err("matrix values must be rows × cols".into());
                }
            }
            ArtifactData::Table { columns, rows } => {
                if rows.iter().any(|r| r.len() != columns.len()) {
                    return Err("every table row needs one cell per column".into());
                }
            }
            ArtifactData::Graph { nodes, edges } => {
                let ids: std::collections::HashSet<&str> =
                    nodes.iter().map(|n| n.id.as_str()).collect();
                if let Some(e) = edges
                    .iter()
                    .find(|e| !ids.contains(e.source.as_str()) || !ids.contains(e.target.as_str()))
                {
                    return Err(format!(
                        "graph edge {} -> {} names a missing node",
                        e.source, e.target
                    ));
                }
            }
            ArtifactData::Scatter {
                points, highlight, ..
            } if highlight.iter().any(|&i| i >= points.len()) => {
                return Err("scatter highlight index out of range".into());
            }
            _ => {}
        }
        Ok(())
    }
}
