// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    convert::{TryFrom, TryInto},
    sync::{Arc, Weak},
};

use jmt::{JellyfishMerkleTree, KeyHash};
use log::*;
use tari_common_types::{
    chain_metadata::ChainMetadata,
    types::{CompressedSignature, FixedHash},
};
use tari_comms::{
    peer_manager::NodeId,
    protocol::rpc::{Request, Response, RpcStatus, RpcStatusResultExt, Streaming},
};
use tari_node_components::blocks::BlockHeader;
use tari_transaction_components::transaction_components::{Transaction, TransactionOutput};
use tari_utilities::{ByteArray, hex::Hex};
use tokio::{
    sync::{Mutex, mpsc},
    task,
};
use url::Url;

use crate::{
    base_node::{
        RPC_STREAM_MAX_BUFFERED_BYTES,
        StateMachineHandle,
        rpc::{
            BaseNodeWalletService,
            HeightAtTimeError,
            MAX_ALLOWED_QUERY_SIZE,
            height_at_time,
            sync_utxos_by_block_task::{
                OUTPUT_REBUILD_CONFLICT_MSG,
                SyncUtxosByBlockTask,
                committed_outputs_are_consistent,
                fetch_committed_header_by_hash,
                fetch_optional_header,
            },
        },
        state_machine_service::states::StateInfo,
    },
    chain_storage::{
        BlockchainBackend,
        ChainStorageError,
        MinedInfo,
        OutputMinedInfo,
        SmtHasher,
        async_db::AsyncBlockchainDb,
    },
    mempool::{TxStorageResponse, service::MempoolHandle},
    proto,
    proto::{
        base_node::{
            FetchMatchingUtxos,
            FetchUtxosResponse,
            GetMempoolFeePerGramStatsRequest,
            GetMempoolFeePerGramStatsResponse,
            GetWalletQueryHttpServiceAddressResponse,
            QueryDeletedData,
            QueryDeletedRequest,
            QueryDeletedResponse,
            Signatures as SignaturesProto,
            SyncUtxosByBlockRequest,
            SyncUtxosByBlockResponse,
            TipInfoResponse,
            TxLocation,
            TxQueryBatchResponse,
            TxQueryBatchResponses,
            TxQueryResponse,
            TxSubmissionRejectionReason,
            TxSubmissionResponse,
            UtxoQueryRequest,
            UtxoQueryResponse,
            UtxoQueryResponses,
        },
        types::{Signature as SignatureProto, Transaction as TransactionProto},
    },
};

const LOG_TARGET: &str = "c::base_node::rpc";

async fn with_read_db<B, T, F>(db: &AsyncBlockchainDb<B>, f: F) -> Result<T, RpcStatus>
where
    B: BlockchainBackend + 'static,
    T: Send + 'static,
    F: FnOnce(&B) -> Result<T, ChainStorageError> + Send + 'static,
{
    let inner = db.inner().clone();
    task::spawn_blocking(move || {
        let guard = inner.db_read_access()?;
        f(&*guard)
    })
    .await
    .map_err(|err| RpcStatus::log_internal_error(LOG_TARGET)(err))?
    .rpc_status_internal_error(LOG_TARGET)
}

fn fetch_committed_header_at_height<B: BlockchainBackend>(
    db: &B,
    height: u64,
) -> Result<Option<BlockHeader>, ChainStorageError> {
    let tip_height = db.fetch_chain_metadata()?.best_block_height();
    if height > tip_height {
        return Ok(None);
    }
    fetch_optional_header(db, height)
}

fn fetch_committed_kernel_info<B: BlockchainBackend>(
    db: &B,
    tip_height: u64,
    signature: &CompressedSignature,
) -> Result<Option<(FixedHash, BlockHeader)>, ChainStorageError> {
    let Some((_, block_hash)) = db.fetch_kernel_by_excess_sig(signature)? else {
        return Ok(None);
    };
    let Some(header) = fetch_committed_header_by_hash(db, &block_hash, tip_height)? else {
        return Ok(None);
    };
    Ok(Some((block_hash, header)))
}

fn fetch_latest_committed_output<B: BlockchainBackend>(
    db: &B,
    hash: &FixedHash,
    tip_height: u64,
) -> Result<Option<OutputMinedInfo>, ChainStorageError> {
    let mut committed = Vec::new();
    for info in db.fetch_outputs(hash)? {
        if info.mined_height <= tip_height &&
            fetch_committed_header_by_hash(db, &info.header_hash, tip_height)?.is_some()
        {
            committed.push(info);
        }
    }
    committed.sort_by_key(|o| o.mined_height);
    Ok(committed.into_iter().next_back())
}

fn fetch_committed_unspent_outputs<B: BlockchainBackend>(
    db: &B,
    hashes: Vec<FixedHash>,
) -> Result<Option<Vec<TransactionOutput>>, ChainStorageError> {
    let metadata = db.fetch_chain_metadata()?;
    if !committed_outputs_are_consistent(db, &metadata)? {
        return Ok(None);
    }
    let tip_height = metadata.best_block_height();
    let (smt_reader, current_version) = db.create_smt_reader()?;
    let smt = JellyfishMerkleTree::<_, SmtHasher>::new(&smt_reader);
    let mut outputs = Vec::with_capacity(hashes.len());
    for hash in hashes {
        if let Some(mined_info) = fetch_latest_committed_output(db, &hash, tip_height)? {
            let key_bytes: [u8; 32] = mined_info
                .output
                .commitment
                .as_bytes()
                .try_into()
                .map_err(|_| ChainStorageError::UnexpectedResult("Commitment must be 32 bytes".to_string()))?;
            let smt_key = KeyHash(key_bytes);
            if smt
                .get(smt_key, current_version)
                .map_err(ChainStorageError::JellyfishMerkleTreeError)?
                .is_some()
            {
                outputs.push(mined_info.output);
            }
        }
    }
    Ok(Some(outputs))
}

type CommittedOutputsMinedInfo = (ChainMetadata, Vec<Option<OutputMinedInfo>>);

fn fetch_committed_outputs_mined_info<B: BlockchainBackend>(
    db: &B,
    hashes: Vec<FixedHash>,
) -> Result<Option<CommittedOutputsMinedInfo>, ChainStorageError> {
    let metadata = db.fetch_chain_metadata()?;
    if !committed_outputs_are_consistent(db, &metadata)? {
        return Ok(None);
    }
    let tip_height = metadata.best_block_height();
    let mut result = Vec::with_capacity(hashes.len());
    for hash in hashes {
        result.push(fetch_latest_committed_output(db, &hash, tip_height)?);
    }
    Ok(Some((metadata, result)))
}

fn sanitize_committed_mined_info<B: BlockchainBackend>(
    db: &B,
    info: MinedInfo,
    tip_height: u64,
) -> Result<Option<MinedInfo>, ChainStorageError> {
    let output = match info.output {
        Some(o)
            if o.mined_height <= tip_height &&
                fetch_committed_header_by_hash(db, &o.header_hash, tip_height)?.is_some() =>
        {
            Some(o)
        },
        _ => None,
    };
    let input = match info.input {
        Some(i)
            if i.spent_height <= tip_height &&
                fetch_committed_header_by_hash(db, &i.header_hash, tip_height)?.is_some() =>
        {
            Some(i)
        },
        _ => None,
    };
    if output.is_some() || input.is_some() {
        Ok(Some(MinedInfo { input, output }))
    } else {
        Ok(None)
    }
}

enum QueryDeletedOutcome {
    Found((ChainMetadata, Vec<QueryDeletedData>)),
    MissingHeader,
    OutputRebuild,
}

