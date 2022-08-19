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
use tari_crypto::{signatures::CommitmentSignature, tari_utilities::hex::*};
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
            OutputFeaturesVersion,
            OutputType,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
            TransactionOutputVersion,
        },
    },
    ValidatorNodeMmr,
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
        PublicKey::from_hex("e22ad0f02bfc08e3b04c667cca050072e091a477c3a4d10345c4114ed4266818").unwrap(),
        PrivateKey::from_hex("2685f18306717ed7ccfad9f96185e5cbca52b3fe109673b1075d130fad54f60e").unwrap(),
    );
    let coinbase_meta_sig = CommitmentSignature::new(
        Commitment::from_hex("ecab12e0bab23ab32a0014b592fcdb4d22be7e02cb5031632ad9c3c9b2560229").unwrap(),
        PrivateKey::from_hex("8a87214524cb2025a3dbaaf7cb5a6287c4d37f7521e667cdc253909adb48c70e").unwrap(),
        PrivateKey::from_hex("6b5f940eaba65b6a46edc112ab9186be9310aeb086cf0878a68bccf73f712600").unwrap(),
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
                "dc07cc8ad8106d33d38239f63bc308959f48d47c8dbe2a65e32662b93262ba09",
            )
                .unwrap(),
            BulletRangeProof::from_hex("01002c676a37bd85610b752598fdc493b0d0023b752b5c620e052731ae1278721dcc1ac376f04e6196083a830115a07452a79c82334b2130bec19784dc60d7dd4418f1fafe27b7519ba72c12dad7e8aa98ca52b5db9e051dc8d58a39f47157d72496c13c839f89fa58fa0c9303d2bf2d51bd8fbe00105602c69a75b9d1f9673f75a6abc51ab102e2ffafe96c5b13d49e2eae5a506d768dd4647aee98fa75b9a364cc3c29b0c01ca7fcc6fbf212e592f68bf104ef2c1cc5202ec500e5b37949e95062090b3d947427a7459b128215dbe75629656651362298691a8ef895d7b0bb3090b15b807a38eba20da1349dbc9bd6bb221fee6a79183433ddac29ef2027877a0230eda904e275ab6c9d87d9d2ea0fca13c92cb678edf5782eea1bdcec0d200c944a9e8c0a20ddcbc9e107ec7a84e7a6a498ba059f9bd9aded2c427a8c021e1c28e961c2f6cc4f490fda74407d99ac3cd54737050e68d7208eea5a7cfa85000fded0cfc6422a66da834bdcb14402bf1857467619143ded6de7a454b778dc1015f848bd278fe2d415334bc29b1113a76bcac126d00a9803ed931ec56fa9f085428ac9197191295e05bbae762092f0918d489a4e39e7220d91f2fc0a7de9b45676eee23c36d05967dd00073e436992456adf5974c3acc618fc11b6a216d8647a6fbaf033cd25898ee229002fec218f531a8de40d0dc4a352296bb92ececc5f4f0e46c1f81ba195fa8a667258bcabe027a44ccee5154fa2821b90ce2694c90883028db0ccd61c59fc8123b9d60bc3a4ed6024addee150c04f0cf410701a865fae07").unwrap(),
            // For genesis block: A default script can never be spent, intentionally
            script!(Nop),
            // Script offset never checked for coinbase, thus can use default
            PublicKey::from_hex("9234814d039bf3ac6545ed40a63570a2720b9376dcbde0bc1a75d081eec50446").unwrap(),
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
                "18d80887a36fae6c2cbef5941d5eedd927aeae1003798bb63c3f292cb68cbe00",
            )
                .unwrap(),
            sig,None

        )],
    );
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("08 Aug 2022 10:00:00 +0200").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: timestamp.into(),
            output_mr: FixedHash::from_hex("55cd15eb1966b15e3dc8f8066371702a86b573915cd409cf8c20c7529a73c027").unwrap(),
            witness_mr: FixedHash::from_hex("188b79e4cd780914fc0dfe7d57b9f32bfae04293052b867fce25c4af8b5191dc")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("2e3fde9cd20b48f699523d1b107b4742c6aa03ed1cb210f580d0c7426463b966").unwrap(),
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
            validator_node_merkle_root: vec![0; 32],
        },
        body,
    }
}

