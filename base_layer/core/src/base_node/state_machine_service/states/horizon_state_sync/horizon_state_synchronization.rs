//  Copyright 2020, The Tari Project
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

use super::error::HorizonSyncError;
use crate::{
    base_node::{
        state_machine_service::{
            states::events_and_states::{HorizonSyncInfo, HorizonSyncStatus, StateInfo},
            BaseNodeStateMachine,
        },
        sync::rpc,
    },
    blocks::BlockHeader,
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend, ChainStorageError, MmrTree, PrunedOutput},
    proto::base_node::{SyncKernelsRequest, SyncUtxosRequest, SyncUtxosResponse},
    transactions::{
        transaction::{TransactionKernel, TransactionOutput},
        types::{HashDigest, HashOutput, RangeProofService},
    },
};
use croaring::Bitmap;
use futures::StreamExt;
use log::*;
use std::convert::TryInto;
use tari_comms::PeerConnection;
use tari_crypto::{
    commitment::HomomorphicCommitment,
    tari_utilities::{hex::Hex, Hashable},
};
use tari_mmr::{MerkleMountainRange, MutableMmr};

const LOG_TARGET: &str = "c::bn::state_machine_service::states::horizon_state_sync";

pub struct HorizonStateSynchronization<'a, B: BlockchainBackend> {
    shared: &'a mut BaseNodeStateMachine<B>,
    sync_peer: PeerConnection,
    horizon_sync_height: u64,
    prover: &'a RangeProofService,
    num_kernels: u64,
    num_outputs: u64,
}

