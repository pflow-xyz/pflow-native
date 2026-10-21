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
    let custom_columns = ["case", "activity", "time", "resource"]
        .iter()
        .any(|k| a.get(*k).is_some());
    if format == "csv" && !custom_columns {
        if let Some(table) = canonical_csv(&body, s(a, "delimiter"))? {
            return store(sess, a, path, table);
        }
    }
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
    store(sess, a, path, eventlog::to_table(&log))
}

/// Stores an event-log table as an artifact (validating it reads as a log).
fn store(
    sess: &mut Session,
    a: &Value,
    _path: Option<&str>,
    table: crate::artifact::ArtifactData,
) -> Result<Value> {
    let probe = crate::artifact::Artifact {
        id: "new".into(),
        title: String::new(),
        source: Step {
            command: String::new(),
            args: Value::Null,
        },
        data: table,
    };
    let log = eventlog::from_artifact(&probe)?;
    if log.num_events() == 0 {
        return Err(Error::Invalid("the log has no events".into()));
    }
    let table = probe.data;
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
    let id = sess.add_artifact(title, source, table);
    Ok(json!({ "artifact": id, "summary": summary }))
}

/// A CSV in this app's own event-log table shape (as `export.artifact`
/// writes it: columns case, activity, time in ms, resource, lifecycle),
/// which pflow-eventlog's reader would not take (it wants case_id /
/// timestamp and has no numeric time format). `None` when the header is
/// not that shape.
fn canonical_csv(
    body: &str,
    delimiter: Option<&str>,
) -> Result<Option<crate::artifact::ArtifactData>> {
    let d = delimiter.and_then(|d| d.chars().next()).unwrap_or(',');
    let mut lines = body.lines().filter(|l| !l.trim().is_empty());
    let Some(head) = lines.next() else {
        return Ok(None);
    };
    let columns = split_csv(head, d);
    if !(columns.iter().any(|c| c == "case") && columns.iter().any(|c| c == "activity")) {
        return Ok(None);
    }
    let mut rows = vec![];
    for (k, line) in lines.enumerate() {
        let cells = split_csv(line, d);
        if cells.len() != columns.len() {
            return Err(Error::Parse(format!(
                "row {}: {} cells for {} columns",
                k + 1,
                cells.len(),
                columns.len()
            )));
        }
        rows.push(
            cells
                .into_iter()
                .zip(&columns)
                .map(|(c, col)| match (col.as_str(), c.parse::<i64>()) {
                    ("time", Ok(n)) => json!(n),
                    _ => json!(c),
                })
                .collect(),
        );
    }
    Ok(Some(crate::artifact::ArtifactData::Table { columns, rows }))
}

/// One CSV line, honoring double-quoted fields with "" escapes.
fn split_csv(line: &str, d: char) -> Vec<String> {
    let (mut out, mut cur, mut quoted) = (vec![], String::new(), false);
    let mut it = line.chars().peekable();
    while let Some(ch) = it.next() {
        match ch {
            '"' if quoted && it.peek() == Some(&'"') => {
                cur.push('"');
                it.next();
            }
            '"' => quoted = !quoted,
            c if c == d && !quoted => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

fn log_info(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = s(a, "log").unwrap();
    let art = sess
        .artifact(id)
        .ok_or_else(|| Error::NotFound(format!("no artifact {id:?}")))?;
    Ok(eventlog::summary(&eventlog::from_artifact(art)?))
}
