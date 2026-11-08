//! One module per command family: the functions the dispatch in `main` calls and, for a family
//! with subcommands, its own `run`. A family's clap arguments live beside its code.

pub mod adding;
pub mod analysis;
pub mod approvals;
pub mod beans;
pub mod firmware;
pub mod labels;
pub mod orders;
pub mod plans;
pub mod prediction;
pub mod profiles;
pub mod records;
pub mod reports;
pub mod roasted;
pub mod roasters;
pub mod roasts;
pub mod server;
pub mod settings;
pub mod share;
pub mod status;
