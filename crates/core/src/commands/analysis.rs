//! Structural analysis (phase 4): incidence, P/T invariants, siphons and
//! traps, validation, the canonical id with its automorphism orbits, and
//! colour-refinement classes. sim.pflow.xyz's `sim_invariants`,
//! `sim_canonical` and `sim_classify`, over the document's compiled model
//! (see [`crate::analysis`]). Node sets come back as `sets` artifacts naming
//! document node ids, so `artifact.show_set` highlights them on the canvas;
//! on a coloured net the colour is in the label, note and weight keys
//! (`place.color`).

use std::collections::{BTreeMap, HashSet};

use serde_json::{json, Value};

use crate::analysis::farkas::{farkas, transpose};
use crate::analysis::siphons::{bit, members, Io, Mask};
use crate::analysis::symmetry::{self, Incidence, WORK_BUDGET};
use crate::analysis::Structure;
use crate::artifact::{ArtifactData, NodeSet};
use crate::registry::{always, b, n, opt, s, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::stochastic::Law;
use crate::{Error, Result};

/// Rows the Farkas elimination may hold (as `sim.crosscheck`'s); past it
/// the result says `truncated`.
pub const MAX_FARKAS_ROWS: usize = 4096;
/// States `analysis.validate {reachability}` may explore.
pub const MAX_VALIDATE_STATES: usize = 200_000;

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "analysis.incidence",
        label: "Incidence Matrix",
        help: "The incidence matrix C = post − pre (rows: places in document order, `place.color` per used colour on a coloured net; columns: transitions). Read and inhibitor arcs move nothing, so they do not enter it. Returns pre and post too.",
        menu: "Analyze",
        shortcut: None,
        args: &[],
        enabled: always,
        run: incidence,
        journal: true,
    },
    CommandSpec {
        id: "analysis.invariants",
        label: "Invariants",
        help: "Minimal semi-positive P-invariants (conservation laws, valued at the current marking) and T-invariants (firing counts that return the net to its marking), by Farkas elimination. Stores one node set per invariant (show one on the net) and the basis as a matrix; reports P-coverage (covered ⇒ structurally bounded) and T-coverage.",
        menu: "Analyze",
        shortcut: None,
        args: &[opt("kind", Str, "\"p\", \"t\" or \"both\" (default both)")],
        enabled: always,
        run: invariants,
        journal: true,
    },
    CommandSpec {
        id: "analysis.siphons",
        label: "Siphons & Traps",
        help: "Every minimal siphon and trap, from the arc structure alone (sim_invariants' siphon/trap report). Marks each siphon empty at the current marking (a deadlock witness: the transitions needing it never fire again) and whether it contains a marked trap (Commoner's property, which certifies deadlock freedom for ordinary nets and liveness for free-choice ones). Refuses nets over 128 token places or a search over budget.",
        menu: "Analyze",
        shortcut: None,
        args: &[],
        enabled: always,
        run: siphons,
        journal: true,
    },
    CommandSpec {
        id: "analysis.validate",
        label: "Validate",
        help: "pflow-validation's structural checks (structure, connectivity, deadlocks, unboundedness, conservation) as a table, with a summary report. `reachability` also explores the state space (up to max_states).",
        menu: "Analyze",
        shortcut: None,
        args: &[
            opt("reachability", Bool, "also run a bounded reachability analysis (default false)"),
            opt("max_states", Int, "state limit for it, 1..200000 (default 10000)"),
        ],
        enabled: always,
        run: validate,
        journal: true,
    },
    CommandSpec {
        id: "analysis.canonical",
        label: "Canonical Id & Orbits",
        help: "An isomorphism-invariant id (equal for two nets that differ only by renaming) and the exact automorphism orbits, by individualization-refinement (a port of sim_canonical). One node set per non-trivial orbit. Refuses past a 1,000,000-step budget.",
        menu: "Analyze",
        shortcut: None,
        args: &[],
        enabled: always,
        run: canonical,
        journal: true,
    },
    CommandSpec {
        id: "analysis.classify",
        label: "Classify",
        help: "Classes of nodes the net cannot tell apart, by colour refinement (a port of sim_classify without its sampled experiments): quantitative colour → FungibleSet candidates, settled by the exact orbits when verify is on; structural colour → Roles. One node set per class.",
        menu: "Analyze",
        shortcut: None,
        args: &[
            opt("scope", Str, "\"all\" (places and transitions, default), \"places\", \"transitions\" or \"pools\" (sim's: conserved/bounded places marked initially)"),
            opt("verify", Bool, "settle fungible candidates by the exact orbit search (default true)"),
        ],
        enabled: always,
        run: classify,
        journal: true,
    },
];

