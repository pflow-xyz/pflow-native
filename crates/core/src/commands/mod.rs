//! Command sets by namespace. Each file exports `SET`; [`ALL`] is what the
//! core registry is built from.

use crate::registry::CommandSpec;

pub mod analysis;
pub mod cid;
pub mod clipboard;
pub mod command;
pub mod compose;
pub mod conformance;
pub mod data;
pub mod details;
pub mod diagram;
pub mod edit;
pub mod export;
pub mod file;
pub mod fileio;
pub mod learn;
pub mod macros;
pub mod models;
pub mod net;
pub mod opt;
pub mod overlay;
pub mod settings;
pub mod sim;
pub mod statespace;
pub mod stochastic;
pub mod view;

pub const ALL: &[&[CommandSpec]] = &[
    file::SET,
    fileio::SET,
    cid::SET,
    edit::SET,
    clipboard::SET,
    net::SET,
    details::SET,
    sim::SET,
    opt::SET,
    analysis::SET,
    stochastic::SET,
    statespace::SET,
    view::SET,
    diagram::SET,
    overlay::SET,
    settings::SET,
    data::SET,
    learn::SET,
    conformance::SET,
    export::SET,
    models::SET,
    compose::SET,
    macros::SET,
    command::SET,
];
