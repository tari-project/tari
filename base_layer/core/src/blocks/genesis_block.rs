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

use crate::{
    blocks::{block::Block, BlockHeader, BlockHeaderAccumulatedData, ChainBlock},
    proof_of_work::{Difficulty, PowAlgorithm, ProofOfWork},
    transactions::{aggregated_body::AggregateBody, transaction_components::TransactionOutput},
};

/// Returns the genesis block for the selected network.
pub fn get_genesis_block(network: Network) -> ChainBlock {
    use Network::{Esmeralda, Igor, LocalNet, MainNet, NextNet, StageNet};
    match network {
        MainNet => get_mainnet_genesis_block(),
        StageNet => get_stagenet_genesis_block(),
        NextNet => get_nextnet_genesis_block(),
        Igor => get_igor_genesis_block(),
        Esmeralda => get_esmeralda_genesis_block(),
        LocalNet => get_esmeralda_genesis_block(),
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
    block.header.output_mmr_size += utxos.len() as u64;
    block.body.add_outputs(utxos);
    block.body.sort();
}

fn print_mr_values(block: &mut Block, print: bool) {
    if !print {
        return;
    }
    use std::convert::TryFrom;

    use croaring::Bitmap;

    use crate::{chain_storage::calculate_validator_node_mr, KernelMmr, MutableOutputMmr};

    let mut kernel_mmr = KernelMmr::new(Vec::new());
    for k in block.body.kernels() {
        println!("k: {}", k);
        kernel_mmr.push(k.hash().to_vec()).unwrap();
    }

    let mut output_mmr = MutableOutputMmr::new(Vec::new(), Bitmap::create()).unwrap();

    for o in block.body.outputs() {
        output_mmr.push(o.hash().to_vec()).unwrap();
    }
    let vn_mmr = calculate_validator_node_mr(&[]);

    block.header.kernel_mr = FixedHash::try_from(kernel_mmr.get_merkle_root().unwrap()).unwrap();
    block.header.output_mr = FixedHash::try_from(output_mmr.get_merkle_root().unwrap()).unwrap();
    block.header.validator_node_mr = FixedHash::try_from(vn_mmr).unwrap();
    println!();
    println!("kernel mr: {}", block.header.kernel_mr.to_hex());
    println!("output mr: {}", block.header.output_mr.to_hex());
    println!("vn mr: {}", block.header.validator_node_mr.to_hex());
}

pub fn get_stagenet_genesis_block() -> ChainBlock {
    let mut block = get_stagenet_genesis_block_raw();

    // Add faucet utxos - enable/disable as required
    let add_faucet_utxos = false;
    if add_faucet_utxos {
        // NB! Update 'consensus_constants.rs/pub fn igor()/ConsensusConstants {faucet_value: ?}' with total value
        // NB: `stagenet_genesis_sanity_check` must pass
        let file_contents = include_str!("faucets/esmeralda_faucet.json");
        add_faucet_utxos_to_genesis_block(file_contents, &mut block);
        // Enable print only if you need to generate new Merkle roots, then disable it again
        let print_values = false;
        print_mr_values(&mut block, print_values);

        // Hardcode the Merkle roots once they've been computed above
        block.header.kernel_mr = FixedHash::from_hex("TODO: Update when required").unwrap();
        block.header.output_mr = FixedHash::from_hex("TODO: Update when required").unwrap();
        block.header.validator_node_mr = FixedHash::from_hex("TODO: Update when required").unwrap();
    }

    let accumulated_data = BlockHeaderAccumulatedData {
        hash: block.hash(),
        total_kernel_offset: block.header.total_kernel_offset.clone(),
        achieved_difficulty: Difficulty::min(),
        total_accumulated_difficulty: 1,
        accumulated_randomx_difficulty: Difficulty::min(),
        accumulated_sha3x_difficulty: Difficulty::min(),
        target_difficulty: Difficulty::min(),
    };
    ChainBlock::try_construct(Arc::new(block), accumulated_data).unwrap()
}

fn get_stagenet_genesis_block_raw() -> Block {
    // Set genesis timestamp
    let genesis_timestamp = DateTime::parse_from_rfc2822("15 Jun 2023 14:00:00 +0200").expect("parse may not fail");
    get_raw_block(&genesis_timestamp)
}

pub fn get_nextnet_genesis_block() -> ChainBlock {
    let mut block = get_nextnet_genesis_block_raw();

    // Add faucet utxos - enable/disable as required
    let add_faucet_utxos = false;
    if add_faucet_utxos {
        // NB! Update 'consensus_constants.rs/pub fn igor()/ConsensusConstants {faucet_value: ?}' with total value
        // NB: `nextnet_genesis_sanity_check` must pass
        let file_contents = include_str!("faucets/esmeralda_faucet.json");
        add_faucet_utxos_to_genesis_block(file_contents, &mut block);
        // Enable print only if you need to generate new Merkle roots, then disable it again
        let print_values = false;
        print_mr_values(&mut block, print_values);

        // Hardcode the Merkle roots once they've been computed above
        block.header.kernel_mr = FixedHash::from_hex("TODO: Update when required").unwrap();
        block.header.output_mr = FixedHash::from_hex("TODO: Update when required").unwrap();
        block.header.validator_node_mr = FixedHash::from_hex("TODO: Update when required").unwrap();
    }

    let accumulated_data = BlockHeaderAccumulatedData {
        hash: block.hash(),
        total_kernel_offset: block.header.total_kernel_offset.clone(),
        achieved_difficulty: Difficulty::min(),
        total_accumulated_difficulty: 1,
        accumulated_randomx_difficulty: Difficulty::min(),
        accumulated_sha3x_difficulty: Difficulty::min(),
        target_difficulty: Difficulty::min(),
    };
    ChainBlock::try_construct(Arc::new(block), accumulated_data).unwrap()
}

fn get_nextnet_genesis_block_raw() -> Block {
    // Set genesis timestamp
    let genesis_timestamp = DateTime::parse_from_rfc2822("15 Jun 2023 14:00:00 +0200").expect("parse may not fail");
    get_raw_block(&genesis_timestamp)
}

pub fn get_mainnet_genesis_block() -> ChainBlock {
    unimplemented!()
}

pub fn get_igor_genesis_block() -> ChainBlock {
    // lets get the block
    let mut block = get_igor_genesis_block_raw();

    // Add faucet utxos - enable/disable as required
    let add_faucet_utxos = true;
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
            FixedHash::from_hex("ad494884dabf1337a678625613d016d55c0d6a968c86a5ed57fd3099c207368b").unwrap();
        block.header.output_mr =
            FixedHash::from_hex("15c8730dfcc1414cae73a4614d5c2a8b95f32a8db80f5d2602b6b3e7419cd34e").unwrap();
        block.header.validator_node_mr =
            FixedHash::from_hex("e1d55f91ecc7e435080ac2641280516a355a5ecbe231158987da217b5af30047").unwrap();
    }

    let accumulated_data = BlockHeaderAccumulatedData {
        hash: block.hash(),
        total_kernel_offset: block.header.total_kernel_offset.clone(),
        achieved_difficulty: Difficulty::min(),
        total_accumulated_difficulty: 1,
        accumulated_randomx_difficulty: Difficulty::min(),
        accumulated_sha3x_difficulty: Difficulty::min(),
        target_difficulty: Difficulty::min(),
    };
    ChainBlock::try_construct(Arc::new(block), accumulated_data).unwrap()
}

