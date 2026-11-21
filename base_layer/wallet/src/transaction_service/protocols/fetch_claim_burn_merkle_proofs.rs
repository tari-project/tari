// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    collections::HashSet,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use log::*;
use minotari_node_wallet_client::BaseNodeWalletClient;
use tari_common_types::types::FixedHash;
use tari_shutdown::ShutdownSignal;
use tari_sidechain::BurnOutputProof;
use tari_transaction_components::transaction_components::burn_output_proof::OutputHashPreimageExt;
use tokio::sync::broadcast;

use crate::{
    connectivity_service::WalletConnectivityInterface,
    transaction_service::{
        error::TransactionStorageError,
        handle::TransactionEvent,
        storage::database::{TransactionBackend, TransactionDatabase},
    },
};

const RETRY_DELAY: Duration = Duration::from_secs(10);
const MAX_ATTEMPTS: usize = 10;
const LOG_TARGET: &str = "wallet::transaction_service::protocols::sync_claim_burn_merkle_proofs";

/// Drop guard that claims a non-empty subset of burn output hashes in the shared in-flight set and removes them when
/// the fetch task ends (whether by completion, give-up, shutdown, or panic).
pub struct InFlightBurnProofsGuard {
    in_flight: Arc<Mutex<HashSet<FixedHash>>>,
    claimed: Vec<FixedHash>,
}

impl InFlightBurnProofsGuard {
    /// Filters out duplicates and any hashes already present in `in_flight`, inserts the remaining hashes, and returns
    /// `Some(guard)` if at least one new hash was claimed.
    pub fn try_acquire(
        in_flight: &Arc<Mutex<HashSet<FixedHash>>>,
        candidates: impl IntoIterator<Item = FixedHash>,
    ) -> Option<Self> {
        let mut set = in_flight.lock().unwrap_or_else(PoisonError::into_inner);
        let mut claimed = Vec::new();
        for hash in candidates {
            if set.insert(hash) {
                claimed.push(hash);
            }
        }
        if claimed.is_empty() {
            None
        } else {
            Some(Self {
                in_flight: Arc::clone(in_flight),
                claimed,
            })
        }
    }

    pub fn hashes(&self) -> &[FixedHash] {
        &self.claimed
    }
}

impl Drop for InFlightBurnProofsGuard {
    fn drop(&mut self) {
        let mut set = self.in_flight.lock().unwrap_or_else(PoisonError::into_inner);
        for hash in &self.claimed {
            set.remove(hash);
        }
    }
}

/// Fetch and store burn output proofs for the burn hashes held by `in_flight_guard`, retrying transient failures.
///
/// This holds the wallet database and does remote I/O, so it holds `shutdown_signal` (counting as a listener for the
/// wallet's shutdown drain) and stops at the next await point once shutdown happens. Database calls are synchronous,
/// so stopping never interrupts a write.
pub async fn execute<TBackend, TConnectivity>(
    db: TransactionDatabase<TBackend>,
    connectivity: TConnectivity,
    event_publisher: broadcast::Sender<Arc<TransactionEvent>>,
    in_flight_guard: InFlightBurnProofsGuard,
    mut shutdown_signal: ShutdownSignal,
) where
    TBackend: TransactionBackend + 'static,
    TConnectivity: WalletConnectivityInterface,
{
    tokio::select! {
        biased;
        _ = shutdown_signal.wait() => {
            debug!(target: LOG_TARGET, "sync_claim_burn_merkle_proofs stopped by shutdown signal");
        },
        _ = execute_with_retries(&db, &connectivity, &event_publisher, in_flight_guard.hashes()) => {},
    }
}

async fn execute_with_retries<TBackend, TConnectivity>(
    db: &TransactionDatabase<TBackend>,
    connectivity: &TConnectivity,
    event_publisher: &broadcast::Sender<Arc<TransactionEvent>>,
    confirmed_burns: &[FixedHash],
) where
    TBackend: TransactionBackend + 'static,
    TConnectivity: WalletConnectivityInterface,
{
    execute_with_retries_and_delay(db, connectivity, event_publisher, confirmed_burns, RETRY_DELAY).await;
}

