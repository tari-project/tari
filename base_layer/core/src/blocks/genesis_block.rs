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

use std::sync::Arc;

use chrono::{DateTime, FixedOffset};
use tari_common::configuration::Network;
use tari_common_types::types::{FixedHash, PrivateKey};
use tari_crypto::tari_utilities::hex::*;
use tari_mmr::sparse_merkle_tree::{NodeKey, ValueHash};
use tari_utilities::ByteArray;

use crate::{
    blocks::{block::Block, BlockHeader, BlockHeaderAccumulatedData, ChainBlock},
    proof_of_work::{AccumulatedDifficulty, Difficulty, PowAlgorithm, ProofOfWork},
    transactions::{aggregated_body::AggregateBody, transaction_components::TransactionOutput},
    OutputSmt,
};

// This can be adjusted as required, but must be limited
const NOT_BEFORE_PROOF_BYTES_SIZE: usize = u16::MAX as usize;

/// Returns the genesis block for the selected network.
pub fn get_genesis_block(network: Network) -> ChainBlock {
    use Network::{Esmeralda, Igor, LocalNet, MainNet, NextNet, StageNet};
    match network {
        MainNet => get_mainnet_genesis_block(),
        StageNet => get_stagenet_genesis_block(),
        NextNet => get_nextnet_genesis_block(),
        Igor => get_igor_genesis_block(),
        Esmeralda => get_esmeralda_genesis_block(),
        LocalNet => get_localnet_genesis_block(),
    }
}

fn add_faucet_utxos_to_genesis_block(file: &str, block: &mut Block) {
    let mut utxos = Vec::new();
    let mut counter = 1;
    let lines_count = file.lines().count();
    for line in file.lines() {
        if counter < lines_count {
            let utxo: TransactionOutput = serde_json::from_str(line).unwrap();
            utxos.push(utxo);
        } else {
            block.body.add_kernel(serde_json::from_str(line).unwrap());
            block.header.kernel_mmr_size += 1;
        }
        counter += 1;
    }
    block.header.output_smt_size += utxos.len() as u64;
    block.body.add_outputs(utxos);
    block.body.sort();
}

fn print_mr_values(block: &mut Block, print: bool) {
    if !print {
        return;
    }
    use std::convert::TryFrom;

    use crate::{chain_storage::calculate_validator_node_mr, KernelMmr};

    let mut kernel_mmr = KernelMmr::new(Vec::new());
    for k in block.body.kernels() {
        println!("k: {}", k);
        kernel_mmr.push(k.hash().to_vec()).unwrap();
    }

    let mut output_smt = OutputSmt::new();

    for o in block.body.outputs() {
        let smt_key = NodeKey::try_from(o.commitment.as_bytes()).unwrap();
        let smt_node = ValueHash::try_from(o.smt_hash(block.header.height).as_slice()).unwrap();
        output_smt.insert(smt_key, smt_node).unwrap();
    }
    let vn_mmr = calculate_validator_node_mr(&[]);

    block.header.kernel_mr = FixedHash::try_from(kernel_mmr.get_merkle_root().unwrap()).unwrap();
    block.header.output_mr = FixedHash::try_from(output_smt.hash().as_slice()).unwrap();
    block.header.validator_node_mr = FixedHash::try_from(vn_mmr).unwrap();
    println!();
    println!("kernel mr: {}", block.header.kernel_mr.to_hex());
    println!("output mr: {}", block.header.output_mr.to_hex());
    println!("vn mr: {}", block.header.validator_node_mr.to_hex());
}

