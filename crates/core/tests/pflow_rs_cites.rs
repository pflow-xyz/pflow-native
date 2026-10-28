//! The pflow-rs line cites of the composition views
//! (`src/composition/pflow_rs_cites.json`), held to the pinned revision.
//!
//! The prototype cited pflow-rs at c396d46; the pin (aacb7eb) moved and
//! rewrote plan.rs, game.rs and order.rs, so every cite was re-read. The
//! table is only worth having while it stays true, so when a pflow-rs source
//! tree can be found each entry's `needle` must still be in lines
//! `from..=to` of its file. The tree is `PFLOW_RS_SRC` (a checkout at the
//! pin), else cargo's own checkout of the pinned git dependency
//! (`$CARGO_HOME/git/checkouts/pflow-rs-*/aacb7eb*`). With neither, the
//! needle test prints a note and skips; the structural tests still run.

use std::path::{Path, PathBuf};

use pflow_native_core::search_plan as sp;
use pflow_native_core::Session;
use serde_json::{json, Value};

const STATUSES: [&str; 5] = ["same", "moved", "rewritten", "gone", "new"];

fn table() -> &'static serde_json::Map<String, Value> {
    sp::pflow_rs_cites().as_object().expect("an object")
}

/// A pflow-rs tree at the pin, if one can be found.
fn pflow_rs_tree() -> Option<PathBuf> {
    let ok = |p: &Path| p.join("crates/pflow-metamodel/src/plan.rs").is_file();
    if let Some(p) = std::env::var_os("PFLOW_RS_SRC").map(PathBuf::from) {
        return ok(&p).then_some(p);
    }
    let home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")))?;
    let checkouts = home.join("git/checkouts");
    for repo in std::fs::read_dir(checkouts).ok()?.flatten() {
        if !repo.file_name().to_string_lossy().starts_with("pflow-rs-") {
            continue;
        }
        for rev in std::fs::read_dir(repo.path()).ok()?.flatten() {
            if rev
                .file_name()
                .to_string_lossy()
                .starts_with(sp::PLAN_RS_REV)
                && ok(&rev.path())
            {
                return Some(rev.path());
            }
        }
    }
    None
}

#[test]
fn every_entry_is_well_formed() {
    assert!(
        table().len() > 150,
        "the table has {} entries",
        table().len()
    );
    for (key, v) in table() {
        let (stem, name) = key.split_once('.').expect("<file>.<name>");
        assert!(!stem.is_empty() && !name.is_empty(), "{key}");
        let status = v["status"].as_str().unwrap_or_default();
        assert!(STATUSES.contains(&status), "{key}: status {status:?}");
        let path = v["path"].as_str().unwrap_or_default();
        assert!(path.starts_with("crates/pflow-"), "{key}: path {path:?}");
        let (from, to) = (v["from"].as_u64().unwrap(), v["to"].as_u64().unwrap());
        assert!(
            from <= to,
            "{key}: lines {from}-{to} (0-0 is the whole file)"
        );
        assert!(v["note"].as_str().is_some_and(|n| !n.is_empty()), "{key}");
        assert!(
            v["needle"].as_str().is_some_and(|n| !n.is_empty()),
            "{key}: no needle"
        );
        // `was` says where the prototype had it; `new` has none, the rest do.
        let was = &v["was"];
        if status == "new" {
            assert!(was.is_null(), "{key}: a new cite was nowhere");
            continue;
        }
        let same_place =
            was["path"] == v["path"] && was["from"] == v["from"] && was["to"] == v["to"];
        assert!(
            was["path"].is_string() && was["from"].is_u64() && was["to"].is_u64(),
            "{key}: no `was`"
        );
        assert_eq!(
            status == "same",
            same_place,
            "{key}: `same` means the prototype's own lines, and only then"
        );
    }
}

#[test]
fn cites_are_read_back_with_the_label_views_show() {
    let c = sp::pflow_rs_cite("plan.search").unwrap();
    assert_eq!(c.path, "crates/pflow-metamodel/src/game.rs");
    assert_eq!((c.from, c.to), (1046, 1179));
    assert_eq!(
        c.label(),
        "pflow-rs crates/pflow-metamodel/src/game.rs:1046-1179 @aacb7eb \
         (Solver::search: fail-soft alpha-beta with the transposition table)"
    );
    // One line, and a whole file (the Connect-4 fixture), format as the
    // app's `cite_label` does.
    assert_eq!(
        sp::pflow_rs_cite("plan.cutoff").unwrap().label(),
        "pflow-rs crates/pflow-metamodel/src/game.rs:1164 @aacb7eb \
         (chain.cutoff: a cutoff is reported back to the pieces)"
    );
    let fx = sp::pflow_rs_cite("boundaries.fixture").unwrap();
    assert_eq!((fx.from, fx.to), (0, 0));
    assert!(fx
        .label()
        .starts_with("pflow-rs crates/pflow-metamodel/tests/fixtures/connect-4.json @aacb7eb ("));
    assert!(sp::pflow_rs_cite("no.such.key").is_none());
    // What the pin gave up is marked, and says what took its place.
    let gone = sp::pflow_rs_cite("spiders.tableDefault").unwrap();
    assert_eq!(gone.status, "gone");
    assert_eq!(gone.was.unwrap().from, 469);
    assert!(gone.note.contains("Solver::MEMO"), "{}", gone.note);
}

