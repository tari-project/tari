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
use futures::StreamExt;
use log::*;
use std::{
    convert::TryFrom,
    str::FromStr,
    time::{Duration, Instant},
};
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey, PeerConnection};
use tari_core::{
    base_node::sync::rpc::BaseNodeSyncRpcClient,
    blocks::BlockHeader,
    crypto::tari_utilities::hex::Hex,
    proto,
    proto::base_node::SyncUtxosRequest,
    tari_utilities::Hashable,
    transactions::{tari_amount::MicroTari, transaction::TransactionOutput},
};
use tokio::sync::broadcast;

pub const LOG_TARGET: &str = "wallet::recovery";

pub const RECOVERY_HEIGHT_KEY: &str = "recovery/height-progress";
const RECOVERY_NUM_UTXOS_KEY: &str = "recovery/num-utxos";
const RECOVERY_UTXO_INDEX_KEY: &str = "recovery/utxos-index";
const RECOVERY_TOTAL_AMOUNT_KEY: &str = "recovery/total-amount";

#[derive(Debug, Default, Clone)]
pub struct WalletRecoveryTaskBuilder {
    retry_limit: usize,
    peer_seeds: Vec<CommsPublicKey>,
}

impl WalletRecoveryTaskBuilder {
    /// Set the maximum number of times we retry recovery. A failed recovery is counted as _all_ peers have failed.
    /// i.e. worst-case number of recovery attempts = number of sync peers * retry limit
    pub fn with_retry_limit(&mut self, limit: usize) -> &mut Self {
        self.retry_limit = limit;
        self
    }

    pub fn with_peer_seeds(&mut self, peer_seeds: Vec<CommsPublicKey>) -> &mut Self {
        self.peer_seeds = peer_seeds;
        self
    }

    pub fn build(&mut self, wallet: WalletSqlite) -> WalletRecoveryTask {
        WalletRecoveryTask::new(wallet, self.peer_seeds.drain(..).collect(), self.retry_limit)
    }
}

pub struct WalletRecoveryTask {
    wallet: WalletSqlite,
    event_sender: broadcast::Sender<WalletRecoveryEvent>,
    retry_limit: usize,
    num_retries: usize,
    peer_seeds: Vec<CommsPublicKey>,
    peer_index: usize,
}

impl WalletRecoveryTask {
    fn new(wallet: WalletSqlite, peer_seeds: Vec<CommsPublicKey>, retry_limit: usize) -> Self {
        let (event_sender, _) = broadcast::channel(100);
        Self {
            wallet,
            peer_seeds,
            event_sender,
            retry_limit,
            peer_index: 0,
            num_retries: 0,
        }
    }

    pub fn builder() -> WalletRecoveryTaskBuilder {
        WalletRecoveryTaskBuilder::default()
    }

    pub fn get_event_receiver(&mut self) -> broadcast::Receiver<WalletRecoveryEvent> {
        self.event_sender.subscribe()
    }

    fn get_next_peer(&mut self) -> Option<NodeId> {
        let peer = self.peer_seeds.get(self.peer_index).map(NodeId::from_public_key);
        self.peer_index += 1;
        peer
    }