fn query_committed_deleted<B: BlockchainBackend>(
    db: &B,
    required_header: Option<FixedHash>,
    hashes: Vec<FixedHash>,
) -> Result<QueryDeletedOutcome, ChainStorageError> {
    let metadata = db.fetch_chain_metadata()?;
    if !committed_outputs_are_consistent(db, &metadata)? {
        return Ok(QueryDeletedOutcome::OutputRebuild);
    }
    let tip_height = metadata.best_block_height();
    if let Some(hash) = required_header &&
        fetch_committed_header_by_hash(db, &hash, tip_height)?.is_none()
    {
        return Ok(QueryDeletedOutcome::MissingHeader);
    }
    // One read for metadata, header inclusion, and all hashes; each entry's mined and spent halves describe the same
    // committed lifecycle.
    let mut return_data = Vec::with_capacity(hashes.len());
    for hash in hashes {
        let mut committed_infos = Vec::new();
        for info in db.fetch_mined_info_by_output_hash(&hash)? {
            if let Some(clean) = sanitize_committed_mined_info(db, info, tip_height)? {
                committed_infos.push(clean);
            }
        }
        let latest = MinedInfo::latest(committed_infos);
        let mut data = match latest.output {
            None => QueryDeletedData {
                mined_at_height: 0,
                block_mined_in: Vec::new(),
                height_deleted_at: 0,
                block_deleted_in: Vec::new(),
            },
            Some(u) => QueryDeletedData {
                mined_at_height: u.mined_height,
                block_mined_in: u.header_hash.to_vec(),
                height_deleted_at: 0,
                block_deleted_in: Vec::new(),
            },
        };
        if let Some(input) = latest.input {
            data.height_deleted_at = input.spent_height;
            data.block_deleted_in = input.header_hash.to_vec();
        }
        return_data.push(data);
    }
    Ok(QueryDeletedOutcome::Found((metadata, return_data)))
}

pub struct BaseNodeWalletRpcService<B> {
    db: AsyncBlockchainDb<B>,
    mempool: MempoolHandle,
    state_machine: StateMachineHandle,
    wallet_query_service_address: Option<Url>,
    /// Peers with an open `sync_utxos_by_block` stream
    active_sessions: Mutex<Vec<Weak<NodeId>>>,
    /// The most bytes buffered per `sync_utxos_by_block` stream (see [RPC_STREAM_MAX_BUFFERED_BYTES])
    max_buffered_bytes: usize,
}

impl<B: BlockchainBackend + 'static> BaseNodeWalletRpcService<B> {
    pub fn new(
        db: AsyncBlockchainDb<B>,
        mempool: MempoolHandle,
        state_machine: StateMachineHandle,
        wallet_query_service_address: Option<Url>,
    ) -> Self {
        Self {
            db,
            mempool,
            state_machine,
            wallet_query_service_address,
            active_sessions: Mutex::new(Vec::new()),
            max_buffered_bytes: RPC_STREAM_MAX_BUFFERED_BYTES,
        }
    }

    #[cfg(test)]
    fn with_max_buffered_bytes(mut self, max_bytes: usize) -> Self {
        self.max_buffered_bytes = max_bytes;
        self
    }

    async fn try_add_exclusive_session(&self, peer: NodeId) -> Result<Arc<NodeId>, RpcStatus> {
        let mut lock = self.active_sessions.lock().await;
        *lock = lock.drain(..).filter(|l| l.strong_count() > 0).collect();
        debug!(target: LOG_TARGET, "Number of active sync_utxos_by_block sessions: {}", lock.len());

        if lock.iter().any(|p| p.upgrade().filter(|p| **p == peer).is_some()) {
            return Err(RpcStatus::forbidden(
                "Existing sync_utxos_by_block session found for this client. Only a single session is permitted",
            ));
        }

        let token = Arc::new(peer);
        lock.push(Arc::downgrade(&token));
        Ok(token)
    }

    #[inline]
    fn db(&self) -> AsyncBlockchainDb<B> {
        self.db.clone()
    }

    #[inline]
    pub fn mempool(&self) -> MempoolHandle {
        self.mempool.clone()
    }

    #[inline]
    pub fn state_machine(&self) -> StateMachineHandle {
        self.state_machine.clone()
    }

    fn is_synced(&self) -> bool {
        let state_machine = self.state_machine();
        let status_watch = state_machine.get_status_info_watch();
        match status_watch.borrow().state_info {
            StateInfo::Listening(li) => li.is_synced(),
            _ => false,
        }
    }

    async fn tx_location_from_mempool(&self, signature: CompressedSignature) -> Result<TxLocation, RpcStatus> {
        let mut mempool = self.mempool();
        let location = match mempool
            .get_tx_state_by_excess_sig(signature)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
        {
            TxStorageResponse::UnconfirmedPool => TxLocation::InMempool,
            TxStorageResponse::ReorgPool |
            TxStorageResponse::NotStoredOrphan |
            TxStorageResponse::NotStoredTimeLocked |
            TxStorageResponse::NotStoredAlreadySpent |
            TxStorageResponse::NotStoredConsensus(_) |
            TxStorageResponse::NotStored(_) |
            TxStorageResponse::NotStoredFeeTooLow |
            TxStorageResponse::NotStoredValidatorNodeSlotTaken |
            TxStorageResponse::NotStoredMempoolPolicy(_) |
            TxStorageResponse::NotStoredAlreadyMined => TxLocation::NotStored,
        };
        Ok(location)
    }

    async fn fetch_kernel(
        &self,
        signature: CompressedSignature,
        is_synced: bool,
    ) -> Result<TxQueryResponse, RpcStatus> {
        let sig_for_db = signature.clone();
        let (chain_metadata, mined) = with_read_db(&self.db, move |db| {
            let metadata = db.fetch_chain_metadata()?;
            let mined = fetch_committed_kernel_info(db, metadata.best_block_height(), &sig_for_db)?;
            Ok((metadata, mined))
        })
        .await?;

        if let Some((block_hash, header)) = mined {
            let confirmations = chain_metadata.best_block_height().saturating_sub(header.height);
            return Ok(TxQueryResponse {
                location: TxLocation::Mined as i32,
                best_block_hash: block_hash.to_vec(),
                confirmations,
                is_synced,
                best_block_height: chain_metadata.best_block_height(),
                mined_timestamp: header.timestamp.as_u64(),
            });
        }

        // If not in a committed block then check the mempool
        let location = self.tx_location_from_mempool(signature).await?;
        Ok(TxQueryResponse {
            location: location as i32,
            best_block_hash: vec![],
            confirmations: 0,
            is_synced,
            best_block_height: chain_metadata.best_block_height(),
            mined_timestamp: 0,
        })
    }
}

