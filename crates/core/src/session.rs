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

/// Nodes (and arcs) the canvas emphasizes: a siphon, an invariant's
/// support, a trace step's transition. Set by `view.highlight`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Highlight {
    pub label: String,
    pub nodes: Vec<String>,
    #[serde(default)]
    pub arcs: Vec<(String, String)>,
    /// Per-node numbers shown as badges (invariant coefficients).
    #[serde(default)]
    pub weights: BTreeMap<String, f64>,
}

/// A note pinned for the person at the window (`note.*`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub text: String,
    pub author: String,
    pub target: NoteTarget,
}

/// Where a note is pinned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NoteTarget {
    /// The net as a whole (shown in the Inspector and the canvas corner).
    Net,
    Node(String),
    Artifact(String),
    /// A canvas point, in world coordinates.
    Point {
        x: f64,
        y: f64,
    },
}

/// What the app last reported it shows (`view.report_focus`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Focus {
    /// False until a front end reports (headless).
    pub reported: bool,
    pub artifact: Option<String>,
    pub tab: Option<String>,
    pub bottom_panel: bool,
    pub window_visible: bool,
}

impl Default for Focus {
    fn default() -> Self {
        Self {
            reported: false,
            artifact: None,
            tab: None,
            bottom_panel: true,
            window_visible: true,
        }
    }
}

/// Work a command hands to the front end because it needs pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UiRequest {
    Screenshot {
        path: String,
    },
    /// Open this artifact in the Results tab.
    ShowArtifact {
        id: String,
    },
}

#[derive(Clone)]
pub(crate) struct Snapshot {
    pub label: String,
    pub doc: Document,
    pub marking: Marking,
    pub selection: Vec<String>,
    /// Session rates, only for steps that change them in bulk (calibrate /
    /// fit / distill with `apply`): undoing an unrelated edit must not
    /// rewind rate tweaks made since, so ordinary snapshots carry `None`.
    pub rates: Option<BTreeMap<String, f64>>,
    /// The open composition and search plan: they change only through
    /// undoable edits (`bundle.*`, `plan.*`), so every snapshot carries them.
    pub composition: Option<crate::composition::Composition>,
    pub search_plan: Option<crate::search_plan::OpenPlan>,
}

