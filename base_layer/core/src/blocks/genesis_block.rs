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
        PublicKey::from_hex("7c1108158cc5629d012234a09e46ccea1d55beb8b7da0285de97786b2627455d").unwrap(),
        PrivateKey::from_hex("a22bb83d1cce3e1a840a853ca5f791f765bedfb3f5cd535228903d3ed4d75108").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("f2f9365d8f8b6ad4708a79023206fad1441b4b6e4b908b00f8e54959e44de82f").unwrap(),
        PublicKey::from_hex("323220072bcb607d59a7556d1d80ffdc96a7cc3ffb8cc9c5dd00daef21aa7656").unwrap(),
        PrivateKey::from_hex("14efc02a92097572ae11d5b5f92d1c80736af51d20266e1c3831c3cc90fbba0e").unwrap(),
        PrivateKey::from_hex("93799267d6f6b519d707d14b8fda0ee242f8a691d2b873591c7e087c5c64ab09").unwrap(),
        PrivateKey::from_hex("d174079deaf759e7ea30b9f7f5163ebf71db50948ad5fad7328369eabe3f5f06").unwrap(),
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
                "e6a10a1c67381d3f64c352750f5f43424eb56b6984cf6a9c7818fc5b0697f66c",
            )
                .unwrap(),
            BulletRangeProof::from_hex("013e2f41ec8a64d4246f4c021aff840e44b2e0663f6acd0b90e8aa962592ff0137d6b015093ea0f1c6ed22071b60ed517389f06f8962431bc09f9a0030345be476541252dfa39fc21541a433b374fad2e2e3da2e39f78b2b02b43ee6a3a8cde718a229f9c3a6b2bb604e475042000494581993496609389c73a52bbe9658e50c135c31f035e9301a06a46c1c40cc436c2bbfbaa1a415302507f0f8dc7b7f450431d85279cf0e55380982d3ee5ab50c2c3e63ba176b8f0c0a8031aa60ace2ce1b1784db5176fc0cacd03cd0158d644ccdd472f388e945c5625d9c63ba39fa2bda3b9819b017a49098a8f4b741f993aa5dcbeb3012153cf8ba27abcfe0dfd7e9de1ea29cc3a4aaab1ac5e5d30402b9618560325b598e01871e63c95eecac0861d06434c82ef10e0cdda5015122bed67ad7bcb45745aa05d10bb06ec7bd4aacf5d14cc82c3174b989a1906524ff13f6199060e7701340112624a5c209e92cc30f664426588a063f1f998a81645797f69669a5daf17c158fbfb510da74fe851d1ad201666cb0c933bff95a9d40f9feddc268f4d869cbce7b687af53245ad27a232cb0cc61cdf105383ca0c0ae8373ad7e158537713390bf87960c0c97fed2c5562486f8405596d03a2c248523f3931fd7cfbf5eca3f3749fa5e14dee2e199acc4ded7f8d7ac5d2ceafd825bc4b8f545eebbde751115c0facf638ba1f147b43cec0a40b24cf37941845489c62bd84f7435d3c788b096775b806164dc465f2cee759410ff2d02d5f01bd5771b96118a9e795778f08ddcda37d2986f49f7f66f03d29a30c").unwrap(),
            // For genesis block: A default script can never be spent, intentionally
            script!(Nop),
            // Script offset never checked for coinbase, thus can use default
            PublicKey::from_hex("22e11cad45190efa73c118fb6709cc23e62b7aa76452a5f102ef541e448fee3c").unwrap(),
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
                "6880b1235ae8f3b074433d6899c01dacacc3c77dc7ed061bd61d5de7e6dd3412",
            )
                .unwrap(),
            sig,None

        )],
    );
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("30 Aug 2022 11:48:00 +0100").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: timestamp.into(),
            output_mr: FixedHash::from_hex("9e009822fab5b5b9bcb8b5345a5fb9e30d5c8196ba37fffe6f11d09daa4ad006").unwrap(),
            witness_mr: FixedHash::from_hex("956b885c332f639dfdc4eef1d9c6008cfff73e30f200df21a95e22ebf2a04fc5")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("56c3f43877fbbbd55bd5f77d277ab7236f0accd747c66786e93a68312ef8db1a").unwrap(),
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
        PublicKey::from_hex("72047a1aee71211aadf3a54b9e75cea75ab3aa7a919ba09f74ceb4cae60da315").unwrap(),
        PrivateKey::from_hex("0e71bd9c96f24a77bb0b612489b7a766a6fb2b921c44af4850dbff9dcb224806").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("307770b875e1e8026b32bca6bd9049e9c7541d09c30357537514b1d95bb87a22").unwrap(),
        PublicKey::from_hex("da012fa9054bb010ef0a7253a6355b98c24b75e1e3585a34a3cd880d9992f82d").unwrap(),
        PrivateKey::from_hex("e9fc1e405dfb151f8e061bc1c9446ea32e72c12a996c05e08b3db5d5b6b1d40f").unwrap(),
        PrivateKey::from_hex("45252f91fa48978bbbee4181b7a4339c60df766bcc836eaccb22ac7ad9c3f70c").unwrap(),
        PrivateKey::from_hex("0ba1454fcf5eb571584f04eab2dfd9d83bcf30b001ebce1df1d59bfa0204ff01").unwrap(),
    );
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures::create_coinbase(6).unwrap(),
        Commitment::from_hex("2e26e546a855d4905ed736305d25d6c2eccc87ad2f0fd7abe895ebcfae337f1f").unwrap(),
        BulletRangeProof::from_hex("01228797a7bc59b43b9633ce16e1457e237598f52425631c8fe70f916122287747b2a1105b685306ca37b83fd35b568810bbf65bbc1389529f1a89dc88d48d2c744026965bf310425e036d4728541dd5d9575b476e7215f36e2a05f15bb23edb1c86bf9b3514c7f1aaaef1c6a1fb54060fc15321ae279243087a308e4dfba61506a853aa9f4b9186369d5f9f38c68957df07265ae917b8db61ad7983a259c09639eaa859ee6b447111caf5cc705525cafe4481f7d9b75af7a6a49d7d9ce5beed11c8ddc506378c029e59ab0b3bf143bd430fd651c7ed2c09ff888b3e7d3c39d05bc4816fa0421803321593bcc52de85b5a77e2821b4b3005733d73a62fe16b7e48522ad2186bee77f5dabccbadc2d22bf6c56da4bdf6f761b84eb8d73c7d37151da036705b0d1ec4a443b43f0ed76e051c84fbcc742d1ba6bba574d364ab73bc6f46185e77d51f5e1062f2695795344f9ff271434261eacfe80a3743b33725a246ac943abeca8570a5a2603e5d06bca5fa1989fab80d295abecb6cc2fef1218901b0f6cc3e02dabb9da606ab0b9f50f170dcce9b5941055cb664e11ef45a487368be3b3ffa4ed8aea3fd805553524b106348ff4374dbf1210a7a76b7133abfb809c806991890c89ea0b7ade685f13a7e21709c10738cc42a9b5237eb55b8e12d296e8121a7c884bd62f9130dd6e8d3fc17206dd1e6129443815201a593c22a260829d1311f501765e9d6e5e2143de90d2537bf803bb0726b5fca0500934262220cf2e412076e3aaf643df10afdef62accf35f17edc55dc4dbfa894e74d94baaa07").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("9c0da41d803a1ff02136ead5e3e740a5bb1483c2e22d3229bd42ddfd268e2405").unwrap(),
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
        Commitment::from_hex("98459367ecaafc2176b71cabecde8f513c0708f385980516644940237b27822e").unwrap(),
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
            output_mr: FixedHash::from_hex("8638aa9cd3a78fcda3fccf0e9cbd4277b3fc148953e4fae411b6bf5e583be484").unwrap(),
            witness_mr: FixedHash::from_hex("3f0ac5a672eabdba998073724836cc5439d302834aeb8d946f561b44e68f7fe9")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("c1cd4f1b919601dac24581af034c2e78de999678811a45348ca1afaf706462d0").unwrap(),
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