#[tari_comms::async_trait]
impl<B: BlockchainBackend + 'static> BaseNodeWalletService for BaseNodeWalletRpcService<B> {
    async fn submit_transaction(
        &self,
        request: Request<TransactionProto>,
    ) -> Result<Response<TxSubmissionResponse>, RpcStatus> {
        let message = request.into_message();
        let transaction =
            Transaction::try_from(message).map_err(|_| RpcStatus::bad_request("Transaction was invalid"))?;
        let mut mempool = self.mempool();
        let is_synced = self.is_synced();

        let response = match mempool
            .submit_transaction(transaction.clone())
            .await
            .rpc_status_internal_error(LOG_TARGET)?
        {
            TxStorageResponse::UnconfirmedPool => TxSubmissionResponse {
                accepted: true,
                rejection_reason: TxSubmissionRejectionReason::None.into(),
                is_synced,
            },

            TxStorageResponse::NotStoredOrphan => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::Orphan.into(),
                is_synced,
            },
            TxStorageResponse::NotStoredFeeTooLow => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::FeeTooLow.into(),
                is_synced,
            },
            TxStorageResponse::NotStoredValidatorNodeSlotTaken => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::ValidatorNodeSlotTaken.into(),
                is_synced,
            },
            TxStorageResponse::NotStoredTimeLocked => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::TimeLocked.into(),
                is_synced,
            },
            TxStorageResponse::NotStoredMempoolPolicy(_) => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::MempoolPolicy.into(),
                is_synced,
            },
            TxStorageResponse::NotStoredConsensus(_) | TxStorageResponse::NotStored(_) => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::ValidationFailed.into(),
                is_synced,
            },
            TxStorageResponse::NotStoredAlreadySpent |
            TxStorageResponse::ReorgPool |
            TxStorageResponse::NotStoredAlreadyMined => {
                // Is this transaction a double spend or has this transaction been mined on the committed chain?
                match transaction.first_kernel_excess_sig() {
                    None => TxSubmissionResponse {
                        accepted: false,
                        rejection_reason: TxSubmissionRejectionReason::DoubleSpend.into(),
                        is_synced,
                    },
                    Some(s) => {
                        let sig = s.clone();
                        let mined = with_read_db(&self.db, move |db| {
                            let tip_height = db.fetch_chain_metadata()?.best_block_height();
                            fetch_committed_kernel_info(db, tip_height, &sig)
                        })
                        .await?;
                        let reason = if mined.is_some() {
                            TxSubmissionRejectionReason::AlreadyMined
                        } else {
                            TxSubmissionRejectionReason::DoubleSpend
                        };
                        TxSubmissionResponse {
                            accepted: false,
                            rejection_reason: reason.into(),
                            is_synced,
                        }
                    },
                }
            },
        };
        Ok(Response::new(response))
    }

    async fn transaction_query(
        &self,
        request: Request<SignatureProto>,
    ) -> Result<Response<TxQueryResponse>, RpcStatus> {
        let is_synced = self.is_synced();
        let message = request.into_message();
        let signature =
            CompressedSignature::try_from(message).map_err(|_| RpcStatus::bad_request("Signature was invalid"))?;

        let response = self.fetch_kernel(signature, is_synced).await?;
        Ok(Response::new(response))
    }

    async fn transaction_batch_query(
        &self,
        request: Request<SignaturesProto>,
    ) -> Result<Response<TxQueryBatchResponses>, RpcStatus> {
        let is_synced = self.is_synced();
        let message = request.into_message();
        if message.sigs.is_empty() {
            return Err(RpcStatus::bad_request("Empty signatures"));
        }
        if message.sigs.len() > MAX_ALLOWED_QUERY_SIZE {
            return Err(RpcStatus::bad_request(&format!(
                "Exceeded maximum allowed query signatures. Max: {MAX_ALLOWED_QUERY_SIZE}"
            )));
        }

        let signatures: Vec<CompressedSignature> = message
            .sigs
            .into_iter()
            .map(CompressedSignature::try_from)
            .collect::<Result<_, _>>()
            .map_err(|_| RpcStatus::bad_request("Signature was invalid"))?;

        let sigs_for_db = signatures.clone();
        let (metadata, mined_entries) = with_read_db(&self.db, move |db| {
            let metadata = db.fetch_chain_metadata()?;
            let tip_height = metadata.best_block_height();
            let mut entries = Vec::with_capacity(sigs_for_db.len());
            for sig in &sigs_for_db {
                entries.push(fetch_committed_kernel_info(db, tip_height, sig)?);
            }
            Ok((metadata, entries))
        })
        .await?;

        let mut responses = Vec::with_capacity(signatures.len());
        for (signature, mined) in signatures.into_iter().zip(mined_entries) {
            let sig_proto = Some(SignatureProto::from(&signature));
            let batch_response = match mined {
                Some((block_hash, header)) => TxQueryBatchResponse {
                    signature: sig_proto,
                    location: TxLocation::Mined as i32,
                    best_block_hash: block_hash.to_vec(),
                    confirmations: metadata.best_block_height().saturating_sub(header.height),
                    best_block_height: header.height,
                    mined_timestamp: header.timestamp.as_u64(),
                },
                None => {
                    let location = self.tx_location_from_mempool(signature).await?;
                    TxQueryBatchResponse {
                        signature: sig_proto,
                        location: location as i32,
                        best_block_hash: vec![],
                        confirmations: 0,
                        best_block_height: metadata.best_block_height(),
                        mined_timestamp: 0,
                    }
                },
            };
            responses.push(batch_response);
        }

        Ok(Response::new(TxQueryBatchResponses {
            responses,
            is_synced,
            best_block_hash: metadata.best_block_hash().to_vec(),
            best_block_height: metadata.best_block_height(),
            tip_mined_timestamp: metadata.timestamp(),
        }))
    }

    async fn fetch_matching_utxos(
        &self,
        request: Request<FetchMatchingUtxos>,
    ) -> Result<Response<FetchUtxosResponse>, RpcStatus> {
        let message = request.into_message();
        if message.output_hashes.is_empty() {
            return Err(RpcStatus::bad_request("Empty output hashes"));
        }
        if message.output_hashes.len() > MAX_ALLOWED_QUERY_SIZE {
            return Err(RpcStatus::bad_request(&format!(
                "Exceeded maximum allowed query hashes. Max: {MAX_ALLOWED_QUERY_SIZE}"
            )));
        }

        let is_synced = self.is_synced();
        let hashes: Vec<FixedHash> = message
            .output_hashes
            .into_iter()
            .map(|hash| hash.try_into().map_err(|_| "Malformed pruned hash".to_string()))
            .collect::<Result<_, _>>()
            .map_err(|_| RpcStatus::bad_request("Malformed block hash received"))?;

        let res = with_read_db(&self.db, move |db| fetch_committed_unspent_outputs(db, hashes))
            .await?
            .ok_or_else(|| RpcStatus::conflict(OUTPUT_REBUILD_CONFLICT_MSG))?;

        Ok(Response::new(FetchUtxosResponse {
            outputs: res
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, String>>()
                .map_err(|err| RpcStatus::bad_request(&err))?,
            is_synced,
        }))
    }

    async fn utxo_query(&self, request: Request<UtxoQueryRequest>) -> Result<Response<UtxoQueryResponses>, RpcStatus> {
        let message = request.into_message();
        if message.output_hashes.is_empty() {
            return Err(RpcStatus::bad_request("Empty output hashes"));
        }
        if message.output_hashes.len() > MAX_ALLOWED_QUERY_SIZE {
            return Err(RpcStatus::bad_request(&format!(
                "Exceeded maximum allowed query hashes. Max: {MAX_ALLOWED_QUERY_SIZE}"
            )));
        }

        debug!(
            target: LOG_TARGET,
            "Querying {} UTXO(s) for mined state",
            message.output_hashes.len(),
        );
        let hashes: Vec<FixedHash> = message
            .output_hashes
            .into_iter()
            .map(|hash| hash.try_into().map_err(|_| "Malformed pruned hash".to_string()))
            .collect::<Result<_, _>>()
            .map_err(|_| RpcStatus::bad_request("Malformed block hash received"))?;
        trace!(
            target: LOG_TARGET,
            "UTXO hashes queried from wallet: {:?}",
            hashes.iter().map(|h| h.to_hex()).collect::<Vec<String>>()
        );

        let (metadata, mined_info_resp) =
            with_read_db(&self.db, move |db| fetch_committed_outputs_mined_info(db, hashes))
                .await?
                .ok_or_else(|| RpcStatus::conflict(OUTPUT_REBUILD_CONFLICT_MSG))?;

        let num_mined = mined_info_resp.iter().filter(|opt| opt.is_some()).count();
        debug!(
            target: LOG_TARGET,
            "Found {} mined and {} unmined UTXO(s)",
            num_mined,
            mined_info_resp.len().saturating_sub(num_mined)
        );

        Ok(Response::new(UtxoQueryResponses {
            best_block_height: metadata.best_block_height(),
            best_block_hash: metadata.best_block_hash().to_vec(),
            responses: mined_info_resp
                .into_iter()
                .flatten()
                .map(|utxo| {
                    Ok(UtxoQueryResponse {
                        mined_at_height: utxo.mined_height,
                        mined_in_block: utxo.header_hash.to_vec(),
                        output_hash: utxo.output.hash().to_vec(),
                        output: Some(utxo.output.try_into()?),
                        mined_timestamp: utxo.mined_timestamp,
                    })
                })
                .collect::<Result<Vec<_>, String>>()
                .map_err(|err| RpcStatus::bad_request(&err))?,
        }))
    }

    async fn query_deleted(
        &self,
        request: Request<QueryDeletedRequest>,
    ) -> Result<Response<QueryDeletedResponse>, RpcStatus> {
        let message = request.into_message();
        if message.hashes.is_empty() {
            return Err(RpcStatus::bad_request("Empty utxo hashes"));
        }
        if message.hashes.len() > MAX_ALLOWED_QUERY_SIZE {
            return Err(RpcStatus::bad_request(&format!(
                "Exceeded maximum allowed query hashes. Max: {MAX_ALLOWED_QUERY_SIZE}"
            )));
        }
        let required_header = if message.chain_must_include_header.is_empty() {
            None
        } else {
            Some(
                message
                    .chain_must_include_header
                    .try_into()
                    .map_err(|_| RpcStatus::bad_request("Malformed block hash received"))?,
            )
        };
        let hashes: Vec<FixedHash> = message
            .hashes
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()
            .map_err(|_| RpcStatus::bad_request("Malformed utxo hash received"))?;

        let (metadata, return_data) =
            match with_read_db(&self.db, move |db| query_committed_deleted(db, required_header, hashes)).await? {
                QueryDeletedOutcome::Found(found) => found,
                QueryDeletedOutcome::OutputRebuild => return Err(RpcStatus::conflict(OUTPUT_REBUILD_CONFLICT_MSG)),
                QueryDeletedOutcome::MissingHeader => {
                    return Err(RpcStatus::not_found(
                        "Chain does not include header. It might have been reorged out",
                    ));
                },
            };

        Ok(Response::new(QueryDeletedResponse {
            best_block_height: metadata.best_block_height(),
            best_block_hash: metadata.best_block_hash().to_vec(),
            data: return_data,
        }))
    }

    async fn get_tip_info(&self, _request: Request<()>) -> Result<Response<TipInfoResponse>, RpcStatus> {
        let is_synced = self.is_synced();

        let metadata = self
            .db
            .get_chain_metadata()
            .await
            .rpc_status_internal_error(LOG_TARGET)?;

        Ok(Response::new(TipInfoResponse {
            metadata: Some(metadata.into()),
            is_synced,
        }))
    }

    async fn get_header(&self, request: Request<u64>) -> Result<Response<proto::core::BlockHeader>, RpcStatus> {
        let height = request.into_message();
        let header = with_read_db(&self.db, move |db| fetch_committed_header_at_height(db, height))
            .await?
            .ok_or_else(|| RpcStatus::not_found(&format!("Header not found at height {height}")))?;

        Ok(Response::new(header.into()))
    }

    async fn get_header_by_height(
        &self,
        request: Request<u64>,
    ) -> Result<Response<proto::core::BlockHeader>, RpcStatus> {
        let height = request.into_message();
        let header = with_read_db(&self.db, move |db| fetch_committed_header_at_height(db, height))
            .await?
            .ok_or_else(|| RpcStatus::not_found(&format!("Header not found at height {height}")))?;

        Ok(Response::new(header.into()))
    }

    async fn get_height_at_time(&self, request: Request<u64>) -> Result<Response<u64>, RpcStatus> {
        let requested_epoch_time: u64 = request.into_message();
        trace!(target: LOG_TARGET, "requested_epoch_time: {requested_epoch_time}");
        let height = height_at_time(&self.db, requested_epoch_time)
            .await
            .map_err(|e| match e {
                HeightAtTimeError::HeaderNotFound(_) => RpcStatus::not_found(&e.to_string()),
                HeightAtTimeError::Storage(e) => RpcStatus::log_internal_error(LOG_TARGET)(e),
            })?;
        Ok(Response::new(height))
    }

    /// The range is not capped: genesis to tip is a normal wallet recovery, and the client pulls the stream. One stream
    /// per peer and the byte budget on its queued responses bound what a peer costs.
    async fn sync_utxos_by_block(
        &self,
        request: Request<SyncUtxosByBlockRequest>,
    ) -> Result<Streaming<SyncUtxosByBlockResponse>, RpcStatus> {
        let req = request.message();
        let peer = request.context().peer_node_id();
        debug!(
            target: LOG_TARGET,
            "Received sync_utxos_by_block request from {} from header {} to {} ",
            peer,
            req.start_header_hash.to_hex(),
            req.end_header_hash.to_hex(),
        );

        // Number of blocks to load and push to the stream before loading the next batch. Most blocks have 1 output but
        // full blocks will have 500
        const BATCH_SIZE: usize = 5;
        let task = SyncUtxosByBlockTask::new(self.db(), self.max_buffered_bytes);
        let (start_header, end_header) = task.validate(req).await?;
        let session_token = self.try_add_exclusive_session(peer.clone()).await?;
        let (tx, rx) = mpsc::channel(BATCH_SIZE);
        task.run(start_header, end_header, tx, session_token);

        Ok(Streaming::new(rx))
    }

    async fn get_mempool_fee_per_gram_stats(
        &self,
        request: Request<GetMempoolFeePerGramStatsRequest>,
    ) -> Result<Response<GetMempoolFeePerGramStatsResponse>, RpcStatus> {
        let req = request.into_message();
        let count =
            usize::try_from(req.count).map_err(|_| RpcStatus::bad_request("count must be less than or equal to 20"))?;

        if count > 20 {
            return Err(RpcStatus::bad_request("count must be less than or equal to 20"));
        }

        let metadata = self
            .db
            .get_chain_metadata()
            .await
            .rpc_status_internal_error(LOG_TARGET)?;
        let stats = self
            .mempool()
            .get_fee_per_gram_stats(count, metadata.best_block_height())
            .await
            .rpc_status_internal_error(LOG_TARGET)?;

        Ok(Response::new(stats.into()))
    }

    async fn get_wallet_query_http_service_address(
        &self,
        _request: Request<()>,
    ) -> Result<Response<GetWalletQueryHttpServiceAddressResponse>, RpcStatus> {
        Ok(Response::new(GetWalletQueryHttpServiceAddressResponse {
            http_address: self
                .wallet_query_service_address
                .clone()
                .map(|url| url.to_string())
                .unwrap_or_default(),
        }))
    }
}

