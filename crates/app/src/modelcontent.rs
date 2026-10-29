//! The "Model content" panel of the composition views: what the open model
//! means and what it lacks (docs/composition-documents.md §3), drawn under
//! the views.
//!
//! Everything shown is read from registry commands, never from the model
//! directly: `content.get` (the entries for the current view; for the
//! Connect-4 example `{subject: "plan"}`) and `bundle.state` (what the open
//! composition is missing). Both are queries, so [`State::refresh`] runs them
//! at most once per `(revision, view, example)` and the frames draw the
//! cached [`Model`]. That refresh is the only place this panel executes
//! anything. Running the checks goes the other way: the panel hands back
//! `content.check` steps, the app queues and runs them like any other
//! command, and the answer is read from the report that command stored.
//!
//! The entries of kind `scenario` are not listed: they are declared firing
//! scripts that the Process view and the lens test already draw.

use std::collections::{BTreeMap, VecDeque};

use eframe::egui::{self, text::LayoutJob, Color32, RichText, TextFormat};
use pflow_native_core::{composition as comp, ArtifactData, Session, Step};
use serde_json::{json, Value};

/// The sentence shown for a model with no content at all.
pub const BARE: &str = "This model is bare: no model content is attached, so there is no convention example, finding, caveat or note to show for it.";

/// The kinds listed under their own badge, in this order.
const KINDS: [&str; 4] = ["finding", "caveat", "judgment", "note"];

// ------------------------------------------------------------ the model

/// One content entry as the panel shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: String,
    pub title: String,
    pub text: String,
    /// Source cites, one monospace line each.
    pub cites: Vec<String>,
    /// The claim of the entry's check; `Some` when the entry has one.
    pub check: Option<String>,
}

/// One block of the panel, top to bottom.
#[derive(Clone, Debug, PartialEq)]
pub enum Section {
    /// The convention example for the view: the first paragraph.
    Convention(Row),
    /// The other entries of one kind (finding, caveat, judgment, note).
    Group { kind: String, rows: Vec<Row> },
    /// A plain sentence: the registry refused, or the content has nothing
    /// for this view.
    Notice(String),
    /// What the model lacks, right after the convention example: `bare` says
    /// it has no content at all, `items` are `bundle.state`'s `missing` as
    /// `(code, text)` and any dormant entries.
    Lacks {
        bare: bool,
        items: Vec<(String, String)>,
    },
}

/// What the panel shows for one view of one example.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    /// "orders · library:orders · 121 entries", for the header.
    pub head: String,
    pub sections: Vec<Section>,
}

impl Model {
    /// Every row, convention example first.
    pub fn rows(&self) -> impl Iterator<Item = &Row> {
        self.sections.iter().flat_map(|s| match s {
            Section::Convention(r) => std::slice::from_ref(r),
            Section::Group { rows, .. } => rows.as_slice(),
            _ => &[],
        })
    }

    /// The entries that carry a check.
    pub fn checked(&self) -> Vec<String> {
        self.rows()
            .filter(|r| r.check.is_some())
            .map(|r| r.id.clone())
            .collect()
    }
}

