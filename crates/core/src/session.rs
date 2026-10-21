//! Editing state around a [`Document`]: runtime marking, selection, view,
//! snapshot undo, the journal and macros. Everything a front end can change
//! is changed through [`Session::execute`]; the fields are public for
//! reading (rendering, inspection), not for writing around the registry.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::artifact::{Artifact, ArtifactData};
use crate::doc::Document;
use crate::registry::{self, Registry};
use crate::sim::{self, Compiled, Marking};
use crate::{Error, Result};

/// One recorded command: what a macro, a batch and the journal are made of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub command: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub args: Value,
}

/// The canvas viewport. Pure data, so the CLI and MCP can read and set it
/// too; the egui app renders from it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct View {
    /// World point shown at the canvas center.
    pub cx: f64,
    pub cy: f64,
    pub zoom: f64,
    /// Viewport size in screen points, reported by the app (`view.resize`)
    /// so `view.fit` means the same thing headless.
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub settings: Settings,
}

/// Display settings and panel layout: pure data, set through `view.set` /
/// `view.toggle`, persisted by the app between runs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "system", "dark" or "light".
    pub theme: Theme,
    /// Show weight badges on weight-1 arcs (pflow.xyz hides them).
    pub show_unit_weights: bool,
    /// The JSON-LD source pane, as pflow.xyz's editor pane.
    pub json_pane: bool,
    pub inspector: bool,
    pub bottom_panel: bool,
    /// Snap moved and new nodes to the 10-unit grid.
    pub snap: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            show_unit_weights: false,
            json_pane: false,
            inspector: true,
            bottom_panel: true,
            snap: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Dark,
    Light,
}

impl Default for View {
    fn default() -> Self {
        Self {
            cx: 350.0,
            cy: 250.0,
            zoom: 1.0,
            width: 1000.0,
            height: 700.0,
            settings: Settings::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    #[default]
    Select,
    Place,
    Transition,
    Arc,
    Token,
    Delete,
    Fire,
}

impl Tool {
    /// In pflow.xyz key order: 1-6 are the editing tools; Fire is play mode.
    pub const ALL: [Tool; 7] = [
        Tool::Select,
        Tool::Place,
        Tool::Transition,
        Tool::Arc,
        Tool::Token,
        Tool::Delete,
        Tool::Fire,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Tool::Select => "select",
            Tool::Place => "place",
            Tool::Transition => "transition",
            Tool::Arc => "arc",
            Tool::Token => "token",
            Tool::Delete => "delete",
            Tool::Fire => "fire",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == s)
    }
}

/// Work a command hands to the front end because it needs pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UiRequest {
    Screenshot { path: String },
}

#[derive(Clone)]
pub(crate) struct Snapshot {
    pub label: String,
    pub doc: Document,
    pub marking: Marking,
    pub selection: Vec<String>,
}

const HISTORY_LIMIT: usize = 200;
const JOURNAL_LIMIT: usize = 10_000;

pub struct Session {
    pub registry: &'static Registry,
    pub doc: Document,
    pub marking: Marking,
    /// Selected node ids, in selection order.
    pub selection: Vec<String>,
    pub view: View,
    pub tool: Tool,
    pub path: Option<String>,
    /// Bumped on every change to the document or marking.
    pub revision: u64,
    /// The document as last opened or saved; dirty = differs from it.
    pub saved_doc: Document,
    /// Mass-action rates by transition (session-only, as in pflow.xyz;
    /// absent = 1.0, 0 disables).
    pub rates: BTreeMap<String, f64>,
    /// Typed results of data-producing commands, newest last.
    pub artifacts: Vec<Artifact>,
    next_artifact: u64,
    /// Transitions fired since the last reset.
    pub fired: u64,
    /// Every journaled command run at top level, oldest first.
    pub journal: Vec<Step>,
    pub macros: BTreeMap<String, Vec<Step>>,
    /// Journal length when `macro.record` started.
    pub recording: Option<usize>,
    /// Nodes (and the arcs between them) from `edit.copy` / `edit.cut`.
    pub clipboard: Option<Document>,
    /// Set by a front end that can fulfil [`UiRequest`]s (the app).
    pub has_ui: bool,
    /// Requests only a UI can carry out (screenshots); the app drains them.
    pub ui_requests: Vec<UiRequest>,
    pub(crate) undo: Vec<Snapshot>,
    pub(crate) redo: Vec<Snapshot>,
    compiled: Option<std::result::Result<Compiled, String>>,
    depth: usize,
}