pub fn get_stagenet_genesis_block() -> ChainBlock {
    let mut block = get_stagenet_genesis_block_raw();

    // Add faucet utxos - enable/disable as required
    let add_faucet_utxos = true;
    if add_faucet_utxos {
        // NB! Update 'consensus_constants.rs/pub fn igor()/ConsensusConstants {faucet_value: ?}' with total value
        // NB: `stagenet_genesis_sanity_check` must pass
        let file_contents = include_str!("faucets/stagenet_faucet.json");
        add_faucet_utxos_to_genesis_block(file_contents, &mut block);
        // Enable print only if you need to generate new Merkle roots, then disable it again
        let print_values = false;
        print_mr_values(&mut block, print_values);

        // Hardcode the Merkle roots once they've been computed above
        block.header.kernel_mr =
            FixedHash::from_hex("a08ff15219beea81d4131465290443fb3bd99d28b8af85975dbb2c77cb4cb5a0").unwrap();
        block.header.output_mr =
            FixedHash::from_hex("435f13e21be06b0d0ae9ad3869ac7c723edd933983fa2e26df843c82594b3245").unwrap();
        block.header.validator_node_mr =
            FixedHash::from_hex("277da65c40b2cf99db86baedb903a3f0a38540f3a94d40c826eecac7e27d5dfc").unwrap();
    }

    let accumulated_data = BlockHeaderAccumulatedData {
        hash: block.hash(),
        total_kernel_offset: block.header.total_kernel_offset.clone(),
        achieved_difficulty: Difficulty::min(),
        total_accumulated_difficulty: 1.into(),
        accumulated_randomx_difficulty: AccumulatedDifficulty::min(),
        accumulated_sha3x_difficulty: AccumulatedDifficulty::min(),
        target_difficulty: Difficulty::min(),
    };
    ChainBlock::try_construct(Arc::new(block), accumulated_data).unwrap()
}

fn get_stagenet_genesis_block_raw() -> Block {
    // Set genesis timestamp
    let genesis_timestamp = DateTime::parse_from_rfc2822("11 Mar 2024 08:00:00 +0200").expect("parse may not fail");
    let not_before_proof = b"i am the stagenet genesis block, watch out, here i come \
        \
        The New York Times , 2000/01/01 \
        \
        Lorem Ipsum \
        \
        Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore \
        magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo \
        consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla \
        pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id \
        est laborum.";
    get_raw_block(&genesis_timestamp, &not_before_proof.to_vec())
}

pub fn get_nextnet_genesis_block() -> ChainBlock {
    let mut block = get_nextnet_genesis_block_raw();

    // Add faucet utxos - enable/disable as required
    let add_faucet_utxos = true;
    if add_faucet_utxos {
        // NB! Update 'consensus_constants.rs/pub fn igor()/ConsensusConstants {faucet_value: ?}' with total value
        // NB: `nextnet_genesis_sanity_check` must pass
        let file_contents = include_str!("faucets/nextnet_faucet.json");
        add_faucet_utxos_to_genesis_block(file_contents, &mut block);
        // Enable print only if you need to generate new Merkle roots, then disable it again
        let print_values = false;
        print_mr_values(&mut block, print_values);

        // Hardcode the Merkle roots once they've been computed above
        block.header.kernel_mr =
            FixedHash::from_hex("36881d87e25183f5189d2dca5f7da450c399e7006dafd9bd9240f73a5fb3f0ad").unwrap();
        block.header.output_mr =
            FixedHash::from_hex("7b65d5140485b44e33eef3690d46c41e4dc5c4520ad7464d7740f376f4f0a728").unwrap();
        block.header.validator_node_mr =
            FixedHash::from_hex("277da65c40b2cf99db86baedb903a3f0a38540f3a94d40c826eecac7e27d5dfc").unwrap();
    }

    let accumulated_data = BlockHeaderAccumulatedData {
        hash: block.hash(),
        total_kernel_offset: block.header.total_kernel_offset.clone(),
        achieved_difficulty: Difficulty::min(),
        total_accumulated_difficulty: 1.into(),
        accumulated_randomx_difficulty: AccumulatedDifficulty::min(),
        accumulated_sha3x_difficulty: AccumulatedDifficulty::min(),
        target_difficulty: Difficulty::min(),
    };
    ChainBlock::try_construct(Arc::new(block), accumulated_data).unwrap()
}

fn get_nextnet_genesis_block_raw() -> Block {
    // Set genesis timestamp
    let genesis_timestamp = DateTime::parse_from_rfc2822("11 Mar 2024 08:00:00 +0200").expect("parse may not fail");
    // Let us add a "not before" proof to the genesis block
    let not_before_proof = b"nextnet has a blast, its prowess echoed in every gust \
        \
        The New York Times , 2000/01/01 \
        \
        Lorem Ipsum \
        \
        Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore \
        magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo \
        consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla \
        pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id \
        est laborum.";
    get_raw_block(&genesis_timestamp, &not_before_proof.to_vec())
}

pub fn get_mainnet_genesis_block() -> ChainBlock {
    unimplemented!()
}

