//! `store.*`: the local content-addressed store ([`crate::store`]) —
//! models by CID, the knowledge graph (link / edges / neighbors / query),
//! collections, licenses, versioning and content-addressed tables. sim's
//! `create_model` … `map_list` family, against a store on this machine.
//!
//! The store is per-user and outlives the session, so it is reached
//! through [`crate::store::open`], not a `Session` field (see that module).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde_json::{json, Value};

use crate::artifact::{ArtifactData, GraphEdge, GraphNode};
use crate::doc::Document;
use crate::registry::{always, arg, b, n, opt, s, strs, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::store::{self, predicate, Snapshot};
use crate::{Error, Result};

const MENU: &str = "Store";

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "store.put",
        label: "Put in Store…",
        help:
            "Seal a model (its pflow.xyz CID) and keep it in the local store. Idempotent by CID: \
               storing the same net again returns existed: true and keeps the first metadata. \
               `parents` records provenance (defaults to the document's own `parents`).",
        menu: MENU,
        shortcut: None,
        args: &[
            opt(
                "model",
                Str,
                "\"current\" (default) or a library model name",
            ),
            opt(
                "title",
                Str,
                "title for listings (defaults to the net's name)",
            ),
            opt(
                "parents",
                StrList,
                "CIDs this model was derived from, nearest first",
            ),
        ],
        enabled: always,
        run: put,
        journal: true,
    },
    CommandSpec {
        id: "store.get",
        label: "Get Stored Model",
        help: "A stored model's pflow.xyz JSON-LD (with @id = its CID).",
        menu: "",
        shortcut: None,
        args: &[arg("cid", Str, "model CID")],
        enabled: always,
        run: get,
        journal: false,
    },
    CommandSpec {
        id: "store.open",
        label: "Open from Store…",
        help: "Make a stored model the open net (undoable; the net has no file path).",
        menu: MENU,
        shortcut: None,
        args: &[arg("cid", Str, "model CID")],
        enabled: always,
        run: open,
        journal: true,
    },
    CommandSpec {
        id: "store.list",
        label: "List Stored Models",
        help: "Stored models, newest first, with title, size, created (ms), collections, license \
               and successor. `query` filters by title or CID substring. Stores a table artifact.",
        menu: MENU,
        shortcut: None,
        args: &[
            opt(
                "query",
                Str,
                "case-insensitive substring of the title or CID",
            ),
            opt("limit", Int, "at most this many rows (default 100)"),
        ],
        enabled: always,
        run: list,
        journal: false,
    },
    CommandSpec {
        id: "store.delete",
        label: "Delete from Store",
        help: "Remove a stored model. Refused for a licensed model (irrevocable) and, unless \
               force, for one any edge names; edges are kept either way.",
        menu: "",
        shortcut: None,
        args: &[
            arg("cid", Str, "model CID"),
            opt("force", Bool, "delete even though edges name it"),
        ],
        enabled: always,
        run: delete,
        journal: true,
    },
    CommandSpec {
        id: "store.link",
        label: "Link…",
        help: "Record a typed edge between stored entities (sim_link): predicate from sim's \
               registry (@type, about, broader, cites, closeMatch, hasMember, related), both ends \
               stored entities of the kinds it relates. Re-linking returns the existing edge.",
        menu: MENU,
        shortcut: None,
        args: &[
            arg("subject", Str, "id of the entity the edge starts from"),
            arg(
                "predicate",
                Str,
                "registered predicate (store.info lists them)",
            ),
            arg(
                "object",
                Str,
                "id of the entity the edge points to; for @type a model kind",
            ),
        ],
        enabled: always,
        run: link,
        journal: true,
    },
    CommandSpec {
        id: "store.edges",
        label: "Edges",
        help: "Every relation touching an entity, as subject or object, ordered by time. \
               Stores a table artifact.",
        menu: "",
        shortcut: None,
        args: &[
            arg("id", Str, "entity id"),
            opt("predicate", Str, "only this predicate's edges"),
        ],
        enabled: always,
        run: edges,
        journal: false,
    },
    CommandSpec {
        id: "store.neighbors",
        label: "Neighbors",
        help:
            "One hop from an entity through the registry (sim_neighbors): no predicate = objects \
               of its outgoing edges; an inverse name (narrower) reads its stored predicate \
               backwards; a symmetric one counts both ways. direction: out (default), in, both.",
        menu: "",
        shortcut: None,
        args: &[
            arg("id", Str, "entity id"),
            opt("predicate", Str, "restrict to this predicate"),
            opt("direction", Str, "out | in | both"),
        ],
        enabled: always,
        run: neighbors,
        journal: false,
    },
    CommandSpec {
        id: "store.query",
        label: "Query Graph…",
        help: "Conjunctive pattern query over the edges (sim_query): {select, where: [{subject, \
               predicate, object, mode: edge|path|closure, maxHops}]}, ?name for variables; no \
               variables = a yes/no question. Stores table and graph artifacts.",
        menu: MENU,
        shortcut: None,
        args: &[arg(
            "pattern",
            Json,
            "the query: {select?, where}, a pattern list, or its JSON text",
        )],
        enabled: always,
        run: query,
        journal: false,
    },
    CommandSpec {
        id: "store.collection_create",
        label: "New Collection…",
        help: "Create a named collection (the name is its content id, so creating it twice \
               returns the same collection).",
        menu: MENU,
        shortcut: None,
        args: &[arg("name", Str, "collection name")],
        enabled: always,
        run: collection_create,
        journal: true,
    },
    CommandSpec {
        id: "store.collection_add",
        label: "Add to Collection…",
        help: "Add a stored entity to a collection (a hasMember edge).",
        menu: MENU,
        shortcut: None,
        args: &[
            arg("collection", Str, "collection id or name"),
            arg("cid", Str, "model CID (or any stored entity id)"),
        ],
        enabled: always,
        run: collection_add,
        journal: true,
    },
    CommandSpec {
        id: "store.collection_list",
        label: "List Collections",
        help: "Collections with their member counts. Stores a table artifact.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: collection_list,
        journal: false,
    },
    CommandSpec {
        id: "store.license",
        label: "License…",
        help: "Dedicate a stored model to the commons (CC0-1.0, CC-BY-4.0, CC-BY-SA-4.0). \
               Irrevocable: a licensed model cannot be relicensed or deleted.",
        menu: MENU,
        shortcut: None,
        args: &[
            arg("cid", Str, "model CID"),
            arg("license", Str, "CC0-1.0 | CC-BY-4.0 | CC-BY-SA-4.0"),
        ],
        enabled: always,
        run: license,
        journal: true,
    },
    CommandSpec {
        id: "store.supersede",
        label: "Supersede…",
        help: "Record that `new` replaces `old` (a supersededBy edge). Old edges stay; \
               store.migration_offers lists those that can be carried forward.",
        menu: MENU,
        shortcut: None,
        args: &[
            arg("old", Str, "the superseded model's CID"),
            arg("new", Str, "its successor's CID"),
        ],
        enabled: always,
        run: supersede,
        journal: true,
    },
    CommandSpec {
        id: "store.migration_offers",
        label: "Migration Offers",
        help: "What supersession does to each edge of a superseded model: taxonomic edges are \
               offered for the current successor (with the link check on the proposed edge); \
               tool-written and structural edges stay. Stores a table artifact.",
        menu: "",
        shortcut: None,
        args: &[
            arg("cid", Str, "the superseded model's CID"),
            opt("predicate", Str, "only this predicate's edges"),
        ],
        enabled: always,
        run: migration_offers,
        journal: false,
    },
    CommandSpec {
        id: "store.accept_migration",
        label: "Accept Migration…",
        help: "Carry offered edges of a superseded model to its current successor: each becomes \
               the same edge on the successor plus a migratedFrom edge back (the old edge stays). \
               All-or-nothing; `edges` defaults to every pending offer.",
        menu: MENU,
        shortcut: None,
        args: &[
            arg("old", Str, "the superseded model's CID"),
            opt(
                "new",
                Str,
                "the successor you read the offers against (checked)",
            ),
            opt(
                "edges",
                StrList,
                "relation ids from store.migration_offers (1–25)",
            ),
        ],
        enabled: always,
        run: accept_migration,
        journal: true,
    },
    CommandSpec {
        id: "store.map_put",
        label: "Put Table…",
        help: "Store a key→value table; its id is the hash of its canonical JSON (sim's map id), \
               so the same data in any key order is the same map.",
        menu: "",
        shortcut: None,
        args: &[arg("data", Json, "a JSON object (or its JSON text)")],
        enabled: always,
        run: map_put,
        journal: true,
    },
    CommandSpec {
        id: "store.map_get",
        label: "Get Table",
        help: "A stored table's data by id.",
        menu: "",
        shortcut: None,
        args: &[arg("id", Str, "map id")],
        enabled: always,
        run: map_get,
        journal: false,
    },
    CommandSpec {
        id: "store.map_list",
        label: "List Tables",
        help: "The id and key count of every stored table. Stores a table artifact.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: map_list,
        journal: false,
    },
    CommandSpec {
        id: "store.graph",
        label: "Knowledge Graph",
        help: "The store as a graph: models, collections and the edges between them (plus \
               provenance parents). `around` limits it to `depth` hops (default 2) of one entity.",
        menu: MENU,
        shortcut: None,
        args: &[
            opt("around", Str, "entity id (or collection name) to centre on"),
            opt("depth", Int, "hops from `around` (default 2)"),
        ],
        enabled: always,
        run: graph,
        journal: false,
    },
    CommandSpec {
        id: "store.info",
        label: "Store Info",
        help: "Where the store lives, what it holds, and the predicate registry.",
        menu: MENU,
        shortcut: None,
        args: &[],
        enabled: always,
        run: info,
        journal: false,
    },
];

