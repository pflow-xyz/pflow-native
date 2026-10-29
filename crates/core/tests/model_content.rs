//! Model content (models-first D11): docs/composition-documents.md §3.
//!
//! * the orders and Connect-4 libraries validate against their models and every
//!   check they ship passes, through the commands and through the core API;
//! * the checks can fail: a model whose claim is false turns its check red;
//! * the validator, one test per code of §3.3;
//! * the `content.*` commands: filters, dormant entries, undo, isolation;
//! * the element ids of §3.2, each for the right thing and not the wrong one;
//! * a small content document for a raw Bundle (cafe), and declared scenarios;
//! * library content belongs to the library example; a user's model is bare.

use pflow_native_core::cid;
use pflow_native_core::composition::checks::{self, Base};
use pflow_native_core::composition::content::{self, Entry, Ids, ModelContent};
use pflow_native_core::composition::document::canonical_json;
use pflow_native_core::composition::{self as comp, Composition};
use pflow_native_core::store::{self, Store};
use pflow_native_core::Session;
use serde_json::{json, Value};
use std::sync::Arc;

const CONTEXT: &str = "https://pflow.xyz/schema";
/// cafe.bundle.json sealed as received (docs/composition-documents.md §7).
const CAFE_CID: &str = "z4EBG9j1SUhuY7XnEcPfHQPzQkmqjUtgrD2wHADw8qdRbpnNt6W";

// ------------------------------------------------------------ helpers

/// A session on a store of its own: nothing here reads or writes the user's.
fn session() -> Session {
    store::set_thread_store(Some(Arc::new(Store::memory())));
    Session::default()
}

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// The refusal a command gives, `[CODE] message` for the feature's own.
fn refused(s: &mut Session, id: &str, args: Value) -> String {
    match s.execute(id, args) {
        Ok(v) => panic!("{id} should have been refused, got {v}"),
        Err(e) => e.to_string(),
    }
}

