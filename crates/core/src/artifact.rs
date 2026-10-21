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
    Report(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub label: String,
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
            ArtifactData::Report(_) => "report",
        }
    }

    /// Export formats this kind supports (JSON always).
    pub fn formats(&self) -> &'static [&'static str] {
        match self {
            ArtifactData::TimeSeries { .. } => &["json", "csv", "svg"],
            ArtifactData::Report(_) | ArtifactData::Graph { .. } => &["json"],
            _ => &["json", "csv"],
        }
    }
}