/// Builds the panel's model from the two registry answers: `content.get`'s
/// and, for the composition example, `bundle.state`'s. Pure.
pub fn model(
    view: &str,
    content: &Result<Value, String>,
    open: Option<&Result<Value, String>>,
) -> Model {
    let c = match content {
        Ok(c) => c,
        Err(e) => {
            return Model {
                head: String::new(),
                sections: vec![Section::Notice(format!("No model content to show: {e}"))],
            }
        }
    };
    let total = c["total"].as_u64().unwrap_or(0) as usize;
    let dormant = c["dormant"].as_u64().unwrap_or(0) as usize;
    let entries: Vec<&Value> = c["entries"].as_array().into_iter().flatten().collect();
    let rows_of = |kind: &str| -> Vec<Row> {
        entries
            .iter()
            .filter(|e| e["kind"] == kind)
            .map(|e| row(e))
            .collect()
    };

    let mut sections: Vec<Section> = rows_of("convention-example")
        .into_iter()
        .map(Section::Convention)
        .collect();
    let mut seen: Vec<&str> = KINDS.to_vec();
    seen.extend(["convention-example", "scenario"]);
    let others: Vec<&str> = entries
        .iter()
        .filter_map(|e| e["kind"].as_str())
        .filter(|k| !seen.contains(k))
        .fold(vec![], |mut v, k| {
            if !v.contains(&k) {
                v.push(k);
            }
            v
        });
    for kind in KINDS.iter().copied().chain(others) {
        let rows = rows_of(kind);
        if !rows.is_empty() {
            sections.push(Section::Group {
                kind: kind.into(),
                rows,
            });
        }
    }
    let bare = total == 0;
    if !bare && sections.is_empty() {
        sections.push(Section::Notice(format!(
            "This model's content has nothing to say in the {view} view."
        )));
    }

    // What the open composition lacks. NO_CONTENT is the bare sentence.
    let mut items: Vec<(String, String)> = match open {
        Some(Ok(s)) => s["missing"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| {
                Some((
                    m["code"].as_str()?.to_string(),
                    m["text"].as_str()?.to_string(),
                ))
            })
            .filter(|(code, _)| !(bare && code == "NO_CONTENT"))
            .collect(),
        _ => vec![],
    };
    if dormant > 0 {
        items.push((
            "DORMANT".into(),
            format!(
                "{dormant} content entries name elements this model does not have, so no view shows them"
            ),
        ));
    }
    // Right under the convention example, not at the end: the groups above
    // run to dozens of collapsed headers, and a reader must not have to
    // scroll past them to learn what the model lacks.
    if bare || !items.is_empty() {
        let at = sections
            .iter()
            .take_while(|s| matches!(s, Section::Convention(_)))
            .count();
        sections.insert(at, Section::Lacks { bare, items });
    }

    let name = c["name"].as_str().filter(|n| !n.is_empty());
    let source = c["source"].as_str().unwrap_or("");
    let head = match (name, bare) {
        (Some(n), false) => format!("{n} · {source} · {total} entries"),
        _ => format!("{source} · no entries"),
    };
    Model { head, sections }
}

fn row(e: &Value) -> Row {
    let text = e["text"].as_str().unwrap_or("").to_string();
    let id = e["id"].as_str().unwrap_or("").to_string();
    let title = match e["title"].as_str().filter(|t| !t.is_empty()) {
        Some(t) => t.to_string(),
        None => first_line(&text, &id),
    };
    Row {
        cites: e["cites"]
            .as_array()
            .into_iter()
            .flatten()
            .map(cite_line)
            .collect(),
        check: e
            .get("check")
            .filter(|c| c.is_object())
            .map(|c| c["claim"].as_str().unwrap_or("").to_string()),
        id,
        title,
        text,
    }
}

/// The header of an untitled entry: its text up to the first full stop, cut
/// at 90 characters; its id when it has no text either.
fn first_line(text: &str, id: &str) -> String {
    let s = text.split_once(". ").map_or(text, |(a, _)| a).trim();
    if s.is_empty() {
        return id.to_string();
    }
    if s.chars().count() > 90 {
        let cut: String = s.chars().take(89).collect();
        return format!("{cut}…");
    }
    s.to_string()
}