fn src(id: &str, a: &Value) -> Step {
    Step {
        command: id.into(),
        args: a.clone(),
    }
}

fn table(
    sess: &mut Session,
    title: &str,
    step: Step,
    columns: &[&str],
    rows: Vec<Vec<Value>>,
) -> String {
    sess.add_artifact(
        title.into(),
        step,
        ArtifactData::Table {
            columns: columns.iter().map(|c| c.to_string()).collect(),
            rows,
        },
    )
}

/// A short label for an id: a model's title, a collection's name.
fn label(snap: &Snapshot, id: &str) -> String {
    if let Some(m) = snap.models.get(id) {
        if !m.title.is_empty() {
            return m.title.clone();
        }
    }
    if let Some(c) = snap.collections.get(id) {
        return c.name.clone();
    }
    if id.len() > 12 {
        format!("{}…", &id[..10])
    } else {
        id.to_string()
    }
}

fn put(sess: &mut Session, a: &Value) -> Result<Value> {
    let doc = super::models::resolve(sess, s(a, "model").unwrap_or("current"))?;
    store::open().put_model(&doc, s(a, "title"), strs(a, "parents"))
}

fn get(_: &mut Session, a: &Value) -> Result<Value> {
    store::open().get_model(s(a, "cid").unwrap())
}