#[cfg(test)]
mod test {
    use tari_common::configuration::Network;
    use tari_comms::protocol::rpc::RpcStatusCode;
    use tari_shutdown::Shutdown;
    use tokio::sync::{broadcast, watch};

    use super::*;
    use crate::{
        base_node::state_machine_service::states::StatusInfo,
        chain_storage::{DbTransaction, HorizonStateTreeUpdate},
        mempool::test_utils::{
            covenant_over_token_limit,
            mock::create_mempool_service_mock,
            transaction_proto_with_covenant,
        },
        test_helpers::blockchain::{
            create_chained_blocks,
            create_main_chain,
            create_new_blockchain,
            create_new_blockchain_with_network,
        },
    };

    /// The service has no connectivity handle, so it cannot ban: a rejection here is only ever a bad request
    #[tokio::test]
    async fn submit_transaction_rejects_a_covenant_over_the_token_limit() {
        let db = AsyncBlockchainDb::from(create_new_blockchain_with_network(Network::LocalNet));
        let (mempool, mempool_state) = create_mempool_service_mock();
        let (state_tx, _state_rx) = broadcast::channel(10);
        let (_status_tx, status_rx) = watch::channel(StatusInfo::new());
        let shutdown = Shutdown::new();
        let state_machine = StateMachineHandle::new(state_tx, status_rx, shutdown.to_signal());
        let service = BaseNodeWalletRpcService::new(db, mempool, state_machine, None);

        let err = service
            .submit_transaction(Request::new(
                0.into(),
                transaction_proto_with_covenant(&covenant_over_token_limit()),
            ))
            .await
            .unwrap_err();
        assert_eq!(err.as_status_code(), RpcStatusCode::BadRequest);
        assert!(mempool_state.submitted_transactions().await.is_empty());

        // The same transaction with a covenant at the limit reaches the mempool
        service
            .submit_transaction(Request::new(0.into(), transaction_proto_with_covenant(&[0x20; 128])))
            .await
            .unwrap();
        assert_eq!(mempool_state.submitted_transactions().await.len(), 1);
    }