    pub async fn run(mut self) -> Result<(), WalletError> {
        loop {
            match self.get_next_peer() {
                Some(peer) => match self.attempt_sync(peer.clone()).await {
                    Ok((total_scanned, final_utxo_pos, elapsed)) => {
                        info!(target: LOG_TARGET, "Recovery successful to UTXO #{}", final_utxo_pos);
                        self.finalize(total_scanned, final_utxo_pos, elapsed).await?;
                        return Ok(());
                    },
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "Failed to sync wallet from base node {}: {}", peer, e
                        );

                        continue;
                    },
                },
                None => {
                    self.publish_event(WalletRecoveryEvent::RecoveryRoundFailed {
                        num_retries: self.num_retries,
                        retry_limit: self.retry_limit,
                    });

                    if self.num_retries >= self.retry_limit {
                        return Err(WalletError::WalletRecoveryError(format!(
                            "Failed to recover wallet after {} attempt(s) using all {} sync peer(s). Aborting...",
                            self.num_retries,
                            self.peer_seeds.len()
                        )));
                    }

                    self.num_retries += 1;
                    // Reset peer index to try connect to the first peer again
                    self.peer_index = 0;
                },
            }
        }
    }

    async fn finalize(&self, total_scanned: u64, final_utxo_pos: u64, elapsed: Duration) -> Result<(), WalletError> {
        let num_recovered = self.get_metadata(RecoveryMetadataKey::NumUtxos).await?.unwrap_or(0);
        let total_amount = self
            .get_metadata(RecoveryMetadataKey::TotalAmount)
            .await?
            .unwrap_or_else(|| 0.into());

        self.clear_metadata(RecoveryMetadataKey::Height).await?;
        self.clear_metadata(RecoveryMetadataKey::NumUtxos).await?;
        self.clear_metadata(RecoveryMetadataKey::TotalAmount).await?;
        self.clear_metadata(RecoveryMetadataKey::UtxoIndex).await?;

        self.publish_event(WalletRecoveryEvent::Progress(final_utxo_pos, final_utxo_pos));
        self.publish_event(WalletRecoveryEvent::Completed(
            total_scanned,
            num_recovered,
            total_amount,
            elapsed,
        ));

        Ok(())
    }

    async fn connect_to_peer(&mut self, peer: NodeId) -> Result<PeerConnection, WalletError> {
        self.publish_event(WalletRecoveryEvent::ConnectingToBaseNode(peer.clone()));
        match self.wallet.comms.connectivity().dial_peer(peer.clone()).await {
            Ok(conn) => Ok(conn),
            Err(e) => {
                self.publish_event(WalletRecoveryEvent::ConnectionFailedToBaseNode {
                    peer,
                    num_retries: self.num_retries,
                    retry_limit: self.retry_limit,
                    error: e.to_string(),
                });

                Err(e.into())
            },
        }
    }

    async fn attempt_sync(&mut self, peer: NodeId) -> Result<(u64, u64, Duration), WalletError> {
        let mut connection = self.connect_to_peer(peer.clone()).await?;

        let mut client = connection
            .connect_rpc_using_builder(BaseNodeSyncRpcClient::builder().with_deadline(Duration::from_secs(60)))
            .await
            .map_err(to_wallet_recovery_error)?;

        let latency = client
            .get_last_request_latency()
            .await
            .map_err(to_wallet_recovery_error)?;
        self.publish_event(WalletRecoveryEvent::ConnectedToBaseNode(
            peer.clone(),
            latency.unwrap_or_default(),
        ));

        let timer = Instant::now();
        let mut total_scanned = 0u64;
        loop {
            let start_index = self.get_start_utxo_mmr_pos().await?;
            let tip_header = self.get_chain_tip_header(&mut client, &peer).await?;
            let output_mmr_size = tip_header.output_mmr_size;
            debug!(
                target: LOG_TARGET,
                "Checking if all UTXOs are synced (start_index = {}, output_mmr_size = {}, height = {}, tip_hash = {})",
                start_index,
                output_mmr_size,
                tip_header.height,
                tip_header.hash().to_hex()
            );

            // start_index could be greater than output_mmr_size if we switch to a new peer that is behind the original
            // peer. In the common case, we wait for start index.
            if start_index >= output_mmr_size - 1 {
                debug!(
                    target: LOG_TARGET,
                    "Sync complete to UTXO #{} in {:.2?}",
                    start_index,
                    timer.elapsed()
                );
                return Ok((total_scanned, start_index, timer.elapsed()));
            }

            let num_scanned = self.recover_utxos(&mut client, start_index, tip_header).await?;
            debug!(
                target: LOG_TARGET,
                "Round completed UTXO #{} in {:.2?} ({} scanned)",
                output_mmr_size,
                timer.elapsed(),
                num_scanned
            );
            total_scanned += num_scanned;
        }
    }

    async fn get_chain_tip_header(
        &self,
        client: &mut BaseNodeSyncRpcClient,
        peer: &NodeId,
    ) -> Result<BlockHeader, WalletError>
    {
        let chain_metadata = client.get_chain_metadata().await.map_err(to_wallet_recovery_error)?;
        if chain_metadata.effective_pruned_height > 0 {
            return Err(WalletError::WalletRecoveryError(format!(
                "Node {} is not an archival node",
                peer
            )));
        }
        let chain_height = chain_metadata.height_of_longest_chain();
        let end_header = client
            .get_header_by_height(chain_height)
            .await
            .map_err(to_wallet_recovery_error)?;
        let end_header = BlockHeader::try_from(end_header).map_err(to_wallet_recovery_error)?;

        Ok(end_header)
    }

    async fn get_start_utxo_mmr_pos(&self) -> Result<u64, WalletError> {
        let previous_sync_height = self
            .get_metadata::<u64>(RecoveryMetadataKey::Height)
            .await
            .ok()
            .flatten();
        let previous_utxo_index = self
            .get_metadata::<u64>(RecoveryMetadataKey::UtxoIndex)
            .await
            .ok()
            .flatten();

        if previous_sync_height.is_none() || previous_utxo_index.is_none() {
            // Set a value in here so that if the recovery fails on the genesis block the client will know a
            // recover was started. Important on Console wallet that otherwise makes this decision based on the
            // presence of the data file
            self.set_metadata(RecoveryMetadataKey::Height, 0u64).await?;
            self.set_metadata(RecoveryMetadataKey::UtxoIndex, 0u64).await?;
        }

        Ok(previous_utxo_index.unwrap_or(0u64))
    }

    async fn recover_utxos(
        &mut self,
        client: &mut BaseNodeSyncRpcClient,
        start_mmr_leaf_index: u64,
        end_header: BlockHeader,
    ) -> Result<u64, WalletError>
    {
        info!(
            target: LOG_TARGET,
            "Wallet recovery attempting to recover from UTXO #{} to #{} (height {})",
            start_mmr_leaf_index,
            end_header.output_mmr_size,
            end_header.height
        );

        let end_header_hash = end_header.hash();
        let end_header_size = end_header.output_mmr_size;

        let mut num_recovered = 0u64;
        let mut total_amount = MicroTari::from(0);
        let mut total_scanned = 0;

        self.publish_event(WalletRecoveryEvent::Progress(start_mmr_leaf_index, end_header_size - 1));
        let request = SyncUtxosRequest {
            start: start_mmr_leaf_index,
            end_header_hash,
            include_pruned_utxos: false,
            include_deleted_bitmaps: false,
        };

        let utxo_stream = client.sync_utxos(request).await.map_err(to_wallet_recovery_error)?;
        // We download in chunks just because rewind_outputs works with multiple outputs (and could parallelized
        // rewinding)
        let mut utxo_stream = utxo_stream.chunks(10);
        let mut last_utxo_index = 0u64;
        let mut iteration_count = 0u64;
        while let Some(response) = utxo_stream.next().await {
            let response: Vec<proto::base_node::SyncUtxosResponse> = response
                .into_iter()
                .map(|v| v.map_err(to_wallet_recovery_error))
                .collect::<Result<Vec<_>, _>>()?;

            let current_utxo_index = response
                // Assumes correct ordering which is otherwise not required for this protocol
                .last()
                .ok_or_else(|| {
                    WalletError::WalletRecoveryError("Invalid response from base node: response was empty".to_string())
                })?
                .mmr_index;
            if current_utxo_index < last_utxo_index {
                return Err(WalletError::WalletRecoveryError(
                    "Invalid response from base node: mmr index must be non-decreasing".to_string(),
                ));
            }
            last_utxo_index = current_utxo_index;

            let outputs = response
                .into_iter()
                .filter_map(|utxo| {
                    utxo.into_utxo()
                        .and_then(|o| o.utxo)
                        .and_then(|utxo| utxo.into_transaction_output())
                        .map(|output| TransactionOutput::try_from(output).map_err(to_wallet_recovery_error))
                })
                .collect::<Result<Vec<_>, _>>()?;

            total_scanned += outputs.len();
            // Reduce the number of db hits by only persisting progress every N iterations
            const COMMIT_EVERY_N: u64 = 100;
            if iteration_count % COMMIT_EVERY_N == 0 || current_utxo_index >= end_header_size - 1 {
                self.publish_event(WalletRecoveryEvent::Progress(current_utxo_index, end_header_size - 1));
                self.set_metadata(RecoveryMetadataKey::UtxoIndex, current_utxo_index)
                    .await?;
            }

            iteration_count += 1;
            let unblinded_outputs = self.wallet.output_manager_service.rewind_outputs(outputs).await?;
            if unblinded_outputs.is_empty() {
                continue;
            }

            let source_public_key = self.wallet.comms.node_identity_ref().public_key().clone();

            for uo in unblinded_outputs {
                match self
                    .wallet
                    .import_utxo(
                        uo.value,
                        &uo.spending_key,
                        &source_public_key,
                        uo.features,
                        format!("Recovered on {}.", Utc::now().naive_utc()),
                    )
                    .await
                {
                    Ok(_) => {
                        num_recovered = num_recovered.saturating_add(1);
                        total_amount += uo.value;
                    },
                    Err(WalletError::OutputManagerError(OutputManagerError::OutputManagerStorageError(
                        OutputManagerStorageError::DuplicateOutput,
                    ))) => warn!(target: LOG_TARGET, "Recovered output already in database"),
                    Err(e) => return Err(e),
                }
            }
        }

        self.set_metadata(RecoveryMetadataKey::Height, end_header.height)
            .await?;

        let current_num_utxos = self.get_metadata(RecoveryMetadataKey::NumUtxos).await?.unwrap_or(0u64);
        self.set_metadata(RecoveryMetadataKey::NumUtxos, current_num_utxos + num_recovered)
            .await?;

        let current_total_amount = self
            .get_metadata::<MicroTari>(RecoveryMetadataKey::TotalAmount)
            .await?
            .unwrap_or_else(|| 0.into());

        self.set_metadata(RecoveryMetadataKey::UtxoIndex, last_utxo_index)
            .await?;
        self.set_metadata(
            RecoveryMetadataKey::TotalAmount,
            (current_total_amount + total_amount).as_u64(),
        )
        .await?;

        self.publish_event(WalletRecoveryEvent::Progress(end_header_size - 1, end_header_size - 1));

        Ok(total_scanned as u64)
    }

    async fn set_metadata<T: ToString>(&self, key: RecoveryMetadataKey, value: T) -> Result<(), WalletError> {
        self.wallet
            .db
            .set_client_key_value(key.as_key_str().to_string(), value.to_string())
            .await?;
        Ok(())
    }

    async fn get_metadata<T>(&self, key: RecoveryMetadataKey) -> Result<Option<T>, WalletError>
    where
        T: FromStr,
        T::Err: ToString,
    {
        let value = self
            .wallet
            .db
            .get_client_key_from_str(key.as_key_str().to_string())
            .await?;
        Ok(value)
    }

    async fn clear_metadata(&self, key: RecoveryMetadataKey) -> Result<(), WalletError> {
        self.wallet.db.clear_client_value(key.as_key_str().to_string()).await?;
        Ok(())
    }

    fn publish_event(&self, event: WalletRecoveryEvent) {
        let _ = self.event_sender.send(event);
    }
}

