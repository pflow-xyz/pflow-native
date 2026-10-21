//! The headless half of pflow-native: a pure-data [`Document`] (the pflow.xyz
//! editor JSON, kept in document order), a [`Session`] wrapping it with the
//! runtime marking, view and selection, and the command [`registry`] every
//! front end (the egui app, the CLI, the JSON control channel and the MCP
//! server) drives it through.
//!
//! Nothing here depends on egui, so the same crate builds for native,
//! headless and `wasm32-unknown-unknown`.

pub mod artifact;
pub mod commands;
pub mod doc;
pub mod export;
pub mod registry;
pub mod session;
pub mod sim;

pub use artifact::{Artifact, ArtifactData};
pub use doc::{Arc, Document, NodeKind, Place, Transition};
pub use registry::{ArgSpec, ArgType, CommandSpec, Registry};
pub use session::{Session, Step, Tool, UiRequest, View};
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
