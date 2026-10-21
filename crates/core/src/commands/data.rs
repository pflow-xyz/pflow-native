//! Event-log intake: load a CSV / JSONL log into the session as a canonical
//! table artifact (see `crate::eventlog`), and summarize one.

use serde_json::{json, Value};

use crate::eventlog;
use crate::registry::{always, arg, opt, s, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::{Error, Result};

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "data.load_log",
        label: "Import Event Log…",
        help: "Load a CSV or JSONL event log (from `path`, or inline `text`) as an event-log table artifact. \
               Column/field names default to case_id, activity, timestamp, resource, lifecycle.",
        menu: "Data",
        shortcut: None,
        args: &[
            opt("path", Str, "file to read (.csv or .jsonl)"),
            opt("text", Str, "the log itself, instead of a path"),
            opt("format", Str, "csv or jsonl (default: from the extension, else csv)"),
            opt("case", Str, "case id column (default case_id)"),
            opt("activity", Str, "activity column (default activity)"),
            opt("time", Str, "timestamp column (default timestamp)"),
            opt("resource", Str, "resource column (default resource)"),
            opt("delimiter", Str, "CSV delimiter (default ,)"),
            opt("title", Str, "artifact title"),
        ],
        enabled: always,
        run: load_log,
        journal: true,
    },
    CommandSpec {
        id: "data.log_info",
        label: "Event Log Summary",
        help: "Cases, events, activities, variants and timing of an event-log artifact.",
        menu: "",
        shortcut: None,
        args: &[arg("log", Str, "event-log artifact id")],
        enabled: always,
        run: log_info,
        journal: false,
    },
];

fn load_log(sess: &mut Session, a: &Value) -> Result<Value> {
    let (path, text) = (s(a, "path"), s(a, "text"));
    let body = match (path, text) {
        (Some(p), None) => {
            std::fs::read_to_string(p).map_err(|e| Error::Io(format!("{p}: {e}")))?
        }
        (None, Some(t)) => t.to_string(),
        _ => {
            return Err(Error::BadArgs(
                "give exactly one of `path` or `text`".into(),
            ))
        }
    };
    let format = s(a, "format").map(str::to_string).unwrap_or_else(|| {
        if path.is_some_and(|p| p.ends_with(".jsonl") || p.ends_with(".ndjson")) {
            "jsonl".into()
        } else {
            "csv".into()
        }
    });
    let log = match format.as_str() {
        "csv" => {
            let mut c = pflow_eventlog::CsvConfig::default();
            if let Some(v) = s(a, "case") {
                c.case_id_column = v.into();
            }
            if let Some(v) = s(a, "activity") {
                c.activity_column = v.into();
            }
            if let Some(v) = s(a, "time") {
                c.timestamp_column = v.into();
            }
            if let Some(v) = s(a, "resource") {
                c.resource_column = v.into();
            }
            if let Some(d) = s(a, "delimiter") {
                let mut it = d.chars();
                c.delimiter = match (it.next(), it.next()) {
                    (Some(ch), None) => ch,
                    _ => return Err(Error::BadArgs("delimiter must be one character".into())),
                };
            }
            pflow_eventlog::parse_csv_str(&body, &c).map_err(|e| Error::Parse(e.to_string()))?
        }
        "jsonl" => {
            let mut c = pflow_eventlog::JsonlConfig::default();
            if let Some(v) = s(a, "case") {
                c.case_id_field = v.into();
            }
            if let Some(v) = s(a, "activity") {
                c.activity_field = v.into();
            }
            if let Some(v) = s(a, "time") {
                c.timestamp_field = v.into();
            }
            if let Some(v) = s(a, "resource") {
                c.resource_field = v.into();
            }
            pflow_eventlog::parse_jsonl_str(&body, &c).map_err(|e| Error::Parse(e.to_string()))?
        }
        f => return Err(Error::BadArgs(format!("format {f:?}: csv or jsonl"))),
    };
    if log.num_events() == 0 {
        return Err(Error::Invalid(
            "the log has no events (check the column names)".into(),
        ));
    }
    let summary = eventlog::summary(&log);
    let title = s(a, "title").map(str::to_string).unwrap_or_else(|| {
        format!(
            "Event log ({} cases, {} events)",
            log.num_cases(),
            log.num_events()
        )
    });
    // Inline text is not kept in the source step: the table is the data.
    let mut args = a.clone();
    if let Some(m) = args.as_object_mut() {
        m.remove("text");
    }
    let source = Step {
        command: "data.load_log".into(),
        args,
    };
    let id = sess.add_artifact(title, source, eventlog::to_table(&log));
    Ok(json!({ "artifact": id, "summary": summary }))
}

fn log_info(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = s(a, "log").unwrap();
    let art = sess
        .artifact(id)
        .ok_or_else(|| Error::NotFound(format!("no artifact {id:?}")))?;
    Ok(eventlog::summary(&eventlog::from_artifact(art)?))
}
