// Copyright 2021. The Tari Project
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

use crate::{
    error::WalletError,
    output_manager_service::error::{OutputManagerError, OutputManagerStorageError},
    WalletSqlite,
};
use chrono::Utc;
use futures::{channel::mpsc, SinkExt, StreamExt};
use log::*;
use std::{cmp, convert::TryFrom};
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};
use tari_core::{
    base_node::sync::rpc::BaseNodeSyncRpcClient,
    blocks::BlockHeader,
    proto::base_node::{SyncUtxosRequest, SyncUtxosResponse},
    tari_utilities::Hashable,
    transactions::{tari_amount::MicroTari, transaction::TransactionOutput},
};

pub const LOG_TARGET: &str = "wallet::recovery";

pub const RECOVERY_HEIGHT_KEY: &str = "recovery_height_progress";
const RECOVERY_NUM_UTXOS_KEY: &str = "recovery_num_utxos";
const RECOVERY_TOTAL_AMOUNT_KEY: &str = "recovery_total_amount";
const RECOVERY_BATCH_SIZE: u64 = 10;

pub struct WalletRecoveryTask {
    wallet: WalletSqlite,
    base_node_public_key: CommsPublicKey,
    event_sender: Option<mpsc::Sender<WalletRecoveryEvent>>,
    event_receiver: Option<mpsc::Receiver<WalletRecoveryEvent>>,
}

impl WalletRecoveryTask {
    pub fn new(wallet: WalletSqlite, base_node_public_key: CommsPublicKey) -> Self {
        let (event_sender, event_receiver) = mpsc::channel(1000);
        Self {
            wallet,
            base_node_public_key,
            event_sender: Some(event_sender),
            event_receiver: Some(event_receiver),
        }
    }

    pub fn get_event_receiver(&mut self) -> Option<mpsc::Receiver<WalletRecoveryEvent>> {
        self.event_receiver.take()
    }

