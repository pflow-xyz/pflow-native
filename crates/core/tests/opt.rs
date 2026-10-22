//! The ODE workbench: sim.ode options, sim.equilibrium, and the opt.*
//! commands (rate scan, sweep, heatmap, optimize).

use pflow_native_core::{ArtifactData, Session};
use serde_json::{json, Value};

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn sir() -> Session {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "infect", "rate": 0.001}),
    );
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "recover", "rate": 0.1}),
    );
    s
}

/// A(10) ⇄ B through `fwd` and `back`.
fn reversible(fwd: f64, back: f64) -> Session {
    let mut s = Session::default();
    run(
        &mut s,
        "net.add_place",
        json!({"x": 100, "y": 100, "initial": 10}),
    );
    run(&mut s, "net.add_place", json!({"x": 300, "y": 100}));
    run(&mut s, "net.add_transition", json!({"x": 200, "y": 50}));
    run(&mut s, "net.add_transition", json!({"x": 200, "y": 150}));
    for (a, b) in [("p0", "t0"), ("t0", "p1"), ("p1", "t1"), ("t1", "p0")] {
        run(&mut s, "net.add_arc", json!({"source": a, "target": b}));
    }
    run(&mut s, "sim.set_rate", json!({"id": "t0", "rate": fwd}));
    run(&mut s, "sim.set_rate", json!({"id": "t1", "rate": back}));
    s
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * (1.0 + a.abs().max(b.abs()))
}

#[test]
fn ode_options_places_and_delta() {
    let mut s = sir();
    let r = run(
        &mut s,
        "sim.ode",
        json!({"t_end": 50, "places": ["S", "R"], "reltol": 1e-6, "abstol": 1e-9, "dt": 0.001, "dtmax": 0.5}),
    );
    let keys: Vec<&String> = r["final"].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["R", "S"], "{r}");
    match &s.artifact(r["artifact"].as_str().unwrap()).unwrap().data {
        ArtifactData::TimeSeries { series, .. } => {
            let names: Vec<&str> = series.iter().map(|x| x.name.as_str()).collect();
            assert_eq!(names, ["S", "R"]);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(f(&r["initial"]["S"]), 999.0);
    assert_eq!(f(&r["initial"]["R"]), 0.0);
    for p in ["S", "R"] {
        let d = f(&r["final"][p]) - f(&r["initial"][p]);
        assert!(close(f(&r["delta"][p]), d), "{p}: {r}");
    }
    assert!(f(&r["delta"]["S"]) < 0.0 && f(&r["delta"]["R"]) > 0.0);

    // Tighter tolerances agree with the defaults to well within the
    // default reltol.
    let d = run(&mut s, "sim.ode", json!({"t_end": 50}));
    let (a, b) = (f(&r["final"]["R"]), f(&d["final"]["R"]));
    assert!((a - b).abs() / a < 1e-2, "{a} vs {b}");

    assert!(s.execute("sim.ode", json!({"places": ["nope"]})).is_err());
    assert!(s.execute("sim.ode", json!({"reltol": 0})).is_err());
    assert!(s.execute("sim.ode", json!({"dt": -1})).is_err());
}

#[test]
fn ode_places_filter_by_base_id_on_colored_net() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "coffee-shop"}));
    let r = run(
        &mut s,
        "sim.ode",
        json!({"t_end": 1, "places": ["CoffeeBeans"]}),
    );
    let keys: Vec<&String> = r["final"].as_object().unwrap().keys().collect();
    assert!(!keys.is_empty());
    assert!(
        keys.iter().all(|k| k.starts_with("CoffeeBeans.")),
        "{keys:?}"
    );
}

#[test]
fn equilibrium_matches_the_analytic_steady_state() {
    // d[A]/dt = -k1 A + k2 B, A + B = 10  =>  A* = 10 k2 / (k1 + k2).
    let mut s = reversible(1.0, 3.0);
    let r = run(&mut s, "sim.equilibrium", json!({"tolerance": 1e-7}));
    assert_eq!(r["reached"], json!(true), "{r}");
    assert!((f(&r["state"]["p0"]) - 7.5).abs() < 1e-4, "{r}");
    assert!((f(&r["state"]["p1"]) - 2.5).abs() < 1e-4, "{r}");
    assert!(f(&r["time"]) < 1000.0);
    let art = s.artifact(r["artifact"].as_str().unwrap()).unwrap();
    assert!(matches!(art.data, ArtifactData::Report(_)));

    // A net that never settles in the time allowed says so.
    let mut s = sir();
    let r = run(&mut s, "sim.equilibrium", json!({"t_max": 1}));
    assert_eq!(r["reached"], json!(false), "{r}");
}