async fn execute_with_retries_and_delay<TBackend, TConnectivity>(
    db: &TransactionDatabase<TBackend>,
    connectivity: &TConnectivity,
    event_publisher: &broadcast::Sender<Arc<TransactionEvent>>,
    confirmed_burns: &[FixedHash],
    retry_delay: Duration,
) where
    TBackend: TransactionBackend + 'static,
    TConnectivity: WalletConnectivityInterface,
{
    let mut seen = HashSet::with_capacity(confirmed_burns.len());
    let mut pending: Vec<FixedHash> = confirmed_burns
        .iter()
        .copied()
        .filter(|output_hash| seen.insert(*output_hash))
        .collect();
    if pending.is_empty() {
        return;
    }

    debug!(
        target: LOG_TARGET,
        "Starting sync_claim_burn_merkle_proofs with {} confirmed burns",
        pending.len()
    );

    for attempt in 1..=MAX_ATTEMPTS {
        pending = execute_inner(db, connectivity, &pending, event_publisher).await;
        if pending.is_empty() {
            return;
        }

        if attempt < MAX_ATTEMPTS {
            warn!(
                target: LOG_TARGET,
                "sync_claim_burn_merkle_proofs attempt {}/{} left {} burn output(s) pending; retrying in {:.2?}",
                attempt,
                MAX_ATTEMPTS,
                pending.len(),
                retry_delay
            );
            tokio::time::sleep(retry_delay).await;
        } else {
            error!(
                target: LOG_TARGET,
                "sync_claim_burn_merkle_proofs failed for {} burn output(s) after {} attempts, giving up",
                pending.len(),
                MAX_ATTEMPTS
            );
        }
    }
}

async fn execute_inner<TBackend, TConnectivity>(
    db: &TransactionDatabase<TBackend>,
    connectivity: &TConnectivity,
    pending_burns: &[FixedHash],
    event_publisher: &broadcast::Sender<Arc<TransactionEvent>>,
) -> Vec<FixedHash>
where
    TBackend: TransactionBackend + 'static,
    TConnectivity: WalletConnectivityInterface,
{
    if pending_burns.is_empty() {
        return Vec::new();
    }

    let timer = std::time::Instant::now();
    debug!(
        target: LOG_TARGET,
        "Syncing claim burn merkle proofs for {} confirmed burns. Waiting for client connection...",
        pending_burns.len()
    );
    let client = connectivity.obtain_base_node_wallet_rpc_client().await;
    debug!(
        target: LOG_TARGET,
        "Connected to base node wallet client after {:.2?}",
        timer.elapsed()
    );

    let mut failed = Vec::new();
    for output_hash in pending_burns {
        if !fetch_and_store_single_burn(db, &client, event_publisher, output_hash).await {
            failed.push(*output_hash);
        }
    }

    debug!(
        target: LOG_TARGET,
        "Syncing claim burn merkle proofs pass completed in {:.2?} ({} remaining)",
        timer.elapsed(),
        failed.len()
    );

    failed
}

async fn fetch_and_store_single_burn<TBackend, TClient>(
    db: &TransactionDatabase<TBackend>,
    client: &TClient,
    event_publisher: &broadcast::Sender<Arc<TransactionEvent>>,
    output_hash: &FixedHash,
) -> bool
where
    TBackend: TransactionBackend + 'static,
    TClient: BaseNodeWalletClient,
{
    let burn = match db.fetch_burn_proof(output_hash) {
        Ok(Some(burn)) => burn,
        Ok(None) => {
            warn!(
                target: LOG_TARGET,
                "No burn proof found in database for output {}, skipping",
                output_hash
            );
            return true;
        },
        Err(err) => {
            error!(
                target: LOG_TARGET,
                "Failed to read burn proof from database for output {}: {}",
                output_hash,
                err
            );
            return false;
        },
    };

    if burn.burn_output_proof.is_some() {
        warn!(
            target: LOG_TARGET,
            "Burn output {} already has a burn output proof, refetching",
            output_hash
        );
    }

    debug!(
        target: LOG_TARGET,
        "Updating burn output {} with burn output proof",
        output_hash
    );

    let resp = match client.get_burn_output_proof(&burn.burn_proof.commitment).await {
        Ok(resp) => resp,
        Err(err) => {
            error!(
                target: LOG_TARGET,
                "Failed to get burn output proof for burn output {}: {}",
                output_hash,
                err
            );
            return false;
        },
    };

    let proof = resp.proof;
    info!(
        target: LOG_TARGET,
        "Received burn output proof for burn output {} mined in block #{} ({})",
        output_hash,
        proof.block_height,
        proof.block_hash
    );

    if let Err(reason) = validate_burn_output_proof(output_hash, &proof) {
        error!(
            target: LOG_TARGET,
            "Base node returned an invalid burn output proof for output {}: {}",
            output_hash,
            reason
        );
        return false;
    }

    match db.update_burn_proof_set_burn_output_proof(output_hash, &proof) {
        Ok(()) => {
            info!(
                target: LOG_TARGET,
                "Successfully updated burn output {} with burn output proof (mined in height {})",
                output_hash,
                proof.block_height
            );
            let _ignore = event_publisher.send(Arc::new(TransactionEvent::TransactionBurnConfirmed {
                output_hash: *output_hash,
                commitment: Box::new(burn.burn_proof.commitment),
            }));
            true
        },
        Err(TransactionStorageError::ValuesNotFound) => {
            warn!(
                target: LOG_TARGET,
                "Burn proof entry for output {} was removed before update completed, skipping",
                output_hash
            );
            true
        },
        Err(err) => {
            error!(
                target: LOG_TARGET,
                "Failed to store burn output proof for output {}: {}",
                output_hash,
                err
            );
            false
        },
    }
}

