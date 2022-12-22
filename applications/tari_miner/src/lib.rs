mod cli;
pub use cli::Cli;
use tari_common::exit_codes::ExitError;
mod run_miner;
use run_miner::start_miner;
mod config;
mod difficulty;
mod errors;
mod miner;
mod stratum;
mod utils;

pub async fn run_miner(cli: Cli) -> Result<(), ExitError> {
    start_miner(cli).await
}