#[test]
fn rate_scan_more_infection_leaves_fewer_susceptible() {
    let mut s = sir();
    let r = run(
        &mut s,
        "opt.rate_scan",
        json!({"transition": "infect", "from": 0.0002, "to": 0.005, "steps": 6}),
    );
    let values: Vec<f64> = r["values"].as_array().unwrap().iter().map(f).collect();
    assert_eq!(values.len(), 6);
    assert!(close(values[0], 0.0002) && close(values[5], 0.005));
    let art = s.artifact(r["artifact"].as_str().unwrap()).unwrap();
    let ArtifactData::TimeSeries { x_label, x, series } = &art.data else {
        panic!("{:?}", art.data)
    };
    assert_eq!(x_label, "rate(infect)");
    assert_eq!(x, &values);
    let names: Vec<&str> = series.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(names, ["S", "I", "R"]);
    let susceptible = &series[0].values;
    assert!(
        susceptible.windows(2).all(|w| w[1] < w[0]),
        "S at t_end should fall as infect rises: {susceptible:?}"
    );
    for (i, row) in r["results"].as_array().unwrap().iter().enumerate() {
        assert_eq!(f(&row["S"]), susceptible[i]);
    }

    // Cap and argument checks.
    let too_many = json!({"transition": "infect", "from": 0, "to": 1, "steps": 26});
    assert!(s.execute("opt.rate_scan", too_many).is_err());
    let bad = json!({"transition": "nope", "from": 0, "to": 1});
    assert!(s.execute("opt.rate_scan", bad).is_err());
    let neg = json!({"transition": "infect", "from": -1, "to": 1});
    assert!(s.execute("opt.rate_scan", neg).is_err());
}

#[test]
fn sweep_overlays_one_series_per_rate() {
    let mut s = sir();
    let r = run(
        &mut s,
        "opt.sweep",
        json!({"transition": "recover", "place": "I", "from": 0.05, "to": 0.5, "steps": 4, "t_end": 30, "points": 50}),
    );
    let art = s.artifact(r["artifact"].as_str().unwrap()).unwrap();
    let ArtifactData::TimeSeries { x_label, x, series } = &art.data else {
        panic!("{:?}", art.data)
    };
    assert_eq!(x_label, "t");
    assert_eq!(x.len(), 50);
    assert!(close(x[0], 0.0) && close(x[49], 30.0));
    assert_eq!(series.len(), 4);
    let groups: Vec<&str> = series.iter().map(|x| x.group.as_deref().unwrap()).collect();
    assert_eq!(groups, ["k=0.05", "k=0.2", "k=0.35", "k=0.5"]);
    assert!(series.iter().all(|x| x.name == "I" && x.values.len() == 50));
    assert!(series.iter().all(|x| x.values[0] == 1.0));
    // Faster recovery leaves fewer infected at the end.
    let fins: Vec<f64> = r["trajectories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| f(&t["final"]))
        .collect();
    assert!(fins.windows(2).all(|w| w[1] < w[0]), "{fins:?}");
}

fn heatmap_args() -> Value {
    json!({
        "param_x": "infect", "param_y": "recover",
        "x_from": 0.0002, "x_to": 0.005, "y_from": 0.02, "y_to": 0.5,
        "x_steps": 6, "y_steps": 4, "observable": "R", "t_end": 50,
    })
}

#[test]
fn heatmap_dimensions_and_determinism() {
    let mut s = sir();
    let a = run(&mut s, "opt.heatmap", heatmap_args());
    let art = s.artifact(a["artifact"].as_str().unwrap()).unwrap().clone();
    let ArtifactData::Heatmap {
        x_label,
        y_label,
        xs,
        ys,
        z,
    } = &art.data
    else {
        panic!("{:?}", art.data)
    };
    assert_eq!(
        (x_label.as_str(), y_label.as_str()),
        ("rate(infect)", "rate(recover)")
    );
    assert_eq!((xs.len(), ys.len()), (6, 4));
    assert_eq!(z.len(), 4, "rows are y");
    assert!(z.iter().all(|row| row.len() == 6), "columns are x");
    // More infection => more recovered; more recovery (at fixed infection)
    // => a smaller epidemic.
    assert!(z[0][5] > z[0][0], "{z:?}");
    assert!(z[3][0] < z[0][0], "{z:?}");
    let (lo, hi) = (f(&a["min"]), f(&a["max"]));
    assert!(z.iter().flatten().all(|&v| lo <= v && v <= hi));

    // The same call gives the same grid (ODE sums may differ in the last
    // bit across runs: pflow-core keeps transitions in a HashMap).
    let b = run(&mut s, "opt.heatmap", heatmap_args());
    for (ra, rb) in a["z"]
        .as_array()
        .unwrap()
        .iter()
        .zip(b["z"].as_array().unwrap())
    {
        for (va, vb) in ra.as_array().unwrap().iter().zip(rb.as_array().unwrap()) {
            assert!(
                (f(va) - f(vb)).abs() <= 1e-6 * (1.0 + f(va).abs()),
                "{va} vs {vb}"
            );
        }
    }

    // A heatmap cell agrees with a plain ODE run at the same rates.
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "infect", "rate": xs[5]}),
    );
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "recover", "rate": ys[0]}),
    );
    let o = run(&mut s, "sim.ode", json!({"t_end": 50}));
    let rel = (f(&o["final"]["R"]) - z[0][5]).abs() / z[0][5];
    assert!(rel < 1e-2, "grid {} vs ode {}", z[0][5], o["final"]["R"]);

    // Log spacing, and the 40-per-axis cap.
    let mut log = heatmap_args();
    log["log"] = json!(true);
    log["steps"] = json!(3);
    log["x_steps"] = Value::Null;
    log["y_steps"] = Value::Null;
    let l = run(&mut s, "opt.heatmap", log);
    let lx: Vec<f64> = l["xs"].as_array().unwrap().iter().map(f).collect();
    assert!(close(lx[1], (0.0002f64 * 0.005).sqrt()), "{lx:?}");
    let mut big = heatmap_args();
    big["x_steps"] = json!(41);
    assert!(s.execute("opt.heatmap", big).is_err());
    let mut same = heatmap_args();
    same["param_y"] = json!("infect");
    assert!(s.execute("opt.heatmap", same).is_err());
}