/// A cite as one line: "go-pflow metamodel/queue.go:71-174 @69257b3 (note)"
/// for a source cite, "J. C. Baez, J. Master, Open Petri Nets (2018)" for a
/// bibliography key.
fn cite_line(c: &Value) -> String {
    let o = comp::ontology();
    if let Some(key) = c["bib"].as_str() {
        let b = &o["bib"][key];
        return match (b["authors"].as_str(), b["title"].as_str()) {
            (Some(a), Some(t)) => match b["year"].as_u64() {
                Some(y) => format!("{a}, {t} ({y})"),
                None => format!("{a}, {t}"),
            },
            _ => format!("bib:{key}"),
        };
    }
    let repo = c["repo"].as_str().unwrap_or("");
    let r = &o["repos"][repo];
    let name = r["name"].as_str().unwrap_or(repo);
    let rev = r["rev"].as_str().or(c["rev"].as_str()).unwrap_or("");
    let (f, t) = (
        c["from"].as_u64().unwrap_or(0),
        c["to"].as_u64().unwrap_or(0),
    );
    let lines = match (f, t) {
        (0, _) => String::new(),
        (f, t) if t != 0 && t != f => format!(":{f}-{t}"),
        (f, _) => format!(":{f}"),
    };
    let note = c["note"].as_str().filter(|n| !n.is_empty());
    format!(
        "{name} {}{lines} @{rev}{}",
        c["path"].as_str().unwrap_or(""),
        note.map(|n| format!(" ({n})")).unwrap_or_default()
    )
}

// ------------------------------------------------------------ checks

/// What one `content.check` said about an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    /// The command was queued and no report came back for it.
    NoAnswer,
}

/// A `content.check` that was handed back and has not answered yet.
struct Inflight {
    entry: String,
    /// The id of the report of that command that was already stored when
    /// the step was handed back (its answer is a different id).
    stale: Option<String>,
}

/// The checks the panel ran, one `content.check {entry}` per frame so a long
/// run never stalls the window (the whole library takes seconds).
#[derive(Default)]
struct Checks {
    /// `composition_view.example` they ran for; another example drops them.
    example: String,
    /// The session revision they started at; a later revision marks them old.
    rev: u64,
    verdicts: BTreeMap<String, Verdict>,
    todo: VecDeque<String>,
    inflight: Option<Inflight>,
    /// How many the run began with.
    total: usize,
}

impl Checks {
    fn busy(&self) -> bool {
        !self.todo.is_empty() || self.inflight.is_some()
    }

    fn begin(&mut self, entries: Vec<String>, example: &str, rev: u64) {
        *self = Checks {
            example: example.into(),
            rev,
            total: entries.len(),
            todo: entries.into(),
            ..Default::default()
        };
    }

    /// Takes the answer to the step handed back last frame, then hands back
    /// the next step, if any.
    fn pump(&mut self, sess: &Session, plan: bool) -> Option<Step> {
        if let Some(f) = self.inflight.take() {
            let v = answer(sess, &f);
            self.verdicts.insert(f.entry, v);
        }
        let entry = self.todo.pop_front()?;
        self.inflight = Some(Inflight {
            stale: report_of(sess, None).map(|(id, _)| id),
            entry: entry.clone(),
        });
        let args = if plan {
            json!({"subject": "plan", "entry": entry})
        } else {
            json!({"entry": entry})
        };
        Some(Step {
            command: "content.check".into(),
            args,
        })
    }
}

/// The newest stored `content.check` report that is not `stale`, with its
/// artifact id.
fn report_of<'a>(sess: &'a Session, not: Option<&str>) -> Option<(String, &'a Value)> {
    sess.artifacts.iter().rev().find_map(|a| match &a.data {
        ArtifactData::Report(v)
            if a.source.command == "content.check" && Some(a.id.as_str()) != not =>
        {
            Some((a.id.clone(), v))
        }
        _ => None,
    })
}

fn answer(sess: &Session, f: &Inflight) -> Verdict {
    let Some((_, report)) = report_of(sess, f.stale.as_deref()) else {
        return Verdict::NoAnswer;
    };
    match report["checks"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["id"] == f.entry.as_str())
    {
        Some(c) if c["ok"] == true => Verdict::Pass,
        Some(_) => Verdict::Fail,
        None => Verdict::NoAnswer,
    }
}

// ------------------------------------------------------------ state

/// What the cache is keyed by.
#[derive(Clone, PartialEq, Eq)]
struct Key {
    rev: u64,
    view: String,
    example: String,
}

