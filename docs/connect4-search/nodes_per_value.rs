//! Nodes and time per move value on the labelled referee positions.
//!
//! Copy to `crates/core/tests/nodes_per_value.rs`, then:
//!
//!     C4_LABELS=$PWD/docs/connect4-search/referee-labels.jsonl C4_N=60 \
//!       cargo test --release -p pflow-native-core --test nodes_per_value -- --ignored --nocapture
//!
//! C4_N is how many positions to take (each gives one value per legal move).

use pflow_metamodel::game::{Game, Solver};
use pflow_native_core::{doc::Document, sim::Compiled};

#[test]
#[ignore]
fn nodes_per_value() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/connect-4.json");
    let doc = Document::from_json(&std::fs::read(path).unwrap()).unwrap();
    let ix = Compiled::new(&doc).unwrap().model.indexed();
    let auto: Vec<bool> = ix.transitions.iter().map(|t| t.contains("_win_")).collect();
    let sign: Vec<f64> = ix
        .transitions
        .iter()
        .map(|t| if t.starts_with("x_") { 1.0 } else { -1.0 })
        .collect();
    let pi = |id: &str| ix.places.iter().position(|p| p == id).unwrap();
    let reward = vec![(pi("win_x"), 1.0), (pi("win_o"), -1.0)];
    let g = Game::new(ix, auto, sign, reward);
    let take: usize = std::env::var("C4_N").map_or(100, |v| v.parse().unwrap());
    let labels = std::fs::read_to_string(std::env::var("C4_LABELS").unwrap()).unwrap();
    let mut s = Solver::new(&g, 800_000);
    let (mut nodes, mut values, mut skips, mut sizes) = (0usize, 0usize, 0usize, vec![]);
    let t0 = std::time::Instant::now();
    for line in labels.lines().skip(1).take(take) {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        let mk: Vec<i64> = v["mk"].as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect();
        for t in g.moves(&mk) {
            let got = s.value(&g.play(&mk, t).unwrap()).unwrap();
            nodes += s.nodes().min(800_001);
            sizes.push(s.nodes());
            values += 1;
            skips += got.is_none() as usize;
        }
    }
    sizes.sort_unstable();
    let dt = t0.elapsed().as_secs_f64();
    eprintln!(
        "{values} values ({skips} past budget) in {dt:.1}s: {:.0} nodes/value, median {}, p90 {}, {:.2} µs/node",
        nodes as f64 / values as f64,
        sizes[sizes.len() / 2],
        sizes[sizes.len() * 9 / 10],
        dt * 1e6 / nodes as f64
    );
}