pub fn get_igor_genesis_block() -> ChainBlock {
    // lets get the block
    let mut block = get_igor_genesis_block_raw();

    // Add faucet utxos - enable/disable as required
    let add_faucet_utxos = false;
    if add_faucet_utxos {
        // NB! Update 'consensus_constants.rs/pub fn igor()/ConsensusConstants {faucet_value: ?}' with total value
        // NB: `igor_genesis_sanity_check` must pass
        let file_contents = include_str!("faucets/igor_faucet.json");
        add_faucet_utxos_to_genesis_block(file_contents, &mut block);
        // Enable print only if you need to generate new Merkle roots, then disable it again
        let print_values = false;
        print_mr_values(&mut block, print_values);

        // Hardcode the Merkle roots once they've been computed above
        block.header.kernel_mr =
            FixedHash::from_hex("bc5d677b0b8349adc9d7e4a18ace7406986fc7017866f4fd351ecb0f35d6da5e").unwrap();
        block.header.output_mr =
            FixedHash::from_hex("d227ba7b215eab4dae9e0d5a678b84ffbed1d7d3cebdeafae4704e504bd2e5f3").unwrap();
        block.header.validator_node_mr =
            FixedHash::from_hex("277da65c40b2cf99db86baedb903a3f0a38540f3a94d40c826eecac7e27d5dfc").unwrap();
    }

    let accumulated_data = BlockHeaderAccumulatedData {
        hash: block.hash(),
        total_kernel_offset: block.header.total_kernel_offset.clone(),
        achieved_difficulty: Difficulty::min(),
        total_accumulated_difficulty: 1.into(),
        accumulated_randomx_difficulty: AccumulatedDifficulty::min(),
        accumulated_sha3x_difficulty: AccumulatedDifficulty::min(),
        target_difficulty: Difficulty::min(),
    };
    ChainBlock::try_construct(Arc::new(block), accumulated_data).unwrap()
}

fn get_igor_genesis_block_raw() -> Block {
    // Set genesis timestamp
    let genesis_timestamp = DateTime::parse_from_rfc2822("11 Mar 2024 08:00:00 +0200").expect("parse may not fail");
    // Let us add a "not before" proof to the genesis block
    let not_before_proof = b"but igor is the best, it is whispered in the wind \
        \
        The New York Times , 2000/01/01 \
        \
        Lorem Ipsum \
        \
        Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore \
        magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo \
        consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla \
        pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id \
        est laborum.";
    get_raw_block(&genesis_timestamp, &not_before_proof.to_vec())
}

pub fn get_esmeralda_genesis_block() -> ChainBlock {
    // lets get the block
    let mut block = get_esmeralda_genesis_block_raw();

    // Add faucet utxos - enable/disable as required
    let add_faucet_utxos = true;
    if add_faucet_utxos {
        // NB! Update 'consensus_constants.rs/pub fn esmeralda()/ConsensusConstants {faucet_value: ?}' with total value
        // NB: `esmeralda_genesis_sanity_check` must pass
        let file_contents = include_str!("faucets/esmeralda_faucet.json");
        add_faucet_utxos_to_genesis_block(file_contents, &mut block);
        // Enable print only if you need to generate new Merkle roots, then disable it again
        let print_values = false;
        print_mr_values(&mut block, print_values);

        // Hardcode the Merkle roots once they've been computed above
        block.header.kernel_mr =
            FixedHash::from_hex("3f4011ec1e8ddfbd66fb7331c5623b38f529a66e81233d924df85f2070b2aacb").unwrap();
        block.header.output_mr =
            FixedHash::from_hex("3e40efda288a57d3319c63388dd47ffe4b682baaf6a3b58622ec94d77ad712a2").unwrap();
        block.header.validator_node_mr =
            FixedHash::from_hex("277da65c40b2cf99db86baedb903a3f0a38540f3a94d40c826eecac7e27d5dfc").unwrap();
    }

    let accumulated_data = BlockHeaderAccumulatedData {
        hash: block.hash(),
        total_kernel_offset: block.header.total_kernel_offset.clone(),
        achieved_difficulty: Difficulty::min(),
        total_accumulated_difficulty: 1.into(),
        accumulated_randomx_difficulty: AccumulatedDifficulty::min(),
        accumulated_sha3x_difficulty: AccumulatedDifficulty::min(),
        target_difficulty: Difficulty::min(),
    };
    ChainBlock::try_construct(Arc::new(block), accumulated_data).unwrap()
}

