//! Background jobs (`job.*`): a long command runs on a worker thread against
//! a fork of the session, so the window and the control port stay
//! responsive and an agent is not held past its timeout. When the worker
//! finishes, [`Session::poll_jobs`] merges the artifacts it stored into the
//! session under fresh ids and rewrites the ids in its result to match.
//!
//! Only commands whose whole effect is to store results may run as jobs
//! ([`BACKGROUND`]): a job that changed the net, the marking, the rates or
//! the model library has that change discarded and fails, saying so. Remote
//! sends never run as jobs, so they keep their confirmation. On the web
//! (no threads) a job runs to completion inside `job.start`.

use std::collections::HashMap;

use serde_json::{json, Value};
use web_time::Instant;

use crate::artifact::{Artifact, ArtifactData};
use crate::session::Session;

/// Command prefixes (or ids) that may run in the background.
pub const BACKGROUND: &[&str] = &[
    "sim.ode",
    "sim.equilibrium",
    "sim.ssa",
    "sim.sde",
    "sim.scenario",
    "sim.compare",
    "sim.crosscheck",
    "sim.evaluate",
    "sim.receipt",
    "opt.",
    "analysis.",
    "data.generate",
    "data.conformance",
    "data.replay",
    "data.calibrate",
    "system.run",
    "compose.diff",
    "game.",
    "rnn.",
];

/// Jobs that may run at once.
pub const MAX_RUNNING: usize = 4;

pub fn allowed(command: &str) -> bool {
    BACKGROUND.iter().any(|p| {
        if p.ends_with('.') {
            command.starts_with(p)
        } else {
            command == *p
        }
    })
}

/// What a worker hands back.
pub(crate) struct Outcome {
    pub result: std::result::Result<Value, String>,
    /// Artifacts the command stored, with the worker's ids.
    pub artifacts: Vec<Artifact>,
    /// What else it changed, if anything (then nothing is merged).
    pub changed: Option<&'static str>,
}

pub(crate) enum State {
    #[cfg(not(target_arch = "wasm32"))]
    Running(std::thread::JoinHandle<Outcome>),
    Done {
        result: Value,
        artifacts: Vec<String>,
        stale: bool,
    },
    Failed {
        error: String,
    },
}

pub(crate) struct Job {
    pub id: String,
    pub command: String,
    pub args: Value,
    pub started: Instant,
    pub elapsed: Option<f64>,
    /// The session revision when the job started; a different one at the
    /// end means the net or marking changed meanwhile (`stale`).
    pub revision: u64,
    pub state: State,
}

#[derive(Default)]
pub struct Jobs {
    next: u64,
    pub(crate) list: Vec<Job>,
}

impl Jobs {
    pub fn running(&self) -> usize {
        self.list.iter().filter(|j| j.elapsed.is_none()).count()
    }

    pub(crate) fn next_id(&mut self) -> String {
        self.next += 1;
        format!("j{}", self.next)
    }
}

/// Runs `command` on `fork` and reports what it stored and changed.
pub(crate) fn run(mut fork: Session, command: String, args: Value) -> Outcome {
    let before: std::collections::HashSet<String> =
        fork.artifacts.iter().map(|a| a.id.clone()).collect();
    let (rev, rates, models) = (fork.revision, fork.rates.clone(), fork.models.clone());
    let result = fork.execute(&command, args).map_err(|e| e.to_string());
    let changed = if fork.revision != rev {
        Some("the net or its marking")
    } else if fork.rates != rates {
        Some("the rates")
    } else if fork.models != models {
        Some("the model library")
    } else {
        None
    };
    let artifacts = fork
        .artifacts
        .into_iter()
        .filter(|a| !before.contains(&a.id))
        .collect();
    Outcome {
        result,
        artifacts,
        changed,
    }
}

