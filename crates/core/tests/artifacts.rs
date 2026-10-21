use pflow_native_core::Session;
use serde_json::{json, Value};

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn put_validates_and_exports_every_kind() {
    let mut s = Session::default();
    let hm = json!({"kind":"heatmap","x_label":"a","y_label":"b","xs":[1,2,3],"ys":[1,2],"z":[[1,2,3],[4,5,6]]});
    let id = run(&mut s, "artifact.put", json!({"title":"hm","data":hm}))["artifact"].clone();
    let svg = run(&mut s, "export.artifact", json!({"id": id, "format":"svg"}));
    assert!(svg.as_str().unwrap().matches("<rect").count() >= 6);
    let csv = run(&mut s, "export.artifact", json!({"id": id, "format":"csv"}));
    assert_eq!(csv.as_str().unwrap().lines().count(), 3);

    let bad =
        json!({"kind":"heatmap","x_label":"a","y_label":"b","xs":[1,2,3],"ys":[1],"z":[[1,2]]});
    assert!(s
        .execute("artifact.put", json!({"title":"x","data":bad}))
        .is_err());

    let sc = json!({"kind":"scatter","x_label":"x","y_label":"y","points":[[0,1],[1,0],[0.2,0.2]],"highlight":[0,1]});
    let id = run(&mut s, "artifact.put", json!({"title":"sc","data":sc}))["artifact"].clone();
    let svg = run(&mut s, "export.artifact", json!({"id": id, "format":"svg"}));
    assert!(svg.as_str().unwrap().contains("#ef4444"));

    let ts = json!({"kind":"timeseries","x_label":"t","x":[0,1],"series":[
        {"name":"S","values":[1,2],"band":[0.1,0.2],"group":"a"},
        {"name":"S","values":[2,3],"group":"b"}]});
    let id = run(&mut s, "artifact.put", json!({"title":"ts","data":ts}))["artifact"].clone();
    let csv = run(&mut s, "export.artifact", json!({"id": id, "format":"csv"}));
    assert_eq!(
        csv.as_str().unwrap().lines().next(),
        Some("t,a:S,a:S_sd,b:S")
    );
    let svg = run(&mut s, "export.artifact", json!({"id": id, "format":"svg"}));
    assert!(svg.as_str().unwrap().contains("<polygon"), "band drawn");

    let n = s.artifacts.len();
    run(&mut s, "artifact.delete", json!({"id": id}));
    assert_eq!(s.artifacts.len(), n - 1);
    run(&mut s, "artifact.clear", Value::Null);
    assert!(s.artifacts.is_empty());
}

#[test]
fn sets_highlight_and_trace_replay() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    let sets = json!({"kind":"sets","sets":[
        {"label":"S+I+R","nodes":["S","I","R"],"weights":{"S":1,"I":1,"R":1}},
        {"label":"bad","nodes":["nope"]}]});
    let id = run(&mut s, "artifact.put", json!({"title":"inv","data":sets}))["artifact"].clone();
    run(&mut s, "artifact.show_set", json!({"id": id, "index": 0}));
    assert_eq!(s.highlight.as_ref().unwrap().nodes, vec!["S", "I", "R"]);
    assert!(s
        .execute("artifact.show_set", json!({"id": id, "index": 1}))
        .is_err());
    let csv = run(
        &mut s,
        "export.artifact",
        json!({"id": id, "format": "csv"}),
    );
    assert!(csv.as_str().unwrap().contains("S+I+R,S,1,"));
    run(&mut s, "view.clear_highlight", Value::Null);
    assert!(s.highlight.is_none());

    let trace = json!({"kind":"trace","steps":[
        {"transition":"","marking":{"S":[999],"I":[1],"R":[0]}},
        {"transition":"recover","marking":{"S":[999],"I":[0],"R":[1]}}]});
    let id = run(&mut s, "artifact.put", json!({"title":"t","data":trace}))["artifact"].clone();
    run(&mut s, "sim.goto", json!({"id": id, "step": 1}));
    assert_eq!(s.marking["R"], vec![1]);
    assert_eq!(s.highlight.as_ref().unwrap().nodes, vec!["recover"]);
    run(&mut s, "edit.undo", Value::Null);
    assert_eq!(s.marking["R"], vec![0], "goto is one undo step");

    let g = json!({"kind":"graph","nodes":[{"id":"m0","label":"S=999","data":{"initial":true}},{"id":"m1","label":"R=1"}],
        "edges":[{"source":"m0","target":"m1","label":"recover"}]});
    let id = run(&mut s, "artifact.put", json!({"title":"g","data":g}))["artifact"].clone();
    let dot = run(
        &mut s,
        "export.artifact",
        json!({"id": id, "format": "dot"}),
    );
    assert!(dot
        .as_str()
        .unwrap()
        .contains("\"m0\" -> \"m1\" [label=\"recover\"]"));
    let bad = json!({"kind":"graph","nodes":[],"edges":[{"source":"a","target":"b","label":""}]});
    assert!(s
        .execute("artifact.put", json!({"title":"g","data":bad}))
        .is_err());

    let d = json!({"kind":"drawing","width":100,"height":50,"shapes":[
        {"shape":"rect","x":1,"y":1,"w":20,"h":10,"stroke":"#000"},
        {"shape":"line","points":[[0,0],[10,10]]},
        {"shape":"text","x":5,"y":5,"text":"a<b"}]});
    let id = run(&mut s, "artifact.put", json!({"title":"d","data":d}))["artifact"].clone();
    let svg = run(
        &mut s,
        "export.artifact",
        json!({"id": id, "format": "svg"}),
    );
    assert!(svg.as_str().unwrap().contains("a&lt;b"));

    let bars = json!({"kind":"bars","value_label":"Δ R","bars":[{"name":"infect","value":3,"lo":-1,"hi":5}]});
    let id = run(&mut s, "artifact.put", json!({"title":"b","data":bars}))["artifact"].clone();
    let csv = run(
        &mut s,
        "export.artifact",
        json!({"id": id, "format": "csv"}),
    );
    assert_eq!(csv.as_str().unwrap().lines().nth(1), Some("infect,3,-1,5"));
}
