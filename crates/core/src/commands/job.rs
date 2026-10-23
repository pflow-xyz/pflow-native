//! Background jobs: `job.start` runs a long command off the UI thread,
//! `job.status` follows it, `job.forget` drops a record. The machinery is in
//! [`crate::jobs`].

use serde_json::{json, Value};
use web_time::Instant;

use crate::jobs::{self, Job, State, MAX_RUNNING};
use crate::registry::{always, arg, opt, req, s, ArgType::*, CommandSpec};
use crate::session::Session;
use crate::{Error, Result};

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "job.start",
        label: "Start Background Job",
        help: "Run a long command (simulation, optimization, analysis, data, rnn; see job.status's `allowed`) in the background \
               against a copy of the session, and return at once with a job id. The window and the control port stay \
               responsive. When it finishes, the results it stored join the session under fresh ids, which its result is \
               rewritten to name; follow it with job.status. A job may only store results: one that changes the net, marking, \
               rates or model library fails and its change is discarded. At most 4 run at once.",
        menu: "",
        shortcut: None,
        args: &[
            arg("command", Str, "command id, e.g. rnn.self_play"),
            opt("args", Json, "its arguments (default {})"),
        ],
        enabled: always,
        run: start,
        journal: false,
    },
    CommandSpec {
        id: "job.status",
        label: "Background Jobs",
        help: "One job (`job`), with its result once done, or every job: state (running, done, failed), elapsed seconds, and \
               for a done job the artifacts it added and `stale` if the net changed while it ran.",
        menu: "",
        shortcut: None,
        args: &[opt("job", Str, "job id (default: all jobs, without results)")],
        enabled: always,
        run: status,
        journal: false,
    },
    CommandSpec {
        id: "job.forget",
        label: "Forget Job",
        help: "Drop a finished job's record (its artifacts stay). A running job cannot be stopped; forget it after it ends.",
        menu: "",
        shortcut: None,
        args: &[arg("job", Str, "job id")],
        enabled: always,
        run: forget,
        journal: false,
    },
];

fn start(sess: &mut Session, a: &Value) -> Result<Value> {
    sess.poll_jobs();
    let command = req(a, "command").to_string();
    if !jobs::allowed(&command) {
        return Err(Error::Invalid(format!(
            "{command} cannot run in the background; allowed: {}",
            jobs::BACKGROUND.join(", ")
        )));
    }
    if sess.registry().find(&command).is_none() {
        return Err(Error::UnknownCommand(command));
    }
    if sess.jobs.running() >= MAX_RUNNING {
        return Err(Error::Invalid(format!(
            "{MAX_RUNNING} jobs are already running; wait for one (job.status)"
        )));
    }
    let args = a.get("args").cloned().unwrap_or(Value::Null);
    let id = sess.jobs.next_id();
    let fork = sess.fork();
    let job = Job {
        id: id.clone(),
        command: command.clone(),
        args: args.clone(),
        started: Instant::now(),
        elapsed: None,
        revision: sess.revision,
        #[cfg(not(target_arch = "wasm32"))]
        state: State::Running(
            std::thread::Builder::new()
                .name(format!("pflow-native-{id}"))
                .spawn(move || jobs::run(fork, command, args))
                .map_err(|e| Error::Invalid(format!("could not start a thread: {e}")))?,
        ),
        #[cfg(target_arch = "wasm32")]
        state: State::Failed {
            error: String::new(),
        },
    };
    sess.jobs.list.push(job);
    #[cfg(target_arch = "wasm32")]
    {
        // No threads on the web: run it now.
        let i = sess.jobs.list.len() - 1;
        let j = &sess.jobs.list[i];
        let outcome = jobs::run(fork, j.command.clone(), j.args.clone());
        sess.jobs.list[i].elapsed = Some(sess.jobs.list[i].started.elapsed().as_secs_f64());
        let state = sess.finish(i, outcome);
        sess.jobs.list[i].state = state;
    }
    let j = sess.jobs.list.last().expect("just pushed");
    let mut out = sess.job_json(j);
    out["note"] = json!("follow it with job.status {\"job\": \"…\"}");
    Ok(out)
}

fn status(sess: &mut Session, a: &Value) -> Result<Value> {
    sess.poll_jobs();
    match s(a, "job") {
        Some(id) => {
            let j = sess
                .jobs
                .list
                .iter()
                .find(|j| j.id == id)
                .ok_or_else(|| Error::NotFound(format!("no job {id}")))?;
            Ok(sess.job_json(j))
        }
        None => {
            let list: Vec<Value> = sess
                .jobs
                .list
                .iter()
                .map(|j| {
                    let mut v = sess.job_json(j);
                    if let Some(o) = v.as_object_mut() {
                        o.remove("result");
                    }
                    v
                })
                .collect();
            Ok(json!({ "jobs": list, "running": sess.jobs.running(), "allowed": jobs::BACKGROUND }))
        }
    }
}

fn forget(sess: &mut Session, a: &Value) -> Result<Value> {
    sess.poll_jobs();
    let id = req(a, "job");
    let i = sess
        .jobs
        .list
        .iter()
        .position(|j| j.id == id)
        .ok_or_else(|| Error::NotFound(format!("no job {id}")))?;
    if sess.jobs.list[i].elapsed.is_none() {
        return Err(Error::Invalid(format!(
            "{id} is still running; forget it once it has finished"
        )));
    }
    sess.jobs.list.remove(i);
    Ok(json!({ "forgot": id }))
}
