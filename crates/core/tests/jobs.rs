//! Background jobs: job.start / job.status / job.forget.

use std::time::{Duration, Instant};

use pflow_native_core::Session;
use serde_json::{json, Value};

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn sir() -> Session {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    s
}

/// Polls job.status until the job is no longer running.
fn wait(s: &mut Session, job: &Value) -> Value {
    let t0 = Instant::now();
    loop {
        let st = run(s, "job.status", json!({"job": job}));
        if st["state"] != "running" {
            return st;
        }
        assert!(t0.elapsed() < Duration::from_secs(30), "job never finished");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_job_runs_in_the_background_and_its_results_join_the_session() {
    let mut s = sir();
    let j = run(
        &mut s,
        "job.start",
        json!({"command": "sim.ssa", "args": {"t_end": 20, "runs": 50}}),
    );
    // The session keeps working meanwhile, and its own results take ids.
    let ode = run(&mut s, "sim.ode", json!({"t_end": 5}));
    let st = wait(&mut s, &j["job"]);
    assert_eq!(st["state"], "done", "{st}");
    let id = st["result"]["artifact"].as_str().unwrap();
    assert_ne!(
        id,
        ode["artifact"].as_str().unwrap(),
        "fresh id, no collision"
    );
    assert_eq!(st["artifacts"], json!([id]));
    let a = s.artifact(id).expect("merged");
    assert!(a.title.starts_with("SSA"), "{}", a.title);
    assert!(st.get("stale").is_none());
}

#[test]
fn a_result_for_a_net_that_changed_meanwhile_is_stale() {
    let mut s = sir();
    let j = run(
        &mut s,
        "job.start",
        json!({"command": "sim.ode", "args": {}}),
    );
    run(&mut s, "net.add_place", json!({"x": 0, "y": 0}));
    let st = wait(&mut s, &j["job"]);
    assert_eq!(st["state"], "done");
    assert_eq!(st["stale"], true);
}

#[test]
fn failures_and_refusals() {
    let mut s = sir();
    for bad in [
        "net.add_place",
        "sim.set_rate",
        "job.start",
        "view.screenshot",
    ] {
        let e = s
            .execute("job.start", json!({"command": bad}))
            .unwrap_err()
            .to_string();
        assert!(e.contains("cannot run in the background"), "{bad}: {e}");
    }
    let j = run(
        &mut s,
        "job.start",
        json!({"command": "sim.ode", "args": {"t_end": -1}}),
    );
    let st = wait(&mut s, &j["job"]);
    assert_eq!(st["state"], "failed");
    assert!(st["error"].as_str().unwrap().contains("t_end"), "{st}");
    run(&mut s, "job.forget", json!({"job": j["job"]}));
    assert!(s.execute("job.status", json!({"job": j["job"]})).is_err());
    let all = run(&mut s, "job.status", json!({}));
    assert_eq!(all["jobs"], json!([]));
}
