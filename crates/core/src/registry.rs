//! The command registry: every action any front end can take, with its
//! label, menu, shortcut, argument spec, enablement and handler. Command
//! sets live in [`crate::commands`] by namespace; a crate that adds an area
//! builds a bigger [`Registry`] with [`Registry::with`].

use std::sync::OnceLock;

use serde_json::{json, Map, Value};

use crate::session::Session;
use crate::{Error, Result};

pub type Run = fn(&mut Session, &Value) -> Result<Value>;
pub type Enabled = fn(&Session) -> std::result::Result<(), String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgType {
    Str,
    Num,
    Int,
    Bool,
    StrList,
    /// A number list; a bare number is accepted as a one-element list.
    NumList,
    /// Any JSON value.
    Json,
}

impl ArgType {
    fn schema(self) -> Value {
        match self {
            ArgType::Str => json!({"type": "string"}),
            ArgType::Num => json!({"type": "number"}),
            ArgType::Int => json!({"type": "integer"}),
            ArgType::Bool => json!({"type": "boolean"}),
            ArgType::StrList => json!({"type": "array", "items": {"type": "string"}}),
            ArgType::NumList => json!({"oneOf": [
                {"type": "number"},
                {"type": "array", "items": {"type": "number"}}
            ]}),
            ArgType::Json => json!({}),
        }
    }

    fn accepts(self, v: &Value) -> bool {
        match self {
            ArgType::Str => v.is_string(),
            ArgType::Num => v.is_number(),
            ArgType::Int => v.is_i64() || v.as_f64().is_some_and(|f| f.fract() == 0.0),
            ArgType::Bool => v.is_boolean(),
            ArgType::StrList => v.as_array().is_some_and(|a| a.iter().all(Value::is_string)),
            ArgType::NumList => {
                v.is_number() || v.as_array().is_some_and(|a| a.iter().all(Value::is_number))
            }
            ArgType::Json => true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ArgSpec {
    pub name: &'static str,
    pub ty: ArgType,
    pub required: bool,
    pub help: &'static str,
}

pub const fn arg(name: &'static str, ty: ArgType, help: &'static str) -> ArgSpec {
    ArgSpec {
        name,
        ty,
        required: true,
        help,
    }
}

pub const fn opt(name: &'static str, ty: ArgType, help: &'static str) -> ArgSpec {
    ArgSpec {
        name,
        ty,
        required: false,
        help,
    }
}

#[derive(Clone, Copy)]
pub struct CommandSpec {
    /// `<namespace>.<verb>`.
    pub id: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    /// Top-level menu the app lists it under; "" keeps it out of menus.
    pub menu: &'static str,
    /// e.g. "Cmd+Z", "Delete", "1". Parsed by the app.
    pub shortcut: Option<&'static str>,
    pub args: &'static [ArgSpec],
    pub enabled: Enabled,
    pub run: Run,
    /// Recorded in the journal (and so in macros). False for queries and
    /// view-only commands.
    pub journal: bool,
}

pub fn always(_: &Session) -> std::result::Result<(), String> {
    Ok(())
}

impl CommandSpec {
    /// Validates `args` against the spec; `null` means `{}`.
    pub fn check_args(&self, args: Value) -> Result<Value> {
        let obj = match args {
            Value::Null => Map::new(),
            Value::Object(m) => m,
            other => {
                return Err(Error::BadArgs(format!(
                    "{}: args must be an object, got {other}",
                    self.id
                )))
            }
        };
        for k in obj.keys() {
            if !self.args.iter().any(|a| a.name == k) {
                let known: Vec<&str> = self.args.iter().map(|a| a.name).collect();
                return Err(Error::BadArgs(format!(
                    "{}: unknown argument {k:?}; takes {known:?}",
                    self.id
                )));
            }
        }
        for a in self.args {
            match obj.get(a.name) {
                None | Some(Value::Null) if a.required => {
                    return Err(Error::BadArgs(format!(
                        "{}: missing required argument {:?} ({})",
                        self.id, a.name, a.help
                    )))
                }
                Some(v) if !v.is_null() && !a.ty.accepts(v) => {
                    return Err(Error::BadArgs(format!(
                        "{}: argument {:?} should be {:?}, got {v}",
                        self.id, a.name, a.ty
                    )))
                }
                _ => {}
            }
        }
        Ok(Value::Object(obj))
    }

    /// JSON Schema for the args object (for MCP and `command.list`).
    pub fn args_schema(&self) -> Value {
        let mut props = Map::new();
        for a in self.args {
            let mut s = a.ty.schema();
            s["description"] = json!(a.help);
            props.insert(a.name.into(), s);
        }
        let required: Vec<&str> = self
            .args
            .iter()
            .filter(|a| a.required)
            .map(|a| a.name)
            .collect();
        json!({
            "type": "object",
            "properties": props,
            "required": required,
            "additionalProperties": false,
        })
    }

    pub fn describe(&self, s: &Session) -> Value {
        json!({
            "id": self.id,
            "label": self.label,
            "help": self.help,
            "menu": self.menu,
            "shortcut": self.shortcut,
            "args": self.args_schema(),
            "journal": self.journal,
            "enabled": (self.enabled)(s).err().map_or(json!(true), |why| json!(why)),
        })
    }
}

pub struct Registry {
    commands: Vec<CommandSpec>,
}

impl Registry {
    pub fn new(sets: &[&[CommandSpec]]) -> Self {
        let mut r = Registry { commands: vec![] };
        for set in sets {
            r = r.with(set);
        }
        r
    }

    /// Adds a command set. Panics on a duplicate id: that is a build bug.
    pub fn with(mut self, set: &[CommandSpec]) -> Self {
        for c in set {
            assert!(self.find(c.id).is_none(), "duplicate command id {}", c.id);
            self.commands.push(*c);
        }
        self
    }

    pub fn commands(&self) -> &[CommandSpec] {
        &self.commands
    }

    pub fn find(&self, id: &str) -> Option<&CommandSpec> {
        self.commands.iter().find(|c| c.id == id)
    }

    /// Menu names in first-seen order.
    pub fn menus(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = vec![];
        for c in &self.commands {
            if !c.menu.is_empty() && !out.contains(&c.menu) {
                out.push(c.menu);
            }
        }
        out
    }
}

/// The registry of everything in this crate.
pub fn registry() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(|| Registry::new(crate::commands::ALL))
}

// ---- argument accessors for handlers (args are already validated) ----

pub fn s<'a>(a: &'a Value, k: &str) -> Option<&'a str> {
    a.get(k).and_then(Value::as_str)
}

pub fn n(a: &Value, k: &str) -> Option<f64> {
    a.get(k).and_then(Value::as_f64)
}

pub fn b(a: &Value, k: &str) -> Option<bool> {
    a.get(k).and_then(Value::as_bool)
}

pub fn strs(a: &Value, k: &str) -> Option<Vec<String>> {
    a.get(k).and_then(Value::as_array).map(|v| {
        v.iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect()
    })
}

pub fn nums(a: &Value, k: &str) -> Option<Vec<f64>> {
    match a.get(k)? {
        Value::Number(x) => Some(vec![x.as_f64()?]),
        Value::Array(v) => Some(v.iter().filter_map(Value::as_f64).collect()),
        _ => None,
    }
}

pub fn req<'a>(a: &'a Value, k: &str) -> &'a str {
    s(a, k).expect("validated required string")
}