fn source(id: &str, a: &Value) -> Step {
    Step {
        command: id.into(),
        args: a.clone(),
    }
}

fn structure(sess: &Session) -> Result<Structure> {
    Structure::new(&sess.doc, &sess.marking)
}

fn as_f64(m: &[Vec<i64>]) -> Vec<Vec<f64>> {
    m.iter()
        .map(|r| r.iter().map(|&x| x as f64).collect())
        .collect()
}

/// `{a, b, c}` over display names.
fn braces(names: &[String]) -> String {
    format!("{{{}}}", names.join(", "))
}

// ------------------------------------------------------------ incidence

fn incidence(sess: &mut Session, a: &Value) -> Result<Value> {
    let st = structure(sess)?;
    let c = st.incidence();
    let data = ArtifactData::Matrix {
        rows: st.display.clone(),
        cols: st.transitions.clone(),
        values: as_f64(&c),
    };
    let id = sess.add_artifact(
        "Incidence matrix".into(),
        source("analysis.incidence", a),
        data,
    );
    Ok(json!({
        "artifact": id,
        "rows": st.display,
        "cols": st.transitions,
        "incidence": c,
        "pre": st.pre,
        "post": st.post,
        "colored": st.is_colored(),
    }))
}

// ------------------------------------------------------------ invariants

/// Smallest support first, then by the support's position in display order.
fn order_basis(basis: &mut [Vec<i64>]) {
    basis.sort_by_key(|v| {
        let support: Vec<usize> = (0..v.len()).filter(|&i| v[i] != 0).collect();
        (support.len(), support, v.clone())
    });
}

fn term_list(names: &[String], y: &[i64]) -> Vec<(String, i64)> {
    (0..y.len())
        .filter(|&i| y[i] != 0)
        .map(|i| (names[i].clone(), y[i]))
        .collect()
}