fn fixture(path: &str) -> Value {
    let text = std::fs::read_to_string(format!(
        "{}/tests/fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The built-in example: orders, workers and payment with the three idioms,
/// and the orders library content attached.
fn orders() -> Session {
    let mut s = session();
    run(&mut s, "bundle.open", json!({}));
    s
}

fn default_comp() -> Composition {
    let ids: Vec<String> = comp::DEFAULT_INSTANCES
        .iter()
        .map(|s| s.to_string())
        .collect();
    Composition::open(&ids, &comp::default_links()).unwrap()
}

fn cafe_bundle() -> Value {
    fixture("bundle/cafe.bundle.json")
}

fn cafe_content() -> Value {
    fixture("content/cafe.content.json")
}

/// The cafe raw Bundle with its content document attached.
fn cafe() -> Session {
    let mut s = session();
    run(
        &mut s,
        "bundle.load",
        json!({"document": cafe_bundle(), "content": cafe_content()}),
    );
    s
}

fn cafe_comp() -> Composition {
    Composition::from_bundle_document(&cafe_bundle(), &|_: &str| None).unwrap()
}

/// `content.check {entry}`: the one result.
fn check_one(s: &mut Session, entry: &str) -> Value {
    let r = run(s, "content.check", json!({"entry": entry}));
    assert_eq!(r["total"], 1, "{r}");
    r["checks"][0].clone()
}

/// What went red in a check result: `"<command> <path> <op> <value>, actual
/// <actual>"` per failed row, `"<command>: <error>"` per step that did not
/// run, `"check: <error>"` for a check that was refused whole.
fn reds(check: &Value) -> Vec<String> {
    let mut out = vec![];
    if let Some(e) = check["error"].as_str() {
        out.push(format!("check: {e}"));
    }
    for st in check["steps"].as_array().into_iter().flatten() {
        let cmd = st["command"].as_str().unwrap_or_default();
        if let Some(e) = st["error"].as_str() {
            out.push(format!("{cmd}: {e}"));
        }
        for row in st["expect"].as_array().into_iter().flatten() {
            if row["ok"] != true {
                out.push(format!(
                    "{cmd} {} {} {}, actual {}",
                    row["path"].as_str().unwrap_or_default(),
                    row["op"].as_str().unwrap_or_default(),
                    row["value"],
                    row["actual"]
                ));
            }
        }
    }
    out
}

fn assert_green(c: &Value) {
    assert_eq!(c["ok"], true, "{} should hold; red: {:?}", c["id"], reds(c));
}

/// The check is red, and `path` is among the rows that failed (so it went
/// red for the claim's sake and not for an unrelated reason).
fn assert_red_at(c: &Value, path: &str) {
    assert_eq!(c["ok"], false, "{} should have gone red", c["id"]);
    let r = reds(c);
    assert!(
        r.iter().any(|x| x.contains(path)),
        "{}: red rows {r:#?} should include {path}",
        c["id"]
    );
}

/// Exports the attached content, lets `edit` change the entry `id` (its
/// `check` and `scenario` as objects), and loads it back with `content.load`.
fn edit_entry(s: &mut Session, id: &str, edit: impl FnOnce(&mut Value)) {
    let mut doc = run(s, "content.export", json!({}))["document"].clone();
    let e = doc["entries"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no entry {id}"));
    for k in ["check", "scenario"] {
        if let Some(text) = e[k].as_str() {
            e[k] = serde_json::from_str(text).unwrap();
        }
    }
    edit(e);
    run(s, "content.load", json!({"document": doc}));
}

/// A content document about a composition.
fn doc(entries: Vec<Value>) -> Value {
    json!({
        "@context": CONTEXT,
        "@type": "ModelContent",
        "name": "test",
        "subject": {"kind": "composition", "name": "orders"},
        "entries": entries,
    })
}

fn entry(id: &str, kind: &str, about: &[&str]) -> Value {
    json!({"id": id, "kind": kind, "text": "t", "about": about})
}

/// A finding that names the whole model.
fn plain(id: &str) -> Value {
    entry(id, "finding", &["model:"])
}

fn with(mut e: Value, key: &str, v: Value) -> Value {
    e[key] = v;
    e
}

/// The report of `d` against the orders composition.
fn report(d: &Value, strict: bool) -> content::Report {
    let mc = ModelContent::parse(d).unwrap_or_else(|e| panic!("{e}"));
    content::validate(&mc, &default_comp(), strict)
}

fn codes(issues: &[content::Issue]) -> Vec<&str> {
    issues.iter().map(|i| i.code.as_str()).collect()
}

/// `r` has exactly this one error, on `entry`.
fn assert_only_error(r: &content::Report, code: &str, entry: Option<&str>) {
    assert_eq!(codes(&r.errors), vec![code], "{:?}", r.errors);
    assert_eq!(r.errors[0].entry.as_deref(), entry, "{:?}", r.errors);
    assert!(!r.valid());
}

fn check_of(steps: Value) -> Value {
    json!({"claim": "c", "steps": steps})
}

fn step(command: &str) -> Value {
    json!({"command": command, "args": {}, "expect": [{"path": "", "op": "exists"}]})
}

// ------------------------------------------------------------ 1. the library

/// The orders library is valid against the composition it attaches to, with
/// nothing unresolved and nothing dormant, and every entry that has a check
/// has one that passes: through the core API.
#[test]
fn library_content_validates_and_every_check_passes_core_api() {
    let c = default_comp();
    assert_eq!(c.doc.content_source.label(), "library:orders");
    let lib = content::library_entry("orders").expect("orders is in the library");
    let r = content::validate(&lib.content, &c, true);
    assert!(r.valid(), "{:?}", r.errors);
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert!(r.unresolved().is_empty());

    let with_check: Vec<&Entry> = lib
        .content
        .entries
        .iter()
        .filter(|e| e.check.is_some())
        .collect();
    assert!(!with_check.is_empty());
    assert_eq!(lib.content.checks().len(), with_check.len());

    let base = Base::Composition(&c);
    let mut passed = 0;
    let mut failed = vec![];
    for e in &with_check {
        let res = checks::run_check(&base, e);
        assert_eq!(res["id"], e.id.as_str());
        if res["ok"] == true {
            passed += 1;
        } else {
            failed.push(format!("{}: {:?}", e.id, reds(&res)));
        }
    }
    assert!(failed.is_empty(), "{} failed:\n{failed:#?}", failed.len());
    assert_eq!(passed, with_check.len());
}

/// The same through the commands: `content.library` counts the checks,
/// `content.validate` is clean, `content.check` runs every one and they pass,
/// and `content.check {entry}` agrees with the all-checks run.
#[test]
fn library_content_validates_and_every_check_passes_commands() {
    let mut s = orders();
    let lib = run(&mut s, "content.library", json!({}));
    let row = lib["library"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"] == "orders")
        .expect("orders is listed");
    assert_eq!(
        row["subject"],
        json!({"kind": "composition", "name": "orders"})
    );

    // The number of entries that have a check, counted from the document
    // rather than trusted from the listing.
    let exported = run(&mut s, "content.export", json!({}));
    let entries = exported["document"]["entries"].as_array().unwrap();
    let with_check: Vec<&str> = entries
        .iter()
        .filter(|e| e.get("check").is_some())
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert!(!with_check.is_empty());
    assert_eq!(row["entries"].as_u64().unwrap() as usize, entries.len());
    assert_eq!(row["checks"].as_u64().unwrap() as usize, with_check.len());

    let v = run(&mut s, "content.validate", json!({}));
    assert_eq!(v["valid"], true, "{v}");
    assert_eq!(v["errors"], json!([]));
    assert_eq!(v["warnings"], json!([]));
    assert_eq!(v["unresolved"], json!([]));
    assert_eq!(v["source"], "library:orders");
    assert_eq!(v["entries"].as_u64().unwrap() as usize, entries.len());

    let all = run(&mut s, "content.check", json!({}));
    let results = all["checks"].as_array().unwrap();
    let failed: Vec<String> = results
        .iter()
        .filter(|r| r["ok"] != true)
        .map(|r| format!("{}: {:?}", r["id"], reds(r)))
        .collect();
    assert!(failed.is_empty(), "{} failed:\n{failed:#?}", failed.len());
    assert_eq!(all["total"].as_u64().unwrap() as usize, with_check.len());
    assert_eq!(all["passed"].as_u64().unwrap() as usize, with_check.len());
    assert_eq!(all["failed"], 0);
    assert_eq!(all["source"], "library:orders");
    let ran: Vec<&str> = results.iter().map(|r| r["id"].as_str().unwrap()).collect();
    let mut sorted = ran.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ran.len(), "a check ran twice");
    for id in &with_check {
        assert!(ran.contains(id), "{id} did not run");
    }

    // `entry` runs the same check as `all` does: first, middle and last.
    for i in [0, results.len() / 2, results.len() - 1] {
        let one = check_one(&mut s, ran[i]);
        assert_eq!(one, results[i], "{}", ran[i]);
    }
}

/// `content.check` names what it cannot run.
#[test]
fn content_check_refuses_an_unknown_entry_and_one_without_a_check() {
    let mut s = orders();
    let e = refused(&mut s, "content.check", json!({"entry": "no-such-entry"}));
    assert!(e.contains("no entry \"no-such-entry\" with a check"), "{e}");
    // An entry the library ships without a check (the orders library has
    // some; found in the document, not named).
    let exported = run(&mut s, "content.export", json!({}));
    let bare = exported["document"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e.get("check").is_none())
        .expect("some library entry has no check")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let e = refused(&mut s, "content.check", json!({"entry": bare}));
    assert!(
        e.contains(&format!("no entry {bare:?} with a check")),
        "{e}"
    );
    let e = refused(&mut s, "content.check", json!({"subject": "nothing"}));
    assert!(e.contains("composition and plan"), "{e}");
    // Neither refusal stored a report.
    assert!(s
        .artifacts
        .iter()
        .all(|a| a.source.command != "content.check"));
}

/// `content.export`'s CID is the seal of exactly the document it returned,
/// is what parse -> to_document -> cid gives, is the same on the second
/// export, and parse(export) is the ModelContent it came from.
#[test]
fn content_export_is_sealed_stable_and_round_trips() {
    let mut s = orders();
    let a = run(&mut s, "content.export", json!({}));
    let b = run(&mut s, "content.export", json!({}));
    assert_eq!(a["document"], b["document"]);
    assert_eq!(a["cid"], b["cid"]);

    let d = &a["document"];
    assert_eq!(a["cid"], cid::seal(d).unwrap().as_str());
    let mc = ModelContent::parse(d).unwrap();
    assert_eq!(
        a["cid"],
        cid::seal(&mc.to_document()).unwrap().as_str(),
        "parse -> to_document -> cid"
    );
    assert_eq!(mc.cid().unwrap(), a["cid"].as_str().unwrap());
    assert_eq!(&mc.to_document(), d, "to_document is a fixed point");

    let lib = &content::library_entry("orders").unwrap().content;
    assert_eq!(&mc, lib, "parse(export) is the library's ModelContent");
    let l = run(&mut s, "content.export", json!({"library": "orders"}));
    assert_eq!(l["document"], a["document"]);
    assert_eq!(l["cid"], a["cid"]);

    // The exported form carries a check and a scenario as canonical text.
    let mut seen = (0, 0);
    for e in d["entries"].as_array().unwrap() {
        for (k, n) in [("check", &mut seen.0), ("scenario", &mut seen.1)] {
            if let Some(t) = e.get(k) {
                let t = t
                    .as_str()
                    .unwrap_or_else(|| panic!("{} {k} is not text", e["id"]));
                let v: Value = serde_json::from_str(t).unwrap();
                assert_eq!(canonical_json(&v), t, "{} {k} is not canonical", e["id"]);
                *n += 1;
            }
        }
    }
    assert_eq!(seen, (lib.checks().len(), lib.scenarios().len()));

    // Each export stored a report; same-titled results replace each other,
    // so the three exports left one.
    assert_eq!(s.artifacts.len(), 1);
    assert_eq!(s.artifacts[0].source.command, "content.export");
}

/// A document whose `check` and `scenario` are objects reads as the same
/// ModelContent (and seals to the same CID) as the one whose are canonical
/// strings: the library file and the cafe fixture both are authored as
/// objects.
#[test]
fn objects_and_canonical_strings_are_the_same_content() {
    let orders_file: Value = {
        let text = std::fs::read_to_string(format!(
            "{}/src/composition/library/orders.content.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        serde_json::from_str(&text).unwrap()
    };
    for (name, objects_doc) in [("orders", orders_file), ("cafe", cafe_content())] {
        let objects = ModelContent::parse(&objects_doc).unwrap();
        let text_doc = objects.to_document();
        assert!(
            text_doc["entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["check"].is_string()),
            "{name}: the exported form has text"
        );
        let strings = ModelContent::parse(&text_doc).unwrap();
        assert_eq!(objects, strings, "{name}");
        assert_eq!(objects.cid().unwrap(), strings.cid().unwrap(), "{name}");
        assert_eq!(strings.to_document(), text_doc, "{name}");
        // And the object form is what a view is handed.
        let e = objects.entries.iter().find(|e| e.check.is_some()).unwrap();
        assert!(e.to_view_json()["check"].is_object());
        assert!(e.to_view_json()["check"] == *e.check.as_ref().unwrap());
    }

    // Key order inside the text does not matter to what it means: both read
    // as the same object, and the export is the sorted form.
    let mut a = plain("a");
    a["check"] = json!(
        r#"{"steps":[{"command":"bundle.state","expect":[{"path":"","op":"exists"}],"args":{}}],"claim":"c"}"#
    );
    let mut b = plain("a");
    b["check"] = json!(
        r#"{"claim":"c","steps":[{"args":{},"command":"bundle.state","expect":[{"op":"exists","path":""}]}]}"#
    );
    let (ma, mb) = (
        ModelContent::parse(&doc(vec![a])).unwrap(),
        ModelContent::parse(&doc(vec![b])).unwrap(),
    );
    assert_eq!(ma, mb);
    assert_eq!(ma.cid().unwrap(), mb.cid().unwrap());
    assert_eq!(
        ma.to_document()["entries"][0]["check"],
        mb.to_document()["entries"][0]["check"]
    );

    // A check that is neither an object nor text holding one is a
    // CONTENT_FIELD.
    for bad in [json!(7), json!("not json"), json!("[1]"), json!([1])] {
        let e = ModelContent::parse(&doc(vec![with(plain("a"), "check", bad.clone())]))
            .unwrap_err()
            .to_string();
        assert!(e.starts_with("[CONTENT_FIELD]"), "{bad}: {e}");
    }
}

// ------------------------------------------------------------ 2. discrimination
//
// A check that cannot fail backs nothing. For each finding below the model is
// changed so that the claim is false, and the check that backed it goes red at
// the row that states the claim; a control edit that leaves the claim true
// leaves the check green.

/// find-pool-leaks-on-rollback: "the worker stays in workers/in_use after a
/// rollback because release is fused only with commit".
///
/// The release link is re-pointed from payment:commit to payment:undo:reserve
/// (the "move" repair of find-no-single-link-makes-a-lens). Release now fuses
/// with the step that runs on rollback, so the refund run returns the worker:
/// in_use is 0, the refund's workers/available predicate passes, and release's
/// flat transition is no longer the commit one. The claim is false.
#[test]
fn pool_leak_finding_goes_red_when_release_is_repointed() {
    let id = "find-pool-leaks-on-rollback";
    let mut s = orders();
    assert_green(&check_one(&mut s, id));

    run(
        &mut s,
        "bundle.remove_link",
        json!({"kind": "event", "from": "workers:release", "to": "payment:commit"}),
    );
    run(
        &mut s,
        "bundle.add_link",
        json!({"kind": "event", "from": "workers:release", "to": "payment:undo:reserve"}),
    );
    let c = check_one(&mut s, id);
    assert_red_at(&c, "/final_marking/workers~1in_use");
    assert_red_at(&c, "/expect/2/pass");
    assert_red_at(&c, "/transition_map/workers/release");

    // Undoing the two edits puts the claim back.
    run(&mut s, "edit.undo", Value::Null);
    run(&mut s, "edit.undo", Value::Null);
    assert_green(&check_one(&mut s, id));
}

/// The same finding with the release link removed outright: release is no
/// longer fused with commit, so "release is fused only with commit" is
/// false even though the refund still leaks.
#[test]
fn pool_leak_finding_goes_red_when_release_link_is_removed() {
    let id = "find-pool-leaks-on-rollback";
    let mut s = orders();
    run(
        &mut s,
        "bundle.remove_link",
        json!({"kind": "event", "from": "workers:release", "to": "payment:commit"}),
    );
    let c = check_one(&mut s, id);
    assert_red_at(&c, "/transition_map/workers/release");
    // The leak itself is still observable, which is why the check states the
    // fusion and not only the run.
    let run_rows: Vec<&Value> = c["steps"][0]["expect"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["path"] == "/final_marking/workers~1in_use")
        .collect();
    assert_eq!(run_rows[0]["ok"], true, "{:?}", run_rows);
}

/// find-one-saga-one-case: "the second order stops at the second dequeue
/// because the fused dequeue needs payment/next_reserve, which the one-shot
/// saga spent".
///
/// A second saga, payment2, takes the same dequeue (a fourth member of the
/// rendezvous) and shares payment's customer account by a token link. The
/// claim names the three-member fused transition; with payment2 joined the
/// class is a different transition, so the claim as written is false.
#[test]
fn one_saga_one_case_finding_goes_red_when_a_second_saga_joins_the_dequeue() {
    let id = "find-one-saga-one-case";
    let mut s = orders();
    assert_green(&check_one(&mut s, id));

    // Control: an unlinked payment2 is not part of the rendezvous.
    run(&mut s, "bundle.add_instance", json!({"id": "payment2"}));
    assert_green(&check_one(&mut s, id));

    run(
        &mut s,
        "bundle.add_link",
        json!({"kind": "token", "from": "payment:account:customer", "to": "payment2:account:customer"}),
    );
    // The token link alone does not change the claim either: it adds a wire
    // place and no transition.
    assert_green(&check_one(&mut s, id));

    run(
        &mut s,
        "bundle.add_link",
        json!({"kind": "event", "from": "orders:dequeue", "to": "payment2:do:reserve"}),
    );
    let c = check_one(&mut s, id);
    assert_red_at(&c, "/steps/3/flat");
    // The rest of the claim still reads true of this model: it is the
    // statement about which transition is meant that went false.
    assert_eq!(reds(&c).len(), 1, "{:?}", reds(&c));
}

/// find-one-saga-one-case also reads the declared `second-order` scenario.
/// Changed to stop before the second dequeue, the run completes and the
/// claim "it stops at the second dequeue" is false.
#[test]
fn one_saga_one_case_finding_goes_red_when_its_scenario_is_changed() {
    let id = "find-one-saga-one-case";
    let mut s = orders();
    edit_entry(&mut s, "second-order", |e| {
        let script = e["scenario"]["script"].as_array_mut().unwrap();
        assert_eq!(script.pop().unwrap(), "orders/dequeue");
    });
    let c = check_one(&mut s, id);
    assert_red_at(&c, "/completed");
    assert_red_at(&c, "/stopped_at");
    assert_red_at(&c, "/steps/3/flat");
    // The scenario's own check is the one that states the old script.
    // (Nothing else in the content was touched.)
    assert_eq!(
        run(&mut s, "content.validate", json!({}))["valid"],
        true,
        "the edited content is sound; only its claim is false"
    );
}

/// find-dequeue-idioms-are-one-rendezvous: "two binary event links share
/// orders:dequeue, yet Flatten has exactly one class for them".
///
/// With either of the two dequeue links removed there are not two links
/// sharing dequeue, and the three-member class is gone (and the link list the
/// claim reads has lost an element).
#[test]
fn rendezvous_finding_goes_red_when_either_dequeue_link_is_removed() {
    let id = "find-dequeue-idioms-are-one-rendezvous";
    let class = "/fused_groups/fused:orders~1dequeue+payment~1do_reserve+workers~1acquire";
    for to in ["workers:acquire", "payment:do:reserve"] {
        let mut s = orders();
        assert_green(&check_one(&mut s, id));
        run(
            &mut s,
            "bundle.remove_link",
            json!({"kind": "event", "from": "orders:dequeue", "to": to}),
        );
        let c = check_one(&mut s, id);
        assert_red_at(&c, class);
        assert_red_at(&c, "/transition_map/orders/dequeue");
        assert_red_at(&c, "/links/2/key");
    }
}

/// find-not-a-lens-composite: the strict lens test fails because "the pool's
/// release is fused into the forward commit" (step 2 of the forward pass is
/// fused:payment/commit+workers/release and does not compose).
///
/// No single link repairs the lens (that is find-no-single-link-makes-a-lens,
/// whose check is green): every one-link fix `bundle.lens` tries leaves the
/// strict test false. So the red case here is a link removed: without
/// release -> commit, step 2 is payment/commit alone and the claim about the
/// fused step is false. Adding the "repair" link of lens-repair-add-... is the
/// second red case: it changes the fusion and still does not make a lens.
#[test]
fn lens_finding_goes_red_when_the_release_fusion_changes() {
    let id = "find-not-a-lens-composite";
    let mut s = orders();
    assert_green(&check_one(&mut s, id));

    // No single link makes a lens, read straight off the repairs.
    let lens = run(&mut s, "bundle.lens", json!({}));
    let fixes = lens["fixes"].as_array().unwrap();
    assert!(!fixes.is_empty());
    for f in fixes {
        assert_ne!(f["result"]["holds"], true, "a one-link repair holds: {f}");
    }

    run(
        &mut s,
        "bundle.remove_link",
        json!({"kind": "event", "from": "workers:release", "to": "payment:commit"}),
    );
    let c = check_one(&mut s, id);
    assert_red_at(&c, "/analysis/steps/2/flat");
    run(&mut s, "edit.undo", Value::Null);
    assert_green(&check_one(&mut s, id));

    run(
        &mut s,
        "bundle.add_link",
        json!({"kind": "event", "from": "workers:release", "to": "payment:undo:reserve"}),
    );
    let c = check_one(&mut s, id);
    assert_red_at(&c, "/analysis/steps/2/flat");
    let lens = run(&mut s, "bundle.lens", json!({"fixes": false}));
    assert_eq!(
        lens["analysis"]["holds"], false,
        "adding that link does not make the composite a lens"
    );
}

/// An entry's expected value edited so that it disagrees with the model: the
/// row goes red with the actual value beside it. A pointer that finds nothing
/// is red for `==` and green for `absent`.
#[test]
fn a_changed_expected_value_or_pointer_turns_the_check_red() {
    let id = "find-pool-leaks-on-rollback";
    let mut s = orders();
    assert_green(&check_one(&mut s, id));

    // The refund leaves 1 worker in use. Claim 2 instead.
    edit_entry(&mut s, id, |e| {
        let row = e["check"]["steps"][0]["expect"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["path"] == "/final_marking/workers~1in_use")
            .unwrap();
        assert_eq!(row["value"], 1);
        row["value"] = json!(2);
    });
    let c = check_one(&mut s, id);
    assert_red_at(&c, "/final_marking/workers~1in_use");
    assert_eq!(reds(&c).len(), 1, "{:?}", reds(&c));
    let row = c["steps"][0]["expect"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["path"] == "/final_marking/workers~1in_use")
        .unwrap();
    assert_eq!(row["actual"], 1);
    assert_eq!(row["value"], 2);
    assert_eq!(row["found"], true);

    // A pointer into nothing.
    edit_entry(&mut s, id, |e| {
        let rows = e["check"]["steps"][0]["expect"].as_array_mut().unwrap();
        rows.clear();
        rows.push(json!({"path": "/final_marking/workers~1gone", "op": "==", "value": 1}));
        rows.push(json!({"path": "/final_marking/workers~1gone", "op": "absent"}));
        rows.push(json!({"path": "/final_marking/workers~1gone", "op": "exists"}));
    });
    let c = check_one(&mut s, id);
    let rows = c["steps"][0]["expect"].as_array().unwrap();
    assert_eq!(
        rows.iter().map(|r| r["ok"].clone()).collect::<Vec<_>>(),
        vec![json!(false), json!(true), json!(false)]
    );
    assert_eq!(rows[0]["found"], false);
    assert_eq!(rows[0]["actual"], Value::Null);
    // A step whose command fails is red with its error, and the later steps
    // still run.
    edit_entry(&mut s, id, |e| {
        e["check"]["steps"] = json!([
            {"command": "bundle.run", "args": {},
             "expect": [{"path": "/completed", "op": "==", "value": true}]},
            {"command": "bundle.state", "args": {},
             "expect": [{"path": "/instances/0", "op": "==", "value": "orders"}]}
        ]);
    });
    let c = check_one(&mut s, id);
    assert_eq!(c["ok"], false);
    assert!(c["steps"][0]["error"]
        .as_str()
        .unwrap()
        .contains("pass `scenario` or `script`"));
    assert_eq!(c["steps"][0]["ok"], false);
    assert_eq!(c["steps"][1]["ok"], true, "the second step ran");
}

/// A declared scenario edited so that its own prediction is wrong turns the
/// check that reads the prediction red: `commit` claims the worker returns to
/// the pool (workers/available == 2); declared as 3, the run's predicate fails.
#[test]
fn a_changed_scenario_prediction_turns_its_check_red() {
    let mut s = orders();
    assert_green(&check_one(&mut s, "commit"));
    edit_entry(&mut s, "commit", |e| {
        e["scenario"]["expect"][1]["n"] = json!(3);
    });
    let c = check_one(&mut s, "commit");
    assert_red_at(&c, "/expect/1/pass");
    assert_eq!(reds(&c).len(), 1, "{:?}", reds(&c));
    // Other scenarios' checks are untouched.
    assert_green(&check_one(&mut s, "refund"));
}

/// The operators of a check row.
#[test]
fn check_operators_hold_and_fail_where_they_should() {
    let hold = |op: &str, actual: Option<Value>, value: Option<Value>| {
        checks::holds(op, actual.as_ref(), value.as_ref())
    };
    // ==, !=
    assert!(hold("==", Some(json!(1)), Some(json!(1))));
    assert!(!hold("==", Some(json!(1)), Some(json!(2))));
    assert!(!hold("==", Some(json!(1)), Some(json!("1"))));
    assert!(hold("==", Some(json!(["a", "b"])), Some(json!(["a", "b"]))));
    assert!(!hold(
        "==",
        Some(json!(["a", "b"])),
        Some(json!(["b", "a"]))
    ));
    assert!(hold("!=", Some(json!(1)), Some(json!(2))));
    assert!(!hold("!=", Some(json!(1)), Some(json!(1))));
    // A pointer that found nothing is neither equal nor unequal.
    assert!(!hold("==", None, Some(json!(null))));
    assert!(!hold("!=", None, Some(json!(1))));
    // Ordering is numeric.
    for (op, a, b, want) in [
        ("<", 1, 2, true),
        ("<", 2, 2, false),
        ("<=", 2, 2, true),
        ("<=", 3, 2, false),
        (">", 3, 2, true),
        (">", 2, 2, false),
        (">=", 2, 2, true),
        (">=", 1, 2, false),
    ] {
        assert_eq!(
            hold(op, Some(json!(a)), Some(json!(b))),
            want,
            "{a} {op} {b}"
        );
    }
    assert!(
        !hold("<", Some(json!("a")), Some(json!("b"))),
        "strings do not order"
    );
    assert!(!hold(">", Some(json!(true)), Some(json!(0))));
    // contains: substring of a string, member of an array, nothing else.
    assert!(hold("contains", Some(json!("have 0")), Some(json!("have"))));
    assert!(!hold("contains", Some(json!("have 0")), Some(json!("had"))));
    assert!(!hold("contains", Some(json!("have 0")), Some(json!(0))));
    assert!(hold("contains", Some(json!(["a", 2])), Some(json!(2))));
    assert!(!hold("contains", Some(json!(["a", 2])), Some(json!("2"))));
    assert!(!hold("contains", Some(json!({"a": 1})), Some(json!("a"))));
    assert!(!hold("contains", Some(json!(12)), Some(json!(1))));
    // exists, absent: only the pointer matters (null exists).
    assert!(hold("exists", Some(json!(null)), None));
    assert!(!hold("exists", None, None));
    assert!(hold("absent", None, None));
    assert!(!hold("absent", Some(json!(false)), None));
    // An operator no row may use.
    assert!(!hold("~=", Some(json!(1)), Some(json!(1))));
    assert!(!hold("==", Some(json!(1)), None), "== needs a value");

    // RFC 6901 pointers, including the escapes the library checks rely on.
    let doc = json!({"a/b": [10, {"c~d": 1}], "": 5});
    assert_eq!(checks::pointer(&doc, "/a~1b/0"), Some(&json!(10)));
    assert_eq!(checks::pointer(&doc, "/a~1b/1/c~0d"), Some(&json!(1)));
    assert_eq!(checks::pointer(&doc, ""), Some(&doc));
    assert_eq!(checks::pointer(&doc, "/"), Some(&json!(5)));
    assert_eq!(checks::pointer(&doc, "/a~1b/2"), None);
}

// ------------------------------------------------------------ 3. the validator
//
// One test per code of docs/composition-documents.md §3.3. Each breaks one thing
// in an otherwise sound document and asserts that code and no other, so a code
// that is never emitted, or is emitted for the wrong reason, shows.

/// `d` with `f` applied.
fn edited(d: &Value, f: impl FnOnce(&mut Value)) -> Value {
    let mut d = d.clone();
    f(&mut d);
    d
}

/// A declared scenario entry, attached to its own id.
fn scenario_entry(id: &str, script: Value, expect: Value) -> Value {
    json!({
        "id": id, "kind": "scenario", "about": [format!("scenario:{id}")],
        "scenario": {"script": script, "expect": expect},
    })
}

/// The entry ids a `content.get` result holds, in order.
fn ids_of(got: &Value) -> Vec<String> {
    got["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect()
}

/// The codes of `bundle.state`'s `missing`, in order.
fn missing_codes(s: &mut Session) -> Vec<String> {
    run(s, "bundle.state", json!({}))["missing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["code"].as_str().unwrap().to_string())
        .collect()
}

/// The open composition as a PatternComposition / PetriNetBundle document.
fn document_of(s: &mut Session) -> Value {
    run(s, "bundle.export", json!({}))["document"].clone()
}

#[test]
fn a_sound_document_validates_with_every_kind_and_field() {
    let expect = json!([{"place": "orders/items", "op": "==", "n": 1, "says": "one is taken"}]);
    let d = doc(vec![
        plain("whole-model"),
        with(
            entry(
                "on-elements",
                "caveat",
                &["place:workers/in_use", "port:payment:do:reserve"],
            ),
            "views",
            json!(["flat", "process"]),
        ),
        with(
            entry("shown", "convention-example", &["model:"]),
            "views",
            json!(["flat"]),
        ),
        entry("judged", "judgment", &["instance:orders"]),
        entry("noted", "note", &["transition:orders/dequeue"]),
        scenario_entry("one", json!(["orders/dequeue"]), expect),
        with(
            plain("cited"),
            "cites",
            json!([
                {"bib": "baez-master"},
                {"repo": "go-pflow", "rev": "69257b3", "path": "metamodel/compose.go",
                 "from": 1, "to": 2, "note": "n"}
            ]),
        ),
        with(
            plain("checked"),
            "check",
            check_of(json!([step("bundle.state")])),
        ),
    ]);
    let r = report(&d, true);
    assert!(r.valid(), "{:?}", r.errors);
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
}

/// CONTENT_TYPE: not a ModelContent document at all.
#[test]
fn content_type_refuses_a_document_that_is_not_model_content() {
    let good = doc(vec![plain("a")]);
    let wrong_type = edited(&good, |d| d["@type"] = json!("PetriNetBundle"));
    let cases = [
        ("an array", json!([1])),
        ("a string", json!("x")),
        ("a wrong @type", wrong_type.clone()),
        (
            "no @type",
            edited(&good, |d| {
                d.as_object_mut().unwrap().remove("@type");
            }),
        ),
        (
            "a wrong @context",
            edited(&good, |d| {
                d["@context"] = json!("https://example.com/schema")
            }),
        ),
        (
            "no @context",
            edited(&good, |d| {
                d.as_object_mut().unwrap().remove("@context");
            }),
        ),
    ];
    for (what, d) in &cases {
        let e = ModelContent::parse(d).unwrap_err().to_string();
        assert!(e.starts_with("[CONTENT_TYPE]"), "{what}: {e}");
    }

    // The commands refuse it the same way, and what was attached stays.
    let mut s = orders();
    let before = run(&mut s, "content.export", json!({}))["cid"].clone();
    let e = refused(&mut s, "content.load", json!({"document": wrong_type}));
    assert!(e.starts_with("[CONTENT_TYPE]"), "{e}");
    let composition = document_of(&mut s);
    let e = refused(
        &mut s,
        "bundle.load",
        json!({"document": composition, "content": wrong_type}),
    );
    assert!(e.starts_with("[CONTENT_TYPE]"), "{e}");
    assert_eq!(run(&mut s, "content.export", json!({}))["cid"], before);
}

/// CONTENT_FIELD: a field that is missing or the wrong type.
#[test]
fn content_field_refuses_a_missing_or_mistyped_field() {
    let good = doc(vec![plain("a")]);
    let cases = [
        (
            "an unknown document key",
            edited(&good, |d| d["extra"] = json!(1)),
        ),
        (
            "no subject",
            edited(&good, |d| {
                d.as_object_mut().unwrap().remove("subject");
            }),
        ),
        (
            "a subject of the wrong kind",
            edited(&good, |d| d["subject"]["kind"] = json!("net")),
        ),
        (
            "entries that are not a list",
            edited(&good, |d| d["entries"] = json!({})),
        ),
        (
            "no entries",
            edited(&good, |d| {
                d.as_object_mut().unwrap().remove("entries");
            }),
        ),
        (
            "an entry that is not an object",
            edited(&good, |d| d["entries"] = json!(["a"])),
        ),
        (
            "an entry without an id",
            edited(&good, |d| {
                d["entries"][0].as_object_mut().unwrap().remove("id");
            }),
        ),
        (
            "an unknown entry key",
            edited(&good, |d| d["entries"][0]["colour"] = json!("red")),
        ),
        (
            "a title that is not text",
            edited(&good, |d| d["entries"][0]["title"] = json!(5)),
        ),
        (
            "about that is not a list",
            edited(&good, |d| d["entries"][0]["about"] = json!("model:")),
        ),
        (
            "about holding a number",
            edited(&good, |d| d["entries"][0]["about"] = json!([1])),
        ),
        (
            "cites that is not a list",
            edited(&good, |d| d["entries"][0]["cites"] = json!({"bib": "x"})),
        ),
        (
            "a check that is a number",
            edited(&good, |d| d["entries"][0]["check"] = json!(7)),
        ),
        (
            "a scenario that is not JSON text",
            edited(&good, |d| d["entries"][0]["scenario"] = json!("not json")),
        ),
    ];
    for (what, d) in &cases {
        let e = ModelContent::parse(d).unwrap_err().to_string();
        assert!(e.starts_with("[CONTENT_FIELD]"), "{what}: {e}");
        // The same refusal through a command, before anything is attached.
        let mut s = orders();
        let e = refused(&mut s, "content.load", json!({"document": d}));
        assert!(e.starts_with("[CONTENT_FIELD]"), "{what}: {e}");
    }

    // The text every entry but a scenario needs is the validator's to ask for.
    let no_text = json!({"id": "a", "kind": "finding", "about": ["model:"]});
    assert_only_error(
        &report(&doc(vec![no_text.clone()]), true),
        "CONTENT_FIELD",
        Some("a"),
    );
    assert_only_error(
        &report(&doc(vec![with(plain("a"), "text", json!(""))]), true),
        "CONTENT_FIELD",
        Some("a"),
    );
    let mut s = orders();
    let e = refused(
        &mut s,
        "content.load",
        json!({"document": doc(vec![no_text])}),
    );
    assert!(
        e.starts_with("[CONTENT_REFUSED]") && e.contains("[CONTENT_FIELD] text is required"),
        "{e}"
    );
}

#[test]
fn entry_duplicate_names_the_id_that_appears_twice() {
    let r = report(&doc(vec![plain("a"), plain("b"), plain("a")]), true);
    assert_only_error(&r, "ENTRY_DUPLICATE", Some("a"));
    // Three of one id is still one finding per extra copy, never silence.
    let r = report(&doc(vec![plain("a"), plain("a"), plain("a")]), true);
    assert_eq!(codes(&r.errors), vec!["ENTRY_DUPLICATE", "ENTRY_DUPLICATE"]);
}

#[test]
fn entry_id_must_match_the_id_syntax() {
    for bad in [
        "",
        "Upper",
        "has space",
        "-lead",
        ".lead",
        "colon:id",
        "slash/id",
        "ünï",
    ] {
        let r = report(&doc(vec![plain(bad)]), true);
        assert_only_error(&r, "ENTRY_ID", Some(bad));
    }
    for good in ["a", "0", "a.b-c_d", "x9", "find-pool-leaks-on-rollback"] {
        let r = report(&doc(vec![plain(good)]), true);
        assert!(r.valid(), "{good}: {:?}", r.errors);
    }
}

#[test]
fn entry_kind_must_be_one_of_the_six() {
    for bad in ["opinion", "Finding", "findings", ""] {
        let r = report(&doc(vec![entry("a", bad, &["model:"])]), true);
        assert_only_error(&r, "ENTRY_KIND", Some("a"));
    }
    // Every kind there is passes (a scenario with its scenario, a
    // convention example with its one view).
    for kind in content::KINDS {
        let e = match kind {
            "scenario" => scenario_entry("a", json!(["orders/dequeue"]), json!([])),
            "convention-example" => with(entry("a", kind, &["model:"]), "views", json!(["lens"])),
            _ => entry("a", kind, &["model:"]),
        };
        let r = report(&doc(vec![e]), true);
        assert!(r.valid(), "{kind}: {:?}", r.errors);
    }
}

#[test]
fn entry_about_empty_asks_for_at_least_one_element() {
    let empty = with(plain("a"), "about", json!([]));
    assert_only_error(
        &report(&doc(vec![empty]), true),
        "ENTRY_ABOUT_EMPTY",
        Some("a"),
    );
    let absent = json!({"id": "a", "kind": "finding", "text": "t"});
    assert_only_error(
        &report(&doc(vec![absent]), true),
        "ENTRY_ABOUT_EMPTY",
        Some("a"),
    );
}

#[test]
fn entry_view_names_an_unknown_view_or_a_convention_example_without_one_view() {
    let r = report(
        &doc(vec![with(plain("a"), "views", json!(["flat", "venn"]))]),
        true,
    );
    assert_only_error(&r, "ENTRY_VIEW", Some("a"));
    assert_eq!(r.errors[0].reference.as_deref(), Some("venn"));

    // A convention example names exactly one view: none and two are both wrong.
    for views in [json!([]), json!(["flat", "process"])] {
        let e = with(
            entry("c", "convention-example", &["model:"]),
            "views",
            views,
        );
        let r = report(&doc(vec![e]), true);
        assert_only_error(&r, "ENTRY_VIEW", Some("c"));
        assert_eq!(r.errors[0].reference, None);
    }
    let missing = entry("c", "convention-example", &["model:"]);
    assert_only_error(&report(&doc(vec![missing]), true), "ENTRY_VIEW", Some("c"));
    let one = with(
        entry("c", "convention-example", &["model:"]),
        "views",
        json!(["flat"]),
    );
    assert!(report(&doc(vec![one]), true).valid());

    // The ten views, each by itself and all together, on a finding.
    for v in content::VIEWS {
        assert!(report(&doc(vec![with(plain("a"), "views", json!([v]))]), true).valid());
    }
    let all = with(plain("a"), "views", json!(content::VIEWS));
    assert!(report(&doc(vec![all]), true).valid());
}

#[test]
fn id_syntax_is_an_error_even_when_not_strict() {
    for (bad, why) in [
        ("workers/in_use", "not <type>:<ref>"),
        ("", "not <type>:<ref>"),
        ("gadget:x", "unknown element type"),
        ("Place:workers/in_use", "unknown element type"),
        (":x", "unknown element type"),
    ] {
        let d = doc(vec![entry("a", "finding", &[bad])]);
        for strict in [true, false] {
            let r = report(&d, strict);
            assert_only_error(&r, "ID_SYNTAX", Some("a"));
            assert_eq!(r.errors[0].reference.as_deref(), Some(bad));
            assert!(r.errors[0].message.contains(why), "{:?}", r.errors[0]);
            assert!(r.warnings.is_empty());
        }
        // A syntax error is not an unresolved id: the entry is refused, not dormant.
        let mut s = orders();
        let e = refused(
            &mut s,
            "content.load",
            json!({"document": d, "strict": false}),
        );
        assert!(
            e.starts_with("[CONTENT_REFUSED]") && e.contains("[ID_SYNTAX]"),
            "{e}"
        );
    }
}

/// ID_UNRESOLVED under `strict`: an error, and the load is refused.
#[test]
fn id_unresolved_refuses_a_strict_load_and_leaves_the_session_as_it_was() {
    let d = doc(vec![
        plain("ok"),
        entry("gone", "finding", &["place:workers/nope", "model:"]),
    ]);
    let r = report(&d, true);
    assert_only_error(&r, "ID_UNRESOLVED", Some("gone"));
    assert_eq!(r.errors[0].reference.as_deref(), Some("place:workers/nope"));
    assert!(r.warnings.is_empty());
    assert_eq!(
        r.unresolved(),
        vec![json!({"entry": "gone", "ref": "place:workers/nope"})]
    );

    let mut s = orders();
    let before = run(&mut s, "content.export", json!({}))["cid"].clone();
    let composition = document_of(&mut s);
    let wants = |e: &str| {
        e.starts_with("[CONTENT_REFUSED]")
            && e.contains("[ID_UNRESOLVED]")
            && e.contains("place:workers/nope")
    };
    // strict is the default of both loaders, and can be said.
    let e = refused(&mut s, "content.load", json!({"document": d}));
    assert!(wants(&e), "{e}");
    let e = refused(
        &mut s,
        "content.load",
        json!({"document": d, "strict": true}),
    );
    assert!(wants(&e), "{e}");
    let e = refused(
        &mut s,
        "bundle.load",
        json!({"document": composition, "content": d}),
    );
    assert!(wants(&e), "{e}");
    assert_eq!(run(&mut s, "content.export", json!({}))["cid"], before);
    assert_eq!(
        run(&mut s, "bundle.state", json!({}))["content"]["source"],
        "library:orders"
    );

    // A refused load left no undo step: one undo goes back past bundle.open.
    run(&mut s, "edit.undo", Value::Null);
    assert!(s.composition.is_none());
}

/// ID_UNRESOLVED without `strict`: a warning, and the entry is dormant.
#[test]
fn id_unresolved_is_a_warning_and_a_dormant_entry_when_not_strict() {
    let d = doc(vec![
        entry("live", "finding", &["place:workers/in_use"]),
        entry("gone", "finding", &["place:workers/nope"]),
    ]);
    let r = report(&d, false);
    assert!(r.valid(), "{:?}", r.errors);
    assert_eq!(codes(&r.warnings), vec!["ID_UNRESOLVED"]);
    assert_eq!(r.warnings[0].entry.as_deref(), Some("gone"));
    assert_eq!(
        r.unresolved(),
        vec![json!({"entry": "gone", "ref": "place:workers/nope"})]
    );

    // content.load strict false: loads, says what is unresolved.
    let mut s = orders();
    let loaded = run(
        &mut s,
        "content.load",
        json!({"document": d, "strict": false}),
    );
    assert_eq!(loaded["entries"], 2);
    assert_eq!(
        loaded["unresolved"],
        json!([{"entry": "gone", "ref": "place:workers/nope"}])
    );
    // content.get hides the dormant entry unless asked for all of them.
    let got = run(&mut s, "content.get", json!({}));
    assert_eq!(ids_of(&got), ["live"]);
    assert_eq!(
        (got["total"].clone(), got["dormant"].clone()),
        (json!(2), json!(1))
    );
    let all = run(&mut s, "content.get", json!({"all": true}));
    assert_eq!(ids_of(&all), ["live", "gone"]);
    assert_eq!(all["entries"][0]["dormant"], false);
    assert_eq!(all["entries"][1]["dormant"], true);
    assert_eq!(all["dormant"], 1);
    // content.validate is strict unless told: the same content is invalid by
    // default and valid with a warning when `strict` is false.
    let strict = run(&mut s, "content.validate", json!({}));
    assert_eq!(strict["valid"], false);
    assert_eq!(strict["errors"][0]["code"], "ID_UNRESOLVED");
    let lax = run(&mut s, "content.validate", json!({"strict": false}));
    assert_eq!(lax["valid"], true);
    assert_eq!(lax["warnings"][0]["code"], "ID_UNRESOLVED");
    assert_eq!(lax["unresolved"], loaded["unresolved"]);
    // bundle.state carries the same list for the views.
    assert_eq!(
        run(&mut s, "bundle.state", json!({}))["content"]["unresolved"],
        loaded["unresolved"]
    );

    // bundle.load takes the same flag.
    let composition = document_of(&mut s);
    let mut t = session();
    let r = run(
        &mut t,
        "bundle.load",
        json!({"document": composition, "content": d, "strict": false}),
    );
    assert_eq!(
        r["content"]["unresolved"],
        json!([{"entry": "gone", "ref": "place:workers/nope"}])
    );
    assert_eq!(ids_of(&run(&mut t, "content.get", json!({}))), ["live"]);
}

#[test]
fn scenario_invalid_names_the_member_place_op_or_shape_that_is_wrong() {
    let ok_expect = json!([{"place": "orders/items", "op": "==", "n": 1, "says": "s"}]);
    let sound = scenario_entry("s", json!(["orders/dequeue"]), ok_expect.clone());
    assert!(report(&doc(vec![sound]), true).valid());

    // A member that is not a transition of the composition.
    let bad_member = scenario_entry(
        "s",
        json!(["orders/dequeue", "orders/nope"]),
        ok_expect.clone(),
    );
    let r = report(&doc(vec![bad_member]), true);
    assert_only_error(&r, "SCENARIO_INVALID", Some("s"));
    assert_eq!(r.errors[0].reference.as_deref(), Some("orders/nope"));
    assert!(r.errors[0].message.contains("not a transition"));

    // A place that is not a place.
    let bad_place = scenario_entry(
        "s",
        json!(["orders/dequeue"]),
        json!([{"place": "orders/nope", "op": "==", "n": 1, "says": "s"}]),
    );
    let r = report(&doc(vec![bad_place]), true);
    assert_only_error(&r, "SCENARIO_INVALID", Some("s"));
    assert_eq!(r.errors[0].reference.as_deref(), Some("orders/nope"));
    assert!(r.errors[0].message.contains("not a place"));

    // An op outside the six (`contains` is a check's op, not a scenario's).
    let bad_op = scenario_entry(
        "s",
        json!(["orders/dequeue"]),
        json!([{"place": "orders/items", "op": "contains", "n": 1, "says": "s"}]),
    );
    let r = report(&doc(vec![bad_op]), true);
    assert_only_error(&r, "SCENARIO_INVALID", Some("s"));
    assert!(r.errors[0].message.contains("unknown op"));

    // Every op of the six is accepted.
    for op in content::SCENARIO_OPS {
        let e = scenario_entry(
            "s",
            json!(["orders/dequeue"]),
            json!([{"place": "orders/items", "op": op, "n": 1, "says": "s"}]),
        );
        assert!(report(&doc(vec![e]), true).valid(), "{op}");
    }

    // A scenario of the wrong shape: no script, a script of numbers, a row
    // with no n or no place.
    for shape in [
        json!({"expect": []}),
        json!({"script": [1], "expect": []}),
        json!({"script": "orders/dequeue", "expect": []}),
        json!({"script": [], "expect": [{"place": "orders/items", "op": "==", "says": "s"}]}),
        json!({"script": [], "expect": [{"op": "==", "n": 1, "says": "s"}]}),
    ] {
        let e = edited(&scenario_entry("s", json!([]), json!([])), |e| {
            e["scenario"] = shape.clone()
        });
        let r = report(&doc(vec![e]), true);
        assert_only_error(&r, "SCENARIO_INVALID", Some("s"));
    }

    // The shape of a scenario is wrong either way; a member or place the model lacks is an error
    // for an explicit load and a warning (the scenario is dormant) when not strict, like an
    // element id that does not resolve.
    let bad_shape = edited(&scenario_entry("s", json!([]), json!([])), |e| {
        e["scenario"] = json!({"script": [1], "expect": []})
    });
    assert_only_error(
        &report(&doc(vec![bad_shape]), false),
        "SCENARIO_INVALID",
        Some("s"),
    );
    let lacks = scenario_entry("s", json!(["orders/nope"]), json!([]));
    let r = report(&doc(vec![lacks]), false);
    assert!(r.valid(), "{:?}", r.errors);
    assert_eq!(codes(&r.warnings), vec!["SCENARIO_UNRESOLVED"]);
    assert_eq!(r.warnings[0].reference.as_deref(), Some("orders/nope"));
    assert_eq!(
        r.unresolved(),
        vec![json!({"entry": "s", "ref": "orders/nope"})],
        "the load result names it among the unresolved"
    );

    // A plan declares no firing scenarios at all.
    let d = edited(&doc(vec![scenario_entry("s", json!([]), json!([]))]), |d| {
        d["subject"] = json!({"kind": "plan", "name": "connect4"})
    });
    let r = content::validate_plan(&ModelContent::parse(&d).unwrap(), true);
    assert!(
        r.errors
            .iter()
            .any(|i| i.code == "SCENARIO_INVALID" && i.message.contains("no firing scenarios")),
        "{:?}",
        r.errors
    );
}

#[test]
fn cite_bib_unknown_names_the_key_the_ontology_lacks() {
    let r = report(
        &doc(vec![with(
            plain("a"),
            "cites",
            json!([{"bib": "no-such-paper"}]),
        )]),
        true,
    );
    assert_only_error(&r, "CITE_BIB_UNKNOWN", Some("a"));
    assert_eq!(r.errors[0].reference.as_deref(), Some("no-such-paper"));

    // Every key the ontology's bib has is accepted, and a source cite (no
    // `bib`) is not looked up.
    let keys: Vec<Value> = comp::ontology()["bib"]
        .as_object()
        .unwrap()
        .keys()
        .map(|k| json!({"bib": k}))
        .collect();
    assert!(keys.len() > 3);
    assert!(report(&doc(vec![with(plain("a"), "cites", json!(keys))]), true).valid());
    let source = json!([{"repo": "nowhere", "rev": "0", "path": "none"}]);
    assert!(report(&doc(vec![with(plain("a"), "cites", source)]), true).valid());
}

#[test]
fn check_invalid_names_what_is_wrong_with_the_shape_of_a_check() {
    let rows = |row: Value| json!([{"command": "bundle.state", "args": {}, "expect": [row]}]);
    let exists = || json!({"path": "", "op": "exists"});
    let cases = [
        ("no steps key", json!({"claim": "c"}), "needs steps"),
        ("no steps", check_of(json!([])), "at least one step"),
        (
            "a step with no command",
            check_of(json!([{"args": {}, "expect": [exists()]}])),
            "command must be a string",
        ),
        (
            "a command that is a number",
            check_of(json!([{"command": 5, "expect": [exists()]}])),
            "command must be a string",
        ),
        (
            "args that is not an object",
            check_of(json!([{"command": "bundle.state", "args": 5, "expect": [exists()]}])),
            "args must be an object",
        ),
        (
            "no expect",
            check_of(json!([{"command": "bundle.state", "args": {}}])),
            "expect needs at least one row",
        ),
        (
            "an empty expect",
            check_of(json!([{"command": "bundle.state", "args": {}, "expect": []}])),
            "expect needs at least one row",
        ),
        (
            "a pointer with no slash",
            check_of(rows(json!({"path": "final_marking", "op": "exists"}))),
            "path must be a JSON Pointer",
        ),
        (
            "no pointer",
            check_of(rows(json!({"op": "exists"}))),
            "path must be a JSON Pointer",
        ),
        (
            "an op that does not exist",
            check_of(rows(json!({"path": "/ok", "op": "~=", "value": 1}))),
            "there are",
        ),
        (
            "no op",
            check_of(rows(json!({"path": "/ok", "value": 1}))),
            "there are",
        ),
        (
            "== with no value",
            check_of(rows(json!({"path": "/ok", "op": "=="}))),
            "needs a value",
        ),
        (
            "contains with no value",
            check_of(rows(json!({"path": "/ok", "op": "contains"}))),
            "needs a value",
        ),
    ];
    for (what, check, frag) in &cases {
        let r = report(&doc(vec![with(plain("a"), "check", check.clone())]), true);
        assert_only_error(&r, "CHECK_INVALID", Some("a"));
        assert!(
            r.errors[0].message.contains(frag),
            "{what}: {:?}",
            r.errors[0]
        );
    }

    // exists and absent need no value; every other op does.
    for op in ["exists", "absent"] {
        let c = check_of(rows(json!({"path": "/ok", "op": op})));
        assert!(
            report(&doc(vec![with(plain("a"), "check", c)]), true).valid(),
            "{op}"
        );
    }

    // At most 16 steps: 16 passes, 17 is the shape error (checks.rs
    // MAX_STEPS), not a CONTENT_LIMIT, and it is not run.
    let steps = |n: usize| json!((0..n).map(|_| step("bundle.state")).collect::<Vec<_>>());
    let sixteen = with(plain("a"), "check", check_of(steps(16)));
    assert!(report(&doc(vec![sixteen]), true).valid());
    let seventeen = with(plain("a"), "check", check_of(steps(17)));
    let r = report(&doc(vec![seventeen.clone()]), true);
    assert_only_error(&r, "CHECK_INVALID", Some("a"));
    assert!(
        r.errors[0].message.contains("17 steps"),
        "{:?}",
        r.errors[0]
    );
    let mc = ModelContent::parse(&doc(vec![seventeen])).unwrap();
    let res = checks::run_check(&Base::Composition(&default_comp()), &mc.entries[0]);
    assert_eq!(res["ok"], false);
    assert!(res["error"].as_str().unwrap().contains("17 steps"), "{res}");
    assert_eq!(res["steps"], json!([]), "no step of an invalid check runs");

    // And a refused check is refused whole at load.
    let mut s = orders();
    let bad = doc(vec![with(plain("a"), "check", check_of(json!([])))]);
    let e = refused(&mut s, "content.load", json!({"document": bad}));
    assert!(
        e.starts_with("[CONTENT_REFUSED]") && e.contains("[CHECK_INVALID]"),
        "{e}"
    );
}

#[test]
fn check_command_forbidden_refuses_everything_but_a_bundle_or_plan_query() {
    for cmd in [
        "file.save",
        "content.check", // recursion
        "content.load",
        "bundle.load",
        "remote.run",
        "bundle.add_link", // edits
        "bundle.remove_link",
        "bundle.step",
        "bundle.nonexistent", // a bundle.* name no command has
        "nonexistent",
        "plan.nonexistent",
    ] {
        let c = check_of(json!([step(cmd)]));
        let r = report(&doc(vec![with(plain("a"), "check", c.clone())]), true);
        assert_only_error(&r, "CHECK_COMMAND_FORBIDDEN", Some("a"));
        assert!(
            r.errors[0].message.contains(cmd),
            "{cmd}: {:?}",
            r.errors[0]
        );
        assert!(checks::command_allowed(cmd).is_err(), "{cmd}");

        // The same through the loader: refused whole, and the content that
        // was attached stays.
        let mut s = orders();
        let e = refused(
            &mut s,
            "content.load",
            json!({"document": doc(vec![with(plain("a"), "check", c)])}),
        );
        assert!(
            e.starts_with("[CONTENT_REFUSED]") && e.contains("[CHECK_COMMAND_FORBIDDEN]"),
            "{cmd}: {e}"
        );
        assert_eq!(
            run(&mut s, "bundle.state", json!({}))["content"]["source"],
            "library:orders"
        );
    }

    // The reads that are allowed, the ones the library's own checks use.
    for cmd in [
        "bundle.state",
        "bundle.flatten",
        "bundle.incidence",
        "bundle.validate",
        "bundle.lens",
        "bundle.run",
        "plan.state",
    ] {
        assert!(checks::command_allowed(cmd).is_ok(), "{cmd}");
        let c = check_of(json!([step(cmd)]));
        let r = report(&doc(vec![with(plain("a"), "check", c)]), true);
        assert!(r.valid(), "{cmd}: {:?}", r.errors);
    }

    // A step that is both forbidden and the wrong shape reports each, under
    // its own code.
    let c = check_of(json!([{"command": "bundle.add_link", "args": 5, "expect": []}]));
    let r = report(&doc(vec![with(plain("a"), "check", c)]), true);
    let mut got = codes(&r.errors);
    got.sort_unstable();
    assert_eq!(
        got,
        vec!["CHECK_COMMAND_FORBIDDEN", "CHECK_INVALID", "CHECK_INVALID"]
    );
}

/// The runner does not trust the validator: a forbidden check handed to it
/// directly (a document that was never loaded) runs nothing.
#[test]
fn the_runner_refuses_a_forbidden_check_without_running_a_step() {
    let c = default_comp();
    for cmd in [
        "bundle.add_link",
        "bundle.remove_link",
        "content.check",
        "file.save",
    ] {
        let chk = check_of(json!([
            step("bundle.state"),
            {"command": cmd, "args": {}, "expect": [{"path": "", "op": "exists"}]}
        ]));
        let mc = ModelContent::parse(&doc(vec![with(plain("a"), "check", chk)])).unwrap();
        let res = checks::run_check(&Base::Composition(&c), &mc.entries[0]);
        assert_eq!(res["ok"], false, "{cmd}");
        assert!(
            res["error"].as_str().unwrap().contains("not allowed"),
            "{cmd}: {res}"
        );
        assert_eq!(
            res["steps"],
            json!([]),
            "{cmd}: not even the first, allowed, step ran"
        );
    }
}

#[test]
fn composition_cid_stale_is_a_warning_when_the_content_names_another_composition() {
    let own = default_comp().document_cid().unwrap();
    let d = doc(vec![plain("a")]);
    // Absent, or the open composition's own CID: no warning.
    assert!(report(&d, true).warnings.is_empty());
    let same = edited(&d, |d| d["composition"] = json!(own));
    assert!(report(&same, true).warnings.is_empty());

    let stale = edited(&d, |d| d["composition"] = json!("zNotThisOne"));
    let r = report(&stale, true);
    assert!(r.valid(), "{:?}", r.errors);
    assert_eq!(codes(&r.warnings), vec!["COMPOSITION_CID_STALE"]);
    assert_eq!(r.warnings[0].entry, None);
    assert_eq!(r.warnings[0].reference.as_deref(), Some("zNotThisOne"));
    assert!(r.warnings[0].message.contains(&own));

    // A warning does not refuse a strict load; content.validate reports it.
    let mut s = orders();
    run(&mut s, "content.load", json!({"document": stale}));
    let v = run(&mut s, "content.validate", json!({}));
    assert_eq!(v["valid"], true);
    assert_eq!(v["warnings"][0]["code"], "COMPOSITION_CID_STALE");
    assert_eq!(v["unresolved"], json!([]));
}

#[test]
fn content_limit_bounds_the_entries_the_text_and_the_about_list() {
    let many = |n: usize| doc((0..n).map(|i| plain(&format!("e{i}"))).collect());
    assert!(report(&many(1000), true).valid());
    let r = report(&many(1001), true);
    assert_only_error(&r, "CONTENT_LIMIT", None);
    assert!(r.errors[0].message.contains("1001"), "{:?}", r.errors[0]);

    // 32 KiB of text and 512 element ids are the most an entry may carry.
    let text = |n: usize| with(plain("a"), "text", json!("x".repeat(n)));
    assert!(report(&doc(vec![text(32 * 1024)]), true).valid());
    let r = report(&doc(vec![text(32 * 1024 + 1)]), true);
    assert_only_error(&r, "CONTENT_LIMIT", Some("a"));
    let about = |n: usize| with(plain("a"), "about", json!(vec!["model:"; n]));
    assert!(report(&doc(vec![about(512)]), true).valid());
    let r = report(&doc(vec![about(513)]), true);
    assert_only_error(&r, "CONTENT_LIMIT", Some("a"));

    // The loader refuses it, whole.
    let mut s = orders();
    let e = refused(&mut s, "content.load", json!({"document": many(1001)}));
    assert!(
        e.starts_with("[CONTENT_REFUSED]") && e.contains("[CONTENT_LIMIT]"),
        "{e}"
    );
    assert_eq!(
        run(&mut s, "bundle.state", json!({}))["content"]["source"],
        "library:orders"
    );

    // And a document over 4 MiB is refused before it is read, as an object
    // and as text.
    let huge = doc(vec![text(5 * 1024 * 1024)]);
    let e = refused(&mut s, "content.load", json!({"document": huge}));
    assert!(e.starts_with("[DOCUMENT_TOO_LARGE]"), "{e}");
    let e = refused(
        &mut s,
        "content.load",
        json!({"document": "x".repeat(4 * 1024 * 1024 + 1)}),
    );
    assert!(e.starts_with("[DOCUMENT_TOO_LARGE]"), "{e}");
}

// ------------------------------------------------------------ 4. the content.* commands

/// `content.get`'s filters, each checked against the library document rather
/// than against a count.
#[test]
fn content_get_filters_by_kind_about_and_view() {
    let mut s = orders();
    let lib = &content::library_entry("orders").unwrap().content;
    let want = |keep: &dyn Fn(&Entry) -> bool| -> Vec<String> {
        lib.sorted()
            .into_iter()
            .filter(|e| keep(e))
            .map(|e| e.id.clone())
            .collect()
    };

    let all = run(&mut s, "content.get", json!({}));
    assert_eq!(all["source"], "library:orders");
    assert_eq!(all["subject"], "composition");
    assert_eq!(all["name"], "orders");
    assert_eq!(all["total"].as_u64().unwrap() as usize, lib.entries.len());
    assert_eq!(all["dormant"], 0);
    assert_eq!(
        ids_of(&all),
        want(&|_| true),
        "display order, `order` first"
    );

    // kind: each of the six, and a kind nobody has matches nothing.
    for kind in content::KINDS {
        let got = ids_of(&run(&mut s, "content.get", json!({"kind": kind})));
        let exp = want(&|e| e.kind == kind);
        assert!(!exp.is_empty(), "the orders library has {kind} entries");
        assert_eq!(got, exp, "{kind}");
    }
    let none = run(&mut s, "content.get", json!({"kind": "opinion"}));
    assert_eq!(ids_of(&none), Vec::<String>::new());

    // about: an element id an entry names.
    for target in ["place:workers/in_use", "scenario:refund", "model:"] {
        let got = ids_of(&run(&mut s, "content.get", json!({"about": target})));
        let exp = want(&|e| e.about.iter().any(|r| r == target));
        assert!(!exp.is_empty(), "{target}");
        assert_eq!(got, exp, "{target}");
    }
    let nobody = run(
        &mut s,
        "content.get",
        json!({"about": "place:workers/nope"}),
    );
    assert_eq!(ids_of(&nobody), Vec::<String>::new());

    // view: the entries that name the view, and those that name no view,
    // which any view shows.
    assert!(lib.entries.iter().any(|e| e.views.is_empty()));
    for view in content::VIEWS {
        let got = ids_of(&run(&mut s, "content.get", json!({"view": view})));
        let exp = want(&|e| e.views.is_empty() || e.views.iter().any(|v| v == view));
        assert!(!exp.is_empty(), "{view}");
        assert_eq!(got, exp, "{view}");
    }
    let flat = ids_of(&run(&mut s, "content.get", json!({"view": "flat"})));
    assert!(
        flat.len() < lib.entries.len(),
        "an entry for another view is left out"
    );
    assert!(
        lib.entries
            .iter()
            .any(|e| !e.views.is_empty() && !e.views.iter().any(|v| v == "flat")),
        "the library has an entry that names only other views"
    );

    // The filters intersect: the one convention example of the flat view.
    let ce = run(
        &mut s,
        "content.get",
        json!({"kind": "convention-example", "view": "flat"}),
    );
    assert_eq!(ce["entries"].as_array().unwrap().len(), 1, "{ce}");
    assert_eq!(ce["entries"][0]["views"], json!(["flat"]));
    let both = run(
        &mut s,
        "content.get",
        json!({"kind": "finding", "about": "place:workers/in_use", "view": "flat"}),
    );
    assert_eq!(
        ids_of(&both),
        want(&|e| e.kind == "finding"
            && e.about.iter().any(|r| r == "place:workers/in_use")
            && (e.views.is_empty() || e.views.iter().any(|v| v == "flat")))
    );

    // A view reads check and scenario as objects, not as the exported text.
    let findings = run(&mut s, "content.get", json!({"kind": "finding"}));
    assert!(findings["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["check"].is_object()));
    let scenarios = run(&mut s, "content.get", json!({"kind": "scenario"}));
    assert!(scenarios["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["scenario"]["script"].is_array()));
}

/// An entry whose element ids stop resolving goes dormant, and wakes up
/// again with the model.
#[test]
fn dormant_entries_follow_the_model_through_an_edit_and_its_undo() {
    let mut s = orders();
    let lib = &content::library_entry("orders").unwrap().content;
    let link = "link:event:workers:release->payment:commit";
    let naming: Vec<String> = lib
        .entries
        .iter()
        .filter(|e| e.about.iter().any(|r| r == link))
        .map(|e| e.id.clone())
        .collect();
    assert!(
        naming.len() >= 2,
        "the library has entries about the release link"
    );

    run(
        &mut s,
        "bundle.remove_link",
        json!({"kind": "event", "from": "workers:release", "to": "payment:commit"}),
    );
    let shown = run(&mut s, "content.get", json!({}));
    let shown_ids = ids_of(&shown);
    let dormant = shown["dormant"].as_u64().unwrap() as usize;
    assert!(dormant >= naming.len());
    assert_eq!(shown_ids.len() + dormant, lib.entries.len());
    for id in &naming {
        assert!(!shown_ids.contains(id), "{id} names a link that is gone");
    }
    // An entry that names only elements still there is still shown.
    let untouched = lib
        .entries
        .iter()
        .find(|e| e.about == ["place:orders/items"])
        .expect("an entry about only orders/items");
    assert!(shown_ids.contains(&untouched.id));

    // `all` brings them back, flagged; the flags are exactly the hidden ones.
    let all = run(&mut s, "content.get", json!({"all": true}));
    assert_eq!(all["entries"].as_array().unwrap().len(), lib.entries.len());
    let flagged: Vec<String> = all["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["dormant"] == true)
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(flagged.len(), dormant);
    assert!(flagged.iter().all(|id| !shown_ids.contains(id)));
    // `kind` filters what is left; `dormant` still counts what was hidden.
    let caveats = run(&mut s, "content.get", json!({"kind": "caveat"}));
    assert_eq!(caveats["dormant"], shown["dormant"]);

    // Validation agrees: strict it is invalid because of those ids, and not
    // strict it is valid with a warning per dormant reference (plus the stale
    // composition CID, since the library content was written for the
    // unedited one).
    let strict = run(&mut s, "content.validate", json!({}));
    assert_eq!(strict["valid"], false);
    assert!(strict["errors"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["code"] == "ID_UNRESOLVED"));
    let lax = run(&mut s, "content.validate", json!({"strict": false}));
    assert_eq!(lax["valid"], true);
    let warned: Vec<&str> = lax["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap())
        .collect();
    assert!(warned.contains(&"COMPOSITION_CID_STALE"), "{warned:?}");
    assert!(warned.contains(&"ID_UNRESOLVED"));
    assert_eq!(
        run(&mut s, "bundle.state", json!({}))["content"]["unresolved"],
        lax["unresolved"]
    );

    // Undoing the edit wakes them all.
    run(&mut s, "edit.undo", Value::Null);
    let back = run(&mut s, "content.get", json!({}));
    assert_eq!(back["dormant"], 0);
    assert_eq!(ids_of(&back).len(), lib.entries.len());
}

/// `subject: "plan"` reads the library's Connect-4 content, with or without
/// a composition open.
#[test]
fn content_get_subject_plan_returns_the_library_plan_content() {
    let lib = content::library_entry("connect4").unwrap();
    let mut s = session();
    let g = run(&mut s, "content.get", json!({"subject": "plan"}));
    assert_eq!(g["source"], "library:connect4");
    assert_eq!(g["subject"], "plan");
    assert_eq!(g["name"], lib.content.name.as_str());
    assert_eq!(
        g["total"].as_u64().unwrap() as usize,
        lib.content.entries.len()
    );
    assert_eq!(g["dormant"], 0);
    assert_eq!(ids_of(&g).len(), lib.content.entries.len());

    // Its filters are the same filters.
    let findings = run(
        &mut s,
        "content.get",
        json!({"subject": "plan", "kind": "finding"}),
    );
    let exp: Vec<String> = lib
        .content
        .sorted()
        .into_iter()
        .filter(|e| e.kind == "finding")
        .map(|e| e.id.clone())
        .collect();
    assert!(!exp.is_empty());
    assert_eq!(ids_of(&findings), exp);

    // With the orders composition open, the default subject is still the
    // composition's and `plan` is still the plan's.
    run(&mut s, "bundle.open", json!({}));
    let c = run(&mut s, "content.get", json!({}));
    assert_eq!(c["source"], "library:orders");
    assert_ne!(c["total"], g["total"]);
    assert_eq!(run(&mut s, "content.get", json!({"subject": "plan"})), g);
    let e = refused(&mut s, "content.get", json!({"subject": "net"}));
    assert!(e.contains("composition and plan"), "{e}");

    // The plan's content validates against the plan, and the plan subject
    // needs no composition.
    let mut t = session();
    let v = run(&mut t, "content.validate", json!({"subject": "plan"}));
    assert_eq!(v["valid"], true, "{v}");
    assert_eq!(v["source"], "library:connect4");
}

#[test]
fn content_load_and_content_clear_are_one_undo_step_each() {
    let mut s = orders();
    let cid0 = run(&mut s, "content.export", json!({}))["cid"].clone();
    let small = doc(vec![plain("only")]);

    let r = run(&mut s, "content.load", json!({"document": small}));
    assert_eq!(r["source"], "loaded");
    assert_eq!(r["entries"], 1);
    assert_eq!(
        r["cid"],
        ModelContent::parse(&small).unwrap().cid().unwrap().as_str()
    );
    assert_eq!(ids_of(&run(&mut s, "content.get", json!({}))), ["only"]);

    // One undo puts the library content back and leaves the composition (so
    // the load was one step, not two, and not zero).
    run(&mut s, "edit.undo", Value::Null);
    assert!(s.composition.is_some());
    assert_eq!(run(&mut s, "content.export", json!({}))["cid"], cid0);
    assert_eq!(
        run(&mut s, "bundle.state", json!({}))["content"]["source"],
        "library:orders"
    );
    run(&mut s, "edit.redo", Value::Null);
    assert_eq!(ids_of(&run(&mut s, "content.get", json!({}))), ["only"]);
    run(&mut s, "edit.undo", Value::Null);

    // content.clear leaves it bare, and says so.
    let cleared = run(&mut s, "content.clear", json!({}));
    assert_eq!(cleared, json!({"cleared": 121}));
    let st = run(&mut s, "bundle.state", json!({}));
    assert_eq!(st["content"], json!({"source": "none", "entries": 0}));
    let codes = missing_codes(&mut s);
    assert!(codes.contains(&"NO_CONTENT".to_string()), "{codes:?}");
    assert!(codes.contains(&"NO_SCENARIOS".to_string()), "{codes:?}");
    let g = run(&mut s, "content.get", json!({}));
    assert_eq!(
        (g["source"].clone(), g["total"].clone()),
        (json!("none"), json!(0))
    );
    assert!(g["note"].as_str().unwrap().contains("NO_CONTENT"));
    for cmd in ["content.export", "content.validate", "content.check"] {
        let e = refused(&mut s, cmd, json!({}));
        assert!(e.starts_with("[NO_CONTENT]"), "{cmd}: {e}");
    }
    run(&mut s, "edit.undo", Value::Null);
    assert!(s.composition.is_some());
    assert_eq!(run(&mut s, "content.export", json!({}))["cid"], cid0);

    // Clearing what is already bare is a no-op that says 0.
    run(&mut s, "content.clear", json!({}));
    assert_eq!(
        run(&mut s, "content.clear", json!({})),
        json!({"cleared": 0})
    );
}

/// A content document about a plan is not attachable to a composition, by
/// any of the three ways in.
#[test]
fn plan_content_cannot_be_attached_to_a_composition() {
    let mut s = orders();
    let cid0 = run(&mut s, "content.export", json!({}))["cid"].clone();
    let plan_doc =
        run(&mut s, "content.export", json!({"library": "connect4"}))["document"].clone();
    assert_eq!(plan_doc["subject"]["kind"], "plan");
    let composition = document_of(&mut s);

    let wants = |e: &str| e.starts_with("[CONTENT_SUBJECT_UNSUPPORTED]");
    let e = refused(&mut s, "content.load", json!({"document": plan_doc}));
    assert!(wants(&e), "{e}");
    let e = refused(&mut s, "content.load", json!({"library": "connect4"}));
    assert!(wants(&e), "{e}");
    let e = refused(
        &mut s,
        "bundle.load",
        json!({"document": composition, "content": plan_doc}),
    );
    assert!(wants(&e), "{e}");
    let e = refused(
        &mut s,
        "bundle.load",
        json!({"document": composition, "library": "connect4"}),
    );
    assert!(wants(&e), "{e}");
    assert_eq!(run(&mut s, "content.export", json!({}))["cid"], cid0);
    let e = refused(&mut s, "content.load", json!({"library": "no-such"}));
    assert!(e.contains("no library content"), "{e}");
    let e = refused(&mut s, "content.load", json!({}));
    assert!(e.contains("pass `document` or `library`"), "{e}");
}

/// `content.check` reads the model and stores its own report; it touches
/// nothing else of the session.
#[test]
fn content_check_leaves_the_session_alone() {
    let mut s = orders();
    run(&mut s, "bundle.flatten", json!({}));
    run(&mut s, "bundle.scenario", json!({"id": "refund"}));
    let selected = run(&mut s, "bundle.game", json!({}));
    run(&mut s, "bundle.step", json!({}));
    let stepped = run(&mut s, "bundle.game", json!({}));
    assert_ne!(selected, stepped, "the step moved the token game");

    let state0 = run(&mut s, "bundle.state", json!({}));
    let titles =
        |s: &Session| -> Vec<String> { s.artifacts.iter().map(|a| a.title.clone()).collect() };
    let (rev0, journal0, arts0) = (s.revision, s.journal.len(), titles(&s));

    let all = run(&mut s, "content.check", json!({}));
    assert_eq!(all["failed"], 0);
    assert!(all["total"].as_u64().unwrap() > 50);
    let one = run(
        &mut s,
        "content.check",
        json!({"entry": "find-pool-leaks-on-rollback"}),
    );
    assert_eq!(one["total"], 1);

    let arts1 = titles(&s);
    assert_eq!(s.revision, rev0, "no edit was made");
    assert_eq!(s.journal.len(), journal0, "a query is not journaled");
    assert_eq!(run(&mut s, "bundle.state", json!({})), state0);
    assert_eq!(run(&mut s, "bundle.game", json!({})), stepped);
    // The one new artifact is the report; the scratch session's own
    // bundle.run and bundle.flatten artifacts did not come with it.
    assert_eq!(arts1.len(), arts0.len() + 1, "{arts0:?} -> {arts1:?}");
    assert_eq!(arts1.iter().filter(|t| *t == "Content checks").count(), 1);
    assert!(arts1.iter().all(|t| !t.starts_with("Run:")), "{arts1:?}");
    for a in &arts0 {
        assert!(arts1.contains(a), "{a} was lost");
    }
    assert!(s
        .artifacts
        .iter()
        .any(|a| a.title == "Content checks" && a.source.command == "content.check"));

    // And the undo history is the one it was: the next undo undoes the step
    // the check ran after, not the check.
    run(&mut s, "edit.undo", Value::Null);
    assert_eq!(run(&mut s, "bundle.game", json!({})), selected);

    // The result is the same on a rerun.
    let again = run(&mut s, "content.check", json!({}));
    assert_eq!(all["checks"], again["checks"]);
}

#[test]
fn a_check_runs_on_a_copy_of_the_session_that_asked() {
    let id = "find-pool-leaks-on-rollback";
    let (mut a, mut b) = (orders(), orders());
    run(
        &mut a,
        "bundle.remove_link",
        json!({"kind": "event", "from": "workers:release", "to": "payment:commit"}),
    );
    // A sees its own edited composition; B's check is the unedited one's.
    assert_red_at(&check_one(&mut a, id), "/transition_map/workers/release");
    assert_green(&check_one(&mut b, id));
    assert_red_at(&check_one(&mut a, id), "/transition_map/workers/release");
    // A's check ran steps (bundle.run among them) and left both sessions'
    // compositions with the links they had.
    let links = |s: &mut Session| {
        run(s, "bundle.state", json!({}))["links"]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!((links(&mut a), links(&mut b)), (2, 3));

    // The plan checks run on a scratch plan when none is open, and leave the
    // session without one.
    let mut s = session();
    assert!(s.search_plan.is_none());
    let lib = content::library_entry("connect4").unwrap();
    let out = run(&mut s, "content.check", json!({"subject": "plan"}));
    assert_eq!(
        out["total"].as_u64().unwrap() as usize,
        lib.content.checks().len()
    );
    assert_eq!(out["failed"], 0, "{out}");
    assert!(s.search_plan.is_none());
}

// ------------------------------------------------------------ 5. element ids (§3.2)

fn resolves(ids: &Ids, reference: &str) -> bool {
    ids.resolves(reference)
        .unwrap_or_else(|e| panic!("{reference}: {e}"))
}

/// Each reference in `yes` resolves and each in `no` does not, and between
/// them every type in `types` is asked for both a thing it names and a thing
/// it does not.
fn assert_resolution(ids: &Ids, yes: &[&str], no: &[&str], types: &[&str]) {
    for r in yes {
        assert!(resolves(ids, r), "{r} should resolve");
    }
    for r in no {
        assert!(!resolves(ids, r), "{r} should not resolve");
    }
    let kinds = |refs: &[&str]| -> Vec<String> {
        refs.iter()
            .map(|r| r.split_once(':').unwrap().0.to_string())
            .collect()
    };
    let (ky, kn) = (kinds(yes), kinds(no));
    for t in types {
        assert!(
            ky.iter().any(|k| k == t),
            "no reference of type {t} resolves above"
        );
        assert!(
            kn.iter().any(|k| k == t),
            "no reference of type {t} fails to resolve above"
        );
    }
}

#[test]
fn orders_element_ids_resolve_for_the_right_thing_and_not_the_wrong_one() {
    let c = default_comp();
    // `refund` is declared by the document under test; `commit` is not.
    let ids = Ids::of(&c, &["refund".to_string()]);
    let yes = [
        "model:",
        "instance:orders",
        "instance:workers",
        "instance:payment",
        "place:workers/in_use",
        "place:orders/items",
        "place:payment/completed",
        "transition:orders/dequeue",
        "transition:payment/do_reserve",
        "transition:workers/release",
        "port:orders:dequeue",
        "port:workers:release",
        // a port id with colons in it
        "port:payment:do:reserve",
        "link:event:orders:dequeue->workers:acquire",
        "link:event:orders:dequeue->payment:do:reserve",
        "link:event:workers:release->payment:commit",
        // an event link reads either way round
        "link:event:workers:acquire->orders:dequeue",
        "link:event:payment:commit->workers:release",
        "constraint:workers/pool_conservation",
        "constraint:orders/queue_capacity",
        "constraint:payment/saga_conservation",
        "scenario:refund",
        "flat-place:orders/items",
        "flat-place:workers/in_use",
        "flat-transition:orders/enqueue",
        "flat-transition:fused:orders/dequeue+payment/do_reserve+workers/acquire",
        "flat-transition:fused:payment/commit+workers/release",
        "idiom:idiom-pool-release-saga-commit",
        "idiom:idiom-queue-dequeue-pool-acquire",
        "idiom:idiom-queue-dequeue-saga-do",
        "view:cospan",
        "view:spiders",
        "view:operad",
        "view:string",
        "view:flat",
        "view:boundaries",
        "view:lens",
        "view:incidence",
        "view:process",
        "view:laws",
    ];
    let no = [
        "instance:nope",
        "instance:Orders",
        // a transition, and a port, spelled as an instance
        "instance:orders/dequeue",
        "instance:orders:dequeue",
        "place:workers/nope",
        "place:workers",
        "place:payment2/completed",
        // a transition, and a port spelling, as a place
        "place:orders/dequeue",
        "place:orders:items",
        "transition:orders/nope",
        // a place as a transition; the port spelling as a transition
        "transition:orders/items",
        "transition:orders:dequeue",
        "port:nope:dequeue",
        "port:payment:do",
        // the transition spelling as a port
        "port:orders/dequeue",
        // the wrong kind of link, a pair nobody linked, no arrow at all
        "link:token:orders:dequeue->workers:acquire",
        "link:event:orders:dequeue->workers:release",
        "link:event:orders:dequeue->payment:commit",
        "link:event:workers:release",
        // constraints need their subnet, and the right one
        "constraint:pool_conservation",
        "constraint:workers/nope",
        "constraint:orders/pool_conservation",
        // declared by the document: refund is, commit is not
        "scenario:commit",
        "scenario:nope",
        // a transition as a flat place; places that do not exist
        "flat-place:orders/dequeue",
        "flat-place:payment/nope",
        "flat-place:wire:payment/customer",
        // a member of a fused class is not itself a flat transition
        "flat-transition:orders/dequeue",
        "flat-transition:workers/release",
        "flat-transition:orders/items",
        "idiom:nope",
        // the key, not the part after the prefix
        "idiom:pool-release-saga-commit",
        "view:Flat",
        "view:venn",
        "view:",
        // the plan's types are not a composition's
        "piece:x",
        "slot:x",
        "net-place:x",
        "net-transition:x",
    ];
    assert_resolution(
        &ids,
        &yes,
        &no,
        // (`model:` names the whole model, so it has no id it fails to resolve.)
        &[
            "instance",
            "place",
            "transition",
            "port",
            "link",
            "constraint",
            "scenario",
            "flat-place",
            "flat-transition",
            "idiom",
            "view",
        ],
    );

    // Not `<type>:<ref>` of a known type: an error from the resolver, which
    // the validator reports as ID_SYNTAX.
    for bad in [
        "workers/in_use",
        "model",
        "gadget:x",
        "Place:workers/in_use",
        "",
        ":x",
    ] {
        assert!(ids.resolves(bad).is_err(), "{bad:?}");
    }
}

/// A token link fuses two places into a wire place, and is not symmetric.
#[test]
fn a_token_link_adds_its_wire_place_and_does_not_read_both_ways() {
    let mut s = orders();
    run(&mut s, "bundle.add_instance", json!({"id": "payment2"}));
    let ids_now = |s: &Session| Ids::of(s.composition.as_ref().unwrap(), &[]);
    let before = ids_now(&s);
    assert!(resolves(&before, "instance:payment2"));
    assert!(resolves(&before, "flat-place:payment2/customer"));
    assert!(!resolves(&before, "flat-place:wire:payment/customer"));
    assert!(!resolves(
        &before,
        "link:token:payment:account:customer->payment2:account:customer"
    ));

    run(
        &mut s,
        "bundle.add_link",
        json!({"kind": "token", "from": "payment:account:customer", "to": "payment2:account:customer"}),
    );
    let after = ids_now(&s);
    assert_resolution(
        &after,
        &[
            "flat-place:wire:payment/customer",
            "link:token:payment:account:customer->payment2:account:customer",
            // still places of their own subnets
            "place:payment/customer",
            "place:payment2/customer",
        ],
        &[
            "link:token:payment2:account:customer->payment:account:customer",
            // fused into the wire, so no longer flat places
            "flat-place:payment/customer",
            "flat-place:payment2/customer",
            "place:payment2/nope",
        ],
        &["flat-place", "link", "place"],
    );
}

/// The flat ids need a composition that flattens; the others do not.
#[test]
fn flat_ids_need_a_composition_that_flattens() {
    let mut broken = cafe_bundle();
    broken["links"].as_array_mut().unwrap().push(json!({
        "kind": "event",
        "from": {"subnet": "counter", "transition": "nope"},
        "to": {"subnet": "staff", "transition": "acquire_latte"}
    }));
    let mut s = session();
    let r = run(&mut s, "bundle.load", json!({"document": broken.clone()}));
    assert_eq!(r["flatten"]["ok"], false);
    assert!(r["flatten"]["error"]
        .as_str()
        .unwrap()
        .contains("E_BAD_ENDPOINT"));
    let codes: Vec<&str> = r["missing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"FLATTEN_FAILED"), "{codes:?}");

    let ids = Ids::of(s.composition.as_ref().unwrap(), &[]);
    assert_resolution(
        &ids,
        &[
            "instance:counter",
            "place:counter/done",
            "transition:counter/order",
        ],
        &[
            "flat-place:counter/done",
            "flat-transition:counter/order",
            "instance:nope",
            "place:counter/nope",
            "transition:counter/nope",
        ],
        &["instance", "place", "transition"],
    );

    // Content that names a flat id is unresolved there, and so refused when
    // strict and dormant otherwise.
    let about_flat = doc(vec![entry("f", "finding", &["flat-place:counter/done"])]);
    let mut t = session();
    let e = refused(
        &mut t,
        "bundle.load",
        json!({"document": broken, "content": about_flat}),
    );
    assert!(
        e.starts_with("[CONTENT_REFUSED]") && e.contains("[ID_UNRESOLVED]"),
        "{e}"
    );
}

/// Constraints at both levels, on a raw Bundle that has them (cafe has none,
/// so the document under test adds one of each).
#[test]
fn constraints_resolve_at_the_subnet_and_at_the_bundle_level() {
    let mut d = cafe_bundle();
    d["subnets"][2]["model"]["constraints"] = json!([{"id": "staff_conservation", "expr": "tokens(\"available\") + tokens(\"busy\") == 2"}]);
    d["constraints"] = json!([{"id": "total_staff", "expr": "tokens(\"staff/available\") + tokens(\"staff/busy\") == 2"}]);
    let c = Composition::from_bundle_document(&d, &|_: &str| None).unwrap();
    let ids = Ids::of(&c, &[]);
    assert_resolution(
        &ids,
        &[
            "constraint:total_staff",
            "constraint:staff/staff_conservation",
        ],
        &[
            // the subnet's, without its subnet, and the bundle's, with one
            "constraint:staff_conservation",
            "constraint:staff/total_staff",
            "constraint:counter/staff_conservation",
            "constraint:nope",
        ],
        &["constraint"],
    );
    // The bare cafe has neither.
    let bare = Ids::of(&cafe_comp(), &[]);
    assert!(!resolves(&bare, "constraint:total_staff"));
    assert!(!resolves(&bare, "constraint:staff/staff_conservation"));
}

/// The ids of a raw Bundle whose link ends are transitions, not ports.
#[test]
fn cafe_element_ids_include_transition_ended_links() {
    let ids = Ids::of(
        &cafe_comp(),
        &["espresso-order".to_string(), "latte-order".to_string()],
    );
    assert_resolution(
        &ids,
        &[
            "instance:counter",
            "place:pantry/beans",
            "transition:counter/start_espresso",
            "port:pantry:beans",
            // transition ends are written subnet:@transition/<id>
            "link:event:counter:@transition/start_espresso->pantry:@transition/brew_espresso",
            "link:event:pantry:@transition/brew_espresso->counter:@transition/start_espresso",
            "link:guard:counter:@transition/start_espresso->pantry:beans",
            "scenario:espresso-order",
            "flat-place:staff/available",
            "flat-transition:espresso_started",
            "flat-transition:espresso_finished",
            "flat-transition:counter/order",
            "flat-transition:pantry/restock",
            "view:flat",
        ],
        &[
            // a guard link does not read both ways
            "link:guard:pantry:beans->counter:@transition/start_espresso",
            "link:event:counter:@transition/start_espresso->pantry:@transition/brew_latte",
            "link:event:counter:start_espresso->pantry:brew_espresso",
            "port:counter:start_espresso",
            "port:counter:@transition/start_espresso",
            // staff declares no ports
            "port:staff:available",
            "scenario:commit",
            // the starts and brews are fused away into espresso_started
            "flat-transition:counter/start_espresso",
            "flat-transition:pantry/brew_espresso",
            "instance:nope",
            "place:pantry/nope",
            // a place as a transition
            "transition:pantry/beans",
            // a place of the cafe is a flat place; a transition is not
            "flat-place:counter/order",
        ],
        &[
            "instance",
            "place",
            "transition",
            "port",
            "link",
            "scenario",
            "flat-place",
            "flat-transition",
        ],
    );
}

#[test]
fn plan_element_ids_are_the_pieces_slots_and_net_of_the_plan() {
    let ids = Ids::of_plan();
    // One of each type, taken from what the library's plan content names.
    let lib = content::library_entry("connect4").unwrap();
    let named = |t: &str| -> String {
        lib.content
            .entries
            .iter()
            .flat_map(|e| e.about.iter())
            .find(|r| r.split_once(':').is_some_and(|(k, _)| k == t))
            .unwrap_or_else(|| panic!("the plan content names no {t}"))
            .clone()
    };
    let (piece, slot) = (named("piece"), named("slot"));
    let (place, transition) = (named("net-place"), named("net-transition"));
    let (_, place_ref) = place.split_once(':').unwrap();
    let (_, transition_ref) = transition.split_once(':').unwrap();
    assert_resolution(
        &ids,
        &[&piece, &slot, &place, &transition, "view:flat", "model:"],
        &[
            "piece:nope",
            "slot:nope",
            "net-place:nope",
            "net-transition:nope",
            // a place of the net is not one of its transitions, nor the reverse
            &format!("net-transition:{place_ref}"),
            &format!("net-place:{transition_ref}"),
            // the composition's types name nothing in a plan
            "instance:orders",
            "place:workers/in_use",
            "transition:orders/dequeue",
            "port:orders:dequeue",
            "link:event:orders:dequeue->workers:acquire",
            "constraint:workers/pool_conservation",
            "scenario:refund",
            "flat-place:orders/items",
            "idiom:idiom-pool-release-saga-commit",
            "view:venn",
        ],
        &["piece", "slot", "net-place", "net-transition", "view"],
    );
}

// ------------------------------------------------------------ the cafe raw Bundle (§1.2, §2)
//
// cafe.bundle.json has links whose ends are transitions, subnets with no
// ports and no patterns, and no content. tests/fixtures/content/cafe.content.json
// is the content written for it: two declared scenarios, a finding about the
// guard link (with the check that backs it), a judgment, a caveat (with a
// check), a note and the flat view's convention example.

#[test]
fn the_cafe_content_validates_against_the_bundle_and_its_checks_pass() {
    let mut s = cafe();
    assert_eq!(
        cafe_content()["composition"],
        CAFE_CID,
        "written against the bundle as received"
    );
    let v = run(&mut s, "content.validate", json!({}));
    assert_eq!(v["valid"], true, "{v}");
    assert_eq!(v["errors"], json!([]));
    assert_eq!(v["warnings"], json!([]), "no stale CID, nothing dormant");
    assert_eq!(v["unresolved"], json!([]));
    assert_eq!(v["entries"], 7);
    assert_eq!(v["source"], "loaded");

    let ck = run(&mut s, "content.check", json!({}));
    let results = ck["checks"].as_array().unwrap();
    let failed: Vec<String> = results
        .iter()
        .filter(|r| r["ok"] != true)
        .map(|r| format!("{}: {:?}", r["id"], reds(r)))
        .collect();
    assert!(failed.is_empty(), "{failed:#?}");
    let ran: Vec<&str> = results.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(ran, ["espresso-guard", "counter-starts-empty"]);

    // The core API says the same, and the content reads back as written.
    let mc = ModelContent::parse(&cafe_content()).unwrap();
    let r = content::validate(&mc, &cafe_comp(), true);
    assert!(
        r.valid() && r.warnings.is_empty(),
        "{:?} {:?}",
        r.errors,
        r.warnings
    );
    assert_eq!(mc.checks().len(), 2);
    assert_eq!(mc.scenarios().len(), 2);
}

/// What a raw Bundle lacks is named, and loading its content removes exactly
/// the two codes that content answers (NO_SCENARIOS, NO_CONTENT).
#[test]
fn loading_the_cafe_content_removes_the_content_codes_and_no_others() {
    let mut bare = session();
    let r = run(&mut bare, "bundle.load", json!({"document": cafe_bundle()}));
    let codes = |v: &Value| -> Vec<String> {
        v["missing"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["code"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        codes(&r),
        [
            "NO_PATTERN",
            "NO_PORTS",
            "NO_SCENARIOS",
            "NO_CONTENT",
            "NO_LAYOUT"
        ]
    );
    assert_eq!(missing_codes(&mut bare), codes(&r));
    for m in r["missing"].as_array().unwrap() {
        assert!(
            !m["text"].as_str().unwrap().is_empty(),
            "each code says it in words: {m}"
        );
    }

    let mut s = session();
    let r = run(
        &mut s,
        "bundle.load",
        json!({"document": cafe_bundle(), "content": cafe_content()}),
    );
    assert_eq!(codes(&r), ["NO_PATTERN", "NO_PORTS", "NO_LAYOUT"]);
    assert_eq!(
        missing_codes(&mut s),
        ["NO_PATTERN", "NO_PORTS", "NO_LAYOUT"]
    );
    assert_eq!(r["content"]["entries"], 7);
    assert_eq!(
        r["content"]["scenarios"],
        json!(["espresso-order", "latte-order"])
    );
    let caps = run(&mut s, "bundle.state", json!({}))["capabilities"].clone();
    assert_eq!(caps["scenarios"], 2);
    assert_eq!(caps["content_entries"], 7);
    // What the content cannot supply stays false: the cafe has no patterns.
    assert_eq!(caps["patterns"], false);
    assert_eq!(caps["idioms"], false);

    // content.load onto the bare composition does the same, in one undo step.
    run(
        &mut bare,
        "content.load",
        json!({"document": cafe_content()}),
    );
    assert_eq!(
        missing_codes(&mut bare),
        ["NO_PATTERN", "NO_PORTS", "NO_LAYOUT"]
    );
    run(&mut bare, "edit.undo", Value::Null);
    assert_eq!(
        missing_codes(&mut bare),
        [
            "NO_PATTERN",
            "NO_PORTS",
            "NO_SCENARIOS",
            "NO_CONTENT",
            "NO_LAYOUT"
        ]
    );
}

#[test]
fn a_bare_raw_bundle_says_it_is_bare_and_runs_scripts_but_no_scenario() {
    let mut s = session();
    run(&mut s, "bundle.load", json!({"document": cafe_bundle()}));
    let sc = run(&mut s, "bundle.scenarios", json!({}));
    assert_eq!(sc["scenarios"], json!([]));
    assert_eq!(sc["source"], "none");
    assert_eq!(sc["note"], "no scenario is declared for this model");

    let e = refused(&mut s, "bundle.run", json!({"scenario": "espresso-order"}));
    assert!(
        e.contains("no scenario \"espresso-order\" is declared"),
        "{e}"
    );
    assert!(e.contains("there are []"), "{e}");
    // A script of member transitions needs no declaration.
    let r = run(
        &mut s,
        "bundle.run",
        json!({"script": ["counter/order", "counter/start_espresso"]}),
    );
    assert_eq!(r["completed"], true, "{r}");

    let g = run(&mut s, "content.get", json!({}));
    assert_eq!(g["source"], "none");
    assert_eq!(g["entries"], json!([]));
    assert!(g["note"].as_str().unwrap().contains("NO_CONTENT"));
    for cmd in ["content.export", "content.validate", "content.check"] {
        let e = refused(&mut s, cmd, json!({}));
        assert!(e.starts_with("[NO_CONTENT]"), "{cmd}: {e}");
    }
    // The lens test cannot read a commit scenario that is not there.
    let lens = run(&mut s, "bundle.lens", json!({"fixes": false}));
    assert_eq!(lens["analysis"]["ok"], false);
    assert!(lens["analysis"]["errors"][0]
        .as_str()
        .unwrap()
        .contains("no scenario \"commit\" is declared"));
}

#[test]
fn bundle_run_runs_the_scenarios_the_cafe_content_declares() {
    let mut s = cafe();
    let sc = run(&mut s, "bundle.scenarios", json!({}));
    assert_eq!(sc["source"], "loaded");
    let ids: Vec<&str> = sc["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["espresso-order", "latte-order"]);
    assert!(sc.get("note").is_none());
    // The declared form is returned whole, with the note it carries.
    assert_eq!(
        sc["scenarios"][0]["script"],
        json!([
            "counter/order",
            "counter/start_espresso",
            "counter/finish_espresso"
        ])
    );
    assert_eq!(
        sc["scenarios"][0]["expect"][1],
        json!({"place": "pantry/beans", "op": "==", "n": 58, "says": "the brew draws two beans"})
    );
    assert!(sc["scenarios"][1]["note"]
        .as_str()
        .unwrap()
        .contains("non-equality"));

    let r = run(&mut s, "bundle.run", json!({"scenario": "espresso-order"}));
    assert_eq!(r["completed"], true, "{r}");
    assert_eq!(r["final_marking"]["pantry/beans"], 58);
    assert_eq!(r["final_marking"]["counter/done"], 1);
    let rows = r["expect"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|x| x["pass"] == true), "{rows:?}");
    // The script's members are resolved to the cafe's fused transitions.
    assert_eq!(r["steps"][1]["flat"], "espresso_started");
    assert_eq!(r["steps"][2]["flat"], "espresso_finished");

    // The latte run uses the >= operator the declared form allows.
    let r = run(&mut s, "bundle.run", json!({"scenario": "latte-order"}));
    assert_eq!(r["completed"], true, "{r}");
    assert_eq!(r["expect"][1]["op"], ">=");
    assert!(
        r["expect"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["pass"] == true),
        "{r}"
    );

    // Another id lists the declared ones.
    let e = refused(&mut s, "bundle.run", json!({"scenario": "commit"}));
    assert!(e.contains("no scenario \"commit\" is declared"), "{e}");
    assert!(e.contains(r#"["espresso-order", "latte-order"]"#), "{e}");
}

#[test]
fn the_lens_test_on_the_cafe_says_the_commit_scenario_is_not_declared() {
    let mut s = cafe();
    // The cafe declares scenarios, but not the one the lens test reads.
    let lens = run(&mut s, "bundle.lens", json!({"fixes": false}));
    assert_eq!(lens["analysis"]["ok"], false, "{lens}");
    assert_eq!(lens["analysis"]["holds"], false);
    let errors = lens["analysis"]["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    let e = errors[0].as_str().unwrap();
    assert!(
        e.contains("no scenario \"commit\" is declared for this model"),
        "{e}"
    );
    assert!(
        e.contains("lens test reads its forward pass from it"),
        "{e}"
    );
    // ... and nothing was concluded from an empty forward pass.
    assert_eq!(lens["analysis"]["forward_flat"], json!([]));
    assert_eq!(lens["analysis"]["steps"], json!([]));
}

/// The cafe finding is backed by a check, and a check on a raw Bundle can
/// fail: without the guard link there is no read arc and the claim is false.
#[test]
fn the_cafe_guard_finding_goes_red_without_the_guard_link() {
    let id = "espresso-guard";
    let mut s = cafe();
    assert_green(&check_one(&mut s, id));
    let guard = "link:guard:counter:@transition/start_espresso->pantry:beans";
    assert!(ids_of(&run(&mut s, "content.get", json!({}))).contains(&id.to_string()));

    run(
        &mut s,
        "bundle.remove_link",
        json!({"kind": "guard", "from": "counter:@transition/start_espresso", "to": "pantry:beans"}),
    );
    // The claim states the read arc; the espresso run and the fusion it also
    // states are still true, so the one red row is the one that matters.
    let c = check_one(&mut s, id);
    assert_red_at(&c, "/arcs");
    assert_eq!(reds(&c).len(), 1, "{:?}", reds(&c));
    // The finding names the link that is gone, so it is dormant now.
    let got = run(&mut s, "content.get", json!({}));
    assert!(!ids_of(&got).contains(&id.to_string()));
    let all = run(&mut s, "content.get", json!({"all": true, "about": guard}));
    assert_eq!(ids_of(&all), [id]);
    assert_eq!(all["entries"][0]["dormant"], true);
    // The caveat that backs another claim is untouched.
    assert_green(&check_one(&mut s, "counter-starts-empty"));

    run(&mut s, "edit.undo", Value::Null);
    assert_green(&check_one(&mut s, id));
}

#[test]
fn the_cafe_content_round_trips_with_its_bundle() {
    let mut s = cafe();
    let exp = run(&mut s, "bundle.export", json!({"content": true}));
    assert_eq!(exp["form"], "bundle");
    assert_eq!(
        exp["document"],
        cafe_bundle(),
        "an unedited raw load exports as loaded"
    );
    assert_eq!(exp["cid"], CAFE_CID);
    let (content_doc, content_cid) = (
        exp["content"]["document"].clone(),
        exp["content"]["cid"].clone(),
    );
    assert_eq!(content_doc["composition"], CAFE_CID);
    assert_eq!(content_cid, cid::seal(&content_doc).unwrap().as_str());
    // check and scenario are canonical text in the exported form.
    assert!(content_doc["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e.get("check").is_some() || e.get("scenario").is_some())
        .all(|e| e.get("check").is_none_or(Value::is_string)
            && e.get("scenario").is_none_or(Value::is_string)));

    // Load what was exported, and everything is as it was.
    let mut t = session();
    let r = run(
        &mut t,
        "bundle.load",
        json!({"document": exp["document"].clone(), "content": content_doc}),
    );
    assert_eq!(r["cid"], CAFE_CID);
    assert_eq!(r["content"]["entries"], 7);
    assert_eq!(run(&mut t, "content.export", json!({}))["cid"], content_cid);
    assert_eq!(run(&mut t, "bundle.export", json!({}))["cid"], CAFE_CID);
    let ck = run(&mut t, "content.check", json!({}));
    assert_eq!(
        (ck["passed"].clone(), ck["failed"].clone()),
        (json!(2), json!(0))
    );
    let run1 = run(&mut t, "bundle.run", json!({"scenario": "espresso-order"}));
    assert_eq!(run1["completed"], true);

    // An edit changes the document's CID, so the content written for the
    // old one says it is stale: a warning, not a refusal.
    run(
        &mut t,
        "bundle.remove_link",
        json!({"kind": "guard", "from": "counter:@transition/start_espresso", "to": "pantry:beans"}),
    );
    assert_ne!(run(&mut t, "bundle.export", json!({}))["cid"], CAFE_CID);
    let v = run(&mut t, "content.validate", json!({"strict": false}));
    assert_eq!(v["valid"], true);
    let w: Vec<&str> = v["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap())
        .collect();
    assert!(
        w.contains(&"COMPOSITION_CID_STALE") && w.contains(&"ID_UNRESOLVED"),
        "{w:?}"
    );
}

#[test]
fn the_cafe_convention_example_is_the_flat_views_and_only_the_flat_views() {
    let mut s = cafe();
    let ce = run(
        &mut s,
        "content.get",
        json!({"kind": "convention-example", "view": "flat"}),
    );
    assert_eq!(ids_of(&ce), ["flat-convention-cafe"]);
    assert!(ce["entries"][0]["text"]
        .as_str()
        .unwrap()
        .contains("espresso_started"));
    for view in content::VIEWS.iter().filter(|v| **v != "flat") {
        let g = run(
            &mut s,
            "content.get",
            json!({"kind": "convention-example", "view": view}),
        );
        assert_eq!(
            ids_of(&g),
            Vec::<String>::new(),
            "{view} has no convention example"
        );
    }
    // The hover note is attached to the element it is about.
    let note = run(
        &mut s,
        "content.get",
        json!({"about": "transition:pantry/restock"}),
    );
    assert_eq!(ids_of(&note), ["pantry-restock-note"]);
}

// ------------------------------------------------------------ 6. declared scenarios

#[test]
fn bundle_scenarios_on_the_default_composition_has_the_four_in_the_old_shape() {
    let mut s = orders();
    let out = run(&mut s, "bundle.scenarios", json!({}));
    assert_eq!(out["source"], "library:orders");
    assert!(out.get("note").is_none());
    let list = out["scenarios"].as_array().unwrap();
    let ids: Vec<&str> = list.iter().map(|x| x["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["commit", "refund", "charge-fails", "second-order"]);
    // The shape bundle.scenarios has always had: the static list serialised.
    assert_eq!(
        out["scenarios"],
        serde_json::to_value(comp::scenarios()).unwrap()
    );
    for x in list {
        let mut keys: Vec<&str> = x.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert!(
            keys == ["expect", "id", "script", "title"]
                || keys == ["expect", "id", "note", "script", "title"],
            "{keys:?}"
        );
        assert!(x["script"].as_array().unwrap().iter().all(Value::is_string));
        for row in x["expect"].as_array().unwrap() {
            let mut keys: Vec<&str> = row
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            keys.sort_unstable();
            assert_eq!(keys, ["n", "op", "place", "says"]);
            assert!(row["n"].is_i64());
        }
    }
}

#[test]
fn bundle_scenarios_without_a_composition_reads_the_orders_library() {
    let mut s = session();
    assert!(s.composition.is_none());
    let out = run(&mut s, "bundle.scenarios", json!({}));
    assert_eq!(out["source"], "library:orders");
    assert_eq!(
        out["scenarios"],
        serde_json::to_value(comp::scenarios()).unwrap()
    );
    assert!(out.get("note").is_none());
}

#[test]
fn bundle_scenarios_follows_the_content_attached_to_the_composition() {
    let mut s = orders();
    // Bare: an empty list and the note, from the composition not the library.
    run(&mut s, "content.clear", json!({}));
    let out = run(&mut s, "bundle.scenarios", json!({}));
    assert_eq!(out["scenarios"], json!([]));
    assert_eq!(out["source"], "none");
    assert_eq!(out["note"], "no scenario is declared for this model");

    // Content that declares one scenario: that one, labelled as loaded.
    let one = doc(vec![scenario_entry(
        "dequeue-once",
        json!(["orders/dequeue"]),
        json!([{"place": "orders/items", "op": "==", "n": 1, "says": "one is taken"}]),
    )]);
    run(&mut s, "content.load", json!({"document": one}));
    let out = run(&mut s, "bundle.scenarios", json!({}));
    assert_eq!(out["source"], "loaded");
    let ids: Vec<&str> = out["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["dequeue-once"]);
    assert!(out.get("note").is_none());
    let r = run(&mut s, "bundle.run", json!({"scenario": "dequeue-once"}));
    assert_eq!(r["completed"], true, "{r}");
    assert!(r["expect"][0]["pass"] == true, "{r}");

    // The orders scenarios are no longer declared, so they are not run.
    let e = refused(&mut s, "bundle.run", json!({"scenario": "commit"}));
    assert!(e.contains(r#"there are ["dequeue-once"]"#), "{e}");
    // A declared scenario whose prediction is wrong runs and fails its row.
    let wrong = doc(vec![scenario_entry(
        "wrong",
        json!(["orders/dequeue"]),
        json!([{"place": "orders/items", "op": "==", "n": 2, "says": "claims nothing was taken"}]),
    )]);
    run(&mut s, "content.load", json!({"document": wrong}));
    let r = run(&mut s, "bundle.run", json!({"scenario": "wrong"}));
    assert_eq!(r["completed"], true);
    assert_eq!(r["expect"][0]["pass"], false);
    assert_eq!(r["expect"][0]["actual"], 1);
}

#[test]
fn bundle_run_with_an_undeclared_id_lists_the_declared_ones() {
    let mut s = orders();
    let e = refused(&mut s, "bundle.run", json!({"scenario": "nope"}));
    assert!(
        e.contains("no scenario \"nope\" is declared for this model"),
        "{e}"
    );
    assert!(
        e.contains(r#"there are ["commit", "refund", "charge-fails", "second-order"]"#),
        "{e}"
    );
    // Neither or both of scenario and script is a bad call, not a lookup.
    let e = refused(&mut s, "bundle.run", json!({}));
    assert!(e.contains("pass `scenario` or `script`"), "{e}");
    let e = refused(
        &mut s,
        "bundle.run",
        json!({"scenario": "commit", "script": ["orders/dequeue"]}),
    );
    assert!(e.contains("pass `scenario` or `script`"), "{e}");
}

/// A declared scenario runs exactly as the static runner ran it before the
/// scenarios moved into content.
#[test]
fn bundle_run_of_a_declared_scenario_equals_the_static_runner() {
    let mut s = orders();
    let c = default_comp();
    let flat = comp::flatten(&c.bundle).unwrap();
    for sc in comp::scenarios() {
        let mut got = run(&mut s, "bundle.run", json!({"scenario": sc.id}));
        got.as_object_mut().unwrap().remove("artifact");
        assert_eq!(
            got,
            serde_json::to_value(comp::run_scenario(&flat, sc)).unwrap(),
            "{}",
            sc.id
        );
    }
    // What the orders findings say about them: the first three complete, the
    // second order stops at its second dequeue.
    let done = |s: &mut Session, id: &str| {
        run(s, "bundle.run", json!({"scenario": id}))["completed"].clone()
    };
    assert_eq!(done(&mut s, "commit"), true);
    assert_eq!(done(&mut s, "refund"), true);
    assert_eq!(done(&mut s, "second-order"), false);
}

/// `comp::scenarios()` (static, parsed once) is the library content's declared
/// scenarios, member for member.
#[test]
fn the_static_scenarios_are_the_librarys_declared_scenarios_member_for_member() {
    let declared = content::library_entry("orders")
        .unwrap()
        .content
        .scenarios();
    let stat = comp::scenarios();
    assert_eq!(declared.len(), 4);
    assert_eq!(stat.len(), declared.len());
    for (a, b) in stat.iter().zip(&declared) {
        assert_eq!(a.id, b.id);
        assert_eq!(a.title, b.title);
        assert_eq!(a.note, b.note);
        assert_eq!(
            a.script,
            b.script.iter().map(String::as_str).collect::<Vec<_>>()
        );
        assert_eq!(a.expect.len(), b.expect.len(), "{}", a.id);
        for (x, y) in a.expect.iter().zip(&b.expect) {
            assert_eq!(
                (x.place, x.op, x.n, x.says),
                (y.place.as_str(), y.op.as_str(), y.n, y.says.as_str())
            );
        }
        assert_eq!(comp::scenario(a.id).map(|x| x.id), Some(a.id));
        // and to_json is the shape bundle.scenarios returns for it
        assert_eq!(b.to_json(), serde_json::to_value(a).unwrap(), "{}", a.id);
    }
    assert!(comp::scenario("nope").is_none());
}

/// The lens test reads the forward pass from the declared `commit`
/// scenario, not from a list of its own.
#[test]
fn the_lens_test_reads_the_declared_commit_scenario() {
    let mut s = orders();
    let forward = |s: &mut Session| -> Vec<Value> {
        run(s, "bundle.lens", json!({"fixes": false}))["analysis"]["forward_flat"]
            .as_array()
            .unwrap()
            .clone()
    };
    let before = forward(&mut s);
    assert!(before.len() >= 2);
    edit_entry(&mut s, "commit", |e| {
        e["scenario"]["script"].as_array_mut().unwrap().pop();
    });
    let after = forward(&mut s);
    assert_eq!(after.len(), before.len() - 1, "{before:?} -> {after:?}");
    assert_eq!(after[..], before[..after.len()]);

    // Without a commit scenario the test says so instead of concluding.
    run(&mut s, "content.clear", json!({}));
    let lens = run(&mut s, "bundle.lens", json!({"fixes": false}));
    assert_eq!(lens["analysis"]["ok"], false);
    assert_eq!(lens["analysis"]["holds"], false);
    assert!(lens["analysis"]["errors"][0]
        .as_str()
        .unwrap()
        .contains("no scenario \"commit\" is declared"));
}

// ------------------------------------------------------------ library content and bare models

/// Library entries ship content; a user's model is bare and says so.
#[test]
fn library_content_belongs_to_the_library_example_and_a_user_model_is_bare() {
    let source = |s: &mut Session| run(s, "bundle.state", json!({}))["content"]["source"].clone();

    // The example opened from the catalog, and the same document loaded
    // again (its CID is the one the library content was written against).
    let mut s = orders();
    assert_eq!(source(&mut s), "library:orders");
    let example = document_of(&mut s);
    let mut t = session();
    let r = run(&mut t, "bundle.load", json!({"document": example}));
    assert_eq!(r["content"]["source"], "library:orders");
    assert_eq!(r["content"]["entries"], 121);
    assert_eq!(
        r["missing"].as_array().unwrap().len(),
        1,
        "only NO_LAYOUT: {r}"
    );

    // The same composition with an instance added is a user's model: its
    // document has another CID, and it is bare.
    run(&mut s, "bundle.add_instance", json!({"id": "payment2"}));
    assert_eq!(
        source(&mut s),
        "library:orders",
        "edits keep what is attached"
    );
    let edited = document_of(&mut s);
    let mut u = session();
    let r = run(&mut u, "bundle.load", json!({"document": edited}));
    assert_eq!(r["content"], json!({"source": "none", "entries": 0}));
    let codes = missing_codes(&mut u);
    assert!(codes.contains(&"NO_CONTENT".to_string()), "{codes:?}");
    assert!(codes.contains(&"NO_SCENARIOS".to_string()), "{codes:?}");
    let g = run(&mut u, "content.get", json!({}));
    assert_eq!(
        (g["source"].clone(), g["entries"].clone()),
        (json!("none"), json!([]))
    );
    assert_eq!(
        run(&mut u, "bundle.scenarios", json!({}))["scenarios"],
        json!([])
    );

    // A catalog composition that lacks one of the anchors is bare too.
    let mut v = session();
    run(
        &mut v,
        "bundle.open",
        json!({"instances": ["orders", "workers"]}),
    );
    assert_eq!(source(&mut v), "none");
    assert!(missing_codes(&mut v).contains(&"NO_CONTENT".to_string()));

    // ... and a user can give any composition content of their own, which
    // is then labelled as loaded and not as the library's.
    let mine = doc(vec![entry("mine", "note", &["instance:orders"])]);
    run(&mut v, "content.load", json!({"document": mine}));
    assert_eq!(source(&mut v), "loaded");
    assert!(!missing_codes(&mut v).contains(&"NO_CONTENT".to_string()));
}

// ------------------------------------------------------------ the library, as the gate
//
// What the first version of this file held: the library content of both
// subjects is sound against its model, and a red run names what broke.

fn library(s: &mut Session, id: &str) -> Value {
    run(s, "content.library", Value::Null)["library"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"] == id)
        .unwrap_or_else(|| panic!("no library content {id}"))
        .clone()
}

fn all_checks_pass(report: &Value, expected: u64) {
    // Each failure as its id, claim and the steps or expectations that
    // were not ok, so a red run names what broke.
    let failed: Vec<Value> = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["ok"] != json!(true))
        .map(|c| {
            let bad: Vec<Value> = c["steps"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|st| {
                    let expect: Vec<&Value> = st["expect"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|e| e["ok"] != json!(true))
                        .collect();
                    (st["ok"] != json!(true) || !expect.is_empty()).then(|| {
                        json!({"command": st["command"], "ok": st["ok"], "error": st["error"], "expect": expect})
                    })
                })
                .collect();
            json!({"id": c["id"], "claim": c["claim"], "bad_steps": bad})
        })
        .collect();
    assert!(failed.is_empty(), "checks that failed: {:#}", json!(failed));
    assert_eq!(report["failed"], 0, "{report}");
    assert_eq!(report["total"], expected, "every check in the entry is run");
    assert_eq!(report["passed"], expected, "{report}");
}

#[test]
fn the_orders_library_is_sound_against_its_composition_and_every_check_passes() {
    let mut s = session();
    let lib = library(&mut s, "orders");
    run(&mut s, "bundle.open", json!({}));

    let v = run(&mut s, "content.validate", Value::Null);
    assert_eq!(v["valid"], true, "{v}");
    assert_eq!(v["entries"], lib["entries"]);

    let report = run(&mut s, "content.check", Value::Null);
    all_checks_pass(&report, lib["checks"].as_u64().unwrap());
    assert!(report["total"].as_u64().unwrap() > 0);
}

#[test]
fn the_connect4_library_is_sound_against_the_plan_and_every_check_passes() {
    let mut s = session();
    let lib = library(&mut s, "connect4");
    run(&mut s, "plan.open", json!({}));

    let v = run(&mut s, "content.validate", json!({"subject": "plan"}));
    assert_eq!(v["valid"], true, "{v}");
    assert_eq!(v["entries"], lib["entries"]);

    let report = run(&mut s, "content.check", json!({"subject": "plan"}));
    all_checks_pass(&report, lib["checks"].as_u64().unwrap());
    assert!(report["total"].as_u64().unwrap() > 0);
}

#[test]
fn content_without_a_composition_is_refused() {
    let mut s = session();
    let e = refused(&mut s, "content.get", Value::Null);
    assert!(e.contains("no composition is open"), "{e}");
    let e = refused(&mut s, "content.load", json!({"library": "orders"}));
    assert!(e.contains("no composition is open"), "{e}");
}

#[test]
fn the_declared_scenarios_come_from_the_library_content() {
    let mut s = session();
    let out = run(&mut s, "bundle.scenarios", Value::Null);
    assert_eq!(out["source"], "library:orders", "{out}");
    let ids: Vec<&str> = out["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["commit", "refund", "charge-fails", "second-order"]);
}

// ------------------------------------------------------------ the token game reads declared scenarios

/// `bundle.run` has always read the open composition's declared scenarios; the token game
/// (`bundle.scenario`, `bundle.step`) read the orders library's instead, so a scenario a loaded model
/// declares could be run but not stepped.
#[test]
fn a_declared_scenario_is_selected_and_stepped_in_the_token_game() {
    let mut s = cafe();
    let g = run(&mut s, "bundle.scenario", json!({"id": "espresso-order"}));
    assert_eq!(g["scenario"]["id"], "espresso-order");
    assert_eq!(g["scenario"]["shown"], 0);
    let g = run(&mut s, "bundle.step", json!({}));
    assert_eq!(g["scenario"]["shown"], 1);
    assert_eq!(g["fired"], json!(["counter/order"]));
    let g = run(&mut s, "bundle.step", json!({"all": true}));
    let sc = &g["scenario"];
    assert_eq!(sc["done"], true, "{sc}");
    assert_eq!(sc["completed"], true, "{sc}");
    assert_eq!(sc["steps"].as_array().unwrap().len(), 3);
    // The predicates are the declared ones, read at the marking the game reached.
    let expect = sc["expect"].as_array().unwrap();
    assert!(expect.len() >= 3, "{sc}");
    assert!(expect.iter().all(|e| e["pass"] == true), "{expect:?}");
    // And they say what `bundle.run` says.
    let r = run(&mut s, "bundle.run", json!({"scenario": "espresso-order"}));
    assert_eq!(r["completed"], sc["completed"]);
    for (a, b) in r["expect"].as_array().unwrap().iter().zip(expect) {
        assert_eq!(
            (&a["place"], &a["actual"], &a["pass"]),
            (&b["place"], &b["actual"], &b["pass"])
        );
    }
}

#[test]
fn a_scenario_the_model_does_not_declare_cannot_be_selected_in_the_token_game() {
    let mut s = cafe();
    let err = refused(&mut s, "bundle.scenario", json!({"id": "commit"}));
    assert!(
        err.contains("\"commit\"") && err.contains("espresso-order") && err.contains("latte-order"),
        "names what it declares: {err}"
    );
    // The orders example, once its content is cleared, declares none: `commit` is gone with them.
    let mut o = orders();
    run(&mut o, "bundle.scenario", json!({"id": "commit"}));
    run(&mut o, "content.clear", json!({}));
    let err = refused(&mut o, "bundle.scenario", json!({"id": "commit"}));
    assert!(err.contains("declares none"), "{err}");
    // A game left on a scenario that is no longer declared reads as no scenario, not a crash.
    let g = run(&mut o, "bundle.game", json!({}));
    assert!(g["scenario"].is_null(), "{g}");
    let err = refused(&mut o, "bundle.step", json!({}));
    assert!(err.contains("no scenario"), "{err}");
}

// ------------------------------------------------------------ the validator, the rest of the gaps

fn finding_with(extra: Value) -> Value {
    let mut e = plain("f");
    for (k, v) in extra.as_object().unwrap() {
        e[k] = v.clone();
    }
    e
}

fn parse_err(d: &Value) -> String {
    ModelContent::parse(d).expect_err("refused").to_string()
}

/// `by` was whitelisted but never kept, and a non-integer `order` was read as no order: both were
/// accepted and dropped by the next export, changing its CID.
#[test]
fn a_key_or_an_order_the_export_would_drop_is_refused() {
    let err = parse_err(&doc(vec![finding_with(json!({"by": "matt"}))]));
    assert!(
        err.starts_with("[CONTENT_FIELD]") && err.contains("\"by\""),
        "{err}"
    );
    for order in [json!("7"), json!(1.5), json!(true)] {
        let err = parse_err(&doc(vec![finding_with(json!({"order": order}))]));
        assert!(
            err.starts_with("[CONTENT_FIELD]") && err.contains("order"),
            "{err}"
        );
    }
    // An integer, or none, is fine, and survives an export.
    let mc = ModelContent::parse(&doc(vec![finding_with(json!({"order": 7}))])).unwrap();
    assert_eq!(mc.to_document()["entries"][0]["order"], 7);
}

#[test]
fn a_cite_is_a_bib_key_or_a_source_with_a_repo_and_a_path() {
    let bad: [(Value, &str); 9] = [
        (json!(1), "must be an object"),
        (json!("x"), "must be an object"),
        (json!(null), "must be an object"),
        (json!({"bib": 5}), "bib must be a non-empty string"),
        (json!({"bib": "baez-master", "repo": "x"}), "alone"),
        (json!({"foo": "bar"}), "unknown key"),
        (json!({"repo": "x"}), "non-empty path"),
        (
            json!({"repo": "x", "path": "p", "from": 9, "to": 3}),
            "after",
        ),
        (
            json!({"repo": "x", "path": "p", "from": "nine"}),
            "line number",
        ),
    ];
    for (cite, want) in bad {
        let r = report(
            &doc(vec![finding_with(json!({"cites": [cite.clone()]}))]),
            true,
        );
        assert_only_error(&r, "CITE_INVALID", Some("f"));
        assert!(
            r.errors[0].message.contains(want),
            "{cite}: {}",
            r.errors[0].message
        );
    }
    for good in [
        json!({"bib": "baez-master"}),
        json!({"repo": "go-pflow", "path": "metamodel/firing.go", "rev": "69257b3", "from": 142, "to": 196, "note": "n"}),
        json!({"repo": "x", "path": "p"}),
    ] {
        let r = report(
            &doc(vec![finding_with(json!({"cites": [good.clone()]}))]),
            true,
        );
        assert!(r.valid(), "{good}: {:?}", r.errors);
    }
}

#[test]
fn text_that_is_only_whitespace_a_scenario_on_a_finding_and_a_second_convention_example_are_caught()
{
    let r = report(&doc(vec![finding_with(json!({"text": "  \n\t "}))]), true);
    assert_only_error(&r, "CONTENT_FIELD", Some("f"));
    let r = report(
        &doc(vec![finding_with(
            json!({"scenario": {"script": [], "expect": []}}),
        )]),
        true,
    );
    assert_only_error(&r, "CONTENT_FIELD", Some("f"));
    assert!(r.errors[0]
        .message
        .contains("only an entry of kind scenario"));
    // Two convention examples for one view are both drawn: a warning, not a refusal.
    let conv = |id: &str| json!({"id": id, "kind": "convention-example", "text": "t", "about": ["model:"], "views": ["flat"]});
    let r = report(&doc(vec![conv("a"), conv("b")]), true);
    assert!(r.valid(), "{:?}", r.errors);
    assert_eq!(codes(&r.warnings), vec!["CONVENTION_DUPLICATE"]);
    assert_eq!(r.warnings[0].entry.as_deref(), Some("b"));
}

#[test]
fn a_check_that_names_a_scenario_or_a_member_the_model_lacks_is_refused_at_load() {
    let run_step = |args: Value| step_with("bundle.run", args);
    for (args, reference) in [
        (json!({"scenario": "nope"}), "scenario:nope"),
        (
            json!({"script": ["orders/dequeue", "orders/nope"]}),
            "transition:orders/nope",
        ),
    ] {
        let e = finding_with(json!({"check": check_of(json!([run_step(args.clone())]))}));
        let r = report(&doc(vec![e.clone()]), true);
        assert_only_error(&r, "CHECK_REF_UNRESOLVED", Some("f"));
        assert_eq!(r.errors[0].reference.as_deref(), Some(reference));
        // Not strict: dormant, a warning that the load result lists.
        let r = report(&doc(vec![e]), false);
        assert!(r.valid());
        assert_eq!(codes(&r.warnings), vec!["CHECK_REF_UNRESOLVED"]);
        assert_eq!(r.unresolved().len(), 1);
    }
    // A scenario the content declares is fine.
    let sc = scenario_entry("s", json!(["orders/dequeue"]), json!([]));
    let ok = finding_with(json!({"check": check_of(json!([run_step(json!({"scenario": "s"}))]))}));
    assert!(report(&doc(vec![sc, ok]), true).valid());
}

fn step_with(command: &str, args: Value) -> Value {
    json!({"command": command, "args": args, "expect": [{"path": "", "op": "exists"}]})
}

#[test]
fn the_sizes_of_titles_cites_and_the_checks_in_all_are_bounded() {
    let r = report(
        &doc(vec![finding_with(
            json!({"title": "x".repeat(content::MAX_TITLE_BYTES + 1)}),
        )]),
        true,
    );
    assert_only_error(&r, "CONTENT_LIMIT", Some("f"));
    let cites: Vec<Value> = (0..=content::MAX_CITES)
        .map(|_| json!({"bib": "baez-master"}))
        .collect();
    let r = report(&doc(vec![finding_with(json!({"cites": cites}))]), true);
    assert_only_error(&r, "CONTENT_LIMIT", Some("f"));
    // 16 steps an entry is the per-check cap; many entries of 16 add up to the document's.
    let steps: Vec<Value> = (0..checks::MAX_STEPS)
        .map(|_| step_with("bundle.state", json!({})))
        .collect();
    let many = content::MAX_CHECK_STEPS / checks::MAX_STEPS + 1;
    let entries: Vec<Value> = (0..many)
        .map(|i| {
            let mut e = plain(&format!("f{i}"));
            e["check"] = check_of(json!(steps));
            e
        })
        .collect();
    let r = report(&doc(entries), true);
    assert_only_error(&r, "CONTENT_LIMIT", None);
    assert!(r.errors[0].message.contains("steps in all"));
}

/// A check from outside runs reads of the model. `bundle.receipt` signs with the local key and
/// `bundle.export` writes a document: neither is a read.
#[test]
fn a_check_may_not_sign_or_export() {
    for cmd in checks::NOT_READS {
        let e = finding_with(json!({"check": check_of(json!([step_with(cmd, json!({}))]))}));
        let r = report(&doc(vec![e]), true);
        assert_only_error(&r, "CHECK_COMMAND_FORBIDDEN", Some("f"));
        assert!(
            r.errors[0].message.contains("not a read"),
            "{}",
            r.errors[0].message
        );
    }
}

/// The allow-list is the registry's queries in `bundle.` and `plan.` minus the ones named as not
/// reads: a new query is a decision here, not a default.
#[test]
fn every_bundle_and_plan_query_is_either_a_read_or_named_here() {
    let mut queries: Vec<&str> = pflow_native_core::commands::ALL
        .iter()
        .flat_map(|set| set.iter())
        .filter(|c| (c.id.starts_with("bundle.") || c.id.starts_with("plan.")) && !c.journal)
        .map(|c| c.id)
        .collect();
    queries.sort();
    let mut known: Vec<&str> = checks::READS
        .iter()
        .chain(&checks::NOT_READS)
        .copied()
        .collect();
    known.sort();
    assert_eq!(queries, known);
    for id in checks::READS {
        assert!(checks::command_allowed(id).is_ok(), "{id}");
    }
}

// ------------------------------------------------------------ what a load tells the caller

fn small_content() -> Value {
    json!({
        "@context": CONTEXT, "@type": "ModelContent", "name": "small",
        "subject": {"kind": "composition", "name": "orders"},
        "entries": [{"id": "f", "kind": "finding", "text": "t", "about": ["instance:orders"],
            "check": "{\n  \"claim\": \"c\",\n  \"steps\": [ {\"command\": \"bundle.state\", \"args\": {}, \"expect\": [{\"path\": \"/instances/0\", \"op\": \"==\", \"value\": \"orders\"}]} ]\n}"}],
    })
}

/// A load says what it normalised: the CID is the document `content.export` writes, and the seal
/// of the document as received is given beside it when it differs.
#[test]
fn a_content_cid_is_the_normalised_documents_and_the_received_one_is_named() {
    let mut s = orders();
    let received = small_content();
    let out = run(&mut s, "content.load", json!({"document": received}));
    let exported = run(&mut s, "content.export", Value::Null);
    assert_eq!(out["cid"], exported["cid"]);
    assert_eq!(out["received_cid"], json!(cid::seal(&received).unwrap()));
    assert_ne!(out["received_cid"], out["cid"]);
    // The normalised document, loaded again, is its own CID: nothing further to say.
    let again = run(
        &mut s,
        "content.load",
        json!({"document": exported["document"]}),
    );
    assert_eq!(again["cid"], out["cid"]);
    assert!(again.get("received_cid").is_none(), "{again}");
    // And the same bytes through the store name the same CID.
    let put = run(&mut s, "store.put", json!({"document": received}));
    assert_eq!(put["cid"], out["cid"]);
}

/// A warning is part of what a load returns, not only of `content.validate`.
#[test]
fn a_load_returns_its_warnings() {
    let mut s = orders();
    let mut d = small_content();
    d["composition"] = json!("zNOTACID");
    let out = run(&mut s, "content.load", json!({"document": d}));
    let codes: Vec<&str> = out["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["COMPOSITION_CID_STALE"], "{out}");
    // bundle.load reports the same, under `content`.
    let mut t = session();
    let l = run(
        &mut t,
        "bundle.load",
        json!({"document": run(&mut orders(), "bundle.export", Value::Null)["document"], "content": d}),
    );
    assert_eq!(
        l["content"]["warnings"][0]["code"], "COMPOSITION_CID_STALE",
        "{l}"
    );
}

/// Library content on a composition that lacks some of its elements is dormant, not refused, and
/// the model says so beside what else it lacks.
#[test]
fn library_content_on_an_edited_model_is_dormant_and_missing_says_so() {
    let mut s = session();
    run(
        &mut s,
        "bundle.open",
        json!({"instances": ["orders", "workers"]}),
    );
    // The library's content was attached by the instances it needs: this has two of three. Load it
    // on purpose, not strict.
    let out = run(
        &mut s,
        "content.load",
        json!({"library": "orders", "strict": false}),
    );
    assert!(out["dormant"].as_u64().unwrap() > 0, "{out}");
    assert!(
        out["unresolved"]
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["ref"] == "payment/do_charge"),
        "its scenarios' missing members are named: {}",
        out["unresolved"]
    );
    let st = run(&mut s, "bundle.state", Value::Null);
    let dormant = st["missing"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["code"] == "DORMANT")
        .unwrap_or_else(|| panic!("{}", st["missing"]));
    let text = dormant["text"].as_str().unwrap();
    assert!(
        text.contains("content entries") && text.contains("no view shows them"),
        "{text}"
    );
    // Strict, the same load is refused and says which.
    let err = refused(&mut s, "content.load", json!({"library": "orders"}));
    assert!(err.starts_with("[CONTENT_REFUSED]"), "{err}");
    // The full orders example has none dormant.
    let st = run(&mut orders(), "bundle.state", Value::Null);
    assert!(!st["missing"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["code"] == "DORMANT"));
}

// ------------------------------------------------------------ the plan's checks can go red

/// No Connect-4 check had been shown to fail: 35 of the 40 assert pflow-rs vocabulary, which no edit
/// of the plan moves. Each is run here with one expected value made false, and the ones that read the
/// open plan with the plan edited.
#[test]
fn a_connect4_check_goes_red_when_its_claim_is_made_false() {
    let lib = content::library_entry("connect4").unwrap();
    // The empty-order finding is checked by this change; each check of the library, with its
    // first expected value of its first step turned into something else, fails.
    let mut turned = 0;
    for e in lib.content.checks() {
        let mut broken = e.clone();
        let rows = broken.check.as_mut().unwrap()["steps"][0]["expect"]
            .as_array_mut()
            .unwrap();
        let first = &mut rows[0];
        match first["op"].as_str().unwrap() {
            "==" => first["value"] = json!("a value nothing has"),
            "contains" => first["value"] = json!("a text nothing contains"),
            "exists" => first["op"] = json!("absent"),
            "absent" => first["op"] = json!("exists"),
            _ => continue,
        }
        let red = checks::run_check(&Base::Plan(None), &broken);
        assert_eq!(red["ok"], false, "{} should go red: {red}", e.id);
        turned += 1;
    }
    assert!(turned >= 40, "{turned} checks were made false");
    let empty = lib.content.entry("c4-finding-empty-order").unwrap();
    assert!(
        empty.check.is_some(),
        "the empty-order finding carries its check"
    );
    assert_eq!(checks::run_check(&Base::Plan(None), empty)["ok"], true);
}

#[test]
fn a_connect4_check_that_reads_the_open_plan_goes_red_when_the_plan_is_edited() {
    let mut s = session();
    run(&mut s, "plan.open", json!({}));
    let r = run(
        &mut s,
        "content.check",
        json!({"subject": "plan", "entry": "c4-model-plan"}),
    );
    assert_eq!(r["passed"], 1, "{r}");
    // One entry fewer in the order, and the plan is no longer the module-doc example.
    run(
        &mut s,
        "plan.remove_piece",
        json!({"slot": "order", "index": 0}),
    );
    let r = run(
        &mut s,
        "content.check",
        json!({"subject": "plan", "entry": "c4-model-plan"}),
    );
    assert_eq!(r["failed"], 1, "{r}");
    assert!(
        reds(&r["checks"][0])
            .iter()
            .any(|x| x.contains("/plan/order")),
        "{:?}",
        reds(&r["checks"][0])
    );
    run(&mut s, "edit.undo", json!({}));
    let r = run(
        &mut s,
        "content.check",
        json!({"subject": "plan", "entry": "c4-model-plan"}),
    );
    assert_eq!(r["passed"], 1, "{r}");
}