/// The panel's app-side state: the cached model, the checks run, and whether
/// the panel is open. None of it is model state.
pub struct State {
    key: Option<Key>,
    model: Model,
    checks: Checks,
    /// "Run checks" was pressed; handled after the frame is drawn.
    run: bool,
    /// Whether the panel is expanded.
    pub open: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            key: None,
            model: Model::default(),
            checks: Checks::default(),
            run: false,
            open: true,
        }
    }
}

impl State {
    /// Re-reads `content.get` and `bundle.state` when the revision, the view
    /// or the example changed. The only place the panel executes commands.
    pub fn refresh(&mut self, sess: &mut Session, view: &str, example: &str) {
        let key = Key {
            rev: sess.revision,
            view: view.into(),
            example: example.into(),
        };
        if self.key.as_ref() == Some(&key) {
            return;
        }
        let plan = example == "connect4";
        let content = sess
            .execute(
                "content.get",
                if plan {
                    json!({"subject": "plan", "view": view})
                } else {
                    json!({"view": view})
                },
            )
            .map_err(|e| e.to_string());
        let open = (!plan).then(|| {
            sess.execute("bundle.state", json!({}))
                .map_err(|e| e.to_string())
        });
        self.model = model(view, &content, open.as_ref());
        self.key = Some(key);
    }

    /// Asks for the checks of the entries on screen to run (what the "Run
    /// checks" button does); the next frame hands the first step back.
    pub fn run_checks(&mut self) {
        self.run = true;
    }

    /// The model last drawn.
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// What the checks said so far, by entry id.
    pub fn verdicts(&self) -> &BTreeMap<String, Verdict> {
        &self.checks.verdicts
    }

    /// Whether a run is still going.
    pub fn checking(&self) -> bool {
        self.checks.busy()
    }
}

// ------------------------------------------------------------ drawing

const BADGE_FINDING: Color32 = Color32::from_rgb(0x2f, 0x6f, 0xb3);
const BADGE_CAVEAT: Color32 = Color32::from_rgb(0xb4, 0x6a, 0x12);
const BADGE_JUDGMENT: Color32 = Color32::from_rgb(0x8e, 0x44, 0xad);
const BADGE_NOTE: Color32 = Color32::from_rgb(0x4a, 0x7f, 0x78);
const PASS: Color32 = Color32::from_rgb(0x2e, 0x8b, 0x57);
const FAIL: Color32 = Color32::from_rgb(0xc0, 0x39, 0x2b);
const UNSURE: Color32 = Color32::from_rgb(0xb4, 0x6a, 0x12);

fn badge_color(kind: &str) -> Color32 {
    match kind {
        "finding" => BADGE_FINDING,
        "caveat" => BADGE_CAVEAT,
        "judgment" => BADGE_JUDGMENT,
        _ => BADGE_NOTE,
    }
}

fn badge(ui: &mut egui::Ui, kind: &str, n: usize) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(" {} ", kind.to_uppercase()))
                .small()
                .strong()
                .color(Color32::WHITE)
                .background_color(badge_color(kind)),
        );
        ui.weak(n.to_string());
    });
}

/// What a row's check mark shows.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mark {
    /// The entry has a check that has not run (for this model).
    Unrun,
    Ran(Verdict),
    /// Ran at an earlier revision of the session.
    Old(Verdict),
}

impl Mark {
    fn glyph(self) -> (&'static str, Color32) {
        match self {
            Mark::Unrun => ("✔", Color32::GRAY),
            Mark::Ran(v) | Mark::Old(v) => match v {
                Verdict::Pass => ("✔", PASS),
                Verdict::Fail => ("✘", FAIL),
                Verdict::NoAnswer => ("?", UNSURE),
            },
        }
    }
}

fn mark_of(checks: &Checks, example: &str, rev: u64, id: &str) -> Mark {
    match checks.verdicts.get(id) {
        Some(v) if checks.example == example && checks.rev == rev => Mark::Ran(*v),
        Some(v) if checks.example == example => Mark::Old(*v),
        _ => Mark::Unrun,
    }
}

