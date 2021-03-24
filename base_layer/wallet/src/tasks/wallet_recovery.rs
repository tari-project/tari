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
use futures::{channel::mpsc, FutureExt, SinkExt, StreamExt};
use log::*;
use std::{cmp, cmp::max, convert::TryFrom, time::Instant};
use tari_comms::{peer_manager::NodeId, protocol::rpc::ClientStreaming, types::CommsPublicKey, PeerConnection};
use tari_core::{
    base_node::sync::rpc::BaseNodeSyncRpcClient,
    blocks::BlockHeader,
    proto::base_node::{ChainMetadata, SyncUtxosRequest, SyncUtxosResponse},
    tari_utilities::Hashable,
    transactions::{tari_amount::MicroTari, transaction::TransactionOutput},
};

use tari_comms::protocol::rpc::RpcStatusCode;
use tokio::time::{delay_for, Duration};

pub const LOG_TARGET: &str = "wallet::recovery";

const RPC_RETRIES_LIMIT: usize = 10;

pub const RECOVERY_HEIGHT_KEY: &str = "recovery_height_progress";
const RECOVERY_NUM_UTXOS_KEY: &str = "recovery_num_utxos";
const RECOVERY_TOTAL_AMOUNT_KEY: &str = "recovery_total_amount";
const RECOVERY_BATCH_SIZE: u64 = 100;

pub struct WalletRecoveryTask {
    wallet: WalletSqlite,
    peer_seed_public_keys: Vec<CommsPublicKey>,
    event_sender: Option<mpsc::Sender<WalletRecoveryEvent>>,
    event_receiver: Option<mpsc::Receiver<WalletRecoveryEvent>>,
    connection_retries_limit: usize,
    connection_retries: usize,
    rpc_retries: usize,
    print_to_console: bool,
    base_node_connection: Option<PeerConnection>,
    base_node_public_key: CommsPublicKey,
    sync_client_initialized: bool,
    sync_client: Option<BaseNodeSyncRpcClient>,
}

impl WalletRecoveryTask {
    pub fn new(wallet: WalletSqlite, peer_seed_public_keys: Vec<CommsPublicKey>) -> Self {
        let (event_sender, event_receiver) = mpsc::channel(1000);
        Self {
            wallet,
            peer_seed_public_keys,
            event_sender: Some(event_sender),
            event_receiver: Some(event_receiver),
            connection_retries_limit: 1,
            connection_retries: 0,
            rpc_retries: 1,
            print_to_console: false,
            base_node_connection: None,
            base_node_public_key: CommsPublicKey::default(),
            sync_client_initialized: false,
            sync_client: None,
        }
    }

    pub fn set_connection_retries_limit(&mut self, connection_retries_limit: usize) {
        self.connection_retries_limit = max(connection_retries_limit, 1);
    }

    pub fn set_print_to_console(&mut self, print_to_console: bool) {
        self.print_to_console = print_to_console;
    }

    pub fn get_event_receiver(&mut self) -> Option<mpsc::Receiver<WalletRecoveryEvent>> {
        self.event_receiver.take()
    }

    async fn get_connected_base_node_public_key(&mut self) -> Result<CommsPublicKey, WalletError> {
        if !self.sync_client_initialized {
            let _ = self.connect_sync_client().await;
        }
        Ok(self.base_node_public_key.clone())
    }

