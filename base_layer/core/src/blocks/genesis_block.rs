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

use chrono::DateTime;
use tari_common::configuration::Network;
use tari_common_types::types::{BulletRangeProof, Commitment, FixedHash, PrivateKey, PublicKey, Signature};
use tari_crypto::{signatures::CommitmentAndPublicKeySignature, tari_utilities::hex::*};
use tari_script::script;

use crate::{
    blocks::{block::Block, BlockHeader, BlockHeaderAccumulatedData, ChainBlock},
    covenants::Covenant,
    proof_of_work::{PowAlgorithm, ProofOfWork},
    transactions::{
        aggregated_body::AggregateBody,
        tari_amount::MicroTari,
        transaction_components::{
            EncryptedValue,
            KernelFeatures,
            OutputFeatures,
            OutputType,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
            TransactionOutputVersion,
        },
    },
};

/// Returns the genesis block for the selected network.
pub fn get_genesis_block(network: Network) -> ChainBlock {
    use Network::{Dibbler, Esmeralda, Igor, LocalNet, MainNet, Ridcully, Stibbons, Weatherwax};
    match network {
        MainNet => get_mainnet_genesis_block(),
        Igor => get_igor_genesis_block(),
        Esmeralda => get_esmeralda_genesis_block(),
        LocalNet => get_esmeralda_genesis_block(),
        Dibbler => unimplemented!("Dibbler is longer supported"),
        Ridcully => unimplemented!("Ridcully is longer supported"),
        Stibbons => unimplemented!("Stibbons is longer supported"),
        Weatherwax => unimplemented!("Weatherwax longer supported"),
    }
}

pub fn get_mainnet_genesis_block() -> ChainBlock {
    unimplemented!()
}

pub fn get_igor_genesis_block() -> ChainBlock {
    // lets get the block
    let block = get_igor_genesis_block_raw();

    // Use this code to generate the correct header mr fields for igor if the gen block changes.
    //
    // use croaring::Bitmap;
    //
    // use crate::{KernelMmr, MutableOutputMmr, WitnessMmr};
    //
    // let mut kernel_mmr = KernelMmr::new(Vec::new());
    // for k in block.body.kernels() {
    //     println!("k: {}", k);
    //     kernel_mmr.push(k.hash()).unwrap();
    // }
    //
    // let mut witness_mmr = WitnessMmr::new(Vec::new());
    // let mut output_mmr = MutableOutputMmr::new(Vec::new(), Bitmap::create()).unwrap();
    //
    // for o in block.body.outputs() {
    //     witness_mmr.push(o.witness_hash()).unwrap();
    //     output_mmr.push(o.hash()).unwrap();
    // }
    //
    // println!("kernel mr: {}", kernel_mmr.get_merkle_root().unwrap().to_hex());
    // println!("witness mr: {}", witness_mmr.get_merkle_root().unwrap().to_hex());
    // println!("output mr: {}", output_mmr.get_merkle_root().unwrap().to_hex());

    let accumulated_data = BlockHeaderAccumulatedData {
        hash: block.hash(),
        total_kernel_offset: block.header.total_kernel_offset.clone(),
        achieved_difficulty: 1.into(),
        total_accumulated_difficulty: 1,
        accumulated_monero_difficulty: 1.into(),
        accumulated_sha_difficulty: 1.into(),
        target_difficulty: 1.into(),
    };
    ChainBlock::try_construct(Arc::new(block), accumulated_data).unwrap()
}