/// A collapsing header: the title, led by its check mark when it has a check;
/// the body is the text (unless `text` is false: the convention example's is
/// already the paragraph above), the check's claim and the cites.
fn row_ui(ui: &mut egui::Ui, kind: &str, row: &Row, mark: Option<Mark>, text: bool) {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let mut job = LayoutJob::default();
    if let Some(m) = mark {
        let (glyph, color) = m.glyph();
        let color = if matches!(m, Mark::Old(_)) {
            color.gamma_multiply(0.5)
        } else {
            color
        };
        job.append(
            &format!("{glyph} "),
            0.0,
            TextFormat::simple(font.clone(), color),
        );
    }
    job.append(
        &row.title,
        0.0,
        TextFormat::simple(font, Color32::PLACEHOLDER),
    );
    egui::CollapsingHeader::new(job)
        .id_salt(("model-content", kind, &row.id))
        .default_open(false)
        .show(ui, |ui| {
            if text {
                ui.label(&row.text);
            }
            if let Some(claim) = row.check.as_deref().filter(|c| !c.is_empty()) {
                ui.weak(format!("Check: {claim}"));
            }
            for c in &row.cites {
                ui.monospace(c);
            }
        });
}

fn sections_ui(ui: &mut egui::Ui, m: &Model, checks: &Checks, example: &str, rev: u64) {
    for s in &m.sections {
        match s {
            Section::Convention(r) => {
                ui.label(&r.text);
                if r.check.is_some() || !r.cites.is_empty() {
                    let mark = r
                        .check
                        .is_some()
                        .then(|| mark_of(checks, example, rev, &r.id));
                    let sources = Row {
                        title: "Sources and check".into(),
                        ..r.clone()
                    };
                    row_ui(ui, "convention-example", &sources, mark, false);
                }
                ui.add_space(4.0);
            }
            Section::Group { kind, rows } => {
                badge(ui, kind, rows.len());
                for r in rows {
                    let mark = r
                        .check
                        .is_some()
                        .then(|| mark_of(checks, example, rev, &r.id));
                    row_ui(ui, kind, r, mark, true);
                }
                ui.add_space(4.0);
            }
            Section::Notice(t) => {
                ui.weak(t);
            }
            Section::Lacks { bare, items } => {
                ui.separator();
                ui.strong("What this model lacks");
                if *bare {
                    ui.label(BARE);
                }
                for (code, text) in items {
                    ui.horizontal_wrapped(|ui| {
                        ui.monospace(code);
                        ui.label(text);
                    });
                }
                ui.separator();
            }
        }
    }
}

/// The summary next to the button: how many checks, how a run went.
fn checks_summary(m: &Model, checks: &Checks, example: &str, rev: u64) -> String {
    let n = m.checked().len();
    if checks.busy() {
        let done = checks.total - checks.todo.len() - checks.inflight.iter().count();
        return format!("checking {done}/{}…", checks.total);
    }
    let ids = m.checked();
    let ran: Vec<Verdict> = ids
        .iter()
        .filter_map(|id| checks.verdicts.get(id).copied())
        .filter(|_| checks.example == example)
        .collect();
    if ran.is_empty() {
        return format!("{n} checks");
    }
    let pass = ran.iter().filter(|v| **v == Verdict::Pass).count();
    let fail = ran.iter().filter(|v| **v == Verdict::Fail).count();
    let unsure = ran.len() - pass - fail;
    let mut s = format!("{pass} pass, {fail} fail");
    if unsure > 0 {
        s += &format!(", {unsure} unanswered");
    }
    if checks.rev != rev {
        s += " (the model changed since; run again)";
    }
    s
}

