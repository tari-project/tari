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
use log::*;
use minotari_wallet::{utxo_scanner_service::handle::UtxoScannerEvent, WalletSqlite};
use rustyline::Editor;
use tari_common::exit_codes::{ExitCode, ExitError};
use tari_key_manager::{cipher_seed::CipherSeed, mnemonic::Mnemonic, SeedWords};
use tari_utilities::{Hidden, SafePassword};
use tokio::sync::broadcast;

pub const LOG_TARGET: &str = "wallet::recovery";

/// Prompt the user to input their seed words in a single line.
// Sometimes clippy thinks this function is dead code, but it is used in the CLI.
#[allow(dead_code)]
pub fn prompt_private_key_from_seed_words() -> Result<CipherSeed, ExitError> {
    debug!(target: LOG_TARGET, "Prompting for seed words.");
    let mut rl = Editor::<()>::new();

    loop {
        println!("Recovery Mode");
        println!();
        println!("Type or paste all of your seed words on one line, only separated by spaces.");
        let input = Hidden::hide(rl.readline(">> ").map_err(|e| ExitError::new(ExitCode::IOError, e))?);
        let seed_words: SeedWords = SeedWords::new(
            input
                .reveal()
                .split_whitespace()
                .map(|s| Hidden::hide(s.to_string()))
                .collect(),
        );

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
pub fn get_seed_from_seed_words(
    seed_words: &SeedWords,
    passphrase: Option<SafePassword>,
) -> Result<CipherSeed, ExitError> {
    debug!(target: LOG_TARGET, "Return seed derived from the provided seed words");
    match CipherSeed::from_mnemonic(seed_words, passphrase) {
        Ok(seed) => Ok(seed),
        Err(e) => {
            let err_msg = format!("MnemonicError parsing seed words: {}", e);
            warn!(target: LOG_TARGET, "{}", err_msg);
            Err(ExitError::new(ExitCode::RecoveryError, err_msg))
        },
    }
}

/// Recovers wallet funds by connecting to a given base node peer, downloading the transaction outputs stored in the
/// blockchain, and attempting to rewind them. Any outputs that are successfully rewound are then imported into the
/// wallet.
#[allow(clippy::too_many_lines)]
pub async fn wallet_recovery(wallet: &WalletSqlite, retry_limit: usize) -> Result<(), ExitError> {
    println!("\nPress Ctrl-C to stop the recovery process\n");

    let mut event_stream = wallet.utxo_scanner_service.clone().get_event_receiver();

    // Read recovery task events. The event stream will end once recovery has completed.
    let mut failed_events = 0;
    loop {
        if failed_events > retry_limit {
            let err_msg = format!("Recovery process failed after {} attempts. Exiting.", retry_limit);
            error!(target: LOG_TARGET, "{}", err_msg);
            return Err(ExitError::new(ExitCode::RecoveryError, err_msg));
        }
        match event_stream.recv().await {
            Ok(UtxoScannerEvent::Progress {
                current_height,
                tip_height,
                ..
            }) => {
                // its going to fail if the tip height is 0, meaning if you scanned up to 0, you are done
                let percentage_progress = (current_height * 100).checked_div(tip_height).unwrap_or(100);
                let msg = format!(
                    "{}: Recovery process {}% complete (Block {} of {}).",
                    Local::now(),
                    percentage_progress,
                    current_height,
                    tip_height
                );
                println!("{}", msg);
                debug!(target: LOG_TARGET, "{}", msg);
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
                failed_events += 1;
            },
            Ok(UtxoScannerEvent::Completed {
                final_height,
                time_taken,
                ..
            }) => {
                let rate = (final_height as f32) * 1000f32 / (time_taken.as_millis() as f32);
                let stats = format!(
                    "Recovery complete! Scanned {} blocks in {:.2?} ({:.2?} blocks/s)",
                    final_height, time_taken, rate
                );
                info!(target: LOG_TARGET, "{}", stats);
                println!("{}", stats);
                break;
            },
            Err(e @ broadcast::error::RecvError::Lagged(_)) => {
                debug!(target: LOG_TARGET, "Error receiving Wallet recovery events: {}", e);
                continue;
            },
            Err(broadcast::error::RecvError::Closed) => {
                debug!(target: LOG_TARGET, "Wallet Recovery exiting");
                break;
            },
        }
    }
    Ok(())
}