#[test]
fn the_revision_is_the_pin() {
    use pflow_native_core::provenance::PFLOW_RS_REV;
    assert!(PFLOW_RS_REV.starts_with(sp::PLAN_RS_REV));
    let o = pflow_native_core::composition::ontology();
    assert_eq!(o["repos"]["pflowRs"]["rev"], sp::PLAN_RS_REV);
    let c = sp::pflow_rs_cite("plan.driversFlags").unwrap();
    assert_eq!(c.rev, sp::PLAN_RS_REV);
}

/// Every built piece, and every slot a plan has a field for, carries cites
/// in `plan.pieces`; the root's are the new `root` field's.
#[test]
fn the_palette_carries_its_cites() {
    let mut s = Session::default();
    let out = s.execute("plan.pieces", Value::Null).unwrap();
    for p in out["pieces"].as_array().unwrap() {
        let name = p["name"].as_str().unwrap();
        let cites = p["cites"].as_array().unwrap();
        if p["status"] == "built" || p["status"] == "reserved" {
            assert!(!cites.is_empty(), "piece {name} has no pflow-rs cite");
        }
        for c in cites {
            assert_eq!(c["key"].as_str().unwrap().split('.').nth(1), Some(name));
            assert!(c["label"].as_str().unwrap().starts_with("pflow-rs crates/"));
            assert!(c["from"].is_u64() && c["to"].is_u64() && c["status"].is_string());
        }
    }
    for slot in out["slots"].as_array().unwrap() {
        let id = slot["id"].as_str().unwrap();
        let cites = slot["cites"].as_array().unwrap();
        if slot["field"].is_string() {
            assert!(!cites.is_empty(), "slot {id} has no pflow-rs cite");
        }
    }
    // The new root field and its pieces.
    let root = sp::slot_cites("root");
    assert!(root.iter().any(|c| c.path.ends_with("plan.rs")));
    assert!(root.iter().any(|c| c.path.ends_with("root.rs")));
    let ode: Vec<_> = sp::piece_cites("ode")
        .iter()
        .map(|c| c.path.clone())
        .collect();
    assert!(ode.iter().any(|p| p == "crates/pflow-solver/src/game.rs"));
    assert!(ode.iter().any(|p| p.ends_with("root.rs")));
    for name in [
        "non_losing_root",
        "mtd_f",
        "aspiration",
        "must_win",
        "non_losing",
    ] {
        assert!(!sp::piece_cites(name).is_empty(), "{name}");
    }
    // Secondary cites keep the table's order.
    let nw: Vec<_> = sp::piece_cites("null_window")
        .iter()
        .map(|c| c.key.clone())
        .collect();
    assert_eq!(nw, ["piece.null_window", "piece.null_window.2"]);
    assert_eq!(
        json!(sp::piece_cites("tt_best")[0].key),
        json!("piece.tt_best")
    );
}

#[test]
fn every_needle_is_in_its_lines_at_the_pin() {
    let Some(root) = pflow_rs_tree() else {
        println!(
            "pflow_rs_cites: no pflow-rs tree at {} found (set PFLOW_RS_SRC, or build once so \
             cargo checks the git dependency out): the needle test is skipped",
            sp::PLAN_RS_REV
        );
        return;
    };
    println!("pflow_rs_cites: checking against {}", root.display());
    let mut bad = vec![];
    let mut checked = 0;
    for (key, v) in table() {
        let path = v["path"].as_str().unwrap();
        let text = match std::fs::read_to_string(root.join(path)) {
            Ok(t) => t,
            Err(e) => {
                bad.push(format!("{key}: {path}: {e}"));
                continue;
            }
        };
        let lines: Vec<&str> = text.lines().collect();
        let (from, to) = (
            v["from"].as_u64().unwrap() as usize,
            v["to"].as_u64().unwrap() as usize,
        );
        let needle = v["needle"].as_str().unwrap();
        let hay = if from == 0 {
            text.clone()
        } else if to > lines.len() {
            bad.push(format!(
                "{key}: lines {from}-{to} past the end of {path} ({})",
                lines.len()
            ));
            continue;
        } else {
            lines[from - 1..to].join("\n")
        };
        checked += 1;
        if !hay.contains(needle) {
            bad.push(format!("{key}: {needle:?} is not in {path}:{from}-{to}"));
        }
    }
    assert!(
        bad.is_empty(),
        "{} of {checked} cites are off:\n{}",
        bad.len(),
        bad.join("\n")
    );
    assert_eq!(checked, table().len());
}