pub fn get_esmeralda_genesis_block() -> ChainBlock {
    let mut block = get_esmeralda_genesis_block_raw();

    // Add faucet utxos
    let mut utxos = Vec::new();
    let file = include_str!("faucets/esmeralda_faucet.json");
    let mut counter = 1;
    for line in file.lines() {
        if counter < 4001 {
            let utxo: TransactionOutput = serde_json::from_str(line).unwrap();
            utxos.push(utxo);
        } else {
            block.body.add_kernel(serde_json::from_str(line).unwrap());
            block.header.kernel_mmr_size += 1;
        }
        counter += 1;
    }
    block.header.output_mmr_size += utxos.len() as u64;
    block.body.add_outputs(&mut utxos);
    block.body.sort();

    // Use this code if you need to generate new Merkle roots
    // NB: `esmerlada_genesis_sanity_check` must pass
    //
    // use croaring::Bitmap;
    // use std::convert::TryFrom;
    // use crate::{KernelMmr, MutableOutputMmr, WitnessMmr};
    //
    // let mut kernel_mmr = KernelMmr::new(Vec::new());
    // for k in block.body.kernels() {
    //     println!("k: {}", k);
    //     kernel_mmr.push(k.hash().to_vec()).unwrap();
    // }
    //
    // let mut witness_mmr = WitnessMmr::new(Vec::new());
    // let mut output_mmr = MutableOutputMmr::new(Vec::new(), Bitmap::create()).unwrap();
    //
    // for o in block.body.outputs() {
    //     witness_mmr.push(o.witness_hash().to_vec()).unwrap();
    //     output_mmr.push(o.hash().to_vec()).unwrap();
    // }
    //
    // block.header.kernel_mr = FixedHash::try_from(kernel_mmr.get_merkle_root().unwrap()).unwrap();
    // block.header.witness_mr = FixedHash::try_from(witness_mmr.get_merkle_root().unwrap()).unwrap();
    // block.header.output_mr = FixedHash::try_from(output_mmr.get_merkle_root().unwrap()).unwrap();
    // println!("kernel mr: {}", block.header.kernel_mr.to_hex());
    // println!("witness mr: {}", block.header.witness_mr.to_hex());
    // println!("output mr: {}", block.header.output_mr.to_hex());

    // Hardcode the Merkle roots once they've been computed above
    block.header.kernel_mr =
        FixedHash::from_hex("1bfbc0e257b4876a88025bc07d5a9bc09dde36441fcb597dcee326ab76b90eed").unwrap();
    block.header.witness_mr =
        FixedHash::from_hex("0abe819c208dc98149699dd009715b8e302c666b3322a67d31f1d74a0593999f").unwrap();
    block.header.output_mr =
        FixedHash::from_hex("69b9646e0e57a64a4ab44d05e6c45146bfc02c7449cf5cc6e404190064ee4309").unwrap();

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
        PublicKey::from_hex("9e6be7c87533b2e01763de34c309b1c283e5e5e91500a43856a78dcb26b4233f").unwrap(),
        PrivateKey::from_hex("6dc0083f9f4b0deb85b34e6d32598e77a729d6f695568322f461d006929dbb04").unwrap(),
    );
    let coinbase_meta_sig = CommitmentSignature::new(
        Commitment::from_hex("841ae8f109a4fcefb2522de04ebff133288156f9ebd3380c42727b41df9e3b15").unwrap(),
        PrivateKey::from_hex("7270bc342a378be7b16aa5ff6d5e231fd0acfa4cd46f64a4872e42465f2a7b0c").unwrap(),
        PrivateKey::from_hex("51a18d5eb531cee771765a8b4ae5444e17c508549cdba62d942d172a247b0709").unwrap(),
    );
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures {
            version: OutputFeaturesVersion::get_current_version(),
            output_type: OutputType::Coinbase,
            maturity: 6,
            metadata: Vec::new(),
            sidechain_features: None,
            validator_node_public_key: None,
            validator_node_signature: None,
        },
        Commitment::from_hex("2afed894ae877b5e9c7450cc0e29de46aeb6b118cd3d6b0a77da8c8156a1e234").unwrap(),
        BulletRangeProof::from_hex("0136b44930772f85b17139dd8e83789f84ccc2134cf6b2416d908fb8403efa4d3bc0247ec4afbbb1f7f7498d129226f26199eec988bd3e5ccce2572fd7aee16f2c4a2d710fac0e3bc1d612d700af2265e230ae1c45e3b0e4d3aab43cb87534217b56dcdb6598ed859d0cd6d70fae5acaaa38db5bbae6df8339e5e3dd594388bd53cef6f2acda4ac002d8ac6e01d430bdcf8565b8b8823ff3fb7dc8b359e687dd6feab0edf86c7444c713f34d2513145049b9664aae2e3dbc8a3365baae9d26842852ec9f401112a9742560ec220e61b05f65448d75b714839a6bafc723e9a04f25c69c036775fc55b7ec2bb28ef1de25a32cac51c288ed6d43f3819b1c3356d7699ea5f10217d553e90e6c93641649bd289dedb9e5725579539df07301f15093496c8fca3ec66a43332d1be3a3f94b530e1b8ca7feaa24c4ca73e60397a786ab742ac8933ba6bd504ef3c1a53fa1ff4397aba7c42a526507f930fdf9ff00a2a07b521841574d4e2b5beece946a15fa2545c8e556e704eed0ed10c0e3cbb9f5d6147e6e2d260666c79fa04d89c8901eeb3d3793239a68218a2c105f1bcb4211631eea037102bd5c840de751d84f473bb5cf6c41b3b97ec1c978700ec3c132e09a28d0a92c7e141e9968d0d2852c339a85c052356049f6752cb57c3d2b8c03db24525aa1f7db4a4f4d7d48639e27faa8c8bc695ad6c4f7688d43feedabef4d05c20b349ebc1697b3b899038b22fa308546efff290902cdacbe9992450cc31b61fc00652cffe4335c080d8398b061add986626068e17d5982ee9f6f28b4f4579d0406").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("1e036eb452b9098b48edeaa3b91716502fc4786e1ac4363046546f28d26bb337").unwrap(),
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
        Commitment::from_hex("2480268904dbe6cb4b4af290fd51b43383588a575c926af674311691e5a6cc59").unwrap(),
        excess_sig,
        None,
    );
    let mut body = AggregateBody::new(vec![], vec![coinbase], vec![kernel]);
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("24 Aug 2022 22:00:00 +0200").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    let vn_mmr = ValidatorNodeMmr::new(Vec::new());
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: timestamp.into(),
            output_mr: FixedHash::from_hex("49a0bfcf8dd896d59ab2eb1c5a8c96b49cce5ef9c4bed1a172cc2cd713b2a04d").unwrap(),
            witness_mr: FixedHash::from_hex("bd043cfe6304c0cb2b6cdbc6ad52c03ff893f3b53631be41846fbac75c422c7e")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("df4d2a7d15da3485b8fcaf0524f1ee5d409883bbe8901c4df69543b056763d42").unwrap(),
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
            validator_node_merkle_root: vn_mmr.get_merkle_root().unwrap(),
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
        WitnessMmr,
    };

    #[test]
    fn esmeralda_genesis_sanity_check() {
        // Note: Generate new data for `pub fn get_esmeralda_genesis_block()` and `fn get_esmeralda_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_esmeralda_genesis_block();
        assert_eq!(block.block().body.outputs().len(), 4001);

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

        for o in block.block().body.outputs() {
            o.verify_metadata_signature().unwrap();

            witness_mmr.push(o.witness_hash().to_vec()).unwrap();
            output_mmr.push(o.hash().to_vec()).unwrap();
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

        // Check that the faucet UTXOs balance (the faucet_value consensus constant is set correctly and faucet kernel
        // is correct)

        let utxo_sum = block.block().body.outputs().iter().map(|o| &o.commitment).sum();
        let kernel_sum = block.block().body.kernels().iter().map(|k| &k.excess).sum();

        let db = create_new_blockchain_with_network(Network::Esmeralda);

        let lock = db.db_read_access().unwrap();
        ChainBalanceValidator::new(
            ConsensusManager::builder(Network::Esmeralda).build(),
            Default::default(),
        )
        .validate(&*lock, 0, &utxo_sum, &kernel_sum, &Commitment::default())
        .unwrap();
    }

    #[test]
    fn igor_genesis_sanity_check() {
        let block = get_igor_genesis_block();
        assert_eq!(block.block().body.outputs().len(), 1);

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
        assert_eq!(block.block().body.kernels().len(), 1);
        assert_eq!(block.block().body.outputs().len(), 1);
        for o in block.block().body.outputs() {
            witness_mmr.push(o.witness_hash().to_vec()).unwrap();
            output_mmr.push(o.hash().to_vec()).unwrap();
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

        // Check that the faucet UTXOs balance (the faucet_value consensus constant is set correctly and faucet kernel
        // is correct)

        let utxo_sum = block.block().body.outputs().iter().map(|o| &o.commitment).sum();
        let kernel_sum = block.block().body.kernels().iter().map(|k| &k.excess).sum();

        let db = create_new_blockchain_with_network(Network::Igor);

        let lock = db.db_read_access().unwrap();
        ChainBalanceValidator::new(ConsensusManager::builder(Network::Igor).build(), Default::default())
            .validate(&*lock, 0, &utxo_sum, &kernel_sum, &Commitment::default())
            .unwrap();
    }
}