fn get_igor_genesis_block_raw() -> Block {
    // Note: Use print_new_genesis_block_igor in core/tests/helpers/block_builders.rs to generate the required fields
    // below
    let sig = Signature::new(
        PublicKey::from_hex("a8e8a9b31f211b51a1df1005c440b899e79e778f111cffe437b9e6d57c68824b").unwrap(),
        PrivateKey::from_hex("18b43279c8bdf554117e896508ac33100fc1ca571d1e94f412454fe3692ade06").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("ba58614ea2e1ac99da8d084f84ad61610ac141570e5767268060be7abe6c0352").unwrap(),
        PublicKey::from_hex("46541b997ab04adfde6a1967b0affa963102a6a6fd0b570f5cded3fd072a4f32").unwrap(),
        PrivateKey::from_hex("1cb1a07f7486ea8dff34664a3a38809a3ed6a458661ed1b8f533d7db7e30a209").unwrap(),
        PrivateKey::from_hex("5f1fb61522d9970a45aa48d0866b0f3e005d7d92aaa38e164c0d6dbbb7898509").unwrap(),
        PrivateKey::from_hex("187b0b24568fdf49fef85b9b12496a0ef2f141b3b28cf8db72316f059a4ba50c").unwrap(),
    );
    let mut body = AggregateBody::new(
        vec![],
        vec![TransactionOutput::new_current_version(
            OutputFeatures {
                output_type: OutputType::Coinbase,
                maturity: 6,
                .. Default::default()
            },
            Commitment::from_hex(
                "8254d5faf0304a11c3e284539bc2bcf2e59013d6f3e616797338c1fcc9f91c1b",
            )
                .unwrap(),
            BulletRangeProof::from_hex("01008750a43ea1eddfa9a467efdbd2672bfafe7681c582ae99768bc2a1a16b021ba69550c7b8692b3312ec40ce6656bd9c475c47b1d9147c04caeb4fc184757b5a6442e08fd66f1f026cf343028e995283ee0202a18703f0d3a75c921effa7ce1848d29f45d21c8c92bd71cd0fb2ab3174b9562637046a0bf0281afb04a127722bd4a71873991edd3477a602d137e4a3829c4e69e14c9a53abd28926316ce2a966ac07a211f78d2e1336a786ef3dbedafe0ce6148447405b2c287da267ae441c7b00d5adf24fec31811eb54da0d32d3b35341da7337d49d82ded9e9103a7d9592c164e5ba86d6bb1a76a7ba192536c28d78a3d6e54ad89c71316c7f1f1458fe402488151a86ebb02d23d0ba836ccc6cb30f26ccdcaa6702a77e8350f6442cd58611c97bc6f0b78867aa92381d79551a645f65570a316105c28e296c8f6f51f015e88963ba7f72d2e66b3553d503aefcf350d59bf7fa783418ad1ea71d8c77b8f0af09f3d4d0faaf28c863724fe275878ab684011ef5913a10ecb103023db68f56640c0a92ab5f74ab04973dc55b4f2cedb0ae78c4d09af9198266874f665f85b3846487ef39d63355ce9792d276b830116cb16f15918799e02337e3662a100f7440e96046715f118cc1d7d2043be3462d34f0568b96111e44c90aa9353cfc45504bacd0be0629b0496c5c9afd2adb40e33823a45e04d0efb7d072aab608e344f045c0c1754a3c7e4894c51f50ec00501081c11e753b81a8cc741ea7bdd4faea80dd80da8b43416810ab743a2802131b7ca4a56ec56f1a25049366c6fa680e22908").unwrap(),
            // For genesis block: A default script can never be spent, intentionally
            script!(Nop),
            // Script offset never checked for coinbase, thus can use default
            PublicKey::from_hex("feb18c3a04b4d20abce7e178885d2761089f6de66a745c9f0489e90aeecfc072").unwrap(),
            // For genesis block: Metadata signature will never be checked
            coinbase_meta_sig,
            Covenant::default(),
            EncryptedValue::default(),
            // Genesis blocks don't need to prove a minimum value
            MicroTari::zero(),
        )],
        vec![TransactionKernel::new_current_version(
            KernelFeatures::COINBASE_KERNEL,
            MicroTari(0),
            0,
            Commitment::from_hex(
                "b6650b9bf71799be0fb76d01864b075cce2af8cb60d39bb9857b83b70f57fc3d",
            )
                .unwrap(),
            sig,None

        )],
    );
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("23 Nov 2022 17:44:00 +0100").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: timestamp.into(),
            output_mr: FixedHash::from_hex("b1fc5c1d29fe2c3792482ebddbd0b3bfe2a5094ca536bf7dbf6c9366d61289d5").unwrap(),
            witness_mr: FixedHash::from_hex("3f12c75e9eab7134b32d12f3834a4d1bfff19f5bdb1cb948b272ff5fbda389e1")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("6ebdddf3ba7cf04ccb1b36549b6d88d76a14c483238af883c5195e0d16d908fd").unwrap(),
            kernel_mmr_size: 1,
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
                pow_algo: PowAlgorithm::Sha3,
                pow_data: vec![],
            },
            validator_node_mr: FixedHash::from_hex("e1d55f91ecc7e435080ac2641280516a355a5ecbe231158987da217b5af30047")
                .unwrap(),
        },
        body,
    }
}