/// The Model content panel for the view and example the session shows now.
/// Draws a bottom panel in `ui` (call it before the views' own panels, so
/// they keep their layout) and returns the registry commands the person
/// asked for, for the caller to queue.
pub fn panel(state: &mut State, sess: &mut Session, ui: &mut egui::Ui) -> Vec<Step> {
    let (view, example) = (
        sess.composition_view.view.clone(),
        sess.composition_view.example.clone(),
    );
    panel_for(state, sess, &view, &example, ui)
}

fn panel_for(
    state: &mut State,
    sess: &mut Session,
    view: &str,
    example: &str,
    ui: &mut egui::Ui,
) -> Vec<Step> {
    state.refresh(sess, view, example);
    let rev = sess.revision;
    let plan = example == "connect4";
    if state.checks.busy() && state.checks.example != example {
        // The rest of the run was for the other model: its entries are not here.
        state.checks.todo.clear();
        state.checks.inflight = None;
    }
    let mut out: Vec<Step> = state.checks.pump(sess, plan).into_iter().collect();
    let mut press = std::mem::take(&mut state.run);

    let State {
        model: m,
        checks,
        open,
        ..
    } = state;
    let has_checks = !m.checked().is_empty();
    let head = |ui: &mut egui::Ui, open: &mut bool, press: &mut bool, checks: &Checks| {
        ui.horizontal(|ui| {
            if ui
                .selectable_label(*open, "Model content")
                .on_hover_text("What this model means and what it lacks, from content.get")
                .clicked()
            {
                *open = !*open;
            }
            ui.weak(&m.head);
            if *open && has_checks {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let idle = !checks.busy();
                    if ui
                        .add_enabled(idle, egui::Button::new("Run checks"))
                        .on_hover_text("Run each check on screen (content.check) and mark it")
                        .clicked()
                    {
                        *press = true;
                    }
                    ui.weak(checks_summary(m, checks, example, rev));
                });
            }
        });
    };
    if *open {
        egui::Panel::bottom("compviews-model-content")
            .resizable(true)
            .default_size(190.0)
            .min_size(48.0)
            .show(ui, |ui| {
                head(ui, open, &mut press, checks);
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("model-content")
                    .auto_shrink([false, false])
                    .show(ui, |ui| sections_ui(ui, m, checks, example, rev));
            });
    } else {
        egui::Panel::bottom("compviews-model-content-bar")
            .resizable(false)
            .show(ui, |ui| head(ui, open, &mut press, checks));
    }

    if press && has_checks && !checks.busy() {
        checks.begin(m.checked(), example, rev);
        out.extend(checks.pump(sess, plan));
    }
    if checks.busy() {
        ui.ctx().request_repaint();
    }
    out
}

// ------------------------------------------------------------ headless

/// A piece of text a headless frame painted, with where.
#[derive(Clone, Debug)]
pub struct Painted {
    pub text: String,
    pub pos: egui::Pos2,
}

pub(crate) fn painted(shape: &egui::Shape, out: &mut Vec<Painted>) {
    match shape {
        egui::Shape::Text(t) => out.push(Painted {
            text: t.galley.text().to_string(),
            pos: t.pos,
        }),
        egui::Shape::Vec(v) => v.iter().for_each(|s| painted(s, out)),
        _ => {}
    }
}

/// Draws the panel for `view` × `example` once in `ctx` (a headless egui
/// context, kept across frames by the caller), feeding it `events`, and
/// returns the steps it asked for and the text it painted.
pub fn headless(
    ctx: &egui::Context,
    state: &mut State,
    sess: &mut Session,
    view: &str,
    example: &str,
    events: Vec<egui::Event>,
) -> (Vec<Step>, Vec<Painted>) {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::Vec2::new(1600.0, 1000.0),
        )),
        events,
        ..Default::default()
    };
    let mut steps = vec![];
    let out = ctx.run_ui(raw, |ui| {
        steps = panel_for(state, sess, view, example, ui);
    });
    let mut texts = vec![];
    out.shapes
        .iter()
        .for_each(|s| painted(&s.shape, &mut texts));
    out.drop_without_applying_deltas();
    (steps, texts)
}