fn open(sess: &mut Session, a: &Value) -> Result<Value> {
    let cid = s(a, "cid").unwrap();
    let v = store::open().get_model(cid)?;
    let doc = Document::from_value(&v)?;
    let out = json!({
        "cid": cid,
        "places": doc.places.len(),
        "transitions": doc.transitions.len(),
        "arcs": doc.arcs.len(),
    });
    super::file::replace(sess, doc)?;
    sess.path = None;
    Ok(out)
}

fn list(sess: &mut Session, a: &Value) -> Result<Value> {
    let snap = store::open().snapshot()?;
    let q = s(a, "query").unwrap_or("").to_lowercase();
    let limit = n(a, "limit").map_or(100, |l| l.max(0.0) as usize);
    let mut all: Vec<(&String, &store::ModelMeta)> = snap
        .models
        .iter()
        .filter(|(cid, m)| {
            q.is_empty() || cid.to_lowercase().contains(&q) || m.title.to_lowercase().contains(&q)
        })
        .collect();
    all.sort_by(|x, y| y.1.created.cmp(&x.1.created).then(x.0.cmp(y.0)));
    let total = all.len();
    let rows: Vec<Value> = all
        .into_iter()
        .take(limit)
        .map(|(cid, m)| {
            json!({
                "cid": cid,
                "title": m.title,
                "places": m.places,
                "transitions": m.transitions,
                "created": m.created,
                "collections": snap.collections_of(cid),
                "license": m.license,
                "superseded_by": m.superseded_by,
                "parents": m.parents,
            })
        })
        .collect();
    let cols = [
        "cid",
        "title",
        "places",
        "transitions",
        "created",
        "collections",
        "license",
        "superseded_by",
    ];
    let trows = rows
        .iter()
        .map(|r| {
            cols.iter()
                .map(|c| match &r[*c] {
                    Value::Array(v) => json!(v
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")),
                    v => v.clone(),
                })
                .collect()
        })
        .collect();
    let art = table(sess, "Stored models", src("store.list", a), &cols, trows);
    Ok(json!({"rows": rows, "total": total, "location": store::open().location(), "artifact": art}))
}

fn delete(_: &mut Session, a: &Value) -> Result<Value> {
    store::open().delete_model(s(a, "cid").unwrap(), b(a, "force").unwrap_or(false))
}

fn link(_: &mut Session, a: &Value) -> Result<Value> {
    store::open().link(
        s(a, "subject").unwrap(),
        s(a, "predicate").unwrap(),
        s(a, "object").unwrap(),
    )
}

fn edges(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = s(a, "id").unwrap();
    let list = store::open().edges(id, s(a, "predicate").filter(|p| !p.is_empty()))?;
    let cols = ["id", "subject", "predicate", "object", "axis", "at"];
    let rows = list
        .iter()
        .map(|e| cols.iter().map(|c| e[*c].clone()).collect())
        .collect();
    let art = table(
        sess,
        &format!("Edges of {id}"),
        src("store.edges", a),
        &cols,
        rows,
    );
    Ok(json!({"id": id, "edges": list, "artifact": art}))
}

fn neighbors(_: &mut Session, a: &Value) -> Result<Value> {
    Ok(json!(store::open().neighbors(
        s(a, "id").unwrap(),
        s(a, "predicate"),
        s(a, "direction"),
    )?))
}

fn query(sess: &mut Session, a: &Value) -> Result<Value> {
    let snap = store::open().snapshot()?;
    let ans = store::query::run(&snap, &a["pattern"])?;
    let step = src("store.query", a);
    let mut out = ans.json;
    if !ans.select.is_empty() {
        let mut cols: Vec<&str> = ans.select.iter().map(String::as_str).collect();
        cols.push("support");
        let rows_json = out
            .get("rows")
            .or_else(|| out.get("partialRows"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let rows = ans
            .rows
            .iter()
            .zip(&rows_json)
            .map(|(vals, rj)| {
                let mut r: Vec<Value> = vals.iter().map(|v| json!(v)).collect();
                let edges: Vec<String> = rj["support"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|s| s["edges"].as_array().cloned().unwrap_or_default())
                    .filter_map(|e| e.as_str().map(str::to_string))
                    .collect();
                r.push(json!(edges.join(" ")));
                r
            })
            .collect();
        out["table"] = json!(table(sess, "Store query", step.clone(), &cols, rows));
    }
    // The subgraph the answer rests on.
    let mut ids: BTreeSet<String> = ans.rows.iter().flatten().cloned().collect();
    let mut gedges = vec![];
    for eid in &ans.used {
        if let Some(r) = snap.relations.get(eid) {
            ids.insert(r.subject.clone());
            ids.insert(node_id(r));
            gedges.push(edge_of(eid, r));
        }
    }
    let nodes = ids.iter().map(|id| node(&snap, id)).collect();
    out["graph"] = json!(sess.add_artifact(
        "Store query graph".into(),
        step,
        ArtifactData::Graph {
            nodes,
            edges: gedges
        },
    ));
    Ok(out)
}

fn collection_create(_: &mut Session, a: &Value) -> Result<Value> {
    store::open().create_collection(s(a, "name").unwrap())
}

fn collection_add(_: &mut Session, a: &Value) -> Result<Value> {
    let st = store::open();
    let col = st.resolve_collection(s(a, "collection").unwrap())?;
    st.link(&col, predicate::HAS_MEMBER, s(a, "cid").unwrap())
}

fn collection_list(sess: &mut Session, a: &Value) -> Result<Value> {
    let snap = store::open().snapshot()?;
    let rows: Vec<Value> = snap
        .collections
        .iter()
        .map(|(id, c)| {
            let members: Vec<&String> = snap
                .relations
                .values()
                .filter(|r| r.predicate == predicate::HAS_MEMBER && &r.subject == id)
                .map(|r| &r.object)
                .collect();
            json!({"id": id, "name": c.name, "members": members.len(), "member_ids": members})
        })
        .collect();
    let trows = rows
        .iter()
        .map(|r| vec![r["id"].clone(), r["name"].clone(), r["members"].clone()])
        .collect();
    let art = table(
        sess,
        "Collections",
        src("store.collection_list", a),
        &["id", "name", "members"],
        trows,
    );
    Ok(json!({"collections": rows, "artifact": art}))
}

fn license(_: &mut Session, a: &Value) -> Result<Value> {
    store::open().set_license(s(a, "cid").unwrap(), s(a, "license").unwrap())
}

fn supersede(_: &mut Session, a: &Value) -> Result<Value> {
    store::open().supersede(s(a, "old").unwrap(), s(a, "new").unwrap())
}

fn migration_offers(sess: &mut Session, a: &Value) -> Result<Value> {
    let cid = s(a, "cid").unwrap();
    let mut out =
        store::open().migration_offers(cid, s(a, "predicate").filter(|p| !p.is_empty()))?;
    let cols = [
        "id",
        "subject",
        "predicate",
        "object",
        "disposition",
        "status",
        "check",
        "reason",
    ];
    let rows = out["edges"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| {
            cols.iter()
                .map(|c| match *c {
                    "check" => e["check"]["result"].clone(),
                    c => e[c].clone(),
                })
                .collect()
        })
        .collect();
    out["artifact"] = json!(table(
        sess,
        &format!("Migration offers for {cid}"),
        src("store.migration_offers", a),
        &cols,
        rows
    ));
    Ok(out)
}

fn accept_migration(_: &mut Session, a: &Value) -> Result<Value> {
    store::open().accept_migration(s(a, "old").unwrap(), s(a, "new"), strs(a, "edges"))
}

fn map_put(_: &mut Session, a: &Value) -> Result<Value> {
    let data = match &a["data"] {
        Value::String(t) => serde_json::from_str::<Value>(t)
            .map_err(|e| Error::BadArgs(format!("data is not a JSON object: {e}")))?,
        v => v.clone(),
    };
    store::open().put_map(&data)
}

fn map_get(_: &mut Session, a: &Value) -> Result<Value> {
    let id = s(a, "id").unwrap();
    Ok(json!({"id": id, "Data": store::open().get_map(id)?}))
}

fn map_list(sess: &mut Session, a: &Value) -> Result<Value> {
    let st = store::open();
    let mut rows = vec![];
    for id in st.list_maps()? {
        let keys = st.get_map(&id)?.as_object().map_or(0, |m| m.len());
        rows.push(json!({"id": id, "keys": keys}));
    }
    let trows = rows
        .iter()
        .map(|r| vec![r["id"].clone(), r["keys"].clone()])
        .collect();
    let art = table(
        sess,
        "Stored tables",
        src("store.map_list", a),
        &["id", "keys"],
        trows,
    );
    Ok(json!({"maps": rows, "artifact": art}))
}

/// The graph node an edge's object is: the entity, or a `type:<Kind>`
/// literal node for `@type`.
fn node_id(r: &store::Relation) -> String {
    if r.predicate == predicate::TYPE {
        format!("type:{}", r.object)
    } else {
        r.object.clone()
    }
}

fn edge_of(eid: &str, r: &store::Relation) -> GraphEdge {
    GraphEdge {
        source: r.subject.clone(),
        target: node_id(r),
        label: r.predicate.clone(),
        data: json!({"id": eid, "axis": predicate::axis_of(&r.predicate)}),
    }
}

fn node(snap: &Snapshot, id: &str) -> GraphNode {
    if let Some(kind) = id.strip_prefix("type:") {
        return GraphNode {
            id: id.into(),
            label: kind.into(),
            data: json!({"kind": "type"}),
        };
    }
    let kind = snap.kind_of(id).map_or("missing", |k| k.name());
    let mut data = json!({"kind": kind});
    if let Some(m) = snap.models.get(id) {
        data["license"] = json!(m.license);
        data["superseded_by"] = json!(m.superseded_by);
        data["places"] = json!(m.places);
        data["transitions"] = json!(m.transitions);
    }
    GraphNode {
        id: id.into(),
        label: label(snap, id),
        data,
    }
}

fn graph(sess: &mut Session, a: &Value) -> Result<Value> {
    let st = store::open();
    let snap = st.snapshot()?;
    // Every edge between entities (migratedFrom relates two edges, so it
    // is left out), plus provenance: child → parent.
    let mut all: Vec<GraphEdge> = snap
        .relations
        .iter()
        .filter(|(_, r)| r.predicate != predicate::MIGRATED_FROM)
        .map(|(id, r)| edge_of(id, r))
        .collect();
    for (cid, m) in &snap.models {
        for p in &m.parents {
            all.push(GraphEdge {
                source: cid.clone(),
                target: p.clone(),
                label: "parent".into(),
                data: json!({"lineage": true}),
            });
        }
    }
    let mut keep: BTreeSet<String> = BTreeSet::new();
    let edges: Vec<GraphEdge> = match s(a, "around").filter(|x| !x.is_empty()) {
        None => {
            keep.extend(snap.models.keys().cloned());
            keep.extend(snap.collections.keys().cloned());
            for e in &all {
                keep.insert(e.source.clone());
                keep.insert(e.target.clone());
            }
            all
        }
        Some(around) => {
            let start = if snap.kind_of(around).is_some() {
                around.to_string()
            } else {
                st.resolve_collection(around).map_err(|_| {
                    Error::NotFound(format!(
                        "{around:?} is not a stored entity id or a collection name"
                    ))
                })?
            };
            let depth = n(a, "depth").map_or(2, |d| d.max(0.0) as usize);
            let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
            for e in &all {
                adj.entry(&e.source).or_default().push(&e.target);
                adj.entry(&e.target).or_default().push(&e.source);
            }
            let mut dist: BTreeMap<String, usize> = [(start.clone(), 0)].into();
            let mut queue: VecDeque<String> = [start].into();
            while let Some(cur) = queue.pop_front() {
                let d = dist[&cur];
                if d == depth {
                    continue;
                }
                for nb in adj.get(cur.as_str()).into_iter().flatten() {
                    if !dist.contains_key(*nb) {
                        dist.insert(nb.to_string(), d + 1);
                        queue.push_back(nb.to_string());
                    }
                }
            }
            keep.extend(dist.into_keys());
            all.into_iter()
                .filter(|e| keep.contains(&e.source) && keep.contains(&e.target))
                .collect()
        }
    };
    let nodes: Vec<GraphNode> = keep.iter().map(|id| node(&snap, id)).collect();
    let summary = json!({"nodes": nodes.len(), "edges": edges.len()});
    let art = sess.add_artifact(
        "Knowledge graph".into(),
        src("store.graph", a),
        ArtifactData::Graph { nodes, edges },
    );
    let mut out = summary;
    out["artifact"] = json!(art);
    Ok(out)
}

fn info(_: &mut Session, _: &Value) -> Result<Value> {
    let st = store::open();
    let snap = st.snapshot()?;
    Ok(json!({
        "location": st.location(),
        "models": snap.models.len(),
        "relations": snap.relations.len(),
        "collections": snap.collections.len(),
        "maps": st.list_maps()?.len(),
        "licenses": store::LICENSES,
        "model_kinds": predicate::MODEL_KINDS,
        "predicates": predicate::REGISTRY.iter().map(|p| p.to_json()).collect::<Vec<_>>(),
    }))
}