pub fn get_esmeralda_genesis_block() -> ChainBlock {
    let block = get_esmeralda_genesis_block_raw();

    // leaving this code here if we want to add faucets back again the future
    // Add faucet utxos
    // let mut utxos = Vec::new();
    // let file = include_str!("faucets/esmeralda_faucet.json");
    // let mut counter = 1;
    // for line in file.lines() {
    //     if counter < 4001 {
    //         let utxo: TransactionOutput = serde_json::from_str(line).unwrap();
    //         utxos.push(utxo);
    //     } else {
    //         block.body.add_kernel(serde_json::from_str(line).unwrap());
    //         block.header.kernel_mmr_size += 1;
    //     }
    //     counter += 1;
    // }
    // block.header.output_mmr_size += utxos.len() as u64;
    // block.body.add_outputs(&mut utxos);
    // block.body.sort();

    // Use this code if you need to generate new Merkle roots
    // NB: `esmeralda_genesis_sanity_check` must pass
    //
    // use croaring::Bitmap;
    // use std::convert::TryFrom;
    // use crate::{KernelMmr, MutableOutputMmr, WitnessMmr};

    // let mut kernel_mmr = KernelMmr::new(Vec::new());
    // for k in block.body.kernels() {
    //     println!("k: {}", k);
    //     kernel_mmr.push(k.hash().to_vec()).unwrap();
    // }

    // let mut witness_mmr = WitnessMmr::new(Vec::new());
    // let mut output_mmr = MutableOutputMmr::new(Vec::new(), Bitmap::create()).unwrap();

    // for o in block.body.outputs() {
    //     witness_mmr.push(o.witness_hash().to_vec()).unwrap();
    //     output_mmr.push(o.hash().to_vec()).unwrap();
    // if matches!(o.features.output_type, OutputType::ValidatorNodeRegistration) {
    //     let reg = o
    //         .features
    //         .sidechain_feature
    //         .as_ref()
    //         .and_then(|f| f.validator_node_registration())
    //         .unwrap();
    //     vn_mmr.push(reg.derive_shard_key(block.hash()).to_vec()).unwrap();
    // }
    // }

    // let vn_mmr = ValidatorNodeMmr::new(Vec::new());

    // block.header.kernel_mr = FixedHash::try_from(kernel_mmr.get_merkle_root().unwrap()).unwrap();
    // block.header.witness_mr = FixedHash::try_from(witness_mmr.get_merkle_root().unwrap()).unwrap();
    // block.header.output_mr = FixedHash::try_from(output_mmr.get_merkle_root().unwrap()).unwrap();
    // block.header.validator_node_mr = FixedHash::try_from(vn_mmr.get_merkle_root().unwrap()).unwrap();
    // println!("kernel mr: {}", block.header.kernel_mr.to_hex());
    // println!("witness mr: {}", block.header.witness_mr.to_hex());
    // println!("output mr: {}", block.header.output_mr.to_hex());
    // println!("vn mr: {}", block.header.validator_node_mr.to_hex());

    // Hardcode the Merkle roots once they've been computed above
    // block.header.kernel_mr =
    //     FixedHash::from_hex("49bec44ce879f529523c593d2f533fffdc2823512d673e78e1bb6b2c28d9fcf5").unwrap();
    // block.header.witness_mr =
    //     FixedHash::from_hex("8e6bb075239bf307e311f497d35c12c77c4563f218c156895e6630a7d9633de3").unwrap();
    // block.header.output_mr =
    //     FixedHash::from_hex("163304b3fe0f9072170db341945854bf88c8e23e23ecaac3ed86b9231b20e16f").unwrap();
    // block.header.validator_node_mr =
    //     FixedHash::from_hex("e1d55f91ecc7e435080ac2641280516a355a5ecbe231158987da217b5af30047").unwrap();

    let accumulated_data = BlockHeaderAccumulatedData {
        hash: block.hash(),
        total_kernel_offset: block.header.total_kernel_offset.clone(),
        achieved_difficulty: 1.into(),
        total_accumulated_difficulty: 1,
        accumulated_monero_difficulty: 1.into(),
        accumulated_sha_difficulty: 1.into(),
        target_difficulty: 1.into(),
    };
    ChainBlock::try_construct(Arc::new(block), accumulated_data).unwrap()
}

