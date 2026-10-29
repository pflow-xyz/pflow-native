//! The composition views' Model content panel (crates/app/src/modelcontent.rs),
//! drawn headless: it shows what the model means and what it lacks, read from
//! `content.get` and `bundle.state`, caches both per revision, and runs the
//! checks through queued `content.check` steps.

use eframe::egui;
use pflow_native::compviews;
use pflow_native::modelcontent::{self, Model, Painted, Section, State, Verdict};
use pflow_native_core::Session;
use serde_json::{json, Value};

const CAFE: &str = include_str!("../../core/tests/fixtures/bundle/cafe.bundle.json");

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// The default session: the orders composition and the Connect-4 plan open.
fn default_session() -> Session {
    compviews::session_in("default").unwrap()
}

fn draw(
    ctx: &egui::Context,
    st: &mut State,
    s: &mut Session,
    view: &str,
    ex: &str,
) -> Vec<Painted> {
    let (steps, texts) = modelcontent::headless(ctx, st, s, view, ex, vec![]);
    assert!(
        steps.is_empty(),
        "nothing is asked for until Run checks: {steps:?}"
    );
    texts
}

fn any(texts: &[Painted], needle: &str) -> bool {
    texts.iter().any(|p| p.text.contains(needle))
}

fn index_of(texts: &[Painted], needle: &str) -> usize {
    texts
        .iter()
        .position(|p| p.text.contains(needle))
        .unwrap_or_else(|| panic!("{needle:?} was not drawn"))
}

/// `content.get`'s entries of `kind` for `view` (what the panel is built from).
fn entries(s: &mut Session, view: &str, kind: &str) -> Vec<Value> {
    let got = run(s, "content.get", json!({"view": view, "kind": kind}));
    got["entries"].as_array().unwrap().clone()
}

#[test]
fn the_convention_example_comes_first_and_a_finding_is_drawn() {
    let mut s = default_session();
    let conv = entries(&mut s, "flat", "convention-example");
    assert_eq!(conv.len(), 1, "one convention example per view");
    let text = conv[0]["text"].as_str().unwrap();
    let findings = entries(&mut s, "flat", "finding");
    assert!(!findings.is_empty());

    let (ctx, mut st) = (egui::Context::default(), State::default());
    let drawn = draw(&ctx, &mut st, &mut s, "flat", "orders");

    let at = index_of(&drawn, text);
    let title = findings
        .iter()
        .map(|f| f["title"].as_str().unwrap())
        .find(|t| any(&drawn, t))
        .expect("at least one finding title is drawn");
    assert!(
        at < index_of(&drawn, title),
        "the convention example is the first paragraph"
    );
    // The kind badge and the header of the section that says what is lacking.
    assert!(any(&drawn, "FINDING"));
    assert!(any(&drawn, "Model content"));
    assert!(any(&drawn, "Run checks"));
}