    pub async fn run(mut self) -> Result<(), WalletError> {
        let mut event_sender = match self.event_sender {
            Some(sender) => sender,
            None => {
                return Err(WalletError::WalletRecoveryError(
                    "No event channel provided".to_string(),
                ))
            },
        };

        let public_key = self.wallet.comms.node_identity().public_key().clone();

        let base_node_node_id = NodeId::from_public_key(&self.base_node_public_key);

        let mut conn = self.wallet.comms.connectivity().dial_peer(base_node_node_id).await?;
        let mut client = conn
            .connect_rpc::<BaseNodeSyncRpcClient>()
            .await
            .map_err(|e| WalletError::WalletRecoveryError(e.to_string()))?;

        event_sender
            .send(WalletRecoveryEvent::ConnectedToBaseNode(
                self.base_node_public_key.clone(),
            ))
            .await
            .map_err(|e| WalletError::WalletRecoveryError(e.to_string()))?;

        loop {
            let chain_metadata = client
                .get_chain_metadata()
                .await
                .map_err(|e| WalletError::WalletRecoveryError(e.to_string()))?;
            let chain_height = chain_metadata.height_of_longest_chain();

            let start_height = match self
                .wallet
                .db
                .get_client_key_value(RECOVERY_HEIGHT_KEY.to_string())
                .await?
            {
                None => {
                    // Set a value in here so that if the recovery fails on the genesis block the client will know a
                    // recover was started. Important on Console wallet that otherwise makes this decision based on the
                    // presence of the data file
                    self.wallet
                        .db
                        .set_client_key_value(RECOVERY_HEIGHT_KEY.to_string(), "0".to_string())
                        .await?;
                    0
                },
                Some(current_height) => match current_height.parse::<u64>() {
                    Ok(h) => h,
                    Err(_) => 0,
                },
            };

            let next_height = cmp::min(start_height + RECOVERY_BATCH_SIZE, chain_height);

            info!(
                target: LOG_TARGET,
                "Wallet recovery attempting to recover from Block {} to Block {} with current chain tip at {}",
                start_height,
                next_height,
                chain_height
            );
            debug!(
                target: LOG_TARGET,
                "Percentage complete: {}%",
                ((start_height as f32) * 100f32 / (chain_height as f32)).round() as u32
            );

            let start_header = client
                .get_header_by_height(start_height)
                .await
                .map_err(|e| WalletError::WalletRecoveryError(e.to_string()))?;
            let start_header = BlockHeader::try_from(start_header).map_err(WalletError::WalletRecoveryError)?;
            let start_mmr_leaf_index = if start_height == 0 {
                0
            } else {
                start_header.output_mmr_size
            };

            let end_header = client
                .get_header_by_height(next_height)
                .await
                .map_err(|e| WalletError::WalletRecoveryError(e.to_string()))?;
            let end_header = BlockHeader::try_from(end_header).map_err(WalletError::WalletRecoveryError)?;

            let end_header_hash = end_header.hash();
            let request = SyncUtxosRequest {
                start: start_mmr_leaf_index,
                end_header_hash,
            };
            let mut output_stream = client
                .sync_utxos(request)
                .await
                .map_err(|e| WalletError::WalletRecoveryError(e.to_string()))?;

            let mut num_utxos = 0;
            let mut total_amount = MicroTari::from(0);

            while let Some(response) = output_stream.next().await {
                let response: SyncUtxosResponse =
                    response.map_err(|e| WalletError::WalletRecoveryError(e.to_string()))?;

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

                let unblinded_outputs = self.wallet.output_manager_service.rewind_outputs(outputs).await?;

                if !unblinded_outputs.is_empty() {
                    for uo in unblinded_outputs {
                        match self
                            .wallet
                            .import_utxo(
                                uo.value,
                                &uo.spending_key,
                                &public_key,
                                format!("Recovered on {}.", Utc::now().naive_utc()),
                            )
                            .await
                        {
                            Ok(_) => {
                                num_utxos += 1;
                                total_amount += uo.value;
                            },
                            Err(WalletError::OutputManagerError(OutputManagerError::OutputManagerStorageError(
                                OutputManagerStorageError::DuplicateOutput,
                            ))) => debug!(target: LOG_TARGET, "Recovered output already in database"),
                            Err(e) => return Err(e),
                        }
                    }
                }
            }

            let current_num_utxos = match self
                .wallet
                .db
                .get_client_key_value(RECOVERY_NUM_UTXOS_KEY.to_string())
                .await?
            {
                None => 0,
                Some(n_str) => match n_str.parse::<u64>() {
                    Ok(n) => n,
                    Err(_) => 0,
                },
            };

            let current_total_amount = match self
                .wallet
                .db
                .get_client_key_value(RECOVERY_TOTAL_AMOUNT_KEY.to_string())
                .await?
            {
                None => MicroTari::from(0),
                Some(a_str) => match a_str.parse::<u64>() {
                    Ok(a) => MicroTari::from(a),
                    Err(_) => MicroTari::from(0),
                },
            };

            if next_height == chain_height {
                let _ = self
                    .wallet
                    .db
                    .clear_client_value(RECOVERY_HEIGHT_KEY.to_string())
                    .await?;
                let _ = self
                    .wallet
                    .db
                    .clear_client_value(RECOVERY_NUM_UTXOS_KEY.to_string())
                    .await?;
                let _ = self
                    .wallet
                    .db
                    .clear_client_value(RECOVERY_TOTAL_AMOUNT_KEY.to_string())
                    .await?;
                info!(
                    target: LOG_TARGET,
                    "Wallet recovery complete. Imported {} outputs, with a total value of {} ",
                    current_num_utxos + num_utxos,
                    current_total_amount + total_amount
                );
                event_sender
                    .send(WalletRecoveryEvent::Progress(chain_height, chain_height))
                    .await
                    .map_err(|e| WalletError::WalletRecoveryError(e.to_string()))?;
                event_sender
                    .send(WalletRecoveryEvent::Completed(
                        current_num_utxos + num_utxos,
                        current_total_amount + total_amount,
                    ))
                    .await
                    .map_err(|e| WalletError::WalletRecoveryError(e.to_string()))?;

                break;
            } else {
                self.wallet
                    .db
                    .set_client_key_value(RECOVERY_HEIGHT_KEY.to_string(), next_height.to_string())
                    .await?;
                self.wallet
                    .db
                    .set_client_key_value(
                        RECOVERY_NUM_UTXOS_KEY.to_string(),
                        (current_num_utxos + num_utxos).to_string(),
                    )
                    .await?;
                self.wallet
                    .db
                    .set_client_key_value(
                        RECOVERY_TOTAL_AMOUNT_KEY.to_string(),
                        (current_total_amount.0 + total_amount.0).to_string(),
                    )
                    .await?;

                if num_utxos > 0 {
                    debug!(
                        target: LOG_TARGET,
                        "Recovered {} outputs with a value of {} in this batch", num_utxos, total_amount
                    );
                }

                event_sender
                    .send(WalletRecoveryEvent::Progress(next_height, chain_height))
                    .await
                    .map_err(|e| WalletError::WalletRecoveryError(e.to_string()))?;
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum WalletRecoveryEvent {
    ConnectedToBaseNode(CommsPublicKey),
    /// Progress of the recovery process (current_block, current_chain_height)
    Progress(u64, u64),
    /// Completed Recovery (Num of Recovered outputs, Value of recovered outputs)
    Completed(u64, MicroTari),
}