    #[tokio::test]
    async fn duplicate_output_query_deleted_ignores_previous_lifecycle() {
        let (chain, ..) = crate::validation::block_body::test::duplicate_output_chain();
        let spent = chain.get_block_and_smt_by_name("3").unwrap();
        let remined = chain.get_block_and_smt_by_name("4").unwrap();
        let hash = spent.block().body.inputs().first().unwrap().output_hash();
        let (mempool, _) = create_mempool_service_mock();
        let (state_tx, _state_rx) = broadcast::channel(10);
        let (_status_tx, status_rx) = watch::channel(StatusInfo::new());
        let shutdown = Shutdown::new();
        let state_machine = StateMachineHandle::new(state_tx, status_rx, shutdown.to_signal());
        let service = BaseNodeWalletRpcService::new(chain.db().clone().into(), mempool, state_machine, None);
        let response = service
            .query_deleted(Request::new(0.into(), QueryDeletedRequest {
                hashes: vec![hash.to_vec()],
                chain_must_include_header: remined.hash().to_vec(),
            }))
            .await
            .unwrap()
            .into_message();
        let data = response.data.first().unwrap();
        assert_eq!(data.mined_at_height, remined.height());
        assert_eq!(data.height_deleted_at, 0);
        assert!(data.block_deleted_in.is_empty());
    }

    #[tokio::test]
    async fn get_header_rejects_synced_ahead_headers_until_block_is_committed() {
        let db = create_new_blockchain();
        let (_, chain) = create_main_chain(&db, block_specs!(["A->GB"], ["B->A"]));
        let b = chain.get("B").unwrap().clone();
        let (_, future) = create_chained_blocks(&db, block_specs!(["C->GB"]), b);
        let c = future.get("C").unwrap().clone();
        db.insert_valid_headers(vec![c.to_chain_header()]).unwrap();

        let (mempool, _) = create_mempool_service_mock();
        let (state_tx, _state_rx) = broadcast::channel(10);
        let (_status_tx, status_rx) = watch::channel(StatusInfo::new());
        let state_machine = StateMachineHandle::new(state_tx, status_rx, Shutdown::new().to_signal());
        let service = BaseNodeWalletRpcService::new(db.clone().into(), mempool, state_machine, None);

        assert_eq!(
            service
                .get_header(Request::new(0.into(), 2))
                .await
                .unwrap()
                .into_message()
                .height,
            2
        );
        assert_eq!(
            service
                .get_header_by_height(Request::new(0.into(), 2))
                .await
                .unwrap()
                .into_message()
                .height,
            2
        );
        assert_eq!(
            service
                .get_header(Request::new(0.into(), 3))
                .await
                .unwrap_err()
                .as_status_code(),
            RpcStatusCode::NotFound
        );
        assert_eq!(
            service
                .get_header_by_height(Request::new(0.into(), 3))
                .await
                .unwrap_err()
                .as_status_code(),
            RpcStatusCode::NotFound
        );

        db.add_block(c.to_arc_block()).unwrap();
        assert_eq!(
            service
                .get_header(Request::new(0.into(), 3))
                .await
                .unwrap()
                .into_message()
                .height,
            3
        );
        assert_eq!(
            service
                .get_header_by_height(Request::new(0.into(), 3))
                .await
                .unwrap()
                .into_message()
                .height,
            3
        );
    }

    fn setup_staged_horizon_fixture() -> (
        BaseNodeWalletRpcService<crate::test_helpers::blockchain::TempDatabase>,
        Arc<tari_node_components::blocks::ChainBlock>,
        Arc<tari_node_components::blocks::ChainBlock>,
        Arc<tari_node_components::blocks::ChainBlock>,
    ) {
        let db = create_new_blockchain();
        let (_, chain) = create_main_chain(&db, block_specs!(["A->GB"], ["B->A"]));
        let a = chain.get("A").unwrap().clone();
        let b = chain.get("B").unwrap().clone();
        let (_, future) = create_chained_blocks(&db, block_specs!(["C->GB"]), b.clone());
        let c = future.get("C").unwrap().clone();

        // Simulate synced-ahead header C with an uncommitted staged output/kernel batch flush before JMT update while
        // committed tip remains B (#2).
        db.insert_valid_headers(vec![c.to_chain_header()]).unwrap();
        let staged_output = c.block().body.outputs().first().unwrap().clone();
        let staged_kernel = c.block().body.kernels().first().unwrap().clone();
        let mut txn = DbTransaction::new();
        txn.insert_utxo(staged_output, *c.hash(), c.height(), c.header().timestamp.as_u64());
        txn.insert_kernel(staged_kernel, *c.hash(), 0);
        db.write(txn).unwrap();

        let (mempool, _) = create_mempool_service_mock();
        let (state_tx, _state_rx) = broadcast::channel(10);
        let (_status_tx, status_rx) = watch::channel(StatusInfo::new());
        let state_machine = StateMachineHandle::new(state_tx, status_rx, Shutdown::new().to_signal());
        let service = BaseNodeWalletRpcService::new(db.into(), mempool, state_machine, None);
        (service, a, b, c)
    }

