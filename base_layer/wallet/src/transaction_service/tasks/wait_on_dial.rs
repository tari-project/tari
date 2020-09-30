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

use crate::output_manager_service::TxId;
use log::*;
use std::time::Duration;
use tari_comms::types::CommsPublicKey;
use tari_comms_dht::outbound::MessageSendStates;

const LOG_TARGET: &str = "wallet::transaction_service::tasks";
const LOG_TARGET_STRESS: &str = "stress_test::transaction_service::tasks";

/// This function contains the logic to wait on a dial and send of a queued message
pub async fn wait_on_dial(
    send_states: MessageSendStates,
    tx_id: TxId,
    destination_pubkey: CommsPublicKey,
    message: &str,
    direct_send_timeout: Duration,
) -> bool
{
    if send_states.len() == 1 {
        debug!(
            target: LOG_TARGET,
            "{} (TxId: {}) Direct Send to {} queued with Message {}",
            message,
            tx_id,
            destination_pubkey,
            send_states[0].tag,
        );
        debug!(
            target: LOG_TARGET_STRESS,
            "{} (TxId: {}) Direct Send to {} queued with Message {}",
            message,
            tx_id,
            destination_pubkey,
            send_states[0].tag,
        );
        let (sent, failed) = send_states.wait_n_timeout(direct_send_timeout, 1).await;
        if !sent.is_empty() {
            info!(
                target: LOG_TARGET,
                "Direct Send process for {} TX_ID: {} was successful with Message: {}", message, tx_id, sent[0]
            );
            debug!(
                target: LOG_TARGET_STRESS,
                "Direct Send process for {} TX_ID: {} was successful with Message: {}", message, tx_id, sent[0]
            );
            true
        } else {
            if failed.is_empty() {
                warn!(
                    target: LOG_TARGET,
                    "Direct Send process for {} TX_ID: {} timed out", message, tx_id
                );
                debug!(
                    target: LOG_TARGET_STRESS,
                    "Direct Send process for {} TX_ID: {} timed out", message, tx_id
                );
            } else {
                warn!(
                    target: LOG_TARGET,
                    "Direct Send process for {} TX_ID: {} and Message {} was unsuccessful and no message was sent",
                    message,
                    tx_id,
                    failed[0]
                );
                debug!(
                    target: LOG_TARGET_STRESS,
                    "Direct Send process for {} TX_ID: {} and Message {} was unsuccessful and no message was sent",
                    message,
                    tx_id,
                    failed[0]
                );
            }
            false
        }
    } else {
        warn!(target: LOG_TARGET, "{} Send Direct for TxID: {} failed", message, tx_id);
        false
    }
}