fn optimize_args(samples: u32, seed: u32) -> Value {
    json!({
        "parameters": [
            {"transition": "infect", "min": 0.0001, "max": 0.005, "log": true},
            {"transition": "recover", "min": 0.02, "max": 0.5},
        ],
        "objectives": [
            {"place": "R", "sense": "min"},
            {"place": "I", "sense": "max"},
        ],
        "samples": samples, "seed": seed, "t_end": 30,
    })
}

fn dominates(a: &[f64], b: &[f64], maximize: &[bool]) -> bool {
    let mut better = false;
    for k in 0..a.len() {
        let (x, y) = if maximize[k] {
            (a[k], b[k])
        } else {
            (-a[k], -b[k])
        };
        if x < y {
            return false;
        }
        better |= x > y;
    }
    better
}

#[test]
fn optimize_is_seeded_and_flags_exactly_the_non_dominated() {
    let mut s = sir();
    let r = run(&mut s, "opt.optimize", optimize_args(60, 7));
    assert_eq!(r["samples"], json!(60));
    let table = s.artifact(r["table"].as_str().unwrap()).unwrap().clone();
    let ArtifactData::Table { columns, rows } = &table.data else {
        panic!("{:?}", table.data)
    };
    assert_eq!(
        columns,
        &[
            "sample",
            "rate(infect)",
            "rate(recover)",
            "R (min)",
            "I (max)",
            "pareto"
        ]
    );
    assert_eq!(rows.len(), 60);
    let vals: Vec<Vec<f64>> = rows.iter().map(|r| vec![f(&r[3]), f(&r[4])]).collect();
    let flags: Vec<bool> = rows.iter().map(|r| r[5].as_bool().unwrap()).collect();
    for (i, row) in rows.iter().enumerate() {
        let ri = f(&row[1]);
        assert!((0.0001..=0.005).contains(&ri) && (0.02..=0.5).contains(&f(&row[2])));
        let dominated =
            (0..vals.len()).any(|j| j != i && dominates(&vals[j], &vals[i], &[false, true]));
        assert_eq!(flags[i], !dominated, "sample {i}");
    }
    let n_front = flags.iter().filter(|&&x| x).count();
    assert!(n_front >= 1);
    assert_eq!(r["pareto_count"], json!(n_front));
    assert_eq!(r["pareto"].as_array().unwrap().len(), n_front);

    let scatter = s.artifact(r["scatter"].as_str().unwrap()).unwrap().clone();
    let ArtifactData::Scatter {
        x_label,
        y_label,
        points,
        highlight,
    } = &scatter.data
    else {
        panic!("{:?}", scatter.data)
    };
    assert_eq!((x_label.as_str(), y_label.as_str()), ("R (min)", "I (max)"));
    assert_eq!(points.len(), 60);
    let want: Vec<usize> = (0..60).filter(|&i| flags[i]).collect();
    assert_eq!(highlight, &want);

    // Same seed => same samples; another seed => other samples.
    let again = run(&mut s, "opt.optimize", optimize_args(60, 7));
    let t2 = s
        .artifact(again["table"].as_str().unwrap())
        .unwrap()
        .clone();
    let ArtifactData::Table { rows: rows2, .. } = &t2.data else {
        panic!()
    };
    for (a, b) in rows.iter().zip(rows2) {
        assert_eq!(a[1], b[1]);
        assert_eq!(a[2], b[2]);
        assert_eq!(a[3], b[3]);
    }
    let other = run(&mut s, "opt.optimize", optimize_args(60, 8));
    let t3 = s
        .artifact(other["table"].as_str().unwrap())
        .unwrap()
        .clone();
    let ArtifactData::Table { rows: rows3, .. } = &t3.data else {
        panic!()
    };
    assert_ne!(rows[0][1], rows3[0][1]);
}

