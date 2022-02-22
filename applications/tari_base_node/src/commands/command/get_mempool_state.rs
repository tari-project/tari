use anyhow::Error;
use async_trait::async_trait;
use chrono::Utc;
use clap::Parser;
use tari_comms::peer_manager::{PeerFeatures, PeerQuery};
use tari_core::base_node::state_machine_service::states::PeerMetadata;
use tari_utilities::hex::Hex;

use super::{CommandContext, HandleCommand};
use crate::{table::Table, utils::format_duration_basic};

/// Retrieves your mempools state
#[derive(Debug, Parser)]
pub struct Args {}

/// Filters and retrieves details about transactions from the mempool's state
#[derive(Debug, Parser)]
pub struct ArgsTx {
    filter: String,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.get_mempool_state(None).await
    }
}

#[async_trait]
impl HandleCommand<ArgsTx> for CommandContext {
    async fn handle_command(&mut self, args: ArgsTx) -> Result<(), Error> {
        self.get_mempool_state(Some(args.filter)).await
    }
}

impl CommandContext {
    /// Function to process the get-mempool-state command
    pub async fn get_mempool_state(&mut self, filter: Option<String>) -> Result<(), Error> {
        let state = self.mempool_service.get_mempool_state().await?;
        println!("----------------- Mempool -----------------");
        println!("--- Unconfirmed Pool ---");
        for tx in &state.unconfirmed_pool {
            let tx_sig = tx
                .first_kernel_excess_sig()
                .map(|sig| sig.get_signature().to_hex())
                .unwrap_or_else(|| "N/A".to_string());
            if let Some(ref filter) = filter {
                if !tx_sig.contains(filter) {
                    println!("--- TX: {} ---", tx_sig);
                    println!("{}", tx.body);
                    continue;
                }
            } else {
                println!(
                    "    {} Fee: {}, Outputs: {}, Kernels: {}, Inputs: {}, metadata: {} bytes",
                    tx_sig,
                    tx.body.get_total_fee(),
                    tx.body.outputs().len(),
                    tx.body.kernels().len(),
                    tx.body.inputs().len(),
                    tx.body.sum_metadata_size(),
                );
            }
        }
        if filter.is_none() {
            println!("--- Reorg Pool ---");
            for excess_sig in &state.reorg_pool {
                println!("    {}", excess_sig.get_signature().to_hex());
            }
        }
        Ok(())
    }
}