    async fn connect_sync_client(&mut self) -> Result<(), WalletError> {
        let mut shutdown = self.wallet.comms.shutdown_signal().clone();
        if !self.sync_client_initialized {
            trace!(
                target: LOG_TARGET,
                "Peer seed public keys: {:?}",
                self.peer_seed_public_keys
            );
            if self.peer_seed_public_keys.is_empty() {
                return Err(WalletError::WalletRecoveryError(
                    "No base node defined to connect to".to_string(),
                ));
            }
        }
        let mut select_new_base_node = false;
        let mut last_base_node_public_key = if self.sync_client_initialized {
            self.base_node_public_key.clone()
        } else {
            (&self.peer_seed_public_keys[self.peer_seed_public_keys.len() - 1]).clone()
        };
        self.rpc_retries = 1;
        loop {
            // Allow PRC connections to be retried N times before using up a retry
            if self.rpc_retries > RPC_RETRIES_LIMIT {
                self.rpc_retries = 1;
                self.connection_retries += 1;
            }
            if self.connection_retries > self.connection_retries_limit {
                return Err(WalletError::WalletRecoveryError(format!(
                    "Could not connect to base node within the specified number of retries ({})",
                    self.connection_retries_limit
                )));
            }

            let mut reconnect_to_base_node = true;
            if let Some(connection) = self.base_node_connection.clone() {
                reconnect_to_base_node = !connection.is_connected();
                trace!(
                    target: LOG_TARGET,
                    "Base node {} is connected {}",
                    self.base_node_public_key.clone(),
                    connection.is_connected(),
                );
            }

            if reconnect_to_base_node {
                // Select next base node in list to try and connect to, wrapping around
                let mut new_base_node_public_key = last_base_node_public_key.clone();
                if select_new_base_node {
                    for i in 0..(self.peer_seed_public_keys.len()) {
                        if self.peer_seed_public_keys[i] == last_base_node_public_key.clone() {
                            if i != self.peer_seed_public_keys.len() - 1 {
                                new_base_node_public_key = (&self.peer_seed_public_keys[i + 1]).clone();
                            } else {
                                new_base_node_public_key = (&self.peer_seed_public_keys[0]).clone();
                            }
                            last_base_node_public_key = new_base_node_public_key.clone();
                            break;
                        }
                    }
                }

                // Attempt new base node connection
                debug!(
                    target: LOG_TARGET,
                    "Trying to connect to {} (retries {} of {})",
                    new_base_node_public_key,
                    self.connection_retries,
                    self.connection_retries_limit
                );
                if select_new_base_node && self.print_to_console {
                    println!(
                        "Trying to connect to {} (retries {} of {})",
                        new_base_node_public_key.clone(),
                        self.connection_retries,
                        self.connection_retries_limit
                    );
                }
                let base_node_node_id = NodeId::from_public_key(&new_base_node_public_key);
                let mut connectivity_requester = self.wallet.comms.connectivity();
                let delay = delay_for(Duration::from_secs(60));
                futures::select! {
                    dial_result = connectivity_requester.dial_peer(base_node_node_id.clone()).fuse() => {
                        match dial_result {
                            Ok(c) => {
                                self.base_node_connection = Some(c);
                                self.base_node_public_key = new_base_node_public_key.clone();
                                select_new_base_node = false;
                                trace!(
                                    target: LOG_TARGET,
                                    "New base node connection to {}",
                                    self.base_node_public_key.clone(),
                                );
                            },
                            Err(e) => {
                                warn!(
                                    target: LOG_TARGET,
                                    "Base node connection error to {} (retries {} of {}): {}",
                                    new_base_node_public_key.clone(),
                                    self.connection_retries,
                                    self.connection_retries_limit,
                                    e
                                );
                                if self.print_to_console {
                                    println!(
                                        "Base node connection error to {} (retries {} of {}: {})",
                                        new_base_node_public_key.clone(),
                                        self.connection_retries,
                                        self.connection_retries_limit,
                                        e
                                    );
                                }
                                select_new_base_node = true;
                                self.connection_retries += 1;
                                continue;
                            },
                        }
                    },
                    _ = delay.fuse() => {
                        continue;
                    },
                    _ = shutdown => {
                        info!(
                            target: LOG_TARGET,
                            "Wallet recovery shutting down because it received the shutdown signal",
                        );
                        return Err(WalletError::Shutdown)
                    },
                }
            }

            // Attempt new RPC connection to the connected base node
            if let Some(mut connection) = self.base_node_connection.clone() {
                if connection.is_connected() {
                    self.rpc_retries += 1;
                    self.sync_client = match connection
                        .connect_rpc_using_builder(
                            BaseNodeSyncRpcClient::builder().with_deadline(Duration::from_secs(60)),
                        )
                        .await
                    {
                        Ok(c) => Some(c),
                        Err(e) => {
                            warn!(
                                target: LOG_TARGET,
                                "RPC connection error to base node {}: {}", self.base_node_public_key, e
                            );
                            continue;
                        },
                    };
                    if !self.sync_client_initialized {
                        self.sync_client_initialized = true;
                    }
                    debug!(
                        target: LOG_TARGET,
                        "New RPC connection for base node {}",
                        self.base_node_public_key.clone(),
                    );
                    return Ok(());
                }
            };
        }
    }