const HISTORY_LIMIT: usize = 200;
const JOURNAL_LIMIT: usize = 10_000;
/// Most results kept at once; the oldest goes first.
pub const MAX_ARTIFACTS: usize = 64;

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
    /// Named models besides the open one (`model.*`): what systems bind,
    /// diffs compare, and composition draws from. Not part of undo.
    pub models: BTreeMap<String, Document>,
    /// Port bindings and named systems over the library (`system.*`). Like
    /// the library, not part of undo.
    pub systems: crate::systems::SystemsState,
    /// What the canvas emphasizes (`view.highlight`).
    pub highlight: Option<Highlight>,
    /// Nodes (and the arcs between them) from `edit.copy` / `edit.cut`,
    /// their session rates in the transitions' `rate` fields.
    pub clipboard: Option<Document>,
    /// Set by a front end that can fulfil [`UiRequest`]s (the app).
    pub has_ui: bool,
    /// Requests only a UI can carry out (screenshots); the app drains them.
    pub ui_requests: Vec<UiRequest>,
    /// Background jobs (`job.*`).
    pub jobs: crate::jobs::Jobs,
    /// Notes pinned for the person at the window (`note.*`).
    pub notes: Vec<Note>,
    next_note: u64,
    /// What the app shows (`view.report_focus`, read by `view.focus`).
    pub focus: Focus,
    /// The open composition of pattern instances (`bundle.*`), for the
    /// composition views. Separate from the document; part of undo.
    pub composition: Option<crate::composition::Composition>,
    /// The open search plan (`plan.*`). Part of undo.
    pub search_plan: Option<crate::search_plan::OpenPlan>,
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
            models: BTreeMap::new(),
            systems: Default::default(),
            highlight: None,
            clipboard: None,
            has_ui: false,
            ui_requests: vec![],
            jobs: Default::default(),
            notes: vec![],
            next_note: 0,
            focus: Focus::default(),
            composition: None,
            search_plan: None,
            undo: vec![],
            redo: vec![],
            compiled: None,
            depth: 0,
        }
    }

    pub fn registry(&self) -> &'static Registry {
        self.registry
    }

    /// A copy of the session's data for a background job: the net, marking,
    /// rates, results, library and systems, with no history, UI or jobs.
    pub fn fork(&self) -> Session {
        let mut s = Session::with_registry(self.doc.clone(), self.registry);
        s.marking = self.marking.clone();
        s.selection = self.selection.clone();
        s.view = self.view;
        s.path = self.path.clone();
        s.revision = self.revision;
        s.saved_doc = self.saved_doc.clone();
        s.rates = self.rates.clone();
        s.artifacts = self.artifacts.clone();
        s.next_artifact = self.next_artifact;
        s.macros = self.macros.clone();
        s.models = self.models.clone();
        s.systems = self.systems.clone();
        s.notes = self.notes.clone();
        s.next_note = self.next_note;
        s.composition = self.composition.clone();
        s.search_plan = self.search_plan.clone();
        s
    }

    /// The one entry point: run command `id` with JSON `args`.
    pub fn execute(&mut self, id: &str, args: Value) -> Result<Value> {
        self.execute_as(id, args, true)
    }

    /// [`Session::execute`] without the journal entry, for a front end that
    /// syncs the session to state it keeps elsewhere (pflow.xyz's page
    /// loads its editor's document before every call): that is not an
    /// action of the user's, so a macro must not replay it. Checks, undo
    /// and results are those of `execute`.
    pub fn execute_unjournaled(&mut self, id: &str, args: Value) -> Result<Value> {
        self.execute_as(id, args, false)
    }

    fn execute_as(&mut self, id: &str, args: Value, journal: bool) -> Result<Value> {
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
        if out.is_ok() && journal && spec.journal && self.depth == 0 {
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
        // A batch that sets rates undoes them too (compose.extend's
        // set_rate ops, a macro that tunes rates).
        let before = if steps.iter().any(|st| st.command == "sim.set_rate") {
            self.snapshot_with_rates("batch")
        } else {
            self.snapshot("batch")
        };
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
        let rates_changed = before.rates.as_ref().is_some_and(|r| *r != self.rates);
        if self.revision != rev || rates_changed {
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
            rates: None,
            composition: self.composition.clone(),
            search_plan: self.search_plan.clone(),
        }
    }

    /// The open document as `file.save` writes it: the session rates merged
    /// back onto their transitions' `rate` fields. Every model CID is sealed
    /// over this, never over `doc` alone, so a CID names what a save of the
    /// same session would contain (and what pflow.xyz hashes).
    pub fn export_doc(&self) -> Document {
        crate::commands::file::with_rates(&self.doc, &self.rates)
    }

    /// A snapshot that also restores the session rates.
    pub(crate) fn snapshot_with_rates(&self, label: &str) -> Snapshot {
        Snapshot {
            rates: Some(self.rates.clone()),
            ..self.snapshot(label)
        }
    }

    /// Sets several rates as one undo step (rates absent from `updates`
    /// keep their values).
    pub(crate) fn set_rates(&mut self, updates: &BTreeMap<String, f64>, label: &str) {
        let snap = self.snapshot_with_rates(label);
        for (t, r) in updates {
            self.rates.insert(t.clone(), *r);
        }
        self.push_undo(snap);
        self.revision += 1;
    }

    pub(crate) fn restore(&mut self, s: Snapshot) {
        self.doc = s.doc;
        self.marking = s.marking;
        self.selection = s.selection;
        self.composition = s.composition;
        self.search_plan = s.search_plan;
        if let Some(r) = s.rates {
            self.rates = r;
        }
        self.touch();
    }

    /// Applies `f` as one undoable edit; on error the session is restored.
    /// [`Session::edit`] for edits that also change session rates
    /// (fusing a component that declares rates): undo restores both.
    pub(crate) fn edit_with_rates<T>(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        let snap = self.snapshot_with_rates(label);
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
        if let Some(h) = &mut self.highlight {
            h.nodes.retain(|id| doc.kind_of(id).is_some());
        }
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

    /// Replaces the marking as one undo step (trace replay).
    pub(crate) fn set_marking(&mut self, mk: Marking, label: &str) {
        let snap = self.snapshot(label);
        self.marking = mk;
        self.push_undo(snap);
        self.revision += 1;
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
        if self.artifacts.len() > MAX_ARTIFACTS {
            self.artifacts.remove(0);
        }
        id
    }

    pub(crate) fn next_note_id(&mut self) -> String {
        self.next_note += 1;
        format!("n{}", self.next_note)
    }

    /// The numbers behind the last artifact and note ids handed out
    /// (`session.dump_state`): the next ids follow them.
    pub(crate) fn id_counters(&self) -> (u64, u64) {
        (self.next_artifact, self.next_note)
    }

    /// Replaces the results and their id counter (`session.restore_state`,
    /// which checks that `next` is past every `a<n>` id in `items`).
    pub(crate) fn replace_artifacts(&mut self, items: Vec<Artifact>, next: u64) {
        self.artifacts = items;
        self.next_artifact = next;
    }

    /// Replaces the notes and their id counter, as
    /// [`Session::replace_artifacts`].
    pub(crate) fn replace_notes(&mut self, items: Vec<Note>, next: u64) {
        self.notes = items;
        self.next_note = next;
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
