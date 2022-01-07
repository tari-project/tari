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
use futures::FutureExt;
use log::*;
use rustyline::Editor;
use tari_common::exit_codes::ExitCodes;
use tari_crypto::tari_utilities::hex::Hex;
use tari_key_manager::{cipher_seed::CipherSeed, mnemonic::Mnemonic};
use tari_shutdown::Shutdown;
use tari_wallet::{
    storage::sqlite_db::wallet::WalletSqliteDatabase,
    utxo_scanner_service::{handle::UtxoScannerEvent, service::UtxoScannerService},
    WalletSqlite,
};
use tokio::sync::broadcast;

use crate::wallet_modes::PeerConfig;

pub const LOG_TARGET: &str = "wallet::recovery";

/// Prompt the user to input their seed words in a single line.
pub fn prompt_private_key_from_seed_words() -> Result<CipherSeed, ExitCodes> {
    debug!(target: LOG_TARGET, "Prompting for seed words.");
    let mut rl = Editor::<()>::new();

    loop {
        println!("Recovery Mode");
        println!();
        println!("Type or paste all of your seed words on one line, only separated by spaces.");
        let input = rl.readline(">> ").map_err(|e| ExitCodes::IOError(e.to_string()))?;
        let seed_words: Vec<String> = input.split_whitespace().map(str::to_string).collect();

        match CipherSeed::from_mnemonic(&seed_words, None) {
            Ok(seed) => break Ok(seed),
            Err(e) => {
                debug!(target: LOG_TARGET, "MnemonicError parsing seed words: {}", e);
                println!("Failed to parse seed words! Did you type them correctly?");
                continue;
            },
        }
    }
}

/// Return seed matching the seed words.
pub fn get_seed_from_seed_words(seed_words: Vec<String>) -> Result<CipherSeed, ExitCodes> {
    debug!(target: LOG_TARGET, "Return seed derived from the provided seed words");
    match CipherSeed::from_mnemonic(&seed_words, None) {
        Ok(seed) => Ok(seed),
        Err(e) => {
            let err_msg = format!("MnemonicError parsing seed words: {}", e);
            warn!(target: LOG_TARGET, "{}", err_msg);
            Err(ExitCodes::RecoveryError(err_msg))
        },
    }
}

/// Recovers wallet funds by connecting to a given base node peer, downloading the transaction outputs stored in the
/// blockchain, and attempting to rewind them. Any outputs that are successfully rewound are then imported into the
/// wallet.
pub async fn wallet_recovery(
    wallet: &WalletSqlite,
    base_node_config: &PeerConfig,
    retry_limit: usize,
) -> Result<(), ExitCodes> {
    println!("\nPress Ctrl-C to stop the recovery process\n");
    // We dont care about the shutdown signal here, so we just create one
    let shutdown = Shutdown::new();
    let shutdown_signal = shutdown.to_signal();

    let peers = base_node_config.get_all_peers();

    let peer_manager = wallet.comms.peer_manager();
    let mut peer_public_keys = Vec::with_capacity(peers.len());
    for peer in peers {
        debug!(
            target: LOG_TARGET,
            "Peer added: {} (NodeId: {})",
            peer.public_key.to_hex(),
            peer.node_id.to_hex()
        );
        peer_public_keys.push(peer.public_key.clone());
        peer_manager
            .add_peer(peer)
            .await
            .map_err(|err| ExitCodes::NetworkError(err.to_string()))?;
    }

    let mut recovery_task = UtxoScannerService::<WalletSqliteDatabase>::builder()
        .with_peers(peer_public_keys)
        // Do not make this a small number as wallet recovery needs to be resilient
        .with_retry_limit(retry_limit)
        .build_with_wallet(wallet, shutdown_signal);

    let mut event_stream = recovery_task.get_event_receiver();

    let recovery_join_handle = tokio::spawn(recovery_task.run()).fuse();

    // Read recovery task events. The event stream will end once recovery has completed.
    loop {
        match event_stream.recv().await {
            Ok(UtxoScannerEvent::ConnectingToBaseNode(peer)) => {
                println!("Connecting to base node {}... ", peer);
            },
            Ok(UtxoScannerEvent::ConnectedToBaseNode(_, latency)) => {
                println!("OK (latency = {:.2?})", latency);
            },
            Ok(UtxoScannerEvent::Progress {
                current_height,
                tip_height,
            }) => {
                let percentage_progress = ((current_height as f32) * 100f32 / (tip_height as f32)).round() as u32;
                debug!(
                    target: LOG_TARGET,
                    "{}: Recovery process {}% complete (Block {} of {}).",
                    Local::now(),
                    percentage_progress,
                    current_height,
                    tip_height
                );
                println!(
                    "{}: Recovery process {}% complete (Block {} of {}).",
                    Local::now(),
                    percentage_progress,
                    current_height,
                    tip_height
                );
            },
            Ok(UtxoScannerEvent::ScanningRoundFailed {
                num_retries,
                retry_limit,
                error,
            }) => {
                let s = format!(
                    "Attempt {}/{}: Failed to complete wallet recovery {}.",
                    num_retries, retry_limit, error
                );
                println!("{}", s);
                warn!(target: LOG_TARGET, "{}", s);
            },
            Ok(UtxoScannerEvent::ConnectionFailedToBaseNode {
                peer,
                num_retries,
                retry_limit,
                error,
            }) => {
                let s = format!(
                    "Base node connection error to {} (retries {} of {}: {})",
                    peer, num_retries, retry_limit, error
                );
                println!("{}", s);
                warn!(target: LOG_TARGET, "{}", s);
            },
            Ok(UtxoScannerEvent::Completed {
                final_height,
                num_recovered,
                value_recovered,
                time_taken,
            }) => {
                let rate = (final_height as f32) * 1000f32 / (time_taken.as_millis() as f32);
                let stats = format!(
                    "Recovery complete! Scanned {} blocks in {:.2?} ({:.2?} blocks/s), Recovered {} outputs worth {}",
                    final_height, time_taken, rate, num_recovered, value_recovered
                );
                info!(target: LOG_TARGET, "{}", stats);
                println!("{}", stats);
            },
            Err(e @ broadcast::error::RecvError::Lagged(_)) => {
                debug!(target: LOG_TARGET, "Error receiving Wallet recovery events: {}", e);
                continue;
            },
            Err(broadcast::error::RecvError::Closed) => {
                break;
            },
            Ok(UtxoScannerEvent::ScanningFailed) => {
                error!(target: LOG_TARGET, "Wallet Recovery process failed and is exiting");
            },
        }
    }

    recovery_join_handle
        .await
        .map_err(|e| ExitCodes::RecoveryError(format!("{}", e)))?
        .map_err(|e| ExitCodes::RecoveryError(format!("{}", e)))
}