fn get_igor_genesis_block_raw() -> Block {
    // Set genesis timestamp
    let genesis_timestamp = DateTime::parse_from_rfc2822("15 Jun 2023 14:00:00 +0200").expect("parse may not fail");
    get_raw_block(&genesis_timestamp)
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
            FixedHash::from_hex("8c93eba80af538d89004df33e6d9f52fbd542f2a0e56887bdf1e0b8397e515a3").unwrap();
        block.header.output_mr =
            FixedHash::from_hex("da2723bf3b44acd4b8809433ea5208ca4699603b31f9983cd7a461f92050e8c0").unwrap();
        block.header.validator_node_mr =
            FixedHash::from_hex("e1d55f91ecc7e435080ac2641280516a355a5ecbe231158987da217b5af30047").unwrap();
    }

    let accumulated_data = BlockHeaderAccumulatedData {
        hash: block.hash(),
        total_kernel_offset: block.header.total_kernel_offset.clone(),
        achieved_difficulty: Difficulty::min(),
        total_accumulated_difficulty: 1,
        accumulated_randomx_difficulty: Difficulty::min(),
        accumulated_sha3x_difficulty: Difficulty::min(),
        target_difficulty: Difficulty::min(),
    };
    ChainBlock::try_construct(Arc::new(block), accumulated_data).unwrap()
}

