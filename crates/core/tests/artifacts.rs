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
