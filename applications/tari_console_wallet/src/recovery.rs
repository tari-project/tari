// Copyright 2020. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use chrono::offset::Local;
use futures::StreamExt;
use log::*;
use rustyline::Editor;
use std::convert::TryFrom;
use tari_app_utilities::utilities::ExitCodes;
use tari_comms::peer_manager::Peer;
use tari_core::{
    base_node::sync::rpc,
    blocks::BlockHeader,
    proto::base_node::{SyncUtxosRequest, SyncUtxosResponse},
    tari_utilities::{hex::Hex, Hashable},
    transactions::{tari_amount::MicroTari, transaction::TransactionOutput, types::PrivateKey},
};
use tari_key_manager::mnemonic::to_secretkey;
use tari_wallet::WalletSqlite;

pub const LOG_TARGET: &str = "wallet::recovery";

/// Prompt the user to input their seed words in a single line.
pub fn prompt_private_key_from_seed_words() -> Result<PrivateKey, ExitCodes> {
    debug!(target: LOG_TARGET, "Prompting for seed words.");
    let mut rl = Editor::<()>::new();

    loop {
        println!("Recovery Mode");
        println!();
        println!("Type or paste all of your seed words on one line, only separated by spaces.");
        let input = rl.readline(">> ").map_err(|e| ExitCodes::IOError(e.to_string()))?;
        let seed_words: Vec<String> = input.split_whitespace().map(str::to_string).collect();

        match to_secretkey(&seed_words) {
            Ok(key) => break Ok(key),
            Err(e) => {
                debug!(target: LOG_TARGET, "MnemonicError parsing seed words: {}", e);
                println!("Failed to parse seed words! Did you type them correctly?");
                continue;
            },
        }
    }
}

/// Recovers wallet funds by connecting to a given base node peer, downloading the transaction outputs stored in the
/// blockchain, and attempting to rewind them. Any outputs that are successfully rewound are then imported into the
/// wallet.
pub async fn wallet_recovery(wallet: &mut WalletSqlite, base_node: &Peer) -> Result<(), ExitCodes> {
    println!(
        "Connecting to base node with public key: {}",
        base_node.public_key.to_hex()
    );

    let node_id = wallet.comms.node_identity();
    let public_key = node_id.public_key();

    let mut conn = wallet.comms.connectivity().dial_peer(base_node.node_id.clone()).await?;
    let mut client = conn.connect_rpc::<rpc::BaseNodeSyncRpcClient>().await?;
    println!("Base node connected.");

    let latency = client.get_last_request_latency().await?;
    println!("Latency: {} ms.", latency.unwrap_or_default().as_millis());

    let chain_metadata = client.get_chain_metadata().await?;
    let height = chain_metadata.height_of_longest_chain();
    println!("Chain Height: {}.", height);

    let header = client.get_header_by_height(height).await?;
    let header = BlockHeader::try_from(header).map_err(ExitCodes::ConversionError)?;
    let start = 0;
    let end_header_hash = header.hash();
    let request = SyncUtxosRequest { start, end_header_hash };
    let mut output_stream = client.sync_utxos(request).await?;
    println!("Streaming transaction outputs...");

    let mut num_utxos = 0;
    let mut total_amount = MicroTari::from(0);

    while let Some(response) = output_stream.next().await {
        let response: SyncUtxosResponse = response.map_err(|e| ExitCodes::ConversionError(e.to_string()))?;

        let outputs: Vec<TransactionOutput> = response
            .utxos
            .into_iter()
            .filter_map(|utxo| {
                if let Some(output) = utxo.output {
                    TransactionOutput::try_from(output).ok()
                } else {
                    None
                }
            })
            .collect();
        println!("Scanning {} outputs...", outputs.len());

        let unblinded_outputs = wallet.output_manager_service.rewind_outputs(outputs).await?;

        if !unblinded_outputs.is_empty() {
            println!("Importing {} outputs...", unblinded_outputs.len());

            for uo in unblinded_outputs {
                wallet
                    .import_utxo(
                        uo.value,
                        &uo.spending_key,
                        public_key,
                        format!("Recovered on {}.", Local::now()),
                    )
                    .await?;

                num_utxos += 1;
                total_amount += uo.value;
            }
        }
    }

    println!(
        "Recovered and imported {} outputs, with a total value of {}.",
        num_utxos, total_amount
    );

    Ok(())
}