/// Replaces every string equal to a key of `ids` with its value.
pub(crate) fn remap(v: &mut Value, ids: &HashMap<String, String>) {
    match v {
        Value::String(s) => {
            if let Some(n) = ids.get(s.as_str()) {
                *s = n.clone();
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| remap(x, ids)),
        Value::Object(o) => o.values_mut().for_each(|x| remap(x, ids)),
        _ => {}
    }
}

impl Session {
    /// Merges every finished job into the session (cheap when none is).
    /// The app calls it every logic pass; `job.*` commands call it first.
    pub fn poll_jobs(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        for i in 0..self.jobs.list.len() {
            let finished = matches!(&self.jobs.list[i].state, State::Running(h) if h.is_finished());
            if !finished {
                continue;
            }
            let State::Running(h) = std::mem::replace(
                &mut self.jobs.list[i].state,
                State::Failed {
                    error: String::new(),
                },
            ) else {
                unreachable!()
            };
            let elapsed = self.jobs.list[i].started.elapsed().as_secs_f64();
            self.jobs.list[i].elapsed = Some(elapsed);
            let outcome = h.join().unwrap_or_else(|_| Outcome {
                result: Err("the job panicked".into()),
                artifacts: vec![],
                changed: None,
            });
            let state = self.finish(i, outcome);
            self.jobs.list[i].state = state;
        }
    }

    pub(crate) fn finish(&mut self, i: usize, o: Outcome) -> State {
        let job = &self.jobs.list[i];
        let (command, revision) = (job.command.clone(), job.revision);
        if let Some(what) = o.changed {
            return State::Failed {
                error: format!(
                    "{command} changed {what}, which a background job may not do, so its effect was discarded; run it directly"
                ),
            };
        }
        let mut result = match o.result {
            Ok(v) => v,
            Err(error) => return State::Failed { error },
        };
        let mut ids = HashMap::new();
        let mut added = vec![];
        for a in o.artifacts {
            let new = self.add_artifact(a.title, a.source, a.data);
            ids.insert(a.id, new.clone());
            added.push(new);
        }
        for id in &added {
            if let Some(a) = self.artifacts.iter_mut().find(|a| &a.id == id) {
                if let ArtifactData::Report(r) = &mut a.data {
                    remap(r, &ids);
                }
            }
        }
        remap(&mut result, &ids);
        State::Done {
            result,
            artifacts: added,
            stale: self.revision != revision,
        }
    }

    /// One job as JSON: its state, timing, and (once done) its result.
    pub(crate) fn job_json(&self, j: &Job) -> Value {
        let elapsed = j
            .elapsed
            .unwrap_or_else(|| j.started.elapsed().as_secs_f64());
        let mut v = json!({
            "job": j.id, "command": j.command, "args": j.args,
            "elapsed_s": (elapsed * 10.0).round() / 10.0,
        });
        match &j.state {
            #[cfg(not(target_arch = "wasm32"))]
            State::Running(_) => v["state"] = json!("running"),
            State::Done {
                result,
                artifacts,
                stale,
            } => {
                v["state"] = json!("done");
                v["result"] = result.clone();
                v["artifacts"] = json!(artifacts);
                if *stale {
                    v["stale"] = json!(true);
                    v["note"] = json!("the net or its marking changed while the job ran: the result is for the net as it was when the job started");
                }
            }
            State::Failed { error } => {
                v["state"] = json!("failed");
                v["error"] = json!(error);
            }
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_job_that_changes_state_reports_what_it_changed() {
        let mut s = Session::default();
        s.execute("file.open_example", json!({"name": "sir"}))
            .unwrap();
        let o = run(
            s.fork(),
            "sim.set_rate".into(),
            json!({"id": "infect", "rate": 2}),
        );
        assert_eq!(o.changed, Some("the rates"));
        let o = run(s.fork(), "net.add_place".into(), json!({"x": 0, "y": 0}));
        assert_eq!(o.changed, Some("the net or its marking"));
        let o = run(s.fork(), "sim.ode".into(), json!({"t_end": 5}));
        assert_eq!(o.changed, None);
        assert_eq!(o.artifacts.len(), 1);
    }

    #[test]
    fn allowlist() {
        assert!(allowed("sim.ssa") && allowed("opt.heatmap") && allowed("rnn.train"));
        assert!(
            !allowed("sim.set_rate") && !allowed("net.add_place") && !allowed("remote.sim.call")
        );
        assert!(!allowed("job.start") && !allowed("sim.odex"));
    }
}