#[test]
fn optimize_single_objective_plots_against_the_first_rate() {
    // A -> t -> B: more rate, more B. Only the fastest sample is on the front.
    let mut s = reversible(1.0, 0.0);
    let r = run(
        &mut s,
        "opt.optimize",
        json!({
            "parameters": {"t0": [0.1, 2.0]},
            "objectives": [{"place": "p1", "direction": "max"}],
            "samples": 25, "seed": 1, "t_end": 1,
        }),
    );
    assert_eq!(r["pareto_count"], json!(1), "{r}");
    let scatter = s.artifact(r["scatter"].as_str().unwrap()).unwrap();
    let ArtifactData::Scatter {
        x_label,
        points,
        highlight,
        ..
    } = &scatter.data
    else {
        panic!()
    };
    assert_eq!(x_label, "rate(t0)");
    let best = points
        .iter()
        .enumerate()
        .max_by(|a, b| a.1 .0.total_cmp(&b.1 .0))
        .unwrap()
        .0;
    assert_eq!(highlight, &vec![best]);

    // Refusals.
    for bad in [
        json!({"parameters": [], "objectives": [{"place": "p1"}]}),
        json!({"parameters": [{"transition": "t0", "min": 2, "max": 1}], "objectives": [{"place": "p1"}]}),
        json!({"parameters": [{"transition": "zz", "min": 0, "max": 1}], "objectives": [{"place": "p1"}]}),
        json!({"parameters": [{"transition": "t0", "min": 0, "max": 1, "log": true}], "objectives": [{"place": "p1"}]}),
        json!({"parameters": [{"transition": "t0", "min": 0, "max": 1}], "objectives": [{"place": "p1", "sense": "up"}]}),
        json!({"parameters": [{"transition": "t0", "min": 0, "max": 1}], "objectives": [{"place": "zz"}]}),
        json!({"parameters": [{"transition": "t0", "min": 0, "max": 1}], "objectives": [{"place": "p1"}], "samples": 2001}),
    ] {
        assert!(s.execute("opt.optimize", bad.clone()).is_err(), "{bad}");
    }
}

#[test]
fn workbench_commands_are_journaled_in_the_workbench_menu() {
    let s = Session::default();
    for id in ["opt.rate_scan", "opt.sweep", "opt.heatmap", "opt.optimize"] {
        let c = s.registry().find(id).unwrap();
        assert_eq!(c.menu, "Workbench", "{id}");
        assert!(c.journal, "{id}");
    }
    let c = s.registry().find("sim.equilibrium").unwrap();
    assert_eq!(c.menu, "Simulate");
}
