//! Native headless CLI for TradingAgent.

mod args;
mod config;
mod run;

use thiserror::Error;

pub use args::CliArgs;
pub use config::{DataEndpoints, EnvSource, ProcessEnv, ResolvedCliConfig, resolve_config};
pub use run::{build_run_input, run_from_args};

#[derive(Debug, Error)]
pub enum CliError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("I/O error: {0}")]
    Io(String),
    #[error("runtime error: {0}")]
    Runtime(String),
}
