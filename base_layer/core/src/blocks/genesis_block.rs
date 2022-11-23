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
        PublicKey::from_hex("14847b7d4081291e42939906ded43909c5a181a39130bc8ed62e59fe3153d846").unwrap(),
        PrivateKey::from_hex("29f894b719f0ab99f9a995a78e0ea55f11d342e68bb729ea7cf9b8dcd9c5f808").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("242247355e4c2d00186c81974671826e00c7bf0028a2ae0c7f8e66d972e44e48").unwrap(),
        PublicKey::from_hex("e6e8c1e5d5db6912bca17f067d70aab9aa5b180a62ea6909cfac9a0df30a5a2b").unwrap(),
        PrivateKey::from_hex("e1f0c72e36a5b5374704a129c4b861303dbc4987ac03d68ffe6dcfbac0757601").unwrap(),
        PrivateKey::from_hex("c4358b4a4aae099b4fcbc4d1ecd25f67d741f7b15485c006e8daeec46ea3910e").unwrap(),
        PrivateKey::from_hex("54ab210426035537e2b2d338aafd182adfe15b22b40ade58d1b27b2d64731b09").unwrap(),
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
                "06cb565a8c38409a1759023f5ac56583ba35f088bec6025b085b4cd3e7da1766",
            )
                .unwrap(),
            BulletRangeProof::from_hex("01a4f545bb7746f67909259d7734f9365c21baaa791ae183e50ae4cd68ababbb4048e0dcfa3622f7eac3d8c39b1a6fe11e7bc3887430ca32b6a59c760b5408911eb49a20d517dcb2c2c1dc7075f9f9a25362db48a1311e72ab5e956a26ef45a224d6afdfbe34dcfe3b6f725ad928090eb67c5e99889264e61a30ffd4bfb79a2a575c762abb04839bec7b683daea73e5efe3d22d9892684ae8450c3c89df0fd9251d6f84d62ef3e4e97b017cc3cc7e3fd727ab733b4275e0065504e959c1b94170e62d7cfece34aa71caf2ed8efe9fa09d5293517a5c4bab4b92e7b2c80d2560834ace03c682bfdd8b07fad18a5afd47e44da8da918f81bbd90aa4be3a28f45a31feecd4e87fee3ac1ece88f0f990d1f7f2c17e69211e8abb975308ead5b858767dba459c009120b8f8664d17a76b9560032abd96061465df438bea345d00ee5324de36729e949168b7a0bac3da87933df32250c7db1388c7477a250b4988cc137c5e6affd0103abd50ee2b4073d3e4dea9c9aa2a98c7e52813accca1aab27e6c2fc61693bb5a3311fb482b38bb086ff73239f1b584e25d70a51e4a0a6c920b7f479e3b803af2cd23f0fba5786fc8150996ce4e4067f059c60fb8661383f2c64f775acf74d43f958163fa1b27be3974984c436599065a2ee1fde4755b8d82e14f0415b40a26a7b08dbe14be611ce44162882d42914eb6d80585f0886c0727b11300b76b06b6cfeff7f2a3584800055d27d67250f0cf29b7e9e1f4aa17d4de21020eb87b7bca31bb7b2ca6b309ecf88e9f91ce2f411edcd4047b365db02509e9b10e").unwrap(),
            // For genesis block: A default script can never be spent, intentionally
            script!(Nop),
            // Script offset never checked for coinbase, thus can use default
            PublicKey::from_hex("088289d426b086579a45af44a3b70ab17b716f2b9f7f42f951448d773180c331").unwrap(),
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
                "c09f671da41ebaca0b044a51a1ce9b2db55e530cced36460c58c1948b84f105f",
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
            output_mr: FixedHash::from_hex("453650d39c3ff19d18933d8c735cd72598d14b0cd6968ed3229cf30b4ab923e2").unwrap(),
            witness_mr: FixedHash::from_hex("fb46b2c571666d61db26cd23dea1dc89c9f87f4e8fb932680b1a5384fc562f75")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("aa74b9889fd614b07889b8f0fad06673b2b7c91c655f413330b3bfe913689c45").unwrap(),
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
        PublicKey::from_hex("346ba68495dd79cea7f698508c51e48a56e91a229204399a4ba4500e64c0aa5b").unwrap(),
        PrivateKey::from_hex("fb9dbc30f5def78c012c44b028a2ac8c8b282cec6187d5cc05b67d9b4b938f0c").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("2a32f461592868b8bdcba7731c20622252794dc98a2957d24c1deb594d709f32").unwrap(),
        PublicKey::from_hex("aa2be9118d664d7aa925d93a5487f663ab2ef9176202fe371068f72eccde5c20").unwrap(),
        PrivateKey::from_hex("ae5ff5ae07d4f58af59d962d7fa9092cacb73c3a391f9c45923e4cdd7b5e9b0b").unwrap(),
        PrivateKey::from_hex("8a87815753d6120a4df84705c38ad6f0860c49ff18641669f11a9c0646216106").unwrap(),
        PrivateKey::from_hex("8b5276a0a8bf3a13b45c52f76c448979120f5cd0e50cf3f89dd733a8123d3902").unwrap(),
    );
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures::create_coinbase(6),
        Commitment::from_hex("1234fe73de8048796af4587b36a17d2fb1cb13f7b2b744de18c80ea6bf589953").unwrap(),
        BulletRangeProof::from_hex("0168b9e28f6899b398bac76890390fb9e060b948a1ba27181e98d54a69e3c867342c7be9f749ca5e05ae44d473b50da9fdb239ba1f01f68debb165e5d1da7b4e50a63c8e32b578e09f45dbfad0522b6373a782e252da899339ee4a15142fa7621682ef7d412cef02f03273af690e306c31dda4e5cdf15dfb29ddb00a9702ed30253edd4658425865ba42f5aa5f496f3eceb95c7824bda2796615ea7697a095bd5f2a88d1c5e411cfd786125990be1296be40625e493210039a22b26479a3f2816d909facb363cfb9bb1d6502fa741a52d3888d727c4e704f2f32f1461d3c69955f2e51df36267c75befea69f9894140e703964e7494a2a203f439c94a5b526da753e46769e6c9b3209e936b2a1f7b7990643f0f67ae9e93e4efb796faf8ab5155102c40730b763533fe9077494d0395572b897ba62d13b828ef90c0e9b84d66a4214a0f80db543f55e4bf8345dba29e42d0a63ca60bbf751d3e373992cfbdb1377d8156883f0c894012dc513b7e2b5da9c99f20e71f2b11f90817194f9dab1316fd8ef80736b65420a3bc65d9590ce3a35daea960129135a609aff0d5e1ec45269a66b837bf7c0793381bf3eec408722b52b4208782fa3cce1adfa9c69e391423d3691c0870b116df8919a072dd719f1edeb72ef0974a6a6829169748a7737bb75d0fb963e8c50ee468e4bfc07a9c1f31979670c859a40cb29296d7127edf4bf00f6f07afef114a0bd40fdaf38966cf863e051dc88afc7c06e7f33fa2699cef80c814466f403e3760edc2d4bd96027cffe8886c453241faf1366f5a1ff496bea06").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("d8beb1850d7097904c30f6aedf5b4030f3714822e6cdb70475d432706327875e").unwrap(),
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
        Commitment::from_hex("4c2fde94e048304867ccf7da32761b89ef30bd54bffda0e683c4df440e897610").unwrap(),
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
            output_mr: FixedHash::from_hex("9b941ae04315ed7bd3ede18515da94d85bf050c0ff5dbf8b660a320dfcdbd0d5").unwrap(),
            witness_mr: FixedHash::from_hex("070745b94c11458aa938e8888a5186deacce085372d456ca4354567f9db11d69")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("270c01c596d4c77c988ff6b68df340683073c4d71c14e70945f1cbd7b2124ad3").unwrap(),
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