    async fn assert_wallet_output_queries_conflict<B: BlockchainBackend + 'static>(
        service: &BaseNodeWalletRpcService<B>,
        output_hashes: &[Vec<u8>],
        header_hash: FixedHash,
    ) {
        let err = service
            .utxo_query(Request::new(0.into(), UtxoQueryRequest {
                output_hashes: output_hashes.to_vec(),
            }))
            .await
            .unwrap_err();
        assert_eq!(err.as_status_code(), RpcStatusCode::Conflict);

        let err = service
            .fetch_matching_utxos(Request::new(0.into(), FetchMatchingUtxos {
                output_hashes: output_hashes.to_vec(),
            }))
            .await
            .unwrap_err();
        assert_eq!(err.as_status_code(), RpcStatusCode::Conflict);

        let err = service
            .query_deleted(Request::new(0.into(), QueryDeletedRequest {
                hashes: vec![output_hashes.first().unwrap().clone()],
                chain_must_include_header: header_hash.to_vec(),
            }))
            .await
            .unwrap_err();
        assert_eq!(err.as_status_code(), RpcStatusCode::Conflict);
    }

    #[tokio::test]
    async fn wallet_output_queries_reject_reads_during_horizon_sync_output_rebuild() {
        use crate::chain_storage::HorizonSyncOutputCheckpoint;

        let db = create_new_blockchain();
        let (_, chain) = create_main_chain(&db, block_specs!(["A->GB"], ["B->A"]));
        let gb = chain.get("GB").unwrap().clone();
        let a = chain.get("A").unwrap().clone();
        let b = chain.get("B").unwrap().clone();
        let output_a = a.block().body.outputs().first().unwrap().clone();
        let output_b = b.block().body.outputs().first().unwrap().clone();
        let hashes = vec![output_a.hash().to_vec(), output_b.hash().to_vec()];

        let (mempool, _) = create_mempool_service_mock();
        let (state_tx, _state_rx) = broadcast::channel(10);
        let (_status_tx, status_rx) = watch::channel(StatusInfo::new());
        let state_machine = StateMachineHandle::new(state_tx, status_rx, Shutdown::new().to_signal());
        let service = BaseNodeWalletRpcService::new(db.clone().into(), mempool, state_machine, None);

        // Simulate HorizonStateSynchronization::reset_output_state_to_genesis on a node with nonzero committed tip
        // (#2): output tables and JMT are cleared and restored to genesis without lowering best_block_height.
        let mut clear_txn = DbTransaction::new();
        clear_txn.clear_horizon_sync_output_state();
        db.write(clear_txn).unwrap();

        let mut genesis_txn = DbTransaction::new();
        let mut genesis_updates = Vec::new();
        for output in gb.block().body.outputs() {
            genesis_txn.insert_utxo(output.clone(), *gb.hash(), 0, gb.header().timestamp.as_u64());
            if !output.is_burned() {
                genesis_updates.push(HorizonStateTreeUpdate {
                    key: FixedHash::try_from(output.commitment.as_bytes()).unwrap(),
                    value: Some(output.smt_hash(0)),
                    replaces_value: false,
                });
            }
        }
        if !genesis_updates.is_empty() {
            genesis_txn.apply_horizon_state_tree_updates(genesis_updates);
        }
        db.write(genesis_txn).unwrap();
        assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), 2);

        // Phase 0: outputs wiped back to genesis while tip stays at #2 -> rejected with Conflict.
        assert_wallet_output_queries_conflict(&service, &hashes, *b.hash()).await;

        // Phase 1: restore outputs/JMT up to B (#2), yet keep an active HorizonSyncOutputCheckpoint -> still rejected
        // until finalize_horizon_sync clears the checkpoint!
        let mut rebuild_txn = DbTransaction::new();
        let mut updates = Vec::new();
        for blk in [&a, &b] {
            for output in blk.block().body.outputs() {
                rebuild_txn.insert_utxo(
                    output.clone(),
                    *blk.hash(),
                    blk.height(),
                    blk.header().timestamp.as_u64(),
                );
                updates.push(HorizonStateTreeUpdate {
                    key: FixedHash::try_from(output.commitment.as_bytes()).unwrap(),
                    value: Some(output.smt_hash(blk.height())),
                    replaces_value: false,
                });
            }
            for input in blk.block().body.inputs() {
                updates.push(HorizonStateTreeUpdate {
                    key: FixedHash::try_from(input.commitment().unwrap().as_bytes()).unwrap(),
                    value: None,
                    replaces_value: false,
                });
            }
        }
        rebuild_txn.apply_horizon_state_tree_updates(updates);
        rebuild_txn.set_horizon_sync_output_checkpoint(HorizonSyncOutputCheckpoint {
            checkpoint_height: b.height(),
            checkpoint_hash: *b.hash(),
            sync_target_height: b.height() + 5,
            sync_target_hash: FixedHash::zero(),
        });
        db.write(rebuild_txn).unwrap();
        assert_wallet_output_queries_conflict(&service, &hashes, *b.hash()).await;

        // Clearing the checkpoint once the JMT root matches best_block_hash (#2) restores normal reads.
        let mut finalize_txn = DbTransaction::new();
        finalize_txn.clear_horizon_sync_output_checkpoint();
        db.write(finalize_txn).unwrap();

        let utxo_resp = service
            .utxo_query(Request::new(0.into(), UtxoQueryRequest {
                output_hashes: vec![output_b.hash().to_vec()],
            }))
            .await
            .unwrap()
            .into_message();
        assert_eq!(utxo_resp.responses.len(), 1);
    }

    #[tokio::test]
    async fn wallet_utxo_queries_ignore_uncommitted_horizon_staged_outputs() {
        let (service, _a, b, c) = setup_staged_horizon_fixture();
        let committed_output = b.block().body.outputs().first().unwrap().clone();
        let staged_output = c.block().body.outputs().first().unwrap().clone();

        let utxo_resp = service
            .utxo_query(Request::new(0.into(), UtxoQueryRequest {
                output_hashes: vec![committed_output.hash().to_vec(), staged_output.hash().to_vec()],
            }))
            .await
            .unwrap()
            .into_message();
        assert_eq!(utxo_resp.best_block_height, 2);
        assert_eq!(utxo_resp.responses.len(), 1);
        assert_eq!(
            utxo_resp.responses.first().unwrap().output_hash,
            committed_output.hash().to_vec()
        );

        let matching_resp = service
            .fetch_matching_utxos(Request::new(0.into(), FetchMatchingUtxos {
                output_hashes: vec![committed_output.hash().to_vec(), staged_output.hash().to_vec()],
            }))
            .await
            .unwrap()
            .into_message();
        assert_eq!(matching_resp.outputs.len(), 1);

        let deleted_err = service
            .query_deleted(Request::new(0.into(), QueryDeletedRequest {
                hashes: vec![committed_output.hash().to_vec()],
                chain_must_include_header: c.hash().to_vec(),
            }))
            .await
            .unwrap_err();
        assert_eq!(deleted_err.as_status_code(), RpcStatusCode::NotFound);

        let deleted_resp = service
            .query_deleted(Request::new(0.into(), QueryDeletedRequest {
                hashes: vec![staged_output.hash().to_vec()],
                chain_must_include_header: b.hash().to_vec(),
            }))
            .await
            .unwrap()
            .into_message();
        assert_eq!(deleted_resp.best_block_height, 2);
        assert_eq!(deleted_resp.data.first().unwrap().mined_at_height, 0);
    }

    #[tokio::test]
    async fn wallet_tx_queries_ignore_uncommitted_horizon_staged_kernels() {
        let (service, a, b, c) = setup_staged_horizon_fixture();
        let staged_kernel = c.block().body.kernels().first().unwrap();

        let tx_resp = service
            .transaction_query(Request::new(0.into(), SignatureProto::from(&staged_kernel.excess_sig)))
            .await
            .unwrap()
            .into_message();
        assert_eq!(tx_resp.location, TxLocation::NotStored as i32);
        assert_eq!(tx_resp.best_block_height, 2);

        let kernel_a = a.block().body.kernels().first().unwrap();
        let kernel_b = b.block().body.kernels().first().unwrap();
        let batch_resp = service
            .transaction_batch_query(Request::new(0.into(), SignaturesProto {
                sigs: vec![
                    SignatureProto::from(&kernel_a.excess_sig),
                    SignatureProto::from(&kernel_b.excess_sig),
                    SignatureProto::from(&staged_kernel.excess_sig),
                ],
            }))
            .await
            .unwrap()
            .into_message();
        assert_eq!(batch_resp.best_block_height, 2);
        assert_eq!(batch_resp.best_block_hash, b.hash().to_vec());
        let first = batch_resp.responses.first().unwrap();
        assert_eq!(first.location, TxLocation::Mined as i32);
        assert_eq!(first.confirmations, 1);
        assert_eq!(first.best_block_height, 1);
        let second = batch_resp.responses.get(1).unwrap();
        assert_eq!(second.location, TxLocation::Mined as i32);
        assert_eq!(second.confirmations, 0);
        assert_eq!(second.best_block_height, 2);
        let third = batch_resp.responses.get(2).unwrap();
        assert_eq!(third.location, TxLocation::NotStored as i32);
        assert_eq!(third.confirmations, 0);
        assert_eq!(third.best_block_height, 2);
    }

    mod sync_utxos_by_block {
        use std::{collections::HashMap, time::Duration};

        use tari_comms::{
            peer_manager::PeerFeatures,
            protocol::rpc::mock::RpcRequestMock,
            test_utils::node_identity::build_node_identity,
        };
        use tari_node_components::blocks::ChainBlock;

        use super::*;
        use crate::{
            chain_storage::BlockchainDatabase,
            test_helpers::{blockchain::TempDatabase, create_peer_manager},
        };

        type Responses = mpsc::Receiver<Result<SyncUtxosByBlockResponse, RpcStatus>>;

        /// A service over GB, A to E, and a request mock
        fn setup() -> (
            BaseNodeWalletRpcService<TempDatabase>,
            BlockchainDatabase<TempDatabase>,
            HashMap<String, Arc<ChainBlock>>,
            RpcRequestMock,
        ) {
            let db = create_new_blockchain();
            let (_, chain) = create_main_chain(&db, block_specs!(["A->GB"], ["B->A"], ["C->B"], ["D->C"], ["E->D"]));
            let (mempool, _) = create_mempool_service_mock();
            let (state_tx, _state_rx) = broadcast::channel(10);
            let (_status_tx, status_rx) = watch::channel(StatusInfo::new());
            let state_machine = StateMachineHandle::new(state_tx, status_rx, Shutdown::new().to_signal());
            let service = BaseNodeWalletRpcService::new(db.clone().into(), mempool, state_machine, None);
            (service, db, chain, RpcRequestMock::new(create_peer_manager()))
        }

        fn new_peer() -> NodeId {
            build_node_identity(PeerFeatures::COMMUNICATION_CLIENT)
                .node_id()
                .clone()
        }

        async fn open_stream(
            service: &BaseNodeWalletRpcService<TempDatabase>,
            mock: &RpcRequestMock,
            peer: NodeId,
            start: &ChainBlock,
            end: &ChainBlock,
        ) -> Result<Responses, RpcStatus> {
            let msg = SyncUtxosByBlockRequest {
                start_header_hash: start.hash().to_vec(),
                end_header_hash: end.hash().to_vec(),
            };
            let streaming = service
                .sync_utxos_by_block(mock.request_with_context(peer, msg))
                .await?;
            Ok(streaming.into_inner())
        }

        async fn drain(responses: &mut Responses) -> Vec<Result<SyncUtxosByBlockResponse, RpcStatus>> {
            let mut all = Vec::new();
            while let Some(response) = responses.recv().await {
                all.push(response);
            }
            all
        }

        /// Waits until the stream has queued `num` responses and gives it time to queue more
        async fn wait_for_queued(responses: &Responses, num: usize) {
            tokio::time::timeout(Duration::from_secs(10), async {
                while responses.len() < num {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        /// Retries opening a stream for `peer` until the previous stream's task has ended
        async fn open_stream_when_released(
            service: &BaseNodeWalletRpcService<TempDatabase>,
            mock: &RpcRequestMock,
            peer: &NodeId,
            start: &ChainBlock,
            end: &ChainBlock,
        ) -> Responses {
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    match open_stream(service, mock, peer.clone(), start, end).await {
                        Ok(responses) => return responses,
                        Err(err) => assert_eq!(err.as_status_code(), RpcStatusCode::Forbidden),
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap()
        }

        #[tokio::test]
        async fn it_streams_one_response_per_block_in_height_order() {
            let (service, db, chain, mock) = setup();
            let (gb, e) = (chain.get("GB").unwrap(), chain.get("E").unwrap());
            let mut responses = open_stream(&service, &mock, new_peer(), gb, e).await.unwrap();
            let responses = drain(&mut responses).await;

            assert_eq!(responses.len(), 6, "{responses:?}");
            // The genesis block has no outputs and still gets a response
            assert!(responses.first().unwrap().as_ref().unwrap().outputs.is_empty());
            for (height, response) in (0u64..).zip(responses) {
                let response = response.unwrap();
                let block = db.fetch_block(height, true).unwrap();
                assert_eq!(response.height, height);
                assert_eq!(response.header_hash, block.hash().to_vec());
                assert_eq!(response.mined_timestamp, block.header().timestamp.as_u64());
                assert_eq!(response.outputs.len(), block.block().body.outputs().len());
            }
        }

        #[tokio::test]
        async fn it_streams_unspent_utxos_from_genesis_when_pruned() {
            let (service, db, chain, mock) = setup();
            let mut txn = DbTransaction::new();
            txn.set_pruning_horizon(2);
            db.write(txn).unwrap();
            db.prune_to_height(3).unwrap();
            assert_eq!(db.get_chain_metadata().unwrap().pruned_height(), 3);

            let (gb, e) = (chain.get("GB").unwrap(), chain.get("E").unwrap());
            let mut responses = open_stream(&service, &mock, new_peer(), gb, e).await.unwrap();
            let responses = drain(&mut responses).await;
            assert_eq!(responses.len(), 6);
            assert!(responses.iter().all(Result::is_ok));
        }

        #[tokio::test]
        async fn it_rejects_synced_ahead_headers_without_leaking_session() {
            let (service, db, chain, mock) = setup();
            let a = chain.get("A").unwrap();
            let e = chain.get("E").unwrap();
            let (_, future) = create_chained_blocks(&db, block_specs!(["F->GB"]), e.clone());
            let f = future.get("F").unwrap();
            db.insert_valid_headers(vec![f.to_chain_header()]).unwrap();

            let peer = new_peer();
            let err = open_stream(&service, &mock, peer.clone(), a, f).await.unwrap_err();
            assert_eq!(err.as_status_code(), RpcStatusCode::NotFound);
            let err = open_stream(&service, &mock, peer.clone(), f, f).await.unwrap_err();
            assert_eq!(err.as_status_code(), RpcStatusCode::NotFound);

            // Peer session slot was never consumed by the rejected requests
            let mut responses = open_stream(&service, &mock, peer, a, e).await.unwrap();
            assert_eq!(drain(&mut responses).await.len(), 5);
        }

        #[tokio::test]
        async fn it_rejects_invalid_requests() {
            let (service, _db, chain, mock) = setup();
            let a = chain.get("A").unwrap();
            let e = chain.get("E").unwrap();
            let request = |start: Vec<u8>, end: Vec<u8>| {
                mock.request_with_context(new_peer(), SyncUtxosByBlockRequest {
                    start_header_hash: start,
                    end_header_hash: end,
                })
            };

            for (start, end) in [
                (vec![1; 3], e.hash().to_vec()),
                (a.hash().to_vec(), vec![1; 3]),
                (e.hash().to_vec(), a.hash().to_vec()),
            ] {
                let err = service.sync_utxos_by_block(request(start, end)).await.unwrap_err();
                assert_eq!(err.as_status_code(), RpcStatusCode::BadRequest);
            }
            for (start, end) in [(vec![0; 32], e.hash().to_vec()), (a.hash().to_vec(), vec![0; 32])] {
                let err = service.sync_utxos_by_block(request(start, end)).await.unwrap_err();
                assert_eq!(err.as_status_code(), RpcStatusCode::NotFound);
            }
        }

        #[tokio::test]
        async fn it_allows_one_stream_per_peer() {
            let (service, _db, chain, mock) = setup();
            let service = service.with_max_buffered_bytes(1);
            let (a, e) = (chain.get("A").unwrap(), chain.get("E").unwrap());
            let peer_a = new_peer();

            let _open = open_stream(&service, &mock, peer_a.clone(), a, e).await.unwrap();
            let err = open_stream(&service, &mock, peer_a, a, e).await.unwrap_err();
            assert_eq!(err.as_status_code(), RpcStatusCode::Forbidden);
            open_stream(&service, &mock, new_peer(), a, e).await.unwrap();
        }

        #[tokio::test]
        async fn it_releases_the_session_when_the_stream_ends() {
            let (service, _db, chain, mock) = setup();
            let service = service.with_max_buffered_bytes(1);
            let (a, e) = (chain.get("A").unwrap(), chain.get("E").unwrap());
            let peer_a = new_peer();

            // The stream completes
            let mut responses = open_stream(&service, &mock, peer_a.clone(), a, e).await.unwrap();
            assert_eq!(drain(&mut responses).await.len(), 5);
            let responses = open_stream_when_released(&service, &mock, &peer_a, a, e).await;

            // The client goes away mid-stream
            wait_for_queued(&responses, 1).await;
            drop(responses);
            open_stream_when_released(&service, &mock, &peer_a, a, e).await;
        }

        #[tokio::test]
        async fn it_stops_producing_while_the_byte_budget_is_full() {
            let (service, _db, chain, mock) = setup();
            // Any response fills the budget, so at most one is queued however many the channel holds
            let service = service.with_max_buffered_bytes(1);
            let mut responses = open_stream(
                &service,
                &mock,
                new_peer(),
                chain.get("A").unwrap(),
                chain.get("E").unwrap(),
            )
            .await
            .unwrap();

            wait_for_queued(&responses, 1).await;
            assert_eq!(responses.len(), 1);
            let responses = drain(&mut responses).await;
            assert_eq!(responses.len(), 5);
            assert!(responses.iter().all(Result::is_ok));
        }

        #[tokio::test]
        async fn it_sends_conflict_when_the_end_block_is_replaced() {
            let (service, db, chain, mock) = setup();
            let service = service.with_max_buffered_bytes(1);
            let mut responses = open_stream(
                &service,
                &mock,
                new_peer(),
                chain.get("A").unwrap(),
                chain.get("E").unwrap(),
            )
            .await
            .unwrap();
            wait_for_queued(&responses, 1).await;

            // The fork is longer than E, so it wins over E, which the rewind leaves as an orphan
            db.rewind_to_height(4).unwrap();
            let (fork_names, fork) = create_chained_blocks(
                &db,
                block_specs!(["E2->GB"], ["F2->E2"]),
                chain.get("D").unwrap().clone(),
            );
            for name in &fork_names {
                db.add_block(fork.get(name).unwrap().to_arc_block()).unwrap();
            }
            let e2 = fork.get("E2").unwrap();
            assert_eq!(db.fetch_header(5).unwrap().unwrap().hash(), *e2.hash());
            assert_ne!(e2.hash(), chain.get("E").unwrap().hash());

            let mut responses = drain(&mut responses).await;
            let err = responses.pop().unwrap().unwrap_err();
            assert_eq!(err.as_status_code(), RpcStatusCode::Conflict);
            assert_eq!(
                err.details(),
                "Reorg detected: the requested end block is no longer on the chain"
            );
            // Only blocks fetched before the reorg, all from the original chain, came before the conflict
            assert!(!responses.is_empty() && responses.len() < 5, "{responses:?}");
            for response in responses {
                let response = response.unwrap();
                let name = ["A", "B", "C", "D"]
                    .get(usize::try_from(response.height.saturating_sub(1)).unwrap())
                    .unwrap();
                assert_eq!(response.header_hash, chain.get(*name).unwrap().hash().to_vec());
            }
        }

        #[tokio::test]
        async fn it_sends_conflict_when_blocks_rewind_and_same_headers_resync_mid_stream() {
            let (service, db, chain, mock) = setup();
            let service = service.with_max_buffered_bytes(1);
            let a = chain.get("A").unwrap();
            let e = chain.get("E").unwrap();
            let peer = new_peer();
            let mut responses = open_stream(&service, &mock, peer.clone(), a, e).await.unwrap();
            wait_for_queued(&responses, 1).await;

            // Rewind committed blocks back to A (#1), then re-insert headers B..E as pending headers without bodies.
            db.rewind_to_height(1).unwrap();
            let resynced_headers = ["B", "C", "D", "E"]
                .iter()
                .map(|name| chain.get(*name).unwrap().to_chain_header())
                .collect();
            db.insert_valid_headers(resynced_headers).unwrap();
            assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), 1);
            assert_eq!(db.fetch_header(5).unwrap().unwrap().hash(), *e.hash());

            let mut drained = drain(&mut responses).await;
            let err = drained.pop().unwrap().unwrap_err();
            assert_eq!(err.as_status_code(), RpcStatusCode::Conflict);
            // Only blocks already fetched before the rewind (at most queued #1 and in-flight #2) were sent, all with
            // real outputs — never empty uncommitted blocks C..E.
            assert!(!drained.is_empty() && drained.len() <= 2, "{drained:?}");
            for (expected_height, response) in (1u64..).zip(drained) {
                let response = response.unwrap();
                let name = ["A", "B"]
                    .get(usize::try_from(expected_height.saturating_sub(1)).unwrap())
                    .unwrap();
                assert_eq!(response.height, expected_height);
                assert_eq!(response.header_hash, chain.get(*name).unwrap().hash().to_vec());
                assert!(!response.outputs.is_empty());
            }

            // Session is released cleanly after the conflict.
            let mut next = open_stream_when_released(&service, &mock, &peer, a, a).await;
            assert_eq!(drain(&mut next).await.len(), 1);
        }

        #[tokio::test]
        async fn it_rejects_and_aborts_stream_during_horizon_sync_output_rebuild() {
            let (service, db, chain, mock) = setup();
            let service = service.with_max_buffered_bytes(1);
            let a = chain.get("A").unwrap();
            let e = chain.get("E").unwrap();
            let peer = new_peer();

            // Open stream from A (#1) to E (#5) while outputs are consistent, and pause after queuing the first block.
            let mut responses = open_stream(&service, &mock, peer.clone(), a, e).await.unwrap();
            wait_for_queued(&responses, 1).await;

            // Simulate horizon sync calling reset_output_state_to_genesis while best_block_height remains 5.
            let mut txn = DbTransaction::new();
            txn.clear_horizon_sync_output_state();
            db.write(txn).unwrap();
            assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), 5);

            // Active stream aborts with Conflict before sending any wiped empty blocks from the remaining range.
            let mut drained = drain(&mut responses).await;
            let err = drained.pop().unwrap().unwrap_err();
            assert_eq!(err.as_status_code(), RpcStatusCode::Conflict);
            assert_eq!(err.details(), OUTPUT_REBUILD_CONFLICT_MSG);
            assert!(!drained.is_empty() && drained.len() <= 2, "{drained:?}");
            for response in drained {
                assert!(!response.unwrap().outputs.is_empty());
            }

            // Wait for the closed stream's session token to drop, then verify new requests also reject in validate()
            // without leaking the peer's session slot.
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let err = open_stream(&service, &mock, peer.clone(), a, e).await.unwrap_err();
                    if err.as_status_code() == RpcStatusCode::Conflict {
                        assert_eq!(err.details(), OUTPUT_REBUILD_CONFLICT_MSG);
                        break;
                    }
                    assert_eq!(err.as_status_code(), RpcStatusCode::Forbidden);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();

            // A second call from the same peer still gets Conflict (not Forbidden), proving validate() did not leak a
            // session token.
            let err = open_stream(&service, &mock, peer, a, e).await.unwrap_err();
            assert_eq!(err.as_status_code(), RpcStatusCode::Conflict);
        }
    }
}
