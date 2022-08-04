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
use tari_common_types::types::{
    BulletRangeProof,
    ComSignature,
    Commitment,
    PrivateKey,
    PublicKey,
    Signature,
    BLOCK_HASH_LENGTH,
};
use tari_crypto::tari_utilities::{hash::Hashable, hex::*};
use tari_script::TariScript;

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

    // user this code to generate the correct header mr fields for igor if the gen block changes.
    // use croaring::Bitmap;
    // use tari_mmr::{MerkleMountainRange, MutableMmr};
    // use tari_crypto::hash::blake2::Blake256;
    //
    // let mut kernel_mmr = MerkleMountainRange::<Blake256, _>::new(Vec::new());
    // for k in block.body.kernels() {
    //     println!("k: {}", k);
    //     kernel_mmr.push(k.hash()).unwrap();
    // }
    //
    // let mut witness_mmr = MerkleMountainRange::<Blake256, _>::new(Vec::new());
    // let mut output_mmr = MutableMmr::<Blake256, _>::new(Vec::new(), Bitmap::create()).unwrap();
    //
    // for o in block.body.outputs() {
    //     witness_mmr.push(o.witness_hash()).unwrap();
    //     output_mmr.push(o.hash()).unwrap();
    // }
    //
    // println!("kernel mr: {}",kernel_mmr.get_merkle_root().unwrap().to_hex());
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
    let sig = Signature::new(
        PublicKey::from_hex("203664cc8c35419344b72180e4b02c6cc37ac5c362484f534c0d31fdac06dd61").unwrap(),
        PrivateKey::from_hex("5770dcd08cdd77fdc671142715701eb4376142d3d720b3e2d58ba0229303aa0e").unwrap(),
    );
    let mut body = AggregateBody::new(
        vec![],
        vec![TransactionOutput::new_current_version(
            OutputFeatures {
                output_type: OutputType::Coinbase,
                maturity: 3,
                .. Default::default()
            },
            Commitment::from_hex(
                "18c4353be73b6e9ee6d8ca7debee1cd23b81f4411656dca820f3868315004712",
            )
                .unwrap(),
            BulletRangeProof::from_hex("01c494369ee466d0a3b118d41a9b330df66315d0e636c303eea21a4ccf8860b1274ad84faf8134968e475c171798279abbf49868e519b59d4d4b1102aeded7c55030f67cae6f142f7267e8f9da5d7bd930e9bfa17f1488fb102506b04e33da022ef8c4f0096c465ff3f35b8bcafa50ed45a8a8cccb14c525e91a01b2d202209836b8acbb49657db67326dc6ab08fb2d62d26966a4d6e3ecd835ce0eb531f012864862d45591bc4e4947c70e7b2209e52bc22d74ecde4ff1ab4a1f2c28f8662c22854804cb108ad805a3c5a568db8a49f888aa2117fda9bb2dfc469aba74dc070460268e495afd969b26a6ca15593714a0940eb7fdf6d345a888cfdbc3ba22b884db264c3a9321b50c3b79b2a8c10fcd46f21aa6967e2d4eb0f583c87182bcde6111e7c725d9428625cfff9a7c07ab07fdeb79de450dcdeec3b6cde3c010efbc264748f9ad03d97390482a0c12d50a9098f6e2306068401f114e5c9a1aa78cabd5bde72258c8efa3acc78683f88ebbf135b0859417cf0e2817c21a2bf62fe1a0c668830c91db72d099e5ba701c6c5d1ed421f512732c847cd9abc95862bd1da5042b8964281ebd979f9a0813513afe7d0b74563740fdf9b9232c63535b9dd58111d6c962895f3dac4fc159e7dd91263bc2a51e746639666dcb6d0cba6bce0ca20539544f4789706ff652dd69f142009fc60a0138e9c6665a44bd42ae75bb16d8f090306ad993e00f7f9b70795495f8fa1070aaf07631e36c792c00439dbbe0aeb02169085524917c43cca4ddd5d5d2bca75fc5a4538d203d3157b3de63565a1c604").unwrap(),
            // For genesis block: A default script can never be spent, intentionally
            TariScript::default(),
            // Script offset never checked for coinbase, thus can use default
            Default::default(),
            // For genesis block: Metadata signature will never be checked
            Default::default(),
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
                "9aa9dd0b143c99130fb662cf5920d60c7b74b2a59da7bf62309d5e17e2f03862",
            )
                .unwrap(),
            sig,None

        )],
    );
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("3 Aug 2022 10:00:00 +0200").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: vec![0; BLOCK_HASH_LENGTH],
            timestamp: timestamp.into(),
            output_mr: from_hex("2cb46eac3f46a0cc8c866c74572550fdb37d090381163c5009196cef0a242aa2").unwrap(),
            witness_mr: from_hex("a453af278bf2e25f803a7748c6f088ffc0881a922f5211073b9d64234dbfba6d").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("b1bf9dd18b155899e318fefe4f76aa19eab951efa71ac1fe3731c2d19eee1e47").unwrap(),
            kernel_mmr_size: 1,
            input_mr: vec![0; BLOCK_HASH_LENGTH],
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

    // use croaring::Bitmap;
    // use tari_mmr::{MerkleMountainRange, MutableMmr};
    // use tari_crypto::hash::blake2::Blake256;
    //
    // let mut kernel_mmr = MerkleMountainRange::<Blake256, _>::new(Vec::new());
    // for k in block.body.kernels() {
    //     println!("k: {}", k);
    //     kernel_mmr.push(k.hash()).unwrap();
    // }
    //
    // let mut witness_mmr = MerkleMountainRange::<Blake256, _>::new(Vec::new());
    // let mut output_mmr = MutableMmr::<Blake256, _>::new(Vec::new(), Bitmap::create()).unwrap();
    //
    // for o in block.body.outputs() {
    //     witness_mmr.push(o.witness_hash()).unwrap();
    //     output_mmr.push(o.hash()).unwrap();
    // }
    //
    // block.header.kernel_mr = kernel_mmr.get_merkle_root().unwrap();
    // block.header.witness_mr = witness_mmr.get_merkle_root().unwrap();
    // block.header.output_mr = output_mmr.get_merkle_root().unwrap();
    // println!("kernel mr: {}", block.header.kernel_mr.to_hex());
    // println!("witness mr: {}", block.header.witness_mr.to_hex());
    // println!("output mr: {}", block.header.output_mr.to_hex());

    // Hardcode the Merkle roots once they've been computed above
    block.header.kernel_mr = from_hex("eeabdfb84801078375e9e7cce176ed771899c93824bb8811fa8bbc37fbaa4fe5").unwrap();
    block.header.witness_mr = from_hex("9f885db6d51ede231ae0bb51825842d2b63697fc26f7fe38e054188e1796c220").unwrap();
    block.header.output_mr = from_hex("a253ac233a987a92fd5846299c36cf84dff577a10381ec11b569a6e601c274d3").unwrap();

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
    // Note: Use print_new_genesis_block in core/tests/helpers/block_builders.rs to generate the required fields below
    let excess_sig = Signature::new(
        PublicKey::from_hex("ceb844091f8233963bc5d7eb3225ba7a10c89ef6f21c2a51a8adf4eff308c534").unwrap(),
        PrivateKey::from_hex("16ff8559202ebcdd8a7d05b9c28e11a1a078f89f2cfc08649dc04c17d4a9720d").unwrap(),
    );
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures {
            version: OutputFeaturesVersion::get_current_version(),
            output_type: OutputType::Coinbase,
            maturity: 6,
            metadata: Vec::new(),
            unique_id: None,
            sidechain_features: None,
            parent_public_key: None,
            asset: None,
            mint_non_fungible: None,
            sidechain_checkpoint: None,
            committee_definition: None,
        },
        Commitment::from_hex("b2e5221132d496657dc2fa9637c1055f4d2ac7d940e66592fe1c77942358235e").unwrap(),
        BulletRangeProof::from_hex("01faeaa022e9cd807931efd23bed275fc33591a5966025451c8baf5e82a67fb15872a8268990fdafeae0182e58d9b86991c7dbe44f7bfd1763cbffc3ba87ec34095aa0c4d080024c9bae59d54c074ee5901e413824acb119b3ce33830578876c2d760517d85c938207d017df2ee60226847ddf9adea144a830698a61855e20520ebc407769e5d56139e603c80cff4bd367bb09a6e3eef236b8404ba1a9e408ee706867cbcce6efdb0548adbd89471c2ad7137aaee71c1605b3d3b2a5776d28ba02cee0333c1e712ced501036da0f9109538d77b62e52e515e552066685f85aaa5aa0a00ef48d0686c9dd6651f43b7d9e31f1284f08b9addb65f58cbfeff8dc4b7358d9ade46475b735f71d6ec7ebba0b902e9a11507a6d370fec5d4c39c181ea619818e09173a7165885fab1991e94ebf063de1a8b85e81c248165b57f5f741351885a817e5ef2e271dda7de399bd1310ddc01975d0e66348ed855e117b8d14032a2349bb73fbdc2223d3f3ea24049b7394075b3e1de79e4f9da178c6a24bd1d7e28330c2747bc3349795c1d5d6a3373d359c9be7018224905a8e23316db3a036a403cfe23bd26f41a86953751793175f5df9c707dcdb5de102131cf9e8f590a10a8a617e1ed65283b64db0fda2d18502ba716c75e12bc39036f7679acbb65cf28f940969ca626e0d175c85920f348df23fe54e157023f603d9dcdeef169a12101b134d2db41776c1bc3690f22e42869d74672ce4a20ed66fc7b70d27e33f6d7012ff00875c007b5982c9c44653d65c9549656d054ba6d88c204de2e8936d9b30d").unwrap(),
        // A default script can never be spent, intentionally
        TariScript::default(),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::default(),
        // For genesis block: Metadata signature will never be checked
        ComSignature::default(),
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
        Commitment::from_hex("e4311dab0709ba9772c2481afae53baea73d84d3387120c69011d4d791afac3a").unwrap(),
        excess_sig,
        None,
    );
    let mut body = AggregateBody::new(vec![], vec![coinbase], vec![kernel]);
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("03 Aug 2022 10:00:00 +0200").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: vec![0; BLOCK_HASH_LENGTH],
            timestamp: timestamp.into(),
            output_mr: from_hex("6150e83e5af8d7cb30ffb4502e11d92279f96d5b62bced2bf83b3f69e6397265").unwrap(),
            witness_mr: from_hex("f4c36fdfb0d6eda47e024a31a09532adb8bd03d5a4ffdc07ba6b3aae8e5d9e57").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("4012f576dd139febd8023d15e0427767d4d7a0966f59bd6fd31fa5f6c064a3d3").unwrap(),
            kernel_mmr_size: 1,
            input_mr: vec![0; BLOCK_HASH_LENGTH],
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
    use tari_crypto::hash::blake2::Blake256;
    use tari_mmr::{MerkleMountainRange, MutableMmr};

    use super::*;
    use crate::{
        consensus::ConsensusManager,
        test_helpers::blockchain::create_new_blockchain_with_network,
        transactions::{transaction_components::transaction_output::batch_verify_range_proofs, CryptoFactories},
        validation::{ChainBalanceValidator, FinalHorizonStateValidation},
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
        let mut kernel_mmr = MerkleMountainRange::<Blake256, _>::new(Vec::new());
        for k in block.block().body.kernels() {
            kernel_mmr.push(k.hash()).unwrap();
        }

        let mut witness_mmr = MerkleMountainRange::<Blake256, _>::new(Vec::new());
        let mut output_mmr = MutableMmr::<Blake256, _>::new(Vec::new(), Bitmap::create()).unwrap();

        for o in block.block().body.outputs() {
            if !o.is_coinbase() {
                o.verify_metadata_signature().unwrap();
            }
            witness_mmr.push(o.witness_hash()).unwrap();
            output_mmr.push(o.hash()).unwrap();
        }

        assert_eq!(kernel_mmr.get_merkle_root().unwrap(), block.header().kernel_mr);
        assert_eq!(witness_mmr.get_merkle_root().unwrap(), block.header().witness_mr);
        assert_eq!(output_mmr.get_merkle_root().unwrap(), block.header().output_mr);

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
        let mut kernel_mmr = MerkleMountainRange::<Blake256, _>::new(Vec::new());
        for k in block.block().body.kernels() {
            kernel_mmr.push(k.hash()).unwrap();
        }

        let mut witness_mmr = MerkleMountainRange::<Blake256, _>::new(Vec::new());
        let mut output_mmr = MutableMmr::<Blake256, _>::new(Vec::new(), Bitmap::create()).unwrap();

        for o in block.block().body.outputs() {
            witness_mmr.push(o.witness_hash()).unwrap();
            output_mmr.push(o.hash()).unwrap();
        }

        assert_eq!(kernel_mmr.get_merkle_root().unwrap(), block.header().kernel_mr);
        assert_eq!(witness_mmr.get_merkle_root().unwrap(), block.header().witness_mr);
        assert_eq!(output_mmr.get_merkle_root().unwrap(), block.header().output_mr);

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