fn validate_burn_output_proof(output_hash: &FixedHash, proof: &BurnOutputProof) -> Result<(), String> {
    if i64::try_from(proof.block_height).is_err() {
        return Err(format!(
            "block_height {} exceeds maximum storable SQLite height {}",
            proof.block_height,
            i64::MAX
        ));
    }

    // The proof is checked against the block header by the L2 claim verifier, but a proof for another output is
    // useless, so reject it here.
    let proof_output_hash = proof
        .output
        .hash()
        .map_err(|err| format!("invalid output preimage: {err}"))?;
    if proof_output_hash != *output_hash {
        return Err(format!(
            "proof is for output {proof_output_hash} instead of expected {output_hash}"
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]

    use std::collections::{HashMap, VecDeque};

    use chrono::Utc;
    use digest::Digest;
    use tari_common_types::{
        burn_proof::PartialBurnClaimProof,
        tari_address::TariAddress,
        transaction::{LegacyTransactionStatus, TransactionDirection, TxId},
        types::{ComAndPubSignature, CompressedCommitment, CompressedPublicKey, CompressedSignature, PrivateKey},
    };
    use tari_hashing::hashers::InputMmrHasherBlake256;
    use tari_node_components::blocks::BlockHeader as NodeBlockHeader;
    use tari_script::TariScript;
    use tari_shutdown::Shutdown;
    use tari_sidechain::{MmrInclusionProof, OutputHashPreimage};
    use tari_transaction_components::{
        MicroMinotari,
        rpc::models::{
            self,
            FeePerGramStat,
            GenerateBurnOutputProofResponse,
            GetUtxosByBlockResponse,
            GetUtxosDeletedInfoResponse,
            GetUtxosMinedInfoResponse,
            SyncUtxosByBlockResponseV0,
            TipInfoResponse,
            TxQueryResponse,
            TxSubmissionResponse,
        },
        transaction_components::{
            EncryptedData,
            KernelFeatures,
            MemoField,
            OutputFeatures,
            OutputType,
            Transaction,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
            covenants::Covenant,
        },
    };
    use tari_utilities::{ByteArray, SafePassword};
    use tempfile::tempdir;
    use tokio::sync::{mpsc, watch};

    use super::*;
    use crate::{
        connectivity_service::OnlineStatus,
        storage::{database::WalletDatabase, sqlite_utilities::initialize_sqlite_database_backends},
        transaction_service::storage::{
            models::{CompletedTransaction, TxCancellationReason},
            sqlite_db::TransactionServiceSqliteDatabase,
        },
    };

    #[derive(Clone, Default)]
    struct MockBaseNodeClient {
        state: Arc<Mutex<MockBaseNodeState>>,
    }

    #[derive(Default)]
    struct MockBaseNodeState {
        proof_responses: HashMap<CompressedCommitment, VecDeque<Result<BurnOutputProof, String>>>,
        proof_calls: HashMap<CompressedCommitment, usize>,
    }

    impl MockBaseNodeClient {
        fn push_proof_response(&self, commitment: CompressedCommitment, response: Result<BurnOutputProof, &str>) {
            let mut state = self.state.lock().unwrap();
            state
                .proof_responses
                .entry(commitment)
                .or_default()
                .push_back(response.map_err(ToString::to_string));
        }

        fn set_repeating_proof_response(
            &self,
            commitment: CompressedCommitment,
            response: Result<BurnOutputProof, &str>,
            count: usize,
        ) {
            for _ in 0..count {
                self.push_proof_response(commitment.clone(), response.clone());
            }
        }

        fn proof_call_count(&self, commitment: &CompressedCommitment) -> usize {
            self.state
                .lock()
                .unwrap()
                .proof_calls
                .get(commitment)
                .copied()
                .unwrap_or(0)
        }
    }

    #[async_trait::async_trait]
    impl BaseNodeWalletClient for MockBaseNodeClient {
        async fn get_address(&self) -> String {
            "http://127.0.0.1:18142".to_string()
        }

        async fn is_online(&self) -> bool {
            true
        }

        async fn get_tip_info(&self) -> Result<TipInfoResponse, anyhow::Error> {
            unimplemented!("unused in fetch_claim_burn_merkle_proofs tests")
        }

        async fn get_header_by_height(&self, _height: u64) -> Result<Option<models::BlockHeader>, anyhow::Error> {
            unimplemented!("unused in fetch_claim_burn_merkle_proofs tests")
        }

        async fn get_height_at_time(&self, _epoch_time: u64) -> Result<u64, anyhow::Error> {
            unimplemented!("unused in fetch_claim_burn_merkle_proofs tests")
        }

        async fn get_utxos_by_block(&self, _header_hash: Vec<u8>) -> Result<GetUtxosByBlockResponse, anyhow::Error> {
            unimplemented!("unused in fetch_claim_burn_merkle_proofs tests")
        }

        async fn sync_utxos_by_block(
            &self,
            _start_header_hash: Vec<u8>,
            _shutdown: ShutdownSignal,
        ) -> Result<mpsc::Receiver<Result<SyncUtxosByBlockResponseV0, anyhow::Error>>, anyhow::Error> {
            unimplemented!("unused in fetch_claim_burn_merkle_proofs tests")
        }

        async fn get_last_request_latency(&self) -> Option<Duration> {
            None
        }

        async fn get_utxos_mined_info(
            &self,
            _hashes: Vec<Vec<u8>>,
            _version: u32,
        ) -> Result<GetUtxosMinedInfoResponse, anyhow::Error> {
            unimplemented!("unused in fetch_claim_burn_merkle_proofs tests")
        }

        async fn fetch_utxo(&self, _hash: Vec<u8>) -> Result<Option<TransactionOutput>, anyhow::Error> {
            unimplemented!("unused in fetch_claim_burn_merkle_proofs tests")
        }

        async fn query_deleted_utxos(
            &self,
            _hashes: Vec<Vec<u8>>,
            _must_include_header: Vec<u8>,
        ) -> Result<GetUtxosDeletedInfoResponse, anyhow::Error> {
            unimplemented!("unused in fetch_claim_burn_merkle_proofs tests")
        }

        async fn submit_transaction(&self, _transaction: Transaction) -> Result<TxSubmissionResponse, anyhow::Error> {
            unimplemented!("unused in fetch_claim_burn_merkle_proofs tests")
        }

        async fn transaction_query(
            &self,
            _excess_sig_nonce: Vec<u8>,
            _excess_sig_sig: Vec<u8>,
        ) -> Result<TxQueryResponse, anyhow::Error> {
            unimplemented!("unused in fetch_claim_burn_merkle_proofs tests")
        }

        async fn get_mempool_fee_per_gram_stats(&self, _count: u64) -> Result<FeePerGramStat, anyhow::Error> {
            unimplemented!("unused in fetch_claim_burn_merkle_proofs tests")
        }

        async fn get_burn_output_proof(
            &self,
            commitment: &CompressedCommitment,
        ) -> Result<GenerateBurnOutputProofResponse, anyhow::Error> {
            let mut state = self.state.lock().unwrap();
            let count = state.proof_calls.entry(commitment.clone()).or_insert(0);
            *count = count.saturating_add(1);
            match state.proof_responses.get_mut(commitment).and_then(VecDeque::pop_front) {
                Some(Ok(proof)) => Ok(GenerateBurnOutputProofResponse { proof }),
                Some(Err(err)) => Err(anyhow::anyhow!(err)),
                None => Err(anyhow::anyhow!("no mock burn output proof configured")),
            }
        }
    }

    #[derive(Clone)]
    struct MockConnectivity {
        client: MockBaseNodeClient,
        status_rx: watch::Receiver<OnlineStatus>,
    }

    impl MockConnectivity {
        fn new(client: MockBaseNodeClient) -> Self {
            let (_tx, status_rx) = watch::channel(OnlineStatus::Online {
                latency_ms: 1,
                url: "http://127.0.0.1:18142".to_string(),
            });
            Self { client, status_rx }
        }
    }

    #[async_trait::async_trait]
    impl WalletConnectivityInterface for MockConnectivity {
        type BaseNodeClient = MockBaseNodeClient;

        async fn obtain_base_node_wallet_rpc_client(&self) -> Self::BaseNodeClient {
            self.client.clone()
        }

        async fn get_connectivity_status(&self) -> OnlineStatus {
            OnlineStatus::Online {
                latency_ms: 1,
                url: "http://127.0.0.1:18142".to_string(),
            }
        }

        fn get_connectivity_status_watch(&self) -> watch::Receiver<OnlineStatus> {
            self.status_rx.clone()
        }

        async fn get_last_request_latency(&self) -> Option<Duration> {
            None
        }

        async fn get_address(&self) -> String {
            "http://127.0.0.1:18142".to_string()
        }
    }

    struct BurnFixture {
        tx_id: TxId,
        tx_output: TransactionOutput,
        output_hash: FixedHash,
        commitment: CompressedCommitment,
        kernel: TransactionKernel,
        partial_proof: PartialBurnClaimProof,
        burn_output_proof: BurnOutputProof,
        rpc_header: models::BlockHeader,
    }

    fn setup_dbs() -> (
        tempfile::TempDir,
        WalletDatabase<crate::storage::sqlite_db::wallet::WalletSqliteDatabase>,
        TransactionDatabase<TransactionServiceSqliteDatabase>,
    ) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("wallet.sqlite");
        let (wallet_backend, tx_backend, _, _) =
            initialize_sqlite_database_backends(db_path, SafePassword::from("test-passphrase"), 4).unwrap();
        (
            dir,
            WalletDatabase::new(wallet_backend),
            TransactionDatabase::new(tx_backend),
        )
    }

    fn mmr_single_leaf_root<D: Digest>(leaf: &[u8]) -> [u8; 32] {
        let mut hasher = D::new();
        hasher.update(leaf);
        hasher.finalize().as_slice().try_into().unwrap()
    }

    fn build_burn_fixture(seed: u64, output_type: OutputType, height: u64) -> BurnFixture {
        let non_zero_seed = seed.max(1);
        let seed_byte = u8::try_from(non_zero_seed & 0xFF).unwrap_or(1);
        let sk = PrivateKey::from(non_zero_seed);
        let pk = CompressedPublicKey::from_secret_key(&sk);
        let commitment = CompressedCommitment::from_canonical_bytes(pk.as_bytes()).unwrap();

        let tx_output = TransactionOutput::new_current_version(
            OutputFeatures {
                output_type,
                ..Default::default()
            },
            commitment.clone(),
            None,
            TariScript::default(),
            pk.clone(),
            ComAndPubSignature::default(),
            Covenant::default(),
            EncryptedData::default(),
            MicroMinotari::from(0),
        );
        let output_hash = tx_output.hash();

        let single_leaf_proof = MmrInclusionProof {
            leaf_index: 0,
            mmr_size: 1,
            path: Vec::new(),
            peaks: Vec::new(),
        };
        let normal_output_mr_bytes = mmr_single_leaf_root::<InputMmrHasherBlake256>(output_hash.as_slice());
        let block_output_mr_bytes = mmr_single_leaf_root::<InputMmrHasherBlake256>(&normal_output_mr_bytes);
        let block_output_mr = FixedHash::from(block_output_mr_bytes);

        let mut node_header = NodeBlockHeader::new(1);
        node_header.height = height;
        node_header.block_output_mr = block_output_mr;
        node_header.nonce = non_zero_seed;
        let rpc_header = models::BlockHeader::from(node_header);

        let burn_output_proof = BurnOutputProof {
            block_hash: tari_sidechain::FixedHash::from(<[u8; 32]>::try_from(rpc_header.hash.as_slice()).unwrap()),
            block_height: height,
            output: OutputHashPreimage::from(&tx_output),
            normal_output_proof: single_leaf_proof.clone(),
            normal_output_mr: tari_sidechain::FixedHash::from(normal_output_mr_bytes),
            block_output_proof: single_leaf_proof,
        };

        let partial_proof = PartialBurnClaimProof {
            claim_public_key: pk.clone(),
            commitment: commitment.clone(),
            ownership_proof: CompressedSignature::default(),
            kernel_excess: vec![seed_byte; 32],
            kernel_excess_nonce: vec![0xAA; 32],
            kernel_excess_signature: vec![0xBB; 32],
            sender_offset_public_key: pk,
        };

        let kernel = TransactionKernel::new(
            TransactionKernelVersion::get_current_version(),
            KernelFeatures::create_burn(),
            MicroMinotari::from(100),
            0,
            commitment.clone(),
            CompressedSignature::default(),
            None,
        );

        BurnFixture {
            tx_id: TxId::from(non_zero_seed),
            tx_output,
            output_hash,
            commitment,
            kernel,
            partial_proof,
            burn_output_proof,
            rpc_header,
        }
    }

    fn insert_confirmed_burn(db: &TransactionDatabase<TransactionServiceSqliteDatabase>, fixture: &BurnFixture) {
        insert_burn_with_confirmation(db, fixture, true);
    }

    fn insert_burn_with_confirmation(
        db: &TransactionDatabase<TransactionServiceSqliteDatabase>,
        fixture: &BurnFixture,
        must_be_confirmed: bool,
    ) {
        let tx = Transaction::new(
            Vec::new(),
            vec![fixture.tx_output.clone()],
            vec![fixture.kernel.clone()],
            PrivateKey::default(),
            PrivateKey::default(),
        );
        let completed_tx = CompletedTransaction::new_with_output_hashes(
            fixture.tx_id,
            TariAddress::default(),
            TariAddress::default(),
            MicroMinotari::from(1_000),
            MicroMinotari::from(100),
            tx,
            LegacyTransactionStatus::Completed,
            Utc::now(),
            TransactionDirection::Outbound,
            None,
            None,
            MemoField::new_empty(),
            vec![fixture.output_hash],
            Vec::new(),
            Vec::new(),
            0,
        )
        .unwrap();
        db.insert_completed_transaction(fixture.tx_id, completed_tx).unwrap();
        db.set_transaction_mined_height(
            fixture.tx_id,
            fixture.rpc_header.height,
            fixture.rpc_header.hash,
            1_700_000_000,
            must_be_confirmed,
            LegacyTransactionStatus::Completed,
            fixture.rpc_header.height.saturating_add(10),
        )
        .unwrap();
        db.insert_burn_proof(
            fixture.output_hash,
            &fixture.partial_proof,
            &fixture.kernel,
            &EncryptedData::default(),
            MicroMinotari::from(1_000),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn persists_verified_burn_output_proof_and_publishes_event() {
        let (_dir, wallet_db, tx_db) = setup_dbs();
        let fixture = build_burn_fixture(1, OutputType::Burn, 42);
        insert_confirmed_burn(&tx_db, &fixture);

        let client = MockBaseNodeClient::default();
        client.push_proof_response(fixture.commitment.clone(), Ok(fixture.burn_output_proof.clone()));
        let connectivity = MockConnectivity::new(client.clone());

        let (event_tx, mut event_rx) = broadcast::channel(8);
        execute_with_retries_and_delay(
            &tx_db,
            &connectivity,
            &event_tx,
            &[fixture.output_hash, fixture.output_hash],
            Duration::ZERO,
        )
        .await;

        assert_eq!(client.proof_call_count(&fixture.commitment), 1);
        let stored = tx_db.fetch_burn_proof(&fixture.output_hash).unwrap().unwrap();
        assert_eq!(stored.burn_output_proof, Some(fixture.burn_output_proof));
        assert_eq!(stored.mined_in_height, Some(42));
        assert_eq!(wallet_db.get_all_burn_proofs().unwrap().len(), 1);

        let event = event_rx.try_recv().unwrap();
        match &*event {
            TransactionEvent::TransactionBurnConfirmed {
                output_hash: ev_hash,
                commitment: ev_commitment,
            } => {
                assert_eq!(*ev_hash, fixture.output_hash);
                assert_eq!(**ev_commitment, fixture.commitment);
            },
            other => panic!("unexpected event: {other:?}"),
        }
        assert!(event_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn rejects_height_above_i64_max_in_protocol_and_db_without_corrupting_sqlite() {
        let (_dir, wallet_db, tx_db) = setup_dbs();
        let mut fixture = build_burn_fixture(2, OutputType::Burn, 100);
        insert_confirmed_burn(&tx_db, &fixture);

        // Direct DB call with height 1 << 63 returns Err and leaves the row unchanged
        let mut bad_db_proof = fixture.burn_output_proof.clone();
        bad_db_proof.block_height = 1u64 << 63;
        assert!(
            tx_db
                .update_burn_proof_set_burn_output_proof(&fixture.output_hash, &bad_db_proof)
                .is_err()
        );
        let stored_before = tx_db.fetch_burn_proof(&fixture.output_hash).unwrap().unwrap();
        assert!(stored_before.burn_output_proof.is_none());
        assert!(stored_before.mined_in_height.is_none());

        fixture.burn_output_proof.block_height = u64::MAX;
        let client = MockBaseNodeClient::default();
        client.set_repeating_proof_response(fixture.commitment.clone(), Ok(fixture.burn_output_proof), MAX_ATTEMPTS);
        let connectivity = MockConnectivity::new(client.clone());

        let (event_tx, mut event_rx) = broadcast::channel(8);
        execute_with_retries_and_delay(&tx_db, &connectivity, &event_tx, &[fixture.output_hash], Duration::ZERO).await;

        assert_eq!(client.proof_call_count(&fixture.commitment), MAX_ATTEMPTS);
        let stored = tx_db.fetch_burn_proof(&fixture.output_hash).unwrap().unwrap();
        assert!(stored.burn_output_proof.is_none());
        assert!(stored.mined_in_height.is_none());
        assert_eq!(wallet_db.get_all_burn_proofs().unwrap().len(), 1);
        assert!(event_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn rejects_mismatched_output_hash() {
        let (_dir, _wallet_db, tx_db) = setup_dbs();
        let fixture = build_burn_fixture(4, OutputType::Burn, 60);
        insert_confirmed_burn(&tx_db, &fixture);

        let other_fixture = build_burn_fixture(5, OutputType::Burn, 60);
        let client = MockBaseNodeClient::default();
        client.set_repeating_proof_response(
            fixture.commitment.clone(),
            Ok(other_fixture.burn_output_proof),
            MAX_ATTEMPTS,
        );
        let connectivity = MockConnectivity::new(client.clone());

        let (event_tx, mut event_rx) = broadcast::channel(8);
        execute_with_retries_and_delay(&tx_db, &connectivity, &event_tx, &[fixture.output_hash], Duration::ZERO).await;

        assert_eq!(client.proof_call_count(&fixture.commitment), MAX_ATTEMPTS);
        let stored = tx_db.fetch_burn_proof(&fixture.output_hash).unwrap().unwrap();
        assert!(stored.burn_output_proof.is_none());
        assert!(event_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn retries_only_failed_burns_and_recovers_from_transient_errors() {
        let (_dir, _wallet_db, tx_db) = setup_dbs();
        let burn_1 = build_burn_fixture(10, OutputType::Burn, 101);
        let burn_2 = build_burn_fixture(11, OutputType::Burn, 102);
        let missing_hash = FixedHash::from([0xFE; 32]);

        insert_confirmed_burn(&tx_db, &burn_1);
        insert_confirmed_burn(&tx_db, &burn_2);

        let client = MockBaseNodeClient::default();
        client.push_proof_response(burn_1.commitment.clone(), Ok(burn_1.burn_output_proof.clone()));

        client.push_proof_response(burn_2.commitment.clone(), Err("transient HTTP timeout"));
        client.push_proof_response(burn_2.commitment.clone(), Err("second transient error"));
        client.push_proof_response(burn_2.commitment.clone(), Ok(burn_2.burn_output_proof.clone()));

        let connectivity = MockConnectivity::new(client.clone());
        let (event_tx, mut event_rx) = broadcast::channel(8);

        execute_with_retries_and_delay(
            &tx_db,
            &connectivity,
            &event_tx,
            &[burn_1.output_hash, burn_2.output_hash, missing_hash],
            Duration::ZERO,
        )
        .await;

        assert_eq!(client.proof_call_count(&burn_1.commitment), 1);
        assert_eq!(client.proof_call_count(&burn_2.commitment), 3);

        let stored_1 = tx_db.fetch_burn_proof(&burn_1.output_hash).unwrap().unwrap();
        let stored_2 = tx_db.fetch_burn_proof(&burn_2.output_hash).unwrap().unwrap();
        assert_eq!(stored_1.burn_output_proof, Some(burn_1.burn_output_proof));
        assert_eq!(stored_2.burn_output_proof, Some(burn_2.burn_output_proof));

        let mut confirmed_hashes = Vec::new();
        while let Ok(ev) = event_rx.try_recv() {
            if let TransactionEvent::TransactionBurnConfirmed { output_hash, .. } = &*ev {
                confirmed_hashes.push(*output_hash);
            }
        }
        assert_eq!(confirmed_hashes, vec![burn_1.output_hash, burn_2.output_hash]);
    }

    #[tokio::test]
    async fn sweeps_only_mined_confirmed_non_cancelled_burns_without_proof() {
        let (_dir, _wallet_db, tx_db) = setup_dbs();
        let confirmed_burn = build_burn_fixture(30, OutputType::Burn, 300);
        let unconfirmed_burn = build_burn_fixture(31, OutputType::Burn, 301);
        let unmined_burn = build_burn_fixture(32, OutputType::Burn, 302);
        let cancelled_burn = build_burn_fixture(33, OutputType::Burn, 303);

        insert_confirmed_burn(&tx_db, &confirmed_burn);
        insert_burn_with_confirmation(&tx_db, &unconfirmed_burn, false);

        insert_burn_with_confirmation(&tx_db, &unmined_burn, false);
        tx_db.set_transaction_as_unmined(unmined_burn.tx_id).unwrap();

        insert_confirmed_burn(&tx_db, &cancelled_burn);
        tx_db
            .reject_completed_transaction(cancelled_burn.tx_id, TxCancellationReason::UserCancelled)
            .unwrap();

        let swept = tx_db.fetch_confirmed_burns_without_output_proof().unwrap();
        assert_eq!(swept, vec![confirmed_burn.output_hash]);

        // Once the confirmed burn receives its proof, it is no longer returned by the sweep query.
        let client = MockBaseNodeClient::default();
        client.push_proof_response(
            confirmed_burn.commitment.clone(),
            Ok(confirmed_burn.burn_output_proof.clone()),
        );
        let connectivity = MockConnectivity::new(client.clone());
        let (event_tx, _event_rx) = broadcast::channel(8);
        let in_flight = Arc::new(Mutex::new(HashSet::new()));
        let guard = InFlightBurnProofsGuard::try_acquire(&in_flight, swept).unwrap();
        execute(
            tx_db.clone(),
            connectivity,
            event_tx,
            guard,
            Shutdown::new().to_signal(),
        )
        .await;

        assert!(tx_db.fetch_confirmed_burns_without_output_proof().unwrap().is_empty());
        assert_eq!(client.proof_call_count(&unconfirmed_burn.commitment), 0);
        assert_eq!(client.proof_call_count(&unmined_burn.commitment), 0);
        assert_eq!(client.proof_call_count(&cancelled_burn.commitment), 0);
    }

    #[tokio::test]
    async fn in_flight_guard_prevents_overlapping_fetches_and_cleans_up_on_drop() {
        let (_dir, _wallet_db, tx_db) = setup_dbs();
        let fixture = build_burn_fixture(40, OutputType::Burn, 400);
        insert_confirmed_burn(&tx_db, &fixture);

        let in_flight = Arc::new(Mutex::new(HashSet::new()));
        let first_guard = InFlightBurnProofsGuard::try_acquire(&in_flight, vec![fixture.output_hash]).unwrap();
        assert_eq!(first_guard.hashes(), &[fixture.output_hash]);

        // A second validation run while the first fetch is still in flight does not acquire or start another fetch
        assert!(InFlightBurnProofsGuard::try_acquire(&in_flight, vec![fixture.output_hash]).is_none());

        let client = MockBaseNodeClient::default();
        client.push_proof_response(fixture.commitment.clone(), Ok(fixture.burn_output_proof.clone()));
        let connectivity = MockConnectivity::new(client.clone());
        let (event_tx, _event_rx) = broadcast::channel(8);
        execute(
            tx_db.clone(),
            connectivity,
            event_tx,
            first_guard,
            Shutdown::new().to_signal(),
        )
        .await;

        // After the task finishes, the hash is removed from the in-flight set
        assert!(in_flight.lock().unwrap().is_empty());
        assert!(
            InFlightBurnProofsGuard::try_acquire(&in_flight, vec![fixture.output_hash]).is_some(),
            "Hash should be released after the task completes"
        );
    }

    #[tokio::test]
    async fn stops_promptly_when_shutdown_signal_fires() {
        let (_dir, _wallet_db, tx_db) = setup_dbs();
        let fixture = build_burn_fixture(20, OutputType::Burn, 200);
        insert_confirmed_burn(&tx_db, &fixture);

        let client = MockBaseNodeClient::default();
        let connectivity = MockConnectivity::new(client.clone());
        let (event_tx, mut event_rx) = broadcast::channel(8);

        let shutdown = Shutdown::new();
        let signal = shutdown.to_signal();
        shutdown.trigger();

        let in_flight = Arc::new(Mutex::new(HashSet::new()));
        let guard = InFlightBurnProofsGuard::try_acquire(&in_flight, vec![fixture.output_hash]).unwrap();
        execute(tx_db.clone(), connectivity, event_tx, guard, signal).await;

        assert!(in_flight.lock().unwrap().is_empty());
        assert_eq!(client.proof_call_count(&fixture.commitment), 0);
        assert!(
            tx_db
                .fetch_burn_proof(&fixture.output_hash)
                .unwrap()
                .unwrap()
                .burn_output_proof
                .is_none()
        );
        assert!(event_rx.try_recv().is_err());
    }
}
