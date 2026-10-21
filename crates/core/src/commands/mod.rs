//! Command sets by namespace. Each file exports `SET`; [`ALL`] is what the
//! core registry is built from.

use crate::registry::CommandSpec;

pub mod command;
pub mod edit;
pub mod export;
pub mod file;
pub mod macros;
pub mod net;
pub mod sim;
pub mod view;

pub const ALL: &[&[CommandSpec]] = &[
    file::SET,
    edit::SET,
    net::SET,
    sim::SET,
    view::SET,
    export::SET,
    macros::SET,
    command::SET,
];