#[derive(Debug, Clone)]
enum RecoveryMetadataKey {
    TotalAmount,
    NumUtxos,
    UtxoIndex,
    Height,
}

impl RecoveryMetadataKey {
    pub fn as_key_str(&self) -> &'static str {
        use RecoveryMetadataKey::*;
        match self {
            TotalAmount => RECOVERY_TOTAL_AMOUNT_KEY,
            NumUtxos => RECOVERY_NUM_UTXOS_KEY,
            UtxoIndex => RECOVERY_UTXO_INDEX_KEY,
            Height => RECOVERY_HEIGHT_KEY,
        }
    }
}

#[derive(Debug, Clone)]
pub enum WalletRecoveryEvent {
    ConnectingToBaseNode(NodeId),
    ConnectedToBaseNode(NodeId, Duration),
    ConnectionFailedToBaseNode {
        peer: NodeId,
        num_retries: usize,
        retry_limit: usize,
        error: String,
    },
    RecoveryRoundFailed {
        num_retries: usize,
        retry_limit: usize,
    },
    /// Progress of the recovery process (current_block, current_chain_height)
    Progress(u64, u64),
    /// Completed Recovery (Number scanned, Num of Recovered outputs, Value of recovered outputs, Time taken)
    Completed(u64, u64, MicroTari, Duration),
}

// TODO: Replace this with WalletRecoveryError error object
fn to_wallet_recovery_error<T: ToString>(err: T) -> WalletError {
    WalletError::WalletRecoveryError(err.to_string())
}
