//! Event logs as data: parsed with pflow-eventlog, held in the session as a
//! `table` artifact with canonical columns, so every data command takes a
//! log by artifact id (and an agent can `artifact.put` one).

use pflow_eventlog::{Event, EventLog};
use serde_json::{json, Value};

use crate::artifact::{Artifact, ArtifactData};
use crate::{Error, Result};

/// Canonical event-log table columns. `time` is milliseconds since the
/// epoch (pflow-eventlog's `Timestamp`); `resource`/`lifecycle` may be "".
pub const COLUMNS: [&str; 5] = ["case", "activity", "time", "resource", "lifecycle"];

/// The log as a canonical table, cases in first-seen order, events by time.
pub fn to_table(log: &EventLog) -> ArtifactData {
    let mut rows = vec![];
    for t in log.traces() {
        for e in &t.events {
            rows.push(vec![
                json!(e.case_id),
                json!(e.activity),
                json!(e.timestamp),
                json!(e.resource),
                json!(e.lifecycle),
            ]);
        }
    }
    ArtifactData::Table {
        columns: COLUMNS.iter().map(|c| c.to_string()).collect(),
        rows,
    }
}

/// Reads a log back from a table artifact. Needs `case` and `activity`
/// columns; `time` is optional (row order then defines event order).
pub fn from_artifact(a: &Artifact) -> Result<EventLog> {
    let ArtifactData::Table { columns, rows } = &a.data else {
        return Err(Error::BadArgs(format!(
            "{} is a {}, not an event-log table",
            a.id,
            a.data.kind()
        )));
    };
    let col = |name: &str| columns.iter().position(|c| c == name);
    let (Some(ci), Some(ai)) = (col("case"), col("activity")) else {
        return Err(Error::BadArgs(format!(
            "{} has columns {columns:?}; an event log needs at least \"case\" and \"activity\"",
            a.id
        )));
    };
    let (ti, ri, li) = (col("time"), col("resource"), col("lifecycle"));
    let text = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    let mut log = EventLog::new();
    for (k, row) in rows.iter().enumerate() {
        let time = match ti.map(|i| &row[i]) {
            Some(Value::Number(n)) => n.as_i64().unwrap_or(k as i64),
            Some(Value::String(s)) => {
                pflow_eventlog::parse_timestamp(s, pflow_eventlog::DEFAULT_FORMATS)
                    .ok_or_else(|| Error::Parse(format!("row {k}: unreadable time {s:?}")))?
            }
            _ => k as i64,
        };
        let mut e = Event::new(text(&row[ci]), text(&row[ai]), time);
        if let Some(i) = ri {
            e.resource = text(&row[i]);
        }
        if let Some(i) = li {
            e.lifecycle = text(&row[i]);
        }
        log.add_event(e);
    }
    log.sort_traces();
    Ok(log)
}

/// pflow-eventlog's summary as JSON.
pub fn summary(log: &EventLog) -> Value {
    let s = log.summarize();
    json!({
        "cases": log.num_cases(),
        "events": log.num_events(),
        "activities": log.activities(),
        "resources": log.resources(),
        "variants": s.num_variants,
        "mean_case_length": s.avg_case_length,
        "start_time": s.start_time,
        "end_time": s.end_time,
        "mean_case_duration_ms": s.avg_case_duration_millis,
    })
}
