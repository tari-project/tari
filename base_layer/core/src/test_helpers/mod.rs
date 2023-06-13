// Copyright 2019. The Tari Project
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

//! Common test helper functions that are small and useful enough to be included in the main crate, rather than the
//! integration test folder.

use std::{iter, mem::size_of, path::Path, sync::Arc};

pub use block_spec::{BlockSpec, BlockSpecs};
use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305};
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng, RngCore};
use tari_common::configuration::Network;
use tari_common_sqlite::connection::{DbConnection, DbConnectionUrl};
use tari_common_types::types::PublicKey;
use tari_comms::PeerManager;
use tari_crypto::keys::PublicKey as PublicKeyT;
use tari_key_manager::{
    cipher_seed::CipherSeed,
    key_manager_service::{
        storage::{database::KeyManagerDatabase, sqlite_db::KeyManagerSqliteDatabase},
        KeyId,
    },
};
use tari_storage::{lmdb_store::LMDBBuilder, LMDBWrapper};

use crate::{
    blocks::{Block, BlockHeader, BlockHeaderAccumulatedData, ChainHeader},
    consensus::{ConsensusConstants, ConsensusManager},
    proof_of_work::{sha3x_difficulty, AchievedTargetDifficulty, Difficulty},
    transactions::{
        key_manager::{TransactionKeyManagerBranch, TransactionKeyManagerWrapper},
        transaction_components::{WalletOutput, Transaction},
        CoinbaseBuilder,
        CryptoFactories,
    },
};

#[macro_use]
mod block_spec;
pub mod blockchain;
pub type TestKeyManager = TransactionKeyManagerWrapper<KeyManagerSqliteDatabase<DbConnection>>;

fn random_string(len: usize) -> String {
    iter::repeat(()).map(|_| OsRng.sample(Alphanumeric)).take(len).collect()
}

pub fn create_test_core_key_manager_with_memory_db_with_range_proof_size(size: usize) -> TestKeyManager {
    let connection = DbConnection::connect_url(&DbConnectionUrl::MemoryShared(random_string(8))).unwrap();
    let cipher = CipherSeed::new();

    let mut key = [0u8; size_of::<Key>()];
    OsRng.fill_bytes(&mut key);
    let key_ga = Key::from_slice(&key);
    let db_cipher = XChaCha20Poly1305::new(key_ga);
    let factory = CryptoFactories::new(size);

    TransactionKeyManagerWrapper::<KeyManagerSqliteDatabase<DbConnection>>::new(
        cipher,
        KeyManagerDatabase::new(KeyManagerSqliteDatabase::init(connection, db_cipher)),
        factory,
    )
    .unwrap()
}

pub fn create_test_core_key_manager_with_memory_db() -> TestKeyManager {
    create_test_core_key_manager_with_memory_db_with_range_proof_size(64)
}

pub fn create_consensus_rules() -> ConsensusManager {
    ConsensusManager::builder(Network::LocalNet).build()
}

pub fn create_consensus_constants(height: u64) -> ConsensusConstants {
    create_consensus_rules().consensus_constants(height).clone()
}

/// Create a partially constructed block using the provided set of transactions
/// is chain_block, or rename it to `create_orphan_block` and drop the prev_block argument
pub fn create_orphan_block(block_height: u64, transactions: Vec<Transaction>, consensus: &ConsensusManager) -> Block {
    let mut header = BlockHeader::new(consensus.consensus_constants(block_height).blockchain_version());
    header.height = block_height;
    header.into_builder().with_transactions(transactions).build()
}

pub async fn create_block(
    rules: &ConsensusManager,
    prev_block: &Block,
    spec: BlockSpec,
    km: &TestKeyManager,
) -> (Block, WalletOutput) {
    let mut header = BlockHeader::from_previous(&prev_block.header);
    let block_height = spec.height_override.unwrap_or(prev_block.header.height + 1);
    header.height = block_height;
    // header.prev_hash = prev_block.hash();
    let reward = spec.reward_override.unwrap_or_else(|| {
        rules.calculate_coinbase_and_fees(
            header.height,
            &spec
                .transactions
                .iter()
                .flat_map(|tx| tx.body.kernels().clone())
                .collect::<Vec<_>>(),
        )
    });

    let spend_key_id = KeyId::Managed {
        branch: TransactionKeyManagerBranch::Coinbase.get_branch_key(),
        index: block_height,
    };
    let (coinbase, coinbase_output) = CoinbaseBuilder::new(km.clone())
        .with_block_height(header.height)
        .with_fees(0.into())
        .with_spend_key_id(spend_key_id.clone())
        .with_script_key_id(spend_key_id)
        .build_with_reward(rules.consensus_constants(block_height), reward)
        .await
        .unwrap();

    let mut block = header
        .into_builder()
        .with_transactions(
            Some(coinbase)
                .filter(|_| !spec.skip_coinbase)
                .into_iter()
                .chain(spec.transactions)
                .collect(),
        )
        .build();

    // Keep times constant in case we need a particular target difficulty
    block.header.timestamp = prev_block.header.timestamp.increase(spec.block_time);
    block.header.output_mmr_size = prev_block.header.output_mmr_size + block.body.outputs().len() as u64;
    block.header.kernel_mmr_size = prev_block.header.kernel_mmr_size + block.body.kernels().len() as u64;

    (block, coinbase_output)
}

pub fn mine_to_difficulty(mut block: Block, difficulty: Difficulty) -> Result<Block, String> {
    // When starting from the same nonce, in tests it becomes common to mine the same block more than once without the
    // hash changing. This introduces the required entropy
    block.header.nonce = rand::thread_rng().gen();
    for _i in 0..20000 {
        if sha3x_difficulty(&block.header) == difficulty {
            return Ok(block);
        }
        block.header.nonce += 1;
    }
    Err("Could not mine to difficulty in 20000 iterations".to_string())
}

pub fn create_peer_manager<P: AsRef<Path>>(data_path: P) -> Arc<PeerManager> {
    let peer_database_name = {
        let mut rng = rand::thread_rng();
        iter::repeat(())
            .map(|_| rng.sample(Alphanumeric))
            .take(8)
            .collect::<String>()
    };
    std::fs::create_dir_all(&data_path).unwrap();
    let datastore = LMDBBuilder::new()
        .set_path(data_path)
        .set_env_config(Default::default())
        .set_max_number_of_databases(1)
        .add_database(&peer_database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();
    let peer_database = datastore.get_handle(&peer_database_name).unwrap();
    Arc::new(PeerManager::new(LMDBWrapper::new(Arc::new(peer_database)), None).unwrap())
}

pub fn create_chain_header(header: BlockHeader, prev_accum: &BlockHeaderAccumulatedData) -> ChainHeader {
    let achieved_target_diff = AchievedTargetDifficulty::try_construct(header.pow_algo(), 1.into(), 1.into()).unwrap();
    let accumulated_data = BlockHeaderAccumulatedData::builder(prev_accum)
        .with_hash(header.hash())
        .with_achieved_target_difficulty(achieved_target_diff)
        .with_total_kernel_offset(header.total_kernel_offset.clone())
        .build()
        .unwrap();
    ChainHeader::try_construct(header, accumulated_data).unwrap()
}

pub fn new_public_key() -> PublicKey {
    PublicKey::random_keypair(&mut OsRng).1
}

pub fn make_hash<T: AsRef<[u8]>>(preimage: T) -> [u8; 32] {
    use digest::Digest;
    use tari_crypto::hash::blake2::Blake256;
    Blake256::new().chain(preimage.as_ref()).finalize().into()
}