impl<'a, B: BlockchainBackend + 'static> HorizonStateSynchronization<'a, B> {
    pub fn new(
        shared: &'a mut BaseNodeStateMachine<B>,
        sync_peer: PeerConnection,
        horizon_sync_height: u64,
        prover: &'a RangeProofService,
    ) -> Self
    {
        Self {
            shared,
            sync_peer,
            horizon_sync_height,
            prover,
            num_kernels: 0,
            num_outputs: 0,
        }
    }

    pub async fn synchronize(&mut self) -> Result<(), HorizonSyncError> {
        debug!(
            target: LOG_TARGET,
            "Preparing database for horizon sync to height (#{})", self.horizon_sync_height
        );
        let header = self.db().fetch_header(self.horizon_sync_height).await?.ok_or_else(|| {
            ChainStorageError::ValueNotFound {
                entity: "Header".to_string(),
                field: "height".to_string(),
                value: self.horizon_sync_height.to_string(),
            }
        })?;

        match self.begin_sync(&header).await {
            Ok(_) => match self.finalize_horizon_sync().await {
                Ok(_) => Ok(()),
                Err(err) => {
                    warn!(target: LOG_TARGET, "Error during sync:{}", err);
                    Err(err)
                },
            },
            Err(err) => {
                warn!(target: LOG_TARGET, "Error during sync:{}", err);
                Err(err)
            },
        }
    }

    async fn begin_sync(&mut self, to_header: &BlockHeader) -> Result<(), HorizonSyncError> {
        debug!(target: LOG_TARGET, "Synchronizing kernels");
        self.synchronize_kernels(to_header).await?;
        debug!(target: LOG_TARGET, "Synchronizing outputs");
        self.synchronize_outputs(to_header).await?;
        Ok(())
    }

    async fn synchronize_kernels(&mut self, to_header: &BlockHeader) -> Result<(), HorizonSyncError> {
        let local_num_kernels = self.db().fetch_mmr_size(MmrTree::Kernel).await?;

        let remote_num_kernels = to_header.kernel_mmr_size;
        self.num_kernels = remote_num_kernels;

        if local_num_kernels >= remote_num_kernels {
            debug!(target: LOG_TARGET, "Local kernel set already synchronized");
            return Ok(());
        }

        let info = HorizonSyncInfo::new(
            vec![self.sync_peer.peer_node_id().clone()],
            HorizonSyncStatus::Kernels(local_num_kernels, remote_num_kernels),
        );
        self.shared.set_state_info(StateInfo::HorizonSync(info));

        debug!(
            target: LOG_TARGET,
            "Requesting kernels from {} to {} ({} remaining)",
            local_num_kernels,
            remote_num_kernels,
            remote_num_kernels - local_num_kernels,
        );

        self.sync_kernel_nodes(local_num_kernels, remote_num_kernels, to_header.hash())
            .await
    }

    async fn sync_kernel_nodes(&mut self, start: u64, end: u64, end_hash: HashOutput) -> Result<(), HorizonSyncError> {
        let mut client = self.sync_peer.connect_rpc::<rpc::BaseNodeSyncRpcClient>().await?;
        let latency = client.get_last_request_latency().await?;
        debug!(
            target: LOG_TARGET,
            "Initiating kernel sync with peer `{}` `{}` (latency = {}ms)",
            self.sync_peer.peer_node_id(),
            self.sync_peer.address(),
            latency.unwrap_or_default().as_millis()
        );

        let req = SyncKernelsRequest {
            start,
            end_header_hash: end_hash,
        };
        let mut kernel_stream = client.sync_kernels(req).await?;

        let mut current_header = self.shared.db.fetch_header_containing_kernel_mmr(start + 1).await?;
        debug!(
            target: LOG_TARGET,
            "Found current header in progress for kernels at mmr pos: {} height: {}",
            start,
            current_header.height()
        );
        let mut kernels = vec![];
        let db = self.shared.db.clone();
        let mut txn = db.write_transaction();
        let mut mmr_position = start;
        while let Some(kernel) = kernel_stream.next().await {
            let kernel: TransactionKernel = kernel?.try_into().map_err(HorizonSyncError::ConversionError)?;
            kernel
                .verify_signature()
                .map_err(HorizonSyncError::InvalidKernelSignature)?;

            kernels.push(kernel.clone());
            txn.insert_kernel_via_horizon_sync(kernel, current_header.hash().clone(), mmr_position as u32);
            if mmr_position == current_header.header.kernel_mmr_size - 1 {
                debug!(
                    target: LOG_TARGET,
                    "Checking header {}, added {} kernels",
                    current_header.header.height,
                    kernels.len()
                );
                // Validate root
                let block_data = self
                    .shared
                    .db
                    .fetch_block_accumulated_data(current_header.header.prev_hash.clone())
                    .await?;
                let kernel_pruned_set = block_data.dissolve().0;
                let mut kernel_mmr = MerkleMountainRange::<HashDigest, _>::new(kernel_pruned_set);

                let mut kernel_sum = HomomorphicCommitment::default();
                for kernel in kernels.drain(..) {
                    kernel_sum = &kernel.excess + &kernel_sum;
                    kernel_mmr.push(kernel.hash())?;
                }

                let mmr_root = kernel_mmr.get_merkle_root()?;
                if mmr_root != current_header.header.kernel_mr {
                    return Err(HorizonSyncError::InvalidMmrRoot {
                        mmr_tree: MmrTree::Kernel,
                        at_height: current_header.height(),
                        expected_hex: current_header.header.kernel_mr.to_hex(),
                        actual_hex: mmr_root.to_hex(),
                    });
                }

                txn.update_pruned_hash_set(
                    MmrTree::Kernel,
                    current_header.hash().clone(),
                    kernel_mmr.get_pruned_hash_set()?,
                );

                txn.commit().await?;
                if mmr_position < end - 1 {
                    current_header = self.shared.db.fetch_chain_header(current_header.height() + 1).await?;
                }
            }
            mmr_position += 1;

            if mmr_position % 100 == 0 || mmr_position == self.num_kernels {
                let info = HorizonSyncInfo::new(
                    vec![self.sync_peer.peer_node_id().clone()],
                    HorizonSyncStatus::Kernels(mmr_position, self.num_kernels),
                );
                self.shared.set_state_info(StateInfo::HorizonSync(info));
            }
        }

        if mmr_position != end {
            return Err(HorizonSyncError::IncorrectResponse(
                "Sync node did not send all kernels requested".to_string(),
            ));
        }
        Ok(())
    }

    async fn synchronize_outputs(&mut self, to_header: &BlockHeader) -> Result<(), HorizonSyncError> {
        let local_num_outputs = self.db().fetch_mmr_size(MmrTree::Utxo).await?;

        let remote_num_outputs = to_header.output_mmr_size;
        self.num_outputs = remote_num_outputs;

        if local_num_outputs >= remote_num_outputs {
            debug!(target: LOG_TARGET, "Local output set already synchronized");
            return Ok(());
        }

        let info = HorizonSyncInfo::new(
            vec![self.sync_peer.peer_node_id().clone()],
            HorizonSyncStatus::Outputs(local_num_outputs, self.num_outputs),
        );
        self.shared.set_state_info(StateInfo::HorizonSync(info));

        debug!(
            target: LOG_TARGET,
            "Requesting outputs from {} to {} ({} remaining)",
            local_num_outputs,
            remote_num_outputs,
            remote_num_outputs - local_num_outputs,
        );

        self.sync_output_nodes(local_num_outputs, remote_num_outputs, to_header.hash())
            .await
    }

    async fn sync_output_nodes(&mut self, start: u64, end: u64, end_hash: HashOutput) -> Result<(), HorizonSyncError> {
        let mut client = self.sync_peer.connect_rpc::<rpc::BaseNodeSyncRpcClient>().await?;
        let latency = client.get_last_request_latency().await?;
        debug!(
            target: LOG_TARGET,
            "Initiating output sync with peer `{}` (latency = {}ms)",
            self.sync_peer.peer_node_id(),
            latency.unwrap_or_default().as_millis()
        );

        let req = SyncUtxosRequest {
            start,
            end_header_hash: end_hash,
            include_deleted_bitmaps: true,
            include_pruned_utxos: true,
        };
        let mut output_stream = client.sync_utxos(req).await?;

        let mut current_header = self.shared.db.fetch_header_containing_utxo_mmr(start + 1).await?;
        debug!(
            target: LOG_TARGET,
            "Found current header in progress for utxos at mmr pos: {} height:{}",
            start,
            current_header.height()
        );
        let mut output_hashes = vec![];
        let mut rp_hashes = vec![];
        let db = self.shared.db.clone();
        let mut txn = db.write_transaction();
        let mut unpruned_outputs = vec![];
        let mut mmr_position = start;
        let mut height_utxo_counter = 0;
        let mut height_txo_counter = 0;
        while let Some(response) = output_stream.next().await {
            let res: SyncUtxosResponse = response?;
            debug!(
                target: LOG_TARGET,
                "UTXOs response received from sync peer: ({} outputs, {} deleted bitmaps)",
                res.utxos.len(),
                res.deleted_bitmaps.len()
            );
            let (utxos, mut deleted_bitmaps) = (res.utxos, res.deleted_bitmaps.into_iter());
            for utxo in utxos {
                if let Some(output) = utxo.output {
                    height_utxo_counter += 1;
                    let output: TransactionOutput = output.try_into().map_err(HorizonSyncError::ConversionError)?;
                    output_hashes.push(output.hash());
                    rp_hashes.push(output.proof().hash());
                    unpruned_outputs.push(output.clone());
                    txn.insert_output_via_horizon_sync(output, current_header.hash().clone(), mmr_position as u32);
                } else {
                    height_txo_counter += 1;
                    output_hashes.push(utxo.hash.clone());
                    rp_hashes.push(utxo.rangeproof_hash.clone());
                    txn.insert_pruned_output_via_horizon_sync(
                        utxo.hash,
                        utxo.rangeproof_hash,
                        current_header.hash().clone(),
                        mmr_position as u32,
                    );
                }

                if mmr_position == current_header.header.output_mmr_size - 1 {
                    trace!(
                        target: LOG_TARGET,
                        "Checking header {}, added {} utxos, added {} txos)",
                        current_header.header.height,
                        height_utxo_counter,
                        height_txo_counter
                    );
                    height_txo_counter = 0;
                    height_utxo_counter = 0;
                    // Validate root
                    let block_data = self
                        .shared
                        .db
                        .fetch_block_accumulated_data(current_header.header.prev_hash.clone())
                        .await?;

                    let (_, output_pruned_set, rp_pruned_set, deleted) = block_data.dissolve();
                    let mut output_mmr = MerkleMountainRange::<HashDigest, _>::new(output_pruned_set);
                    let mut proof_mmr = MerkleMountainRange::<HashDigest, _>::new(rp_pruned_set);

                    for hash in output_hashes.drain(..) {
                        output_mmr.push(hash)?;
                    }

                    for hash in rp_hashes.drain(..) {
                        proof_mmr.push(hash)?;
                    }

                    let deleted_diff = deleted_bitmaps.next();
                    if deleted_diff.is_none() {
                        return Err(HorizonSyncError::IncorrectResponse(format!(
                            "No deleted bitmap was provided for the header at height:{}",
                            current_header.height()
                        )));
                    }

                    let bitmap = Bitmap::deserialize(&deleted_diff.unwrap());
                    let deleted = deleted.or(&bitmap);
                    let pruned_output_set = output_mmr.get_pruned_hash_set()?;
                    let output_mmr = MutableMmr::<HashDigest, _>::new(pruned_output_set.clone(), deleted)?;

                    let mmr_root = output_mmr.get_merkle_root()?;
                    if mmr_root != current_header.header.output_mr {
                        return Err(HorizonSyncError::InvalidMmrRoot {
                            mmr_tree: MmrTree::Utxo,
                            at_height: current_header.height(),
                            expected_hex: current_header.header.output_mr.to_hex(),
                            actual_hex: mmr_root.to_hex(),
                        });
                    }
                    let mmr_root = proof_mmr.get_merkle_root()?;
                    if mmr_root != current_header.header.range_proof_mr {
                        return Err(HorizonSyncError::InvalidMmrRoot {
                            mmr_tree: MmrTree::RangeProof,
                            at_height: current_header.height(),
                            expected_hex: current_header.header.range_proof_mr.to_hex(),
                            actual_hex: mmr_root.to_hex(),
                        });
                    }
                    // Validate rangeproofs if the MMR matches
                    for o in unpruned_outputs.drain(..) {
                        o.verify_range_proof(self.prover)
                            .map_err(|err| HorizonSyncError::InvalidRangeProof(o.hash().to_hex(), err.to_string()))?;
                    }

                    txn.update_pruned_hash_set(MmrTree::Utxo, current_header.hash().clone(), pruned_output_set);
                    txn.update_pruned_hash_set(
                        MmrTree::RangeProof,
                        current_header.hash().clone(),
                        proof_mmr.get_pruned_hash_set()?,
                    );
                    txn.update_deleted_with_diff(current_header.hash().clone(), output_mmr.deleted().clone());

                    txn.commit().await?;
                    if mmr_position < end - 1 {
                        current_header = self.shared.db.fetch_chain_header(current_header.height() + 1).await?;
                    }
                }
                mmr_position += 1;

                if mmr_position % 100 == 0 || mmr_position == self.num_outputs {
                    let info = HorizonSyncInfo::new(
                        vec![self.sync_peer.peer_node_id().clone()],
                        HorizonSyncStatus::Outputs(mmr_position, self.num_outputs),
                    );
                    self.shared.set_state_info(StateInfo::HorizonSync(info));
                }
            }
        }
        if mmr_position != end {
            return Err(HorizonSyncError::IncorrectResponse(
                "Sync node did not send all utxos requested".to_string(),
            ));
        }
        Ok(())
    }

    // Finalize the horizon state synchronization by setting the chain metadata to the local tip and committing
    // the horizon state to the blockchain backend.
    async fn finalize_horizon_sync(&mut self) -> Result<(), HorizonSyncError> {
        debug!(target: LOG_TARGET, "Validating horizon state");

        let info = HorizonSyncInfo::new(
            vec![self.sync_peer.peer_node_id().clone()],
            HorizonSyncStatus::Finalizing,
        );
        self.shared.set_state_info(StateInfo::HorizonSync(info));

        let header = self.shared.db.fetch_chain_header(self.horizon_sync_height).await?;
        let mut pruned_utxo_sum = HomomorphicCommitment::default();
        let mut pruned_kernel_sum = HomomorphicCommitment::default();

        let mut prev_mmr = 0;
        let mut prev_kernel_mmr = 0;
        for h in 0..=header.height() {
            let curr_header = self.db().fetch_chain_header(h).await?;

            trace!(
                target: LOG_TARGET,
                "Fetching utxos from db: height:{}, header.output_mmr:{}, prev_mmr:{}, end:{}",
                curr_header.height(),
                curr_header.header.output_mmr_size,
                prev_mmr,
                curr_header.header.output_mmr_size - 1
            );
            let utxos = self
                .db()
                .fetch_utxos_by_mmr_position(prev_mmr, curr_header.header.output_mmr_size - 1, header.hash().clone())
                .await?;
            trace!(
                target: LOG_TARGET,
                "Fetching kernels from db: height:{}, header.kernel_mmr:{}, prev_mmr:{}, end:{}",
                curr_header.height(),
                curr_header.header.kernel_mmr_size,
                prev_kernel_mmr,
                curr_header.header.kernel_mmr_size - 1
            );
            let kernels = self
                .db()
                .fetch_kernels_by_mmr_position(prev_kernel_mmr, curr_header.header.kernel_mmr_size - 1)
                .await?;

            let mut utxo_sum = HomomorphicCommitment::default();
            debug!(target: LOG_TARGET, "Number of kernels returned: {}", kernels.len());
            debug!(target: LOG_TARGET, "Number of utxos returned: {}", utxos.0.len());
            let mut prune_counter = 0;
            for u in utxos.0 {
                match u {
                    PrunedOutput::NotPruned { output } => {
                        utxo_sum = &output.commitment + &utxo_sum;
                    },
                    _ => {
                        prune_counter += 1;
                    },
                }
            }
            if prune_counter > 0 {
                debug!(target: LOG_TARGET, "Pruned {} outputs", prune_counter);
            }
            prev_mmr = curr_header.header.output_mmr_size;

            pruned_utxo_sum = &utxo_sum + &pruned_utxo_sum;

            for k in kernels {
                pruned_kernel_sum = &k.excess + &pruned_kernel_sum;
            }
            prev_kernel_mmr = curr_header.header.kernel_mmr_size;

            trace!(
                target: LOG_TARGET,
                "Height: {} Kernel sum:{:?} Pruned UTXO sum: {:?}",
                h,
                pruned_kernel_sum,
                pruned_utxo_sum
            );
        }

        self.shared
            .sync_validators
            .final_horizon_state
            .validate(
                header.height(),
                &pruned_utxo_sum,
                &pruned_kernel_sum,
                &*self.db().clone().into_inner().db_read_access()?,
            )
            .map_err(HorizonSyncError::FinalStateValidationFailed)?;

        info!(
            target: LOG_TARGET,
            "Horizon state validation succeeded! Committing horizon state."
        );
        self.db()
            .write_transaction()
            .set_best_block(
                header.height(),
                header.hash().clone(),
                header.accumulated_data.total_accumulated_difficulty,
            )
            .set_pruned_height(header.height(), pruned_kernel_sum, pruned_utxo_sum)
            .commit()
            .await?;

        Ok(())
    }

    #[inline]
    fn db(&self) -> &AsyncBlockchainDb<B> {
        &self.shared.db
    }
}
