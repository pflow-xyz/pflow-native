use pflow_native_core::Session;
use serde_json::{json, Value};

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn library_save_import_open_delete() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    run(&mut s, "model.save", json!({"name": "epi"}));
    assert!(
        s.execute("model.save", json!({"name": "epi"})).is_err(),
        "no silent overwrite"
    );
    run(
        &mut s,
        "model.import",
        json!({"name": "shop", "example": "coffee-shop"}),
    );
    let list = run(&mut s, "model.list", Value::Null);
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!(s.doc.places.len(), 3, "import does not open");
    run(&mut s, "model.open", json!({"name": "shop"}));
    assert_eq!(s.doc.places.len(), 4);
    run(&mut s, "edit.undo", Value::Null);
    assert_eq!(s.doc.places.len(), 3, "open is undoable");
    assert_eq!(
        run(&mut s, "model.get", json!({"name": "current"}))["places"]["S"]["initial"],
        json!([999])
    );
    run(&mut s, "model.delete", json!({"name": "epi"}));
    assert!(s.execute("model.get", json!({"name": "epi"})).is_err());
    assert!(s.execute("model.save", json!({"name": "current"})).is_err());
}
