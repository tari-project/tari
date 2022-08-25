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
    // block.header.kernel_mr = FixedHash::try_from(kernel_mmr.get_merkle_root().unwrap()).unwrap();
    // block.header.witness_mr = FixedHash::try_from(witness_mmr.get_merkle_root().unwrap()).unwrap();
    // block.header.output_mr = FixedHash::try_from(output_mmr.get_merkle_root().unwrap()).unwrap();
    // println!("kernel mr: {}", block.header.kernel_mr.to_hex());
    // println!("witness mr: {}", block.header.witness_mr.to_hex());
    // println!("output mr: {}", block.header.output_mr.to_hex());

    // Hardcode the Merkle roots once they've been computed above
    block.header.kernel_mr =
        FixedHash::from_hex("cd75034a1d4c6ef0cdc7cc28e373d6e8611feeabf7074d1c389a357af9f81d06").unwrap();
    block.header.witness_mr =
        FixedHash::from_hex("186bbe903846017d4bcbac0b7d759066480dcc15a35d9af9ad3aa7c5f592dbda").unwrap();
    block.header.output_mr =
        FixedHash::from_hex("a439bc9f389aa9069293431310296a685e0139e6c7bd655c0fd33aa422156f60").unwrap();

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
        PublicKey::from_hex("d6bcca018adacce9599ec467fb1239cc9a3b5c43fb0dedd806549d4169090d2e").unwrap(),
        PrivateKey::from_hex("c8060fe4133d218fe4fef753f50eb4838b2e4fabb571357875447e9c45763500").unwrap(),
    );
    let coinbase_meta_sig = CommitmentSignature::new(
        Commitment::from_hex("98fa97ba38931bc18c78d68531b1f4a617af90d1748a7ca508a68ff055b4214f").unwrap(),
        PrivateKey::from_hex("a3fa0920d32b6e9e888afb6fb29f5d63a9bdaa1cdd2f5fc8ff0eadc272c4b80c").unwrap(),
        PrivateKey::from_hex("51ff5fe696c5ff279b3f59c3de119cb2433c1d60bd0d478dad6950cae6dc3900").unwrap(),
    );
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures {
            version: OutputFeaturesVersion::get_current_version(),
            output_type: OutputType::Coinbase,
            maturity: 360,
            metadata: Vec::new(),
            sidechain_features: None,
        },
        Commitment::from_hex("b850bb8564ae69f9f9a0ee6769674629172f025bcbdf979f981df546aba9e959").unwrap(),
        BulletRangeProof::from_hex("015c0fb37d025defd4f66fbdba00df78a6370ed44e6478422df0829fb8314fa204e23652cc7fc360ddeaf25ab4b2a57ed18c2d21475d53b0f68e25348e889f1c2594d6b67b65e6aae8e287e3021aecc2edd0608c48a0f382cdc32f082fe99cba29e2bc6b2838fdbdced9f5166648fc8a5557050dcde251f9a267500ddbf9ee751182251eeb250ed0f4d7d86f93e314799b62c2dcc38cc7a3f8a1d42d15ba84bc18cc2cdfbb0a2a819d36cf14d927f88a1512beeab2f41128334c438e6ace3aa81f82a4eb9283d9831e3683ae92374b78134d404a264d9c1e07680ef0df608ac2742c93f88b01f40bedcf2c39e7c0fb93665d78069a85b19390b2105a0efa078101ea9dfd7ca1cc3c6d953a19f013cb4f45525ed6fd93eb4f04e43c79c08087ef5d08faef12cdd5256c0319a271b8eeb7a42f6728ba19841335a3cb0b2b4f55775308711f1461a871f5e66f0e4277b03c902bb2551ab62214cf0d817a26f9044e08de031fe66eae7c488319a4464e4894c4dc25eb2f53bd48473a8ddfe2118cd459b8ed064afb449e3feaf212b376b9cd91150c195caa0f6650405705734e8d517bf4facd0c401f09441f06e376ad6c2be29cb3b262d7cb8b72e1364d92ed3bfc4070bb272ca61a963e3f38e26b11c7bdcdfa36defae553382ef2f5b91bd97ed56b264d3d3545e2f7465c6d6fc656a471b8b7a43cd3f1a9b43ca15a07fb3440d60d739227346087e10c1dcf87ddfe9c5732366c5028a50eb81854bba9bc708aa0097760f04e2e91c316802cdeb136a1a4570e506df65d95907f9f863c7fef533e08").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("20602f2433b93e10f64d07fd41fc6eb7e5a08d4297689fa03b46a6e399926117").unwrap(),
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
        Commitment::from_hex("d4276c8ab208e3c2c24c0308ffd86b69edff15161666dbc5a967781dd3d16737").unwrap(),
        excess_sig,
        None,
    );
    let mut body = AggregateBody::new(vec![], vec![coinbase], vec![kernel]);
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("22 Aug 2022 06:00:00 +0200").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: timestamp.into(),
            output_mr: FixedHash::from_hex("b114e77b790595dfbe45e1b4d805e9c0609a0a69a5a95944064a3f7585a9423f").unwrap(),
            witness_mr: FixedHash::from_hex("b6b18f2de77374641f82447d4738dcfbfa0a2273b8f3ea566590d5e1e0141382")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("9b82a1deefa87548060b487a050e9919f6c11bd9cb4e3ec35e65153375c9f4a4").unwrap(),
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
