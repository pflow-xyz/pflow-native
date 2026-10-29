//! Command sets by namespace. Each file exports `SET`; [`ALL`] is what the
//! core registry is built from. Sets that talk to pflow.xyz, sim.pflow.xyz
//! or GitHub exist only with the `services` feature; the experimental RNN
//! set only with `rnn`.

use crate::registry::CommandSpec;

pub mod analysis;
#[cfg(feature = "services")]
pub mod auth;
pub mod bundle;
pub mod bundle_doc;
pub mod cid;
pub mod clipboard;
pub mod command;
pub mod compose;
pub mod composition_receipt;
pub mod conformance;
pub mod content;
pub mod data;
pub mod details;
pub mod diagram;
pub mod edit;
pub mod export;
pub mod file;
pub mod fileio;
pub mod game;
pub mod job;
pub mod learn;
pub mod macros;
pub mod models;
pub mod net;
pub mod note;
pub mod opt;
#[cfg(feature = "services")]
pub mod outbound;
pub mod overlay;
pub mod plan;
pub mod provenance;
#[cfg(feature = "services")]
pub mod remote;
#[cfg(feature = "rnn")]
pub mod rnn;
pub mod session;
pub mod settings;
pub mod sim;
pub mod statespace;
pub mod stochastic;
pub mod store;
pub mod systems;
pub mod view;

pub const ALL: &[&[CommandSpec]] = &[
    file::SET,
    fileio::SET,
    #[cfg(feature = "services")]
    fileio::SERVICE_SET,
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
    note::SET,
    diagram::SET,
    overlay::SET,
    settings::SET,
    data::SET,
    learn::SET,
    game::SET,
    #[cfg(feature = "rnn")]
    rnn::SET,
    conformance::SET,
    export::SET,
    models::SET,
    compose::SET,
    bundle::SET,
    bundle_doc::SET,
    content::SET,
    composition_receipt::SET,
    plan::SET,
    systems::SET,
    #[cfg(feature = "services")]
    outbound::SET,
    #[cfg(feature = "services")]
    auth::SET,
    #[cfg(feature = "services")]
    remote::SET,
    store::SET,
    provenance::SET,
    macros::SET,
    job::SET,
    session::SET,
    command::SET,
];