    async fn get_chain_metadata(&mut self) -> Result<ChainMetadata, WalletError> {
        loop {
            if let Some(ref mut client) = self.sync_client {
                match client
                    .get_chain_metadata()
                    .await
                    .map_err(|e| WalletError::WalletRecoveryError(e.to_string()))
                {
                    Ok(r) => {
                        return Ok(r);
                    },
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "Error communicating to RPC client (get_chain_metadata): {}", e
                        );
                        // Note: `connect_sync_client()` will err with too many connection attempts, exiting loop
                        let _ = self.connect_sync_client().await?;
                        continue;
                    },
                };
            }
        }
    }

    async fn get_header_by_height(&mut self, height: u64) -> Result<tari_core::proto::core::BlockHeader, WalletError> {
        loop {
            if let Some(ref mut client) = self.sync_client {
                match client
                    .get_header_by_height(height)
                    .await
                    .map_err(|e| WalletError::WalletRecoveryError(e.to_string()))
                {
                    Ok(h) => {
                        return Ok(h);
                    },
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "Error communicating to RPC client (get_header_by_height): {}", e
                        );
                        // Note: `connect_sync_client()` will err with too many connection attempts, exiting loop
                        let _ = self.connect_sync_client().await?;
                        continue;
                    },
                };
            }
        }
    }

    async fn get_sync_utxos_stream(
        &mut self,
        request: SyncUtxosRequest,
    ) -> Result<ClientStreaming<SyncUtxosResponse>, WalletError>
    {
        loop {
            if let Some(ref mut client) = self.sync_client {
                match client
                    .sync_utxos(request.clone())
                    .await
                    .map_err(|e| WalletError::WalletRecoveryError(e.to_string()))
                {
                    Ok(s) => {
                        return Ok(s);
                    },
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "Error communicating to RPC client (sync_utxos): {}", e
                        );
                        // Note: `connect_sync_client()` will err with too many connection attempts, exiting loop
                        let _ = self.connect_sync_client().await?;
                        continue;
                    },
                };
            }
        }
    }

    pub async fn run(mut self) -> Result<(), WalletError> {
        let mut event_sender = match self.event_sender.clone() {
            Some(sender) => sender,
            None => {
                return Err(WalletError::WalletRecoveryError(
                    "No event channel provided".to_string(),
                ))
            },
        };

        event_sender
            .send(WalletRecoveryEvent::ConnectedToBaseNode(
                self.get_connected_base_node_public_key().await?,
            ))
            .await
            .map_err(|e| WalletError::WalletRecoveryError(e.to_string()))?;

        let chain_metadata = &self.get_chain_metadata().await?.clone();
        let mut chain_height = chain_metadata.height_of_longest_chain();

        'main: loop {
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

            if start_height + RECOVERY_BATCH_SIZE >= chain_height {
                let chain_metadata = &self.get_chain_metadata().await?;
                chain_height = chain_metadata.height_of_longest_chain();
            }
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

            let timer = Instant::now();
            let start_header = self.get_header_by_height(start_height).await?;
            let start_header = BlockHeader::try_from(start_header).map_err(WalletError::WalletRecoveryError)?;
            let start_mmr_leaf_index = if start_height == 0 {
                0
            } else {
                start_header.output_mmr_size
            };

            let end_header = self.get_header_by_height(next_height).await?;
            let end_header = BlockHeader::try_from(end_header).map_err(WalletError::WalletRecoveryError)?;
            let end_header_hash = end_header.hash();
            let fetch_header_info_time = timer.elapsed().as_millis();

            let request = SyncUtxosRequest {
                start: start_mmr_leaf_index,
                end_header_hash,
            };
            let mut processing_time = 0u128;
            let mut num_utxos = 0;
            let mut total_amount = MicroTari::from(0);
            'stream_utxos: loop {
                let mut output_stream = self.get_sync_utxos_stream(request.clone()).await?;
                while let Some(result) = output_stream.next().await {
                    match result {
                        Ok(response) => {
                            let timer = Instant::now();

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
                                            &self.wallet.comms.node_identity().public_key().clone(),
                                            format!("Recovered on {}.", Utc::now().naive_utc()),
                                        )
                                        .await
                                    {
                                        Ok(_) => {
                                            num_utxos += 1;
                                            total_amount += uo.value;
                                        },
                                        Err(WalletError::OutputManagerError(
                                            OutputManagerError::OutputManagerStorageError(
                                                OutputManagerStorageError::DuplicateOutput,
                                            ),
                                        )) => debug!(target: LOG_TARGET, "Recovered output already in database"),
                                        Err(e) => return Err(e),
                                    }
                                }
                            }
                            processing_time += timer.elapsed().as_millis();
                        },
                        Err(e) => {
                            if e.status_code() == RpcStatusCode::Timeout {
                                debug!(target: LOG_TARGET, "Fetch UTXOs RPC response timeout, retrying...");
                                continue 'stream_utxos;
                            };
                            return Err(WalletError::WalletRecoveryError(e.to_string()));
                        },
                    }
                }
                break 'stream_utxos;
            }
            let fetch_utxos_time = timer.elapsed().as_millis() - fetch_header_info_time - processing_time;
            trace!(
                target: LOG_TARGET,
                "Timings - RPC fetch header info: {} ms, RPC fetch UTXOs: {} ms, Bulletproofs rewinding: {} ms",
                fetch_header_info_time,
                fetch_utxos_time,
                processing_time,
            );

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

                break 'main;
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