impl Default for Session {
    fn default() -> Self {
        Self::new(Document::default())
    }
}

impl Session {
    pub fn new(doc: Document) -> Self {
        Self::with_registry(doc, registry::registry())
    }

    /// A session over a larger registry (core sets plus an extension's).
    pub fn with_registry(doc: Document, registry: &'static Registry) -> Self {
        let marking = sim::initial_marking(&doc);
        let saved_doc = doc.clone();
        Self {
            registry,
            doc,
            marking,
            selection: vec![],
            view: View::default(),
            tool: Tool::Select,
            path: None,
            revision: 0,
            saved_doc,
            rates: BTreeMap::new(),
            artifacts: vec![],
            next_artifact: 0,
            fired: 0,
            journal: vec![],
            macros: BTreeMap::new(),
            recording: None,
            clipboard: None,
            has_ui: false,
            ui_requests: vec![],
            undo: vec![],
            redo: vec![],
            compiled: None,
            depth: 0,
        }
    }

    pub fn registry(&self) -> &'static Registry {
        self.registry
    }

    /// The one entry point: run command `id` with JSON `args`.
    pub fn execute(&mut self, id: &str, args: Value) -> Result<Value> {
        let spec = self
            .registry()
            .find(id)
            .ok_or_else(|| Error::UnknownCommand(suggest(self.registry, id)))?;
        let args = spec.check_args(args)?;
        if let Err(why) = (spec.enabled)(self) {
            return Err(Error::Invalid(format!("{id} is not available: {why}")));
        }
        self.depth += 1;
        let out = (spec.run)(self, &args);
        self.depth -= 1;
        if out.is_ok() && spec.journal && self.depth == 0 {
            self.journal.push(Step {
                command: id.to_string(),
                args: args.clone(),
            });
            if self.journal.len() > JOURNAL_LIMIT && self.recording.is_none() {
                self.journal.drain(..JOURNAL_LIMIT / 2);
            }
        }
        out
    }

    /// Runs `steps` as one undo step; on the first failure everything the
    /// batch did is rolled back and the error names the failing step.
    pub fn batch(&mut self, steps: &[Step]) -> Result<Value> {
        let before = self.snapshot("batch");
        let rev = self.revision;
        let view = self.view;
        let mut results = Vec::with_capacity(steps.len());
        self.depth += 1;
        for (i, step) in steps.iter().enumerate() {
            match self.execute(&step.command, step.args.clone()) {
                Ok(v) => results.push(v),
                Err(e) => {
                    self.depth -= 1;
                    self.restore(before);
                    self.view = view;
                    return Err(Error::Invalid(format!(
                        "step {i} ({}) failed, batch rolled back: {e}",
                        step.command
                    )));
                }
            }
        }
        self.depth -= 1;
        if self.revision != rev {
            self.push_undo(before);
        }
        Ok(json!({ "results": results }))
    }

    /// Records an undo point unless we are inside a batch, whose own
    /// snapshot covers it.
    fn push_undo(&mut self, snap: Snapshot) {
        if self.depth <= 1 {
            self.undo.push(snap);
            if self.undo.len() > HISTORY_LIMIT {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
    }

    pub(crate) fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            label: label.to_string(),
            doc: self.doc.clone(),
            marking: self.marking.clone(),
            selection: self.selection.clone(),
        }
    }

    pub(crate) fn restore(&mut self, s: Snapshot) {
        self.doc = s.doc;
        self.marking = s.marking;
        self.selection = s.selection;
        self.touch();
    }

    /// Applies `f` as one undoable edit; on error the session is restored.
    pub(crate) fn edit<T>(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        let snap = self.snapshot(label);
        match f(self) {
            Ok(v) => {
                self.push_undo(snap);
                self.touch();
                Ok(v)
            }
            Err(e) => {
                self.restore(snap);
                Err(e)
            }
        }
    }

    /// Call after any change to doc or marking.
    pub(crate) fn touch(&mut self) {
        self.revision += 1;
        self.compiled = None;
        let doc = &self.doc;
        self.selection.retain(|id| doc.kind_of(id).is_some());
    }

    pub fn is_dirty(&self) -> bool {
        self.doc != self.saved_doc
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(crate) fn compiled(&mut self) -> Result<&Compiled> {
        if self.compiled.is_none() {
            self.compiled = Some(Compiled::new(&self.doc).map_err(|e| e.to_string()));
        }
        match self.compiled.as_ref().expect("just set") {
            Ok(c) => Ok(c),
            Err(e) => Err(Error::Invalid(e.clone())),
        }
    }

    /// Transitions enabled at the current marking (empty if the net does
    /// not compile).
    pub fn enabled_transitions(&mut self) -> Vec<String> {
        let mk = self.marking.clone();
        self.compiled().map(|c| c.enabled(&mk)).unwrap_or_default()
    }

    pub(crate) fn fire(&mut self, t: &str) -> Result<()> {
        let mk = self.marking.clone();
        let next = self.compiled()?.fire(t, &mk)?;
        let snap = self.snapshot(&format!("fire {t}"));
        self.marking = next;
        self.push_undo(snap);
        self.fired += 1;
        self.revision += 1; // marking only: the compiled model stays valid
        Ok(())
    }

    pub(crate) fn reset_marking(&mut self) {
        self.marking = sim::initial_marking(&self.doc);
        self.fired = 0;
        self.revision += 1;
    }

    /// Stores a result and returns its id. Same-titled artifacts from the
    /// same command replace each other, so re-running the ODE keeps one plot.
    pub fn add_artifact(&mut self, title: String, source: Step, data: ArtifactData) -> String {
        self.artifacts
            .retain(|a| !(a.title == title && a.source.command == source.command));
        self.next_artifact += 1;
        let id = format!("a{}", self.next_artifact);
        self.artifacts.push(Artifact {
            id: id.clone(),
            title,
            source,
            data,
        });
        if self.artifacts.len() > 64 {
            self.artifacts.remove(0);
        }
        id
    }

    pub fn artifact(&self, id: &str) -> Option<&Artifact> {
        self.artifacts.iter().find(|a| a.id == id)
    }

    /// The newest artifact of a kind (`"timeseries"`, ...).
    pub fn latest(&self, kind: &str) -> Option<&Artifact> {
        self.artifacts.iter().rev().find(|a| a.data.kind() == kind)
    }

    pub fn tokens(&self, place: &str) -> i64 {
        self.marking.get(place).map(|v| v.iter().sum()).unwrap_or(0)
    }
}

fn suggest(reg: &Registry, id: &str) -> String {
    let best = reg
        .commands()
        .iter()
        .map(|c| (strsim(id, c.id), c.id))
        .max_by_key(|(s, _)| *s);
    match best {
        Some((s, near)) if s >= 3 => format!("{id:?} (did you mean {near:?}?)"),
        _ => format!("{id:?}"),
    }
}

/// Crude similarity: shared bigrams.
fn strsim(a: &str, b: &str) -> usize {
    let bi = |s: &str| -> Vec<(char, char)> { s.chars().zip(s.chars().skip(1)).collect() };
    let bb = bi(b);
    bi(a).iter().filter(|x| bb.contains(x)).count()
}