fn get_esmeralda_genesis_block_raw() -> Block {
    // Note: Use print_new_genesis_block_esmeralda in core/tests/helpers/block_builders.rs to generate the required
    // fields below
    let excess_sig = Signature::new(
        PublicKey::from_hex("8a64ce83cd987632fd4bcb307aced5740e656adfc3185e50ee489b36bd883d4c").unwrap(),
        PrivateKey::from_hex("222ca4eac161158851b71b4741740214896cc64a3b4c25941021f18244bfbc0a").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("18529ea19bba1284c4a0781c50502a1e176e99057578dd2d72efb26d3ba87415").unwrap(),
        PublicKey::from_hex("84a4b841f12ca9ea0bdc6d081ba82a1de9f65c84bfad283e795bc46134ab640e").unwrap(),
        PrivateKey::from_hex("4573c856b470879e984271b7b69de4edfe8a11ad16d630d74f84a7962adf180c").unwrap(),
        PrivateKey::from_hex("16ccac329424c399992df5e35f1ea6d7fd7cd24fcf40fc5391e728f8fde23707").unwrap(),
        PrivateKey::from_hex("b5cf21ab8e6841941a883951ba7391ebfd9cf8ae0ea7b40a19f525a255492004").unwrap(),
    );
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures::create_coinbase(6),
        Commitment::from_hex("5eb34e11644a0a721843d8415cdb205b55c028905421b6570cfe751940efc964").unwrap(),
        BulletRangeProof::from_hex("01e28ab8dc0ba8df032076d22a088abcb810225b856afc4bbc2e9687438c6ad415049ace9fef9f76e4ae4354adb9d9c4b03c9a22f4b148cf3afeaf9d5c3d7cea18826900abe14b59b114b1c01675853e761309a889cf6b8031071998beb7976b77e4713fe328b4307c041ee7470fc1492d9cef5795809b9166ee967f2eabcea912d6eb6631b8c95e139d205402df81113bf0090f98c9baea8ee76ab30f21bf95674e1a019ab71e540dc7f064c8622f1bb12173408db2419b5d2f2579db5fc8072714d5031407b24ec42314aaa01c63db34b112f7e63d36b99d533d3a1c39dea06f5ada67d92c31b8bfe7456ce470967d5a6cf55e94451745bca57c0dd7376a906faeb52bedc6d350c9436e327aa7ac967d1bbe7c4ee8d3acbcf3ed75943584862e8c1f8434bd4e290ac5431f70f3041ff04162c3fc4c2f97367f21bcd637b1b057fa791edafe0df8df606825ce35cbf6d7bf3ef456a59a3471c4b2fc916e7f8f4338f3ee3ea6ef46ae5f241ada2941d9245b8056242bb37911194d0ca9ea89fa2702d94ac3a45daceff41023e29dde1e80118873951fb446627e95e41c5ecef627466d26e0f4fba6466a1e087d2b873621aca9eaa343ed48b510095bf6b30caa710e5af6153a8e0be99a04ef254d2fec575d9c0e1ddd8d11263562b8d830c8b53654344179efa52369ab0213d5ada90032aecc5ccf6747685c610525d23f247902427facd1beb0c1aa51d44494d6979a71750a4a7d531286d0a2d1120bdbf11c0ef20c9bf309b016519668d174b875ee48a86f2f07764a60f84fbf97533164590a").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("1615a2a039318a99d880005d7c84a74cfa8ce870c11f9caddeda2cd1954e596f").unwrap(),
        // For genesis block: Metadata signature will never be checked
        coinbase_meta_sig,
        // Covenant
        Covenant::default(),
        EncryptedValue::default(),
        // Genesis blocks don't need to prove a minimum value
        MicroTari::zero(),
    );
    let kernel = TransactionKernel::new(
        TransactionKernelVersion::V0,
        KernelFeatures::COINBASE_KERNEL,
        MicroTari(0),
        0,
        Commitment::from_hex("d40459b01cd2693e774d0e93b19e8d49100a6e0d0aff2277bb2930035f6bc654").unwrap(),
        excess_sig,
        None,
    );
    let mut body = AggregateBody::new(vec![], vec![coinbase], vec![kernel]);
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("14 Nov 2022 11:45:00 +0100").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: timestamp.into(),
            output_mr: FixedHash::from_hex("63403781bb1ff028baeebd3cc9f6c5c0650b554d031554e6089d281a15339c1b").unwrap(),
            witness_mr: FixedHash::from_hex("40f68bf70073cff9458724e75557933278caadd456ae6bd2b93154206e45a07c")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("35c2f73ea31753ae58d8b972579e2a714f1c3531baf77d45bf88978675698544").unwrap(),
            kernel_mmr_size: 1,
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
                pow_algo: PowAlgorithm::Sha3,
                pow_data: vec![],
            },
            validator_node_mr: FixedHash::from_hex("e1d55f91ecc7e435080ac2641280516a355a5ecbe231158987da217b5af30047")
                .unwrap(),
        },
        body,
    }
}

