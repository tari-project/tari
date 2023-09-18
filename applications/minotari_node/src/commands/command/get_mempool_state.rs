//  Copyright 2022, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use tari_utilities::hex::Hex;

use super::{CommandContext, HandleCommand};

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
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
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
                let fee = match tx.body.get_total_fee() {
                    Ok(fee) => format!("{}", fee),
                    Err(e) => e.to_string(),
                };
                println!(
                    "    {} Fee: {}, Outputs: {}, Kernels: {}, Inputs: {}, features_and_scripts: {} bytes",
                    tx_sig,
                    fee,
                    tx.body.outputs().len(),
                    tx.body.kernels().len(),
                    tx.body.inputs().len(),
                    tx.body.sum_features_and_scripts_size()?,
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
