//! The headless half of pflow-native: a pure-data [`Document`] (the pflow.xyz
//! editor JSON, kept in document order), a [`Session`] wrapping it with the
//! runtime marking, view and selection, and the command [`registry`] every
//! front end (the egui app, the CLI, the JSON control channel and the MCP
//! server) drives it through.
//!
//! Nothing here depends on egui, so the same crate builds for native,
//! headless and `wasm32-unknown-unknown`.

pub mod analysis;
pub mod artifact;
pub mod cid;
pub mod commands;
pub mod compose;
pub mod conformance;
pub mod doc;
pub mod eventlog;
pub mod export;
pub mod layout;
pub mod learn;
pub mod mining;
pub mod outbound;
pub mod registry;
pub mod session;
pub mod sim;
pub mod statespace;
pub mod stochastic;
pub mod string_diagram;
pub mod sweep;
pub mod systems;

pub use artifact::{Artifact, ArtifactData};
pub use doc::{Arc, Document, NodeKind, Place, Transition};
pub use registry::{ArgSpec, ArgType, CommandSpec, Registry};
pub use session::{Highlight, Session, Settings, Step, Theme, Tool, UiRequest, View};
pub use sim::{Marking, OdeResult};

/// An error from a command, surfaced verbatim to whichever front end ran it.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown command {0}")]
    UnknownCommand(String),
    #[error("{0}")]
    BadArgs(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Io(String),
    #[error("parse: {0}")]
    Parse(String),
}

pub type Result<T> = std::result::Result<T, Error>;