#[cfg(test)]
mod test {

    use croaring::Bitmap;
    use tari_common_types::types::Commitment;

    use super::*;
    use crate::{
        consensus::ConsensusManager,
        test_helpers::blockchain::create_new_blockchain_with_network,
        transactions::{transaction_components::transaction_output::batch_verify_range_proofs, CryptoFactories},
        validation::{ChainBalanceValidator, FinalHorizonStateValidation},
        KernelMmr,
        MutableOutputMmr,
        ValidatorNodeMmr,
        WitnessMmr,
    };

    #[test]
    fn esmeralda_genesis_sanity_check() {
        // Note: Generate new data for `pub fn get_esmeralda_genesis_block()` and `fn get_esmeralda_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_esmeralda_genesis_block();
        check_block(Network::Esmeralda, &block, 1, 1);
    }

    #[test]
    fn igor_genesis_sanity_check() {
        let block = get_igor_genesis_block();
        check_block(Network::Igor, &block, 1, 1);
    }

    fn check_block(network: Network, block: &ChainBlock, expected_outputs: usize, expected_kernels: usize) {
        assert!(block.block().body.inputs().is_empty());
        assert_eq!(block.block().body.kernels().len(), expected_kernels);
        assert_eq!(block.block().body.outputs().len(), expected_outputs);

        let factories = CryptoFactories::default();
        assert!(block.block().body.outputs().iter().any(|o| o.is_coinbase()));
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
        assert!(block
            .block()
            .body
            .kernels()
            .iter()
            .any(|k| k.features.contains(KernelFeatures::COINBASE_KERNEL)));

        // Check MMR
        let mut kernel_mmr = KernelMmr::new(Vec::new());
        for k in block.block().body.kernels() {
            kernel_mmr.push(k.hash().to_vec()).unwrap();
        }

        let mut witness_mmr = WitnessMmr::new(Vec::new());
        let mut output_mmr = MutableOutputMmr::new(Vec::new(), Bitmap::create()).unwrap();
        let mut vn_mmr = ValidatorNodeMmr::new(Vec::new());
        for o in block.block().body.outputs() {
            witness_mmr.push(o.witness_hash().to_vec()).unwrap();
            output_mmr.push(o.hash().to_vec()).unwrap();
            if matches!(o.features.output_type, OutputType::ValidatorNodeRegistration) {
                let reg = o
                    .features
                    .sidechain_feature
                    .as_ref()
                    .and_then(|f| f.validator_node_registration())
                    .unwrap();
                vn_mmr.push(reg.derive_shard_key(block.hash()).to_vec()).unwrap();
            }
        }

        assert_eq!(
            kernel_mmr.get_merkle_root().unwrap().as_slice(),
            block.header().kernel_mr.as_slice()
        );
        assert_eq!(
            witness_mmr.get_merkle_root().unwrap().as_slice(),
            block.header().witness_mr.as_slice()
        );
        assert_eq!(
            output_mmr.get_merkle_root().unwrap().as_slice(),
            block.header().output_mr.as_slice()
        );
        assert_eq!(
            vn_mmr.get_merkle_root().unwrap().as_slice(),
            block.header().validator_node_mr.as_slice()
        );

        // Check that the faucet UTXOs balance (the faucet_value consensus constant is set correctly and faucet kernel
        // is correct)

        let utxo_sum = block.block().body.outputs().iter().map(|o| &o.commitment).sum();
        let kernel_sum = block.block().body.kernels().iter().map(|k| &k.excess).sum();

        let db = create_new_blockchain_with_network(Network::Igor);

        let lock = db.db_read_access().unwrap();
        ChainBalanceValidator::new(ConsensusManager::builder(network).build(), Default::default())
            .validate(&*lock, 0, &utxo_sum, &kernel_sum, &Commitment::default())
            .unwrap();
    }
}
