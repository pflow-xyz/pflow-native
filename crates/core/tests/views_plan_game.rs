//! Playing the Connect-4 net by hand (`plan.game`, `plan.play`,
//! `plan.new_game`): moves go through pflow-metamodel's firing rule and
//! Game::play, the automatic win detectors settle, refusals carry the reason.

use pflow_native_core::Session;
use serde_json::{json, Value};

fn open() -> Session {
    let mut s = Session::default();
    s.execute("plan.open", json!({})).unwrap();
    s
}

fn play(s: &mut Session, t: &str) -> Value {
    s.execute("plan.play", json!({"transition": t})).unwrap()
}

#[test]
fn starts_with_seven_drops_for_x() {
    let mut s = open();
    let g = s.execute("plan.game", Value::Null).unwrap();
    assert_eq!(g["ok"], true);
    assert_eq!(g["moves"].as_array().unwrap().len(), 7);
    assert_eq!(g["side"], "x_ (sign +1)");
    assert_eq!(g["final"], false);
    assert_eq!(g["ply"], 0);
    assert_eq!(g["marking"]["x_turn"], 1);
    assert_eq!(g["marking"]["game_active"], 1);
}

#[test]
fn the_opening_plays_through_and_agrees_with_plan_opening() {
    let mut s = open();
    let want = s.execute("plan.opening", Value::Null).unwrap();
    let mut g = Value::Null;
    for st in want["steps"].as_array().unwrap() {
        g = play(&mut s, st["transition"].as_str().unwrap());
    }
    assert_eq!(g["ply"], want["fired"]);
    assert_eq!(g["side"], "x_ (sign +1)");
    assert_eq!(g["marking"]["x_3_0"], 1);
    assert_eq!(g["marking"]["o_3_1"], 1);
}

#[test]
fn refuses_automatic_disabled_and_unknown_moves() {
    let mut s = open();
    let e = |s: &mut Session, t: &str| {
        s.execute("plan.play", json!({"transition": t}))
            .unwrap_err()
            .to_string()
    };
    assert!(e(&mut s, "x_win_h_0_0").contains("automatic"), "automatic");
    assert!(e(&mut s, "o_play_3_0").len() > 5, "o is not to move");
    assert!(e(&mut s, "nope").contains("no transition"));
    assert_eq!(s.execute("plan.game", Value::Null).unwrap()["ply"], 0);
}

#[test]
fn a_win_settles_by_itself_and_ends_the_game() {
    let mut s = open();
    for t in [
        "x_play_0_0",
        "o_play_1_0",
        "x_play_0_1",
        "o_play_1_1",
        "x_play_0_2",
        "o_play_1_2",
        "x_play_0_3",
    ] {
        play(&mut s, t);
    }
    let g = s.execute("plan.game", Value::Null).unwrap();
    assert_eq!(g["marking"]["win_x"], 1);
    assert_eq!(g["final"], true);
    assert_eq!(g["score"], 1.0);
    assert!(
        !g["last"].as_array().unwrap().is_empty(),
        "a detector settled"
    );
}

#[test]
fn new_game_undo_and_editing_the_plan_keep_the_position() {
    let mut s = open();
    play(&mut s, "x_play_3_0");
    s.execute(
        "plan.add_piece",
        json!({"piece": "threats", "slot": "order"}),
    )
    .unwrap();
    assert_eq!(s.execute("plan.game", Value::Null).unwrap()["ply"], 1);
    s.execute("plan.new_game", Value::Null).unwrap();
    assert_eq!(s.execute("plan.game", Value::Null).unwrap()["ply"], 0);
    s.execute("edit.undo", Value::Null).unwrap();
    assert_eq!(s.execute("plan.game", Value::Null).unwrap()["ply"], 1);
}