fn sum_expr(terms: &[(String, i64)]) -> String {
    terms
        .iter()
        .map(|(n, c)| {
            if *c == 1 {
                n.clone()
            } else {
                format!("{c}·{n}")
            }
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn invariants(sess: &mut Session, a: &Value) -> Result<Value> {
    let kind = s(a, "kind").unwrap_or("both");
    let (want_p, want_t) = match kind {
        "p" => (true, false),
        "t" => (false, true),
        "both" => (true, true),
        other => {
            return Err(Error::BadArgs(format!(
                "kind {other:?}: one of \"p\", \"t\", \"both\""
            )))
        }
    };
    let st = structure(sess)?;
    let c = st.incidence();
    let now = st.marking(&sess.marking);
    let init = st.marking(&crate::sim::initial_marking(&sess.doc));
    let mut sets: Vec<NodeSet> = vec![];
    let mut out = json!({ "kind": kind });
    let mut matrices = vec![];

    if want_p {
        let mut r = farkas(&c, MAX_FARKAS_ROWS);
        order_basis(&mut r.basis);
        let mut covered = vec![false; st.places.len()];
        let mut list = vec![];
        for y in &r.basis {
            let terms = term_list(&st.display, y);
            let value: i64 = y.iter().zip(&now).map(|(c, m)| c * m).sum();
            let initial: i64 = y.iter().zip(&init).map(|(c, m)| c * m).sum();
            let law = Law {
                terms: terms.clone(),
                value,
            };
            for (i, &c) in y.iter().enumerate() {
                covered[i] |= c > 0;
            }
            let support: Vec<String> = (0..y.len())
                .filter(|&i| y[i] != 0)
                .map(|i| st.places[i].clone())
                .collect();
            let nodes = st.doc_nodes(&support);
            let mut note = format!(
                "P-invariant: {} is the same at every marking reachable from here",
                sum_expr(&terms)
            );
            if initial != value {
                note += &format!(" (it was {initial} at the initial marking)");
            }
            sets.push(NodeSet {
                label: law.expression(),
                nodes: nodes.clone(),
                weights: terms.iter().map(|(n, c)| (n.clone(), *c as f64)).collect(),
                note,
            });
            list.push(json!({
                "expression": law.expression(),
                "coefficients": terms.iter().cloned().collect::<BTreeMap<String, i64>>(),
                "value": value,
                "initial_value": initial,
                "nodes": nodes,
            }));
        }
        let uncovered: Vec<&String> = (0..covered.len())
            .filter(|&i| !covered[i])
            .map(|i| &st.display[i])
            .collect();
        let bounded = uncovered.is_empty();
        let m = sess.add_artifact(
            "P-invariant basis".into(),
            source("analysis.invariants", a),
            ArtifactData::Matrix {
                rows: (1..=r.basis.len()).map(|i| format!("P{i}")).collect(),
                cols: st.display.clone(),
                values: as_f64(&r.basis),
            },
        );
        matrices.push(m.clone());
        out["p_invariants"] = json!(list);
        out["p_covered"] = json!(bounded);
        out["uncovered_places"] = json!(uncovered);
        out["structurally_bounded"] = json!(bounded);
        out["p_truncated"] = json!(r.truncated);
        out["p_basis"] = json!(m);
        out["p_summary"] = json!(if bounded && !st.places.is_empty() {
            "every place is covered by a P-invariant, so the net is structurally bounded (bounded from any initial marking)".to_string()
        } else if st.places.is_empty() {
            "no token places".to_string()
        } else {
            format!(
                "not covered by any P-invariant: {} — nothing structural bounds {}",
                uncovered
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                if uncovered.len() == 1 { "it" } else { "them" }
            )
        });
    }

    if want_t {
        let ct = transpose(&c, st.transitions.len());
        let mut r = farkas(&ct, MAX_FARKAS_ROWS);
        order_basis(&mut r.basis);
        let mut covered = vec![false; st.transitions.len()];
        let mut list = vec![];
        for x in &r.basis {
            let terms = term_list(&st.transitions, x);
            for (i, &c) in x.iter().enumerate() {
                covered[i] |= c > 0;
            }
            let nodes: Vec<String> = terms.iter().map(|(t, _)| t.clone()).collect();
            let expr = sum_expr(&terms);
            sets.push(NodeSet {
                label: format!("T-invariant {expr}"),
                nodes: nodes.clone(),
                weights: terms.iter().map(|(n, c)| (n.clone(), *c as f64)).collect(),
                note: format!(
                    "firing {expr} returns every place to its count — from the incidence matrix alone, \
                     not a claim that some order of those firings is enabled"
                ),
            });
            list.push(json!({
                "expression": expr,
                "counts": terms.iter().cloned().collect::<BTreeMap<String, i64>>(),
                "transitions": nodes,
            }));
        }
        let uncovered: Vec<&String> = (0..covered.len())
            .filter(|&i| !covered[i])
            .map(|i| &st.transitions[i])
            .collect();
        let m = sess.add_artifact(
            "T-invariant basis".into(),
            source("analysis.invariants", a),
            ArtifactData::Matrix {
                rows: (1..=r.basis.len()).map(|i| format!("T{i}")).collect(),
                cols: st.transitions.clone(),
                values: as_f64(&r.basis),
            },
        );
        matrices.push(m.clone());
        out["t_invariants"] = json!(list);
        out["t_covered"] = json!(uncovered.is_empty());
        out["uncovered_transitions"] = json!(uncovered);
        out["t_truncated"] = json!(r.truncated);
        out["t_basis"] = json!(m);
        out["t_summary"] = json!(if uncovered.is_empty() && !st.transitions.is_empty() {
            "every transition is in some T-invariant (the net is consistent, as a live and bounded net must be)".to_string()
        } else if st.transitions.is_empty() {
            "no transitions".to_string()
        } else {
            format!(
                "in no T-invariant: {} — a live and bounded net cannot have such transitions",
                uncovered
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        });
    }

    let title = match kind {
        "p" => "P-invariants",
        "t" => "T-invariants",
        _ => "P/T invariants",
    };
    let id = sess.add_artifact(
        title.into(),
        source("analysis.invariants", a),
        ArtifactData::Sets { sets },
    );
    out["sets"] = json!(id);
    out["artifact"] = json!(id);
    out["matrices"] = json!(matrices);
    Ok(out)
}

// ------------------------------------------------------------ siphons

fn siphons(sess: &mut Session, a: &Value) -> Result<Value> {
    let st = structure(sess)?;
    let np = st.places.len();
    let nt = st.transitions.len();
    let pidx: BTreeMap<&str, usize> = st
        .places
        .iter()
        .enumerate()
        .map(|(i, p)| (p.as_str(), i))
        .collect();
    let mut io = Io {
        places: np,
        produces: vec![0; nt],
        needs: vec![0; nt],
    };
    if np <= crate::analysis::siphons::PLACE_CAP {
        for (j, t) in st.transitions.iter().enumerate() {
            for p in 0..np {
                if st.post[p][j] > 0 {
                    io.produces[j] |= bit(p);
                }
                if st.pre[p][j] > 0 {
                    io.needs[j] |= bit(p);
                }
            }
            for r in st.model().tests(t) {
                if r.typ == pflow_metamodel::ArcType::Read {
                    if let Some(&p) = pidx.get(r.place.as_str()) {
                        io.needs[j] |= bit(p);
                    }
                }
            }
        }
    }
    let found = io.minimal_sets(&st.display).map_err(Error::Invalid)?;
    let marked: Mask = st
        .marking(&sess.marking)
        .iter()
        .enumerate()
        .filter(|(_, &v)| v > 0)
        .fold(0, |m, (i, _)| m | bit(i));
    let names = |m: Mask| -> Vec<String> {
        members(m)
            .into_iter()
            .map(|i| st.display[i].clone())
            .collect()
    };
    let ids = |m: Mask| -> Vec<String> {
        let model: Vec<String> = members(m)
            .into_iter()
            .map(|i| st.places[i].clone())
            .collect();
        st.doc_nodes(&model)
    };

    let mut sets = vec![];
    let mut siphon_rows = vec![];
    let mut witnesses = vec![];
    let mut commoner = true;
    let mut no_marked_trap = vec![];
    for &s in &found.siphons {
        let trap = io.max_trap_within(s);
        let trap_marked = trap & marked != 0;
        let empty = s & marked == 0;
        commoner &= trap_marked;
        if !trap_marked {
            no_marked_trap.push(braces(&names(s)));
        }
        let dead: Vec<String> = (0..nt)
            .filter(|&j| io.needs[j] & s != 0)
            .map(|j| st.transitions[j].clone())
            .collect();
        let mut note = if trap_marked {
            format!("contains the marked trap {}", braces(&names(trap)))
        } else if trap != 0 {
            format!(
                "its largest trap {} is unmarked, so it may still empty",
                braces(&names(trap))
            )
        } else {
            "contains no trap, so it may still empty".to_string()
        };
        if empty {
            note = format!(
                "EMPTY at the current marking: it can never gain a token again, so {} can never fire again; {note}",
                if dead.is_empty() { "nothing".to_string() } else { dead.join(", ") }
            );
        }
        sets.push(NodeSet {
            label: format!("siphon {}", braces(&names(s))),
            nodes: ids(s),
            weights: BTreeMap::new(),
            note,
        });
        let row = json!({
            "places": names(s),
            "nodes": ids(s),
            "marked": !empty,
            "max_trap": names(trap),
            "contains_marked_trap": trap_marked,
            "unfireable_transitions": if empty { json!(dead) } else { json!([]) },
        });
        if empty {
            let mut nodes = ids(s);
            nodes.extend(dead.iter().cloned());
            sets.push(NodeSet {
                label: format!("deadlock witness {}", braces(&names(s))),
                nodes,
                weights: BTreeMap::new(),
                note: format!(
                    "{} is a siphon holding no tokens, so {} stay disabled at every marking reachable from here. \
                     Other transitions may keep firing.",
                    braces(&names(s)),
                    dead.join(", ")
                ),
            });
            witnesses.push(json!({
                "siphon": names(s),
                "unfireable_transitions": dead,
            }));
        }
        siphon_rows.push(row);
    }
    let mut trap_rows = vec![];
    for &t in &found.traps {
        let is_marked = t & marked != 0;
        sets.push(NodeSet {
            label: format!("trap {}", braces(&names(t))),
            nodes: ids(t),
            weights: BTreeMap::new(),
            note: if is_marked {
                "marked: it stays marked at every marking reachable from here".into()
            } else {
                "unmarked at the current marking".into()
            },
        });
        trap_rows.push(json!({ "places": names(t), "nodes": ids(t), "marked": is_marked }));
    }

    let ordinary = st.is_ordinary();
    let (fc, efc) = st.free_choice();
    let verdict = if nt == 0 {
        "no transitions: nothing can fire".to_string()
    } else if !witnesses.is_empty() {
        format!(
            "{} empty siphon(s) at the current marking: the transitions they feed are dead forever",
            witnesses.len()
        )
    } else if commoner && ordinary && efc {
        "live: every minimal siphon contains a marked trap and the net is ordinary and (extended) free-choice (Commoner–Hack theorem)".to_string()
    } else if commoner && ordinary {
        "deadlock-free: every minimal siphon contains a marked trap in an ordinary net (Commoner's property), so no reachable marking is dead".to_string()
    } else if !commoner && ordinary && efc {
        format!(
            "not live: {} contain(s) no marked trap in an ordinary free-choice net (Commoner–Hack theorem)",
            no_marked_trap.join(", ")
        )
    } else if commoner {
        "every minimal siphon contains a marked trap, but the net is not ordinary (weighted, read or inhibitor arcs), so that certifies nothing".to_string()
    } else {
        format!(
            "inconclusive: {} contain(s) no marked trap, which may (but need not) empty",
            no_marked_trap.join(", ")
        )
    };
    let report = json!({
        "method": "structural proof: siphons and traps from the arc structure alone (branch and bound, a port of sim's siphons.go)",
        "minimal_siphons": siphon_rows,
        "minimal_traps": trap_rows,
        "deadlock_witnesses": witnesses,
        "commoner": commoner,
        "ordinary": ordinary,
        "free_choice": fc,
        "extended_free_choice": efc,
        "verdict": verdict,
        "marking": "current",
    });
    let set_id = sess.add_artifact(
        "Siphons and traps".into(),
        source("analysis.siphons", a),
        ArtifactData::Sets { sets },
    );
    let rep_id = sess.add_artifact(
        "Siphons and traps (report)".into(),
        source("analysis.siphons", a),
        ArtifactData::Report(report.clone()),
    );
    let mut out = report;
    out["sets"] = json!(set_id);
    out["report"] = json!(rep_id);
    out["artifacts"] = json!([set_id, rep_id]);
    Ok(out)
}

// ------------------------------------------------------------ validate

fn validate(sess: &mut Session, a: &Value) -> Result<Value> {
    let st = structure(sess)?;
    let reach = b(a, "reachability").unwrap_or(false);
    let max_states = n(a, "max_states").unwrap_or(10_000.0);
    if !(1.0..=MAX_VALIDATE_STATES as f64).contains(&max_states) {
        return Err(Error::BadArgs(format!(
            "max_states {max_states} is outside 1..{MAX_VALIDATE_STATES}"
        )));
    }
    let v = pflow_validation::Validator::new(st.model());
    let res = if reach {
        v.validate_with_reachability(max_states as usize)
    } else {
        v.validate()
    };
    let doc_ids = |loc: &[String]| -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for l in loc {
            let id = if st.model().place_by_id(l).is_some()
                || st.model().transition_by_id(l).is_some()
            {
                st.node(l).0
            } else {
                l.clone()
            };
            if !out.contains(&id) {
                out.push(id);
            }
        }
        out
    };
    let mut rows = vec![];
    let mut issues = vec![];
    for i in res.errors.iter().chain(&res.warnings).chain(&res.info) {
        let nodes = doc_ids(&i.location);
        rows.push(vec![
            json!(i.severity.as_str()),
            json!(i.category),
            json!(i.message),
            json!(nodes.join(", ")),
            json!(i.suggestion),
        ]);
        issues.push(json!({
            "severity": i.severity.as_str(),
            "check": i.category,
            "message": i.message,
            "nodes": nodes,
            "suggestion": i.suggestion,
        }));
    }
    let mut report = json!({
        "valid": res.valid,
        "errors": res.errors.len(),
        "warnings": res.warnings.len(),
        "info": res.info.len(),
        "summary": {
            "places": res.summary.places,
            "transitions": res.summary.transitions,
            "arcs": res.summary.arcs,
            "conserved": res.summary.conserved,
        },
        "invariants": res.invariants,
        "method": "pflow-validation (go-pflow's validation checks)",
    });
    if let Some(r) = &res.reachability {
        report["reachability"] = json!({
            "reachable": r.reachable,
            "bounded": r.bounded,
            "deadlock_states": r.deadlock_states.len(),
            "terminal_states": r.terminal_states.len(),
            "has_cycles": r.has_cycles,
            "max_depth": r.max_depth,
            "truncated": r.truncated,
            "truncated_reason": r.truncated_reason,
            "max_tokens": r.max_tokens.iter().collect::<BTreeMap<_, _>>(),
        });
    }
    let table = sess.add_artifact(
        "Validation".into(),
        source("analysis.validate", a),
        ArtifactData::Table {
            columns: ["severity", "check", "message", "nodes", "suggestion"]
                .map(String::from)
                .to_vec(),
            rows,
        },
    );
    let rep = sess.add_artifact(
        "Validation (summary)".into(),
        source("analysis.validate", a),
        ArtifactData::Report(report.clone()),
    );
    let mut out = report;
    out["issues"] = json!(issues);
    out["table"] = json!(table);
    out["report"] = json!(rep);
    out["artifacts"] = json!([table, rep]);
    Ok(out)
}

// ------------------------------------------------------------ symmetry

/// The model nodes the symmetry analyses run over: every transition, every
/// non-token place, and the token places [`Structure`] keeps (all of them
/// on a single-colour net, so the seeds are sim's).
fn keep_fn(st: &Structure) -> impl Fn(&str) -> bool + '_ {
    let token: HashSet<&str> = st.places.iter().map(String::as_str).collect();
    move |id: &str| token.contains(id) || st.model().place_by_id(id).is_some_and(|p| !p.is_token())
}

fn budget_error(e: symmetry::BudgetExceeded) -> Error {
    Error::Invalid(format!(
        "orbits: {e}. refine.* tags that tell symmetric elements apart shrink the search"
    ))
}

fn canonical(sess: &mut Session, a: &Value) -> Result<Value> {
    let st = structure(sess)?;
    let inc = Incidence::build(st.model(), true, keep_fn(&st), &sess.rates);
    let o = inc.orbits(WORK_BUDGET).map_err(budget_error)?;
    let display = |ids: &[String]| -> Vec<String> { ids.iter().map(|id| st.node(id).1).collect() };
    let mut sets = vec![];
    let mut orbits = vec![];
    for orb in &o.orbits {
        let names = display(orb);
        let places = orb
            .iter()
            .filter(|id| st.model().place_by_id(id).is_some())
            .count();
        let what = match (places, orb.len() - places) {
            (p, 0) => format!("{p} places"),
            (0, t) => format!("{t} transitions"),
            (p, t) => format!("{p} places and {t} transitions"),
        };
        sets.push(NodeSet {
            label: format!("orbit {}", braces(&names)),
            nodes: st.doc_nodes(orb),
            weights: BTreeMap::new(),
            note: format!(
                "{what} some automorphism of the net maps onto each other (preserving arcs, weights, \
                 arc kinds, markings, capacities and rates)"
            ),
        });
        orbits.push(names);
    }
    let rigid = o.generators == 0;
    let report = json!({
        "canonical_id": o.canonical_id,
        "orbits": orbits,
        "generators": o.generators,
        "rigid": rigid,
        "leaves": o.leaves,
        "work": o.work,
        "budget": WORK_BUDGET,
        "nodes": inc.nodes.len(),
        "method": symmetry::METHOD,
        "summary": if rigid {
            "rigid: no automorphism but the identity, so every node is told apart".to_string()
        } else {
            format!("{} non-trivial orbit(s) under {} recorded automorphism(s)", orbits.len(), o.generators)
        },
        "session_rates": !sess.rates.is_empty(),
    });
    let set_id = sess.add_artifact(
        "Automorphism orbits".into(),
        source("analysis.canonical", a),
        ArtifactData::Sets { sets },
    );
    let rep_id = sess.add_artifact(
        "Canonical id".into(),
        source("analysis.canonical", a),
        ArtifactData::Report(report.clone()),
    );
    let mut out = report;
    out["sets"] = json!(set_id);
    out["report"] = json!(rep_id);
    out["artifacts"] = json!([set_id, rep_id]);
    Ok(out)
}

fn classify(sess: &mut Session, a: &Value) -> Result<Value> {
    let scope = s(a, "scope").unwrap_or("all");
    let verify = b(a, "verify").unwrap_or(true);
    let st = structure(sess)?;
    let model = st.model();
    let ids: Vec<String> = match scope {
        "all" => st.places.iter().chain(&st.transitions).cloned().collect(),
        "places" => st.places.clone(),
        "transitions" => st.transitions.clone(),
        "pools" => {
            let kinds = pflow_solver::stochastic::classify_supply(model);
            st.places
                .iter()
                .filter(|p| {
                    kinds.get(*p).is_some_and(|k| k.is_capacity())
                        && model.place_by_id(p).is_some_and(|q| q.initial > 0)
                })
                .cloned()
                .collect()
        }
        other => {
            return Err(Error::BadArgs(format!(
                "scope {other:?}: one of \"all\", \"places\", \"transitions\", \"pools\""
            )))
        }
    };
    let quant = Incidence::build(model, true, keep_fn(&st), &sess.rates);
    let structural = Incidence::build(model, false, keep_fn(&st), &sess.rates);
    let (orbits, orbit_status) = if verify {
        match quant.orbits(WORK_BUDGET) {
            Ok(o) => (Some(o), "exact orbits computed".to_string()),
            Err(e) => (
                None,
                format!("exact orbit search refused ({e}); classes stay candidates"),
            ),
        }
    } else {
        (
            None,
            "not run (verify=false); fungible classes are candidates".to_string(),
        )
    };
    let classes = symmetry::classes(&ids, &quant, &structural, orbits.as_ref());
    let display = |ids: &[String]| -> Vec<String> { ids.iter().map(|id| st.node(id).1).collect() };
    let mut sets = vec![];
    let mut rows = vec![];
    for c in &classes {
        let names = display(&c.members);
        let status = if c.verified {
            "proved"
        } else if c.kind == "FungibleSet" {
            "candidate"
        } else {
            "role"
        };
        sets.push(NodeSet {
            label: format!("{} {}: {}", c.kind, c.id, names.join(", ")),
            nodes: st.doc_nodes(&c.members),
            weights: BTreeMap::new(),
            note: format!("{status} — {} ({})", c.evidence, c.method),
        });
        rows.push(json!({
            "id": c.id,
            "kind": c.kind,
            "members": names,
            "method": c.method,
            "verified": c.verified,
            "split": c.split.iter().map(|g| display(g)).collect::<Vec<_>>(),
            "evidence": c.evidence,
        }));
    }
    let report = json!({
        "scope": scope,
        "classes": rows,
        "orbits": orbit_status,
        "method": [
            "colour-refinement/1-WL (quantitative): nodes the net cannot distinguish, given levels, rates and arc kinds",
            "colour-refinement/1-WL (structural): same position in the net, ignoring levels and rates",
            symmetry::METHOD,
        ],
    });
    let set_id = sess.add_artifact(
        "Classes".into(),
        source("analysis.classify", a),
        ArtifactData::Sets { sets },
    );
    let rep_id = sess.add_artifact(
        "Classes (report)".into(),
        source("analysis.classify", a),
        ArtifactData::Report(report.clone()),
    );
    let mut out = report;
    out["sets"] = json!(set_id);
    out["report"] = json!(rep_id);
    out["artifacts"] = json!([set_id, rep_id]);
    Ok(out)
}