fn get_esmeralda_genesis_block_raw() -> Block {
    // Set genesis timestamp
    let genesis_timestamp = DateTime::parse_from_rfc2822("11 Mar 2024 08:00:00 +0200").expect("parse may not fail");
    // Let us add a "not before" proof to the genesis block
    let not_before_proof =
        b"as I sip my drink, thoughts of esmeralda consume my mind, like a refreshing nourishing draught \
        \
        The New York Times , 2000/01/01 \
        \
        Lorem Ipsum \
        \
        Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore \
        magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo \
        consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla \
        pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id \
        est laborum.";
    get_raw_block(&genesis_timestamp, &not_before_proof.to_vec())
}

pub fn get_localnet_genesis_block() -> ChainBlock {
    // lets get the block
    let block = crate::blocks::genesis_block::get_localnet_genesis_block_raw();
    let accumulated_data = BlockHeaderAccumulatedData {
        hash: block.hash(),
        total_kernel_offset: block.header.total_kernel_offset.clone(),
        achieved_difficulty: Difficulty::min(),
        total_accumulated_difficulty: 1.into(),
        accumulated_randomx_difficulty: AccumulatedDifficulty::min(),
        accumulated_sha3x_difficulty: AccumulatedDifficulty::min(),
        target_difficulty: Difficulty::min(),
    };
    ChainBlock::try_construct(Arc::new(block), accumulated_data).unwrap()
}

fn get_localnet_genesis_block_raw() -> Block {
    // Set genesis timestamp
    let genesis_timestamp = DateTime::parse_from_rfc2822("20 Feb 2024 08:01:00 +0200").expect("parse may not fail");
    // Let us add a "not before" proof to the genesis block
    let not_before_proof =
        b"as I sip my drink, thoughts of esmeralda consume my mind, like a refreshing nourishing draught \
        \
        The New York Times , 2000/01/01 \
        \
        Lorem Ipsum \
        \
        Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore \
        magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo \
        consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla \
        pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id \
        est laborum.";
    get_raw_block(&genesis_timestamp, &not_before_proof.to_vec())
}

