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

use monero::{
    consensus,
    consensus::deserialize,
    cryptonote::hash::{Hash as MoneroHash, Hashable as MoneroHashable},
    Block as MoneroBlock,
};
use tari_core::{
    blocks::Block,
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    consensus::{ConsensusConstants, ConsensusManager},
    proof_of_work::{
        lwma_diff::LinearWeightedMovingAverage,
        monero_rx,
        monero_rx::{FixedByteArray, MoneroPowData},
        Difficulty,
        DifficultyAdjustment,
        PowAlgorithm,
    },
    test_helpers::blockchain::TempDatabase,
};
use tari_utilities::hex::Hex;

use super::block_builders::chain_block;

#[allow(dead_code)]
pub fn create_test_pow_blockchain<T: BlockchainBackend>(
    db: &BlockchainDatabase<T>,
    mut pow_algos: Vec<PowAlgorithm>,
    consensus_manager: &ConsensusManager,
) {
    // Remove the first as it will be replaced by the genesis block
    pow_algos.remove(0);
    let block0 = db.fetch_block(0).unwrap().block().clone();
    append_to_pow_blockchain(db, block0, pow_algos, consensus_manager);
}

#[allow(dead_code)]
pub fn append_to_pow_blockchain<T: BlockchainBackend>(
    db: &BlockchainDatabase<T>,
    chain_tip: Block,
    pow_algos: Vec<PowAlgorithm>,
    consensus_manager: &ConsensusManager,
) {
    let mut prev_block = chain_tip;
    for pow_algo in pow_algos {
        let new_block = chain_block(&prev_block, Vec::new(), consensus_manager);
        let mut new_block = db.prepare_new_block(new_block).unwrap();
        new_block.header.timestamp = prev_block.header.timestamp.increase(120);
        new_block.header.pow.pow_algo = pow_algo;

        if new_block.header.pow.pow_algo == PowAlgorithm::Monero {
            let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000";
            let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97";
            let bytes = hex::decode(&blocktemplate_blob).unwrap();
            let mut block = deserialize::<MoneroBlock>(&bytes[..]).unwrap();
            let hash = MoneroHash::from_slice(new_block.header.merged_mining_hash().as_ref());
            monero_rx::append_merge_mining_tag(&mut block, hash).unwrap();
            let count = 1 + (block.tx_hashes.len() as u16);
            let mut hashes = Vec::with_capacity(count as usize);
            let mut proof = Vec::with_capacity(count as usize);
            hashes.push(block.miner_tx.hash());
            proof.push(block.miner_tx.hash());
            for item in block.clone().tx_hashes {
                hashes.push(item);
                proof.push(item);
            }
            let root = monero_rx::tree_hash(hashes.clone().as_ref()).unwrap();
            let monero_data = MoneroPowData {
                header: block.header,
                randomx_key: FixedByteArray::from_hex(seed_hash).unwrap(),
                transaction_count: count,
                merkle_root: root,
                coinbase_merkle_proof: monero_rx::create_merkle_proof(&hashes, &hashes[0]).unwrap(),
                coinbase_tx: block.miner_tx,
            };
            new_block.header.pow.pow_data = consensus::serialize(&monero_data);
        }

        db.add_block(new_block.clone().into()).unwrap();
        prev_block = new_block;
    }
}

// Calculated the accumulated difficulty for the selected blocks in the blockchain db.
#[allow(dead_code)]
pub fn calculate_accumulated_difficulty(
    db: &BlockchainDatabase<TempDatabase>,
    pow_algo: PowAlgorithm,
    heights: Vec<u64>,
    consensus_constants: &ConsensusConstants,
) -> Difficulty {
    let mut lwma = LinearWeightedMovingAverage::new(
        consensus_constants.get_difficulty_block_window() as usize,
        consensus_constants.get_diff_target_block_interval(pow_algo),
        consensus_constants.get_difficulty_max_block_interval(pow_algo),
    );
    for height in heights {
        let (header, accum) = db.fetch_chain_header(height).unwrap().into_parts();
        lwma.add(header.timestamp, accum.target_difficulty).unwrap();
    }
    lwma.get_difficulty().unwrap()
}
