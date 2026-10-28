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
#[cfg(feature = "services")]
pub mod auth;
pub mod cid;
pub mod commands;
pub mod compose;
pub mod composition;
pub mod conformance;
pub mod control_auth;
pub mod doc;
pub mod eventlog;
pub mod export;
pub mod game;
pub mod jobs;
pub mod layout;
pub mod learn;
pub mod lens;
pub mod mining;
#[cfg(feature = "rnn")]
pub(crate) mod mlp_net;
pub mod outbound;
#[cfg(feature = "rnn")]
pub(crate) mod par;
pub mod plan_game;
pub mod provenance;
pub mod registry;
#[cfg(feature = "rnn")]
pub(crate) mod rnn_net;
pub mod search_plan;
pub mod session;
pub mod sim;
#[cfg(feature = "services")]
pub mod simclient;
pub mod statespace;
pub mod stochastic;
pub mod store;
pub mod string_diagram;
pub mod sweep;
pub mod systems;
pub mod zk;

pub use artifact::{Artifact, ArtifactData};
pub use doc::{Arc, Document, NodeKind, Place, Transition};
pub use registry::{ArgSpec, ArgType, CommandSpec, Registry};
pub use session::{
    CompositionView, Focus, Highlight, Note, NoteTarget, Session, Settings, Step, Theme, Tool,
    UiRequest, View,
};
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

/// A refusal from pflow-metamodel (a net that breaks a game's rules, a
/// transition that cannot fire) is the command's input at fault.
impl From<pflow_metamodel::Error> for Error {
    fn from(e: pflow_metamodel::Error) -> Self {
        Error::Invalid(e.to_string())
    }
}
