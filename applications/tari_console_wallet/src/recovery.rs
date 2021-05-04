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
use futures::{FutureExt, StreamExt};
use log::*;
use rustyline::Editor;
use std::cmp;
use tari_app_utilities::utilities::ExitCodes;
use tari_comms::types::CommsPublicKey;
use tari_core::transactions::types::PrivateKey;
use tari_key_manager::mnemonic::to_secretkey;
use tari_wallet::{
    tasks::wallet_recovery::{WalletRecoveryEvent, WalletRecoveryTask},
    WalletSqlite,
};

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

/// Return secret key matching the seed words.
pub fn get_private_key_from_seed_words(seed_words: Vec<String>) -> Result<PrivateKey, ExitCodes> {
    debug!(target: LOG_TARGET, "Return secret key matching the provided seed words");
    match to_secretkey(&seed_words) {
        Ok(key) => Ok(key),
        Err(e) => {
            let err_msg = format!("MnemonicError parsing seed words: {}", e);
            debug!(target: LOG_TARGET, "{}", err_msg);
            Err(ExitCodes::RecoveryError(err_msg))
        },
    }
}

/// Recovers wallet funds by connecting to a given base node peer, downloading the transaction outputs stored in the
/// blockchain, and attempting to rewind them. Any outputs that are successfully rewound are then imported into the
/// wallet.
pub async fn wallet_recovery(wallet: WalletSqlite, peer_seeds: Vec<CommsPublicKey>) -> Result<(), ExitCodes> {
    println!("\nPress Ctrl-C to stop the recovery process\n");
    let mut recovery_task = WalletRecoveryTask::builder()
        .with_peer_seeds(peer_seeds)
        .with_retry_limit(10)
        .build(wallet);

    let mut event_stream = recovery_task.get_event_receiver().fuse();

    let recovery_join_handle = tokio::spawn(recovery_task.run()).fuse();

    // Read recovery task events. The event stream will end once recovery has completed.
    while let Some(event) = event_stream.next().await {
        match event {
            Ok(WalletRecoveryEvent::ConnectingToBaseNode(peer)) => {
                print!("Connecting to base node {}... ", peer);
            },
            Ok(WalletRecoveryEvent::ConnectedToBaseNode(_, latency)) => {
                println!("OK (latency = {:.2?})", latency);
            },
            Ok(WalletRecoveryEvent::Progress(current, total)) => {
                let percentage_progress = ((current as f32) * 100f32 / (total as f32)).round() as u32;
                debug!(
                    target: LOG_TARGET,
                    "{}: Recovery process {}% complete ({} of {} utxos).",
                    Local::now(),
                    percentage_progress,
                    current,
                    total
                );
                println!(
                    "{}: Recovery process {}% complete ({} of {} utxos).",
                    Local::now(),
                    percentage_progress,
                    current,
                    total
                );
            },
            Ok(WalletRecoveryEvent::RecoveryRoundFailed {
                num_retries,
                retry_limit,
            }) => {
                let s = format!("Failed to sync. Attempt {} of {}", num_retries, retry_limit);
                println!("{}", s);
                warn!(target: LOG_TARGET, "{}", s);
            },
            Ok(WalletRecoveryEvent::ConnectionFailedToBaseNode {
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
            Ok(WalletRecoveryEvent::Completed(num_scanned, num_utxos, total_amount, elapsed)) => {
                let stats = format!(
                    "Recovery complete! Scanned = {} in {:.2?} ({} utxos/s), Recovered {} worth {}",
                    num_scanned,
                    elapsed,
                    num_scanned / cmp::max(elapsed.as_secs(), 1),
                    num_utxos,
                    total_amount
                );
                info!(target: LOG_TARGET, "{}", stats);
                println!("{}", stats);
            },
            Err(e) => {
                // Can occur if we read events too slowly (lagging/slow subscriber)
                debug!(target: LOG_TARGET, "Error receiving Wallet recovery events: {}", e);
                continue;
            },
        }
    }

    recovery_join_handle
        .await
        .map_err(|e| ExitCodes::RecoveryError(format!("{}", e)))?
        .map_err(|e| ExitCodes::RecoveryError(format!("{}", e)))
}