fn get_esmeralda_genesis_block_raw() -> Block {
    // Set genesis timestamp
    let genesis_timestamp = DateTime::parse_from_rfc2822("15 Jun 2023 14:00:00 +0200").expect("parse may not fail");
    get_raw_block(&genesis_timestamp)
}

fn get_raw_block(genesis_timestamp: &DateTime<FixedOffset>) -> Block {
    // Note: Use 'print_new_genesis_block_values' in core/tests/helpers/block_builders.rs to generate the required
    // fields below
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis_timestamp.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: timestamp.into(),
            output_mr: FixedHash::from_hex("7319ca29721731cebf9725b7b3b1a5abb8e721d30b11aaa84e10556da4d80acf").unwrap(),
            output_mmr_size: 0,
            kernel_mr: FixedHash::from_hex("e7ab4ea97d3410a402b1f18c7f6b347ee368259a50353c105c0303ab4420a809").unwrap(),
            kernel_mmr_size: 0,
            validator_node_mr: FixedHash::from_hex("e1d55f91ecc7e435080ac2641280516a355a5ecbe231158987da217b5af30047")
                .unwrap(),
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
                pow_data: vec![],
            },
        },
        body: AggregateBody::new(vec![], vec![], vec![]),
    }
}

#[cfg(test)]
mod test {
    use croaring::Bitmap;
    use tari_common_types::{epoch::VnEpoch, types::Commitment};
    use tari_utilities::ByteArray;

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
        MutableOutputMmr,
    };

    #[test]
    fn stagenet_genesis_sanity_check() {
        // Note: Generate new data for `pub fn get_stagenet_genesis_block()` and `fn get_stagenet_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_stagenet_genesis_block();
        check_block(Network::StageNet, &block, 0, 0);
    }

    #[test]
    fn nextnet_genesis_sanity_check() {
        // Note: Generate new data for `pub fn get_nextnet_genesis_block()` and `fn get_stagenet_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_nextnet_genesis_block();
        check_block(Network::NextNet, &block, 0, 0);
    }

    #[test]
    fn esmeralda_genesis_sanity_check() {
        // Note: Generate new data for `pub fn get_esmeralda_genesis_block()` and `fn get_esmeralda_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_esmeralda_genesis_block();
        check_block(Network::Esmeralda, &block, 4965, 1);
    }

    #[test]
    fn igor_genesis_sanity_check() {
        // Note: Generate new data for `pub fn get_igor_genesis_block()` and `fn get_igor_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_igor_genesis_block();
        check_block(Network::Igor, &block, 5526, 1);
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
            block.header().output_mmr_size
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

        let mut output_mmr = MutableOutputMmr::new(Vec::new(), Bitmap::create()).unwrap();
        let mut vn_nodes = Vec::new();
        for o in block.block().body.outputs() {
            o.verify_metadata_signature().unwrap();
            output_mmr.push(o.hash().to_vec()).unwrap();
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
        assert_eq!(output_mmr.get_merkle_root().unwrap(), block.header().output_mr,);
        assert_eq!(calculate_validator_node_mr(&vn_nodes), block.header().validator_node_mr,);

        // Check that the faucet UTXOs balance (the faucet_value consensus constant is set correctly and faucet kernel
        // is correct)

        let utxo_sum = block.block().body.outputs().iter().map(|o| &o.commitment).sum();
        let kernel_sum = block.block().body.kernels().iter().map(|k| &k.excess).sum();

        let db = create_new_blockchain_with_network(Network::Igor);

        let lock = db.db_read_access().unwrap();
        ChainBalanceValidator::new(ConsensusManager::builder(network).build().unwrap(), Default::default())
            .validate(&*lock, 0, &utxo_sum, &kernel_sum, &Commitment::default())
            .unwrap();
    }
}