#[test]
fn the_default_orders_example_lacks_only_a_layout() {
    let mut s = default_session();
    let (ctx, mut st) = (egui::Context::default(), State::default());
    let drawn = draw(&ctx, &mut st, &mut s, "flat", "orders");
    assert!(any(&drawn, "What this model lacks"));
    assert!(any(&drawn, "NO_LAYOUT"));
    for code in ["NO_PATTERN", "NO_PORTS", "NO_SCENARIOS", "NO_CONTENT"] {
        assert!(
            !any(&drawn, code),
            "{code} does not apply to the orders example"
        );
    }
    assert!(!any(&drawn, modelcontent::BARE));
    // Under the convention example and above the (long) groups, so it is on
    // screen without scrolling past dozens of collapsed headers.
    let sections = &st.model().sections;
    assert!(matches!(sections[0], Section::Convention(_)));
    assert!(matches!(sections[1], Section::Lacks { .. }));
    assert!(matches!(sections[2], Section::Group { .. }));
    let lacks: Vec<_> = st
        .model()
        .sections
        .iter()
        .filter_map(|x| match x {
            Section::Lacks { bare, items } => Some((*bare, items.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(lacks.len(), 1);
    assert!(!lacks[0].0);
    assert_eq!(lacks[0].1.len(), 1);
    assert_eq!(lacks[0].1[0].0, "NO_LAYOUT");
}

#[test]
fn the_model_groups_entries_by_kind_in_the_registrys_order() {
    let mut s = default_session();
    let content = Ok(run(&mut s, "content.get", json!({"view": "lens"})));
    let open = Ok(run(&mut s, "bundle.state", json!({})));
    let m = modelcontent::model("lens", &content, Some(&open));

    assert!(matches!(m.sections[0], Section::Convention(_)));
    let kinds: Vec<&str> = m
        .sections
        .iter()
        .filter_map(|x| match x {
            Section::Group { kind, .. } => Some(kind.as_str()),
            _ => None,
        })
        .collect();
    let order = ["finding", "caveat", "judgment", "note"];
    let mut at = 0;
    for k in &kinds {
        let i = order.iter().position(|o| o == k).expect("a listed kind");
        assert!(i >= at, "{kinds:?} follows {order:?}");
        at = i;
    }
    assert!(kinds.contains(&"finding") && kinds.contains(&"caveat"));
    // Every non-scenario entry content.get gave is in exactly one row.
    let got = content.as_ref().unwrap()["entries"].as_array().unwrap();
    let listed = got.iter().filter(|e| e["kind"] != "scenario").count();
    assert_eq!(m.rows().count(), listed);
    // The rows with a check are the entries that carry one.
    let with_check = got
        .iter()
        .filter(|e| e["kind"] != "scenario" && e["check"].is_object())
        .count();
    assert_eq!(m.checked().len(), with_check);
    assert!(with_check > 0);
    // Cites are monospace lines: a source cite names its file and lines.
    let cited = m.rows().find(|r| !r.cites.is_empty()).unwrap();
    assert!(
        cited.cites.iter().any(|c| c.contains(" @")),
        "{:?}",
        cited.cites
    );
}

#[test]
fn a_raw_bundle_says_it_is_bare_and_what_else_it_lacks() {
    let mut s = default_session();
    let doc: Value = serde_json::from_str(CAFE).unwrap();
    run(&mut s, "bundle.load", json!({"document": doc}));
    let (ctx, mut st) = (egui::Context::default(), State::default());
    let drawn = draw(&ctx, &mut st, &mut s, "flat", "orders");

    assert!(any(&drawn, modelcontent::BARE));
    assert!(any(&drawn, "What this model lacks"));
    for code in ["NO_PATTERN", "NO_PORTS", "NO_SCENARIOS", "NO_LAYOUT"] {
        assert!(any(&drawn, code), "{code} is listed for a loaded Bundle");
    }
    // The bare sentence stands in for NO_CONTENT; it is not said twice.
    assert!(!any(&drawn, "NO_CONTENT"));
    assert!(!st
        .model()
        .sections
        .iter()
        .any(|x| matches!(x, Section::Convention(_) | Section::Group { .. })));
    assert!(st.model().checked().is_empty());
    assert!(
        !any(&drawn, "Run checks"),
        "nothing to check in a bare model"
    );
}

#[test]
fn the_connect4_example_reads_the_plans_content() {
    let mut s = default_session();
    let (ctx, mut st) = (egui::Context::default(), State::default());
    let drawn = draw(&ctx, &mut st, &mut s, "boundaries", "connect4");
    assert!(any(&drawn, "Model content"));
    let plan = run(
        &mut s,
        "content.get",
        json!({"subject": "plan", "view": "boundaries"}),
    );
    if plan["total"] == 0 {
        // The library has no Connect-4 content yet: the model is bare, and
        // the composition's NO_LAYOUT is about another model, so not listed.
        assert!(any(&drawn, modelcontent::BARE));
        assert!(!any(&drawn, "NO_LAYOUT"));
    } else {
        assert!(st.model().rows().count() > 0 || any(&drawn, "nothing to say"));
    }
}

/// Presses "Run checks" (`State::run_checks`) and plays the app's part:
/// queues each step handed back and runs it, one per frame, until the run
/// is over. Returns every step handed back.
fn run_all_checks(
    ctx: &egui::Context,
    st: &mut State,
    s: &mut Session,
    view: &str,
    example: &str,
) -> Vec<pflow_native_core::Step> {
    st.run_checks();
    let mut asked = vec![];
    for frame in 0..400 {
        let (steps, _) = modelcontent::headless(ctx, st, s, view, example, vec![]);
        for step in steps {
            s.execute(&step.command, step.args.clone())
                .unwrap_or_else(|e| panic!("{}: {e}", step.command));
            asked.push(step);
        }
        if !st.checking() {
            return asked;
        }
        assert!(frame < 399, "the run never ended");
    }
    unreachable!()
}

#[test]
fn the_connect4_plan_content_is_drawn_and_its_checks_run_with_the_plan_subject() {
    let mut s = default_session();
    let (ctx, mut st) = (egui::Context::default(), State::default());

    // The plan's convention example leads, as it does for the orders example,
    // and the composition's own gaps (NO_LAYOUT) are not listed for the plan.
    let view = "boundaries";
    let got = run(
        &mut s,
        "content.get",
        json!({"subject": "plan", "view": view}),
    );
    assert!(got["total"].as_u64().unwrap() > 0, "the plan ships content");
    let conv = run(
        &mut s,
        "content.get",
        json!({"subject": "plan", "view": view, "kind": "convention-example"}),
    );
    let text = conv["entries"][0]["text"].as_str().unwrap().to_string();
    let drawn = draw(&ctx, &mut st, &mut s, view, "connect4");
    assert!(any(&drawn, &text), "the plan's convention example is drawn");
    assert!(!any(&drawn, modelcontent::BARE));
    assert!(!any(&drawn, "NO_LAYOUT"));
    assert!(
        st.model()
            .head
            .contains(&format!("{} entries", got["total"])),
        "{}",
        st.model().head
    );

    // Run the checks of a view that has some: each step names the plan, all
    // of them come back, and the library's plan checks pass.
    let views = [
        "lens", "flat", "operad", "string", "cospan", "spiders", "process",
    ];
    let mut ran = 0;
    for v in views {
        draw(&ctx, &mut st, &mut s, v, "connect4");
        let wanted = st.model().checked();
        if wanted.is_empty() {
            continue;
        }
        let (rev, undo) = (s.revision, s.can_undo());
        let asked = run_all_checks(&ctx, &mut st, &mut s, v, "connect4");
        assert!(
            asked.iter().all(|a| a.command == "content.check"
                && a.args["subject"] == "plan"
                && a.args["entry"].is_string()),
            "{v}: {asked:?}"
        );
        let ids: Vec<&str> = asked
            .iter()
            .map(|a| a.args["entry"].as_str().unwrap())
            .collect();
        assert_eq!(ids, wanted, "{v}: every checked entry on screen, once");
        assert!(
            st.verdicts().values().all(|x| *x == Verdict::Pass),
            "{v}: the plan's checks pass: {:?}",
            st.verdicts()
        );
        assert_eq!((s.revision, s.can_undo()), (rev, undo), "{v}");
        ran += wanted.len();
    }
    assert!(ran > 0, "some plan view has a checked entry");
}

#[test]
fn nothing_is_re_read_until_the_revision_view_or_example_changes() {
    let mut s = default_session();
    let (ctx, mut st) = (egui::Context::default(), State::default());
    let drawn = draw(&ctx, &mut st, &mut s, "flat", "orders");
    assert!(any(&drawn, "FINDING"));

    // Drop the content behind the session's back, with no revision bump:
    // the next frame draws the cached model, not a fresh content.get.
    let rev = s.revision;
    s.composition.as_mut().unwrap().doc.content = None;
    assert_eq!(s.revision, rev);
    let again = draw(&ctx, &mut st, &mut s, "flat", "orders");
    assert!(any(&again, "FINDING"), "cached");
    assert!(!any(&again, modelcontent::BARE));

    // A different view re-reads (key: revision, view, example).
    let other = draw(&ctx, &mut st, &mut s, "lens", "orders");
    assert!(any(&other, modelcontent::BARE), "re-read for the lens view");
    // Back on the first view it is the same revision, view and example as the
    // last read of another view, so it is read again.
    assert!(any(
        &draw(&ctx, &mut st, &mut s, "flat", "orders"),
        modelcontent::BARE
    ));

    // A revision bump re-reads too.
    let mut s = default_session();
    let (ctx, mut st) = (egui::Context::default(), State::default());
    draw(&ctx, &mut st, &mut s, "flat", "orders");
    run(&mut s, "content.clear", json!({}));
    let after = draw(&ctx, &mut st, &mut s, "flat", "orders");
    assert!(any(&after, modelcontent::BARE) && !any(&after, "FINDING"));
}

#[test]
fn drawing_the_panel_leaves_the_session_alone() {
    // refresh() runs content.get and bundle.state inside the frame, so they
    // must leave no trace: a bumped revision would change the cache key and
    // re-read every frame; a journal entry would end up in macros; an
    // artifact would pile up one per view change.
    for (example, state) in [("orders", "default"), ("connect4", "default")] {
        let mut s = compviews::session_in(state).unwrap();
        let (ctx, mut st) = (egui::Context::default(), State::default());
        let at = (s.revision, s.can_undo(), s.journal.len(), s.artifacts.len());
        for view in ["flat", "lens", "boundaries", "flat"] {
            draw(&ctx, &mut st, &mut s, view, example);
            draw(&ctx, &mut st, &mut s, view, example);
        }
        assert_eq!(
            (s.revision, s.can_undo(), s.journal.len(), s.artifacts.len()),
            at,
            "{example}"
        );
    }
}

#[test]
fn a_collapsed_panel_is_one_bar() {
    let mut s = default_session();
    let (ctx, mut st) = (egui::Context::default(), State::default());
    st.open = false;
    let drawn = draw(&ctx, &mut st, &mut s, "flat", "orders");
    assert!(any(&drawn, "Model content"));
    assert!(!any(&drawn, "FINDING") && !any(&drawn, "What this model lacks"));
    st.open = true;
    assert!(any(
        &draw(&ctx, &mut st, &mut s, "flat", "orders"),
        "FINDING"
    ));
}

/// Moves to `at`, presses and releases over three frames, returning the
/// steps of the frame that saw the release.
fn click(
    ctx: &egui::Context,
    st: &mut State,
    s: &mut Session,
    view: &str,
    at: egui::Pos2,
) -> Vec<pflow_native_core::Step> {
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    modelcontent::headless(
        ctx,
        st,
        s,
        view,
        "orders",
        vec![egui::Event::PointerMoved(at)],
    );
    modelcontent::headless(ctx, st, s, view, "orders", vec![button(true)]);
    modelcontent::headless(ctx, st, s, view, "orders", vec![button(false)]).0
}

#[test]
fn run_checks_queues_one_content_check_per_frame_and_marks_each_entry() {
    let mut s = default_session();
    let (ctx, mut st) = (egui::Context::default(), State::default());
    let drawn = draw(&ctx, &mut st, &mut s, "flat", "orders");
    let wanted = st.model().checked();
    assert!(
        wanted.len() > 3,
        "the flat view has checked entries: {wanted:?}"
    );
    assert!(st.verdicts().is_empty());
    let (rev, undo, journal, artifacts) =
        (s.revision, s.can_undo(), s.journal.len(), s.artifacts.len());

    // Press the button as a person would.
    let button = drawn
        .iter()
        .find(|p| p.text == "Run checks")
        .expect("the button")
        .pos;
    let mut steps = click(&ctx, &mut st, &mut s, "flat", button + egui::vec2(4.0, 4.0));
    assert!(st.checking(), "the click began a run");

    // The app queues each step and runs it at the end of the frame.
    let mut asked: Vec<String> = vec![];
    for _ in 0..wanted.len() + 5 {
        assert!(steps.len() <= 1, "one check per frame: {steps:?}");
        for step in steps.drain(..) {
            assert_eq!(step.command, "content.check");
            assert!(
                step.args["entry"].is_string() && step.args.get("subject").is_none(),
                "{:?}",
                step.args
            );
            asked.push(step.args["entry"].as_str().unwrap().to_string());
            s.execute(&step.command, step.args).expect("a check runs");
        }
        let (next, _) = modelcontent::headless(&ctx, &mut st, &mut s, "flat", "orders", vec![]);
        steps = next;
        if !st.checking() && steps.is_empty() {
            break;
        }
    }
    assert!(!st.checking());
    assert_eq!(
        asked, wanted,
        "every checked entry on screen, in order, once"
    );
    assert_eq!(st.verdicts().len(), wanted.len());
    assert!(
        st.verdicts().values().all(|v| *v == Verdict::Pass),
        "the library's checks pass: {:?}",
        st.verdicts()
    );
    // The run did not touch the model: no revision, undo step or journal entry.
    assert_eq!(
        (s.revision, s.can_undo(), s.journal.len()),
        (rev, undo, journal)
    );
    assert!(
        s.artifacts.len() > artifacts,
        "content.check stored its report"
    );

    let after = draw(&ctx, &mut st, &mut s, "flat", "orders");
    assert!(
        any(&after, &format!("{} pass, 0 fail", wanted.len())),
        "{:?}",
        after.iter().map(|p| &p.text).collect::<Vec<_>>()
    );

    // Edit the model: the marks are kept but flagged as from before.
    run(&mut s, "bundle.reset_links", json!({}));
    let old = draw(&ctx, &mut st, &mut s, "flat", "orders");
    assert!(any(&old, "the model changed since"));
}

#[test]
fn a_check_that_goes_red_is_marked_fail() {
    // The pool-leak finding's check goes red when the pool's release is fused
    // with undo_reserve (the "release-undo" state of the views' goldens).
    let mut s = compviews::session_in("release-undo").unwrap();
    let (ctx, mut st) = (egui::Context::default(), State::default());
    draw(&ctx, &mut st, &mut s, "flat", "orders");
    st.run_checks();
    let mut guard = 0;
    loop {
        let (steps, _) = modelcontent::headless(&ctx, &mut st, &mut s, "flat", "orders", vec![]);
        for step in steps {
            s.execute(&step.command, step.args).unwrap();
        }
        guard += 1;
        assert!(guard < 100);
        if !st.checking() {
            break;
        }
    }
    let failed: Vec<String> = st
        .verdicts()
        .iter()
        .filter(|(_, v)| **v == Verdict::Fail)
        .map(|(k, _)| k.clone())
        .collect();
    assert!(
        failed.iter().any(|id| id.contains("pool-leaks")),
        "the pool-leak check fails when release is fused with undo_reserve: {:?}",
        st.verdicts()
    );
    let drawn = draw(&ctx, &mut st, &mut s, "flat", "orders");
    assert!(any(&drawn, &format!("{} fail", failed.len())));
}

#[test]
fn a_refused_content_get_is_a_sentence_not_a_panic() {
    let m: Model = modelcontent::model(
        "flat",
        &Err("no composition is open (bundle.open, bundle.load)".into()),
        None,
    );
    assert_eq!(m.sections.len(), 1);
    assert!(matches!(&m.sections[0], Section::Notice(t) if t.contains("no composition is open")));

    let mut s = Session::default();
    s.execute("view.composition", json!({"open": false, "view": "flat"}))
        .unwrap();
    let (ctx, mut st) = (egui::Context::default(), State::default());
    let drawn = draw(&ctx, &mut st, &mut s, "flat", "orders");
    assert!(
        any(&drawn, "no composition is open"),
        "{:?}",
        drawn.iter().map(|p| &p.text).collect::<Vec<_>>()
    );
}

#[test]
fn untitled_entries_unknown_kinds_and_dormant_entries() {
    let content = Ok(json!({
        "source": "loaded", "subject": "composition", "name": "toy", "total": 4, "dormant": 2,
        "entries": [
            {"id": "a", "kind": "note", "text": "Short first sentence. And a second one.", "about": ["model:"]},
            {"id": "b", "kind": "scenario", "title": "A run", "about": ["model:"]},
            {"id": "c", "kind": "hunch", "title": "A new kind", "text": "x", "about": ["model:"],
             "cites": [{"bib": "baez-master"}, {"bib": "no-such-key"}]},
            {"id": "d", "kind": "finding", "title": "Backed", "text": "y", "about": ["model:"],
             "check": {"claim": "it holds", "steps": []}}
        ]
    }));
    let open = Ok(json!({"missing": [
        {"code": "NO_LAYOUT", "text": "no positions"},
        {"code": "KEYS_PRESERVED", "text": "keys kept"}
    ]}));
    let m = modelcontent::model("flat", &content, Some(&open));
    let rows: Vec<_> = m.rows().collect();
    assert_eq!(rows.len(), 3, "the scenario entry is not listed");
    assert_eq!(
        rows.iter().find(|r| r.id == "a").unwrap().title,
        "Short first sentence"
    );
    assert_eq!(
        rows.iter().find(|r| r.id == "d").unwrap().check.as_deref(),
        Some("it holds")
    );
    let kinds: Vec<_> = m
        .sections
        .iter()
        .filter_map(|x| match x {
            Section::Group { kind, .. } => Some(kind.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        ["finding", "note", "hunch"],
        "listed kinds first, unknown kinds after"
    );
    let c = rows.iter().find(|r| r.id == "c").unwrap();
    assert!(c.cites[0].contains("Open Petri Nets"), "{:?}", c.cites);
    assert_eq!(c.cites[1], "bib:no-such-key");
    // No convention example here, so the lacks section leads; the groups follow it.
    let Section::Lacks { bare, items } = m.sections.first().unwrap() else {
        panic!("the lacks section comes before the groups")
    };
    assert!(matches!(m.sections[1], Section::Group { .. }));
    assert!(!bare);
    let codes: Vec<&str> = items.iter().map(|(c, _)| c.as_str()).collect();
    assert_eq!(codes, ["NO_LAYOUT", "KEYS_PRESERVED", "DORMANT"]);
    assert_eq!(m.head, "toy · loaded · 4 entries");

    // Content with nothing for this view says so instead of showing nothing.
    let none =
        Ok(json!({"source": "library:x", "name": "x", "total": 7, "dormant": 0, "entries": []}));
    let m = modelcontent::model("laws", &none, None);
    assert_eq!(
        m.sections,
        vec![Section::Notice(
            "This model's content has nothing to say in the laws view.".into()
        )]
    );
}

/// A check that has not run is marked as not run, not with the tick of one that passed.
#[test]
fn a_check_that_has_not_run_is_not_marked_as_passed() {
    let mut s = default_session();
    let (ctx, mut st) = (egui::Context::default(), State::default());
    let before = draw(&ctx, &mut st, &mut s, "flat", "orders");
    assert!(
        !st.model().checked().is_empty(),
        "the flat view has checked entries"
    );
    assert!(
        any(&before, "○"),
        "{:?}",
        before.iter().map(|p| &p.text).collect::<Vec<_>>()
    );
    assert!(!any(&before, "✔"), "nothing has passed yet");
    st.run_checks();
    let mut guard = 0;
    loop {
        let (steps, _) = modelcontent::headless(&ctx, &mut st, &mut s, "flat", "orders", vec![]);
        for step in steps {
            s.execute(&step.command, step.args).unwrap();
        }
        guard += 1;
        assert!(guard < 100);
        if !st.checking() {
            break;
        }
    }
    let after = draw(&ctx, &mut st, &mut s, "flat", "orders");
    assert!(any(&after, "✔"));
    assert!(!any(&after, "○"));
}