fn get_raw_block(genesis_timestamp: &DateTime<FixedOffset>, not_before_proof: &[u8]) -> Block {
    // Note: Use 'print_new_genesis_block_values' in core/tests/helpers/block_builders.rs to generate the required
    // fields below

    let mut not_before_proof = not_before_proof.to_vec();
    not_before_proof.truncate(NOT_BEFORE_PROOF_BYTES_SIZE);

    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis_timestamp.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: timestamp.into(),
            output_mr: FixedHash::zero(),
            output_smt_size: 0,
            kernel_mr: FixedHash::from_hex("c14803066909d6d22abf0d2d2782e8936afc3f713f2af3a4ef5c42e8400c1303").unwrap(),
            kernel_mmr_size: 0,
            validator_node_mr: FixedHash::from_hex("277da65c40b2cf99db86baedb903a3f0a38540f3a94d40c826eecac7e27d5dfc")
                .unwrap(),
            validator_node_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: PrivateKey::from_hex(
                "0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
            total_script_offset: PrivateKey::from_hex(
                "0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
            nonce: 0,
            pow: ProofOfWork {
                pow_algo: PowAlgorithm::Sha3x,
                pow_data: not_before_proof,
            },
        },
        body: AggregateBody::new(vec![], vec![], vec![]),
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use tari_common_types::{epoch::VnEpoch, types::Commitment};
    use tari_utilities::ByteArray;
    use Network;

    use super::*;
    use crate::{
        chain_storage::calculate_validator_node_mr,
        consensus::ConsensusManager,
        test_helpers::blockchain::create_new_blockchain_with_network,
        transactions::{
            transaction_components::{transaction_output::batch_verify_range_proofs, KernelFeatures, OutputType},
            CryptoFactories,
        },
        validation::{ChainBalanceValidator, FinalHorizonStateValidation},
        KernelMmr,
    };

    #[test]
    #[cfg(tari_target_network_testnet)]
    fn esme_genesis_sanity_check() {
        // Note: Generate new data for `pub fn get_esmeralda_genesis_block()` and `fn get_esmeralda_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_esmeralda_genesis_block();
        check_block(Network::Esmeralda, &block, 100, 1);
    }

    #[test]
    #[cfg(tari_target_network_nextnet)]
    fn nextnet_genesis_sanity_check() {
        // Note: Generate new data for `pub fn get_nextnet_genesis_block()` and `fn get_stagenet_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_nextnet_genesis_block();
        check_block(Network::NextNet, &block, 100, 1);
    }

    #[test]
    #[cfg(tari_target_network_mainnet)]
    fn stagenet_genesis_sanity_check() {
        Network::set_current(Network::StageNet).unwrap();
        // Note: Generate new data for `pub fn get_stagenet_genesis_block()` and `fn get_stagenet_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_stagenet_genesis_block();
        check_block(Network::StageNet, &block, 100, 1);
    }

    #[test]
    fn igor_genesis_sanity_check() {
        // Note: If outputs and kernels are added, this test will fail unless you explicitly check that network == Igor
        let block = get_igor_genesis_block();
        check_block(Network::Igor, &block, 0, 0);
    }

    #[test]
    fn localnet_genesis_sanity_check() {
        // Note: If outputs and kernels are added, this test will fail unless you explicitly check that network == Igor
        let block = get_localnet_genesis_block();
        check_block(Network::LocalNet, &block, 0, 0);
    }

    fn check_block(network: Network, block: &ChainBlock, expected_outputs: usize, expected_kernels: usize) {
        assert!(block.block().body.inputs().is_empty());
        assert_eq!(block.block().body.kernels().len(), expected_kernels);
        assert_eq!(block.block().body.outputs().len(), expected_outputs);

        let factories = CryptoFactories::default();
        let some_output_is_coinbase = block.block().body.outputs().iter().any(|o| o.is_coinbase());
        assert!(!some_output_is_coinbase);
        let outputs = block.block().body.outputs().iter().collect::<Vec<_>>();
        batch_verify_range_proofs(&factories.range_proof, &outputs).unwrap();
        // Coinbase and faucet kernel
        assert_eq!(
            block.block().body.kernels().len() as u64,
            block.header().kernel_mmr_size
        );
        assert_eq!(
            block.block().body.outputs().len() as u64,
            block.header().output_smt_size
        );

        for kernel in block.block().body.kernels() {
            kernel.verify_signature().unwrap();
        }
        let some_kernel_contains_coinbase_features = block
            .block()
            .body
            .kernels()
            .iter()
            .any(|k| k.features.contains(KernelFeatures::COINBASE_KERNEL));
        assert!(!some_kernel_contains_coinbase_features);

        // Check MMR
        let mut kernel_mmr = KernelMmr::new(Vec::new());
        for k in block.block().body.kernels() {
            kernel_mmr.push(k.hash().to_vec()).unwrap();
        }
        let mut output_smt = OutputSmt::new();

        let mut vn_nodes = Vec::new();
        for o in block.block().body.outputs() {
            let smt_key = NodeKey::try_from(o.commitment.as_bytes()).unwrap();
            let smt_node = ValueHash::try_from(o.smt_hash(block.header().height).as_slice()).unwrap();
            output_smt.insert(smt_key, smt_node).unwrap();
            o.verify_metadata_signature().unwrap();
            if matches!(o.features.output_type, OutputType::ValidatorNodeRegistration) {
                let reg = o
                    .features
                    .sidechain_feature
                    .as_ref()
                    .and_then(|f| f.validator_node_registration())
                    .unwrap();
                vn_nodes.push((
                    reg.public_key().clone(),
                    reg.derive_shard_key(None, VnEpoch(0), VnEpoch(0), block.hash()),
                ));
            }
        }

        assert_eq!(kernel_mmr.get_merkle_root().unwrap(), block.header().kernel_mr,);
        assert_eq!(
            FixedHash::try_from(output_smt.hash().as_slice()).unwrap(),
            block.header().output_mr,
        );
        assert_eq!(calculate_validator_node_mr(&vn_nodes), block.header().validator_node_mr,);

        // Check that the faucet UTXOs balance (the faucet_value consensus constant is set correctly and faucet kernel
        // is correct)

        let utxo_sum = block.block().body.outputs().iter().map(|o| &o.commitment).sum();
        let kernel_sum = block.block().body.kernels().iter().map(|k| &k.excess).sum();

        let db = create_new_blockchain_with_network(network);

        let lock = db.db_read_access().unwrap();
        ChainBalanceValidator::new(ConsensusManager::builder(network).build().unwrap(), Default::default())
            .validate(&*lock, 0, &utxo_sum, &kernel_sum, &Commitment::default())
            .unwrap();
    }
}
