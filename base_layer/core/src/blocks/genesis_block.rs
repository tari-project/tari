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
use tari_common_types::types::{BulletRangeProof, Commitment, PrivateKey, PublicKey, Signature, BLOCK_HASH_LENGTH};
use tari_crypto::{
    signatures::CommitmentSignature,
    tari_utilities::{hash::Hashable, hex::*},
};
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
        PublicKey::from_hex("5ef2b1f20d1f2af8dd2bf816218d26c1d11a2e2e2b934a190945e97e0428ce54").unwrap(),
        PrivateKey::from_hex("7955c5f2db0c243194d7dbbc587f2a21b8c69107a3fb0f90661a936302a9670e").unwrap(),
    );
    let coinbase_meta_sig = CommitmentSignature::new(
        Commitment::from_hex("beb78b4d22dc8ceccaee9871484d3052e2bd1bb5b4323085f3e17a104b41b552").unwrap(),
        PrivateKey::from_hex("0ee30fde0c595b3fe6ce08079f4cf3e4d062ec1b7c61e9640efcb4d2dbea1704").unwrap(),
        PrivateKey::from_hex("92c0fd252fb03bdf0a470ea2cc957505f764eef8bbeda2a30a21e9d870c3b604").unwrap(),
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
                "e82395916d1090bc2bc8eaada037e6747558d72614c685aefe008ead4caf4637",
            )
                .unwrap(),
            BulletRangeProof::from_hex("0104072270fe6020497cd87d9da4b696a3727f6e46a0b66178f66de21f4c67a33766ccdea1422dd687e834d499b8ad53fe949fea878e5fbb1b00db312a377ca54c8a76776384f834ced9d17f0f5141efb3acd8b2d18b065eedb7b62e468454f87d36f2b5b3a2f430de8ec04ec45498388974af522daed4ede872dc0946c20aff4378402b241968ad850d3c3480ba836d435a78b571447cb4f96365d24c650694584421741197a4b7a206b3d728ce54860de73fb7821af5ebd633448444a275347aea2e10fb95355aa633dfff83a651c1f70337fecb91afe061eb14081f7be9d866acc0cfaf45289c7e937490b86f30c0a2686a65491a9f5cfd40cb3849181dc22fb6706d8f4a7eb27dd0a80fcf6ce121d4a79f709c83280fc21944877a21ec86466cb8aa752d1ea5fc4d822f91c8382798206b3c0157b1f939d659100139cf396664f6b8d945887112038785559dfafe4c7cdc77fc1d822e7c68ef2409ae82f002943650be275c23bbf80ad6fb0d3ab077a7b9e1c1c3051988886b8a78d0d0247f9a7ad70b419b63437959a3ad5a7e0cf821325e901a65f10fa5f58e70c67b5d47b81c9d1201acffede4bb377522771213975bda5366645451075f0f95e9405e26d810dfeddc0e9f2d6801af7db3763126b4c6085d6ae6d3ede44663168126ff4ac1e83deb4cb722f9c7249d18dba33e0e7169326bfd7fa586d990f3faefa6530348f83bb8786e035e4943bcafeb7db5ca97d0ed727e07fe30411063aaa20304061c91c56230c2207254586db650c897bc1782fc40c6f9df2a415bc389b3a6f901 ").unwrap(),
            // For genesis block: A default script can never be spent, intentionally
            script!(Nop),
            // Script offset never checked for coinbase, thus can use default
            PublicKey::from_hex("3087544d44fa95c0a92dc41f52d2586f03344a4f7b12654c0f07478944dd5203").unwrap(),
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
                "6a5b18da02df69f4cc69628b818f1a5bb7079c2b27021d69473d4c5288545a74",
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
            prev_hash: vec![0; BLOCK_HASH_LENGTH],
            timestamp: timestamp.into(),
            output_mr: from_hex("b7303adaedfb8ad36f297b259411285938c9ce3640f83e7e28a326d90e57a22c").unwrap(),
            witness_mr: from_hex("4c65d88db394ae7ca4634a26fda6b71dab8ec19fe69ca0c9a0cda638c2b2f874").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("7b2a06b6cd391f241fd94eec3891ea52554f0a2a8aa7b6f45a125cc7c13e728a").unwrap(),
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
    //
    // use croaring::Bitmap;
    // use tari_crypto::hash::blake2::Blake256;
    // use tari_mmr::{MerkleMountainRange, MutableMmr};
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
    block.header.kernel_mr = from_hex("a6d5e97528fcf06741efdc51e7d7aeb800498a6370175f976d72583c30bdd6b6").unwrap();
    block.header.witness_mr = from_hex("7e5e52e975fb4a60213c3ace23e32be1c25998efd53614c3d941f1938314a002").unwrap();
    block.header.output_mr = from_hex("3421092eec9e185ffeadebc821e0fcfb543e99bba5f2f0df570d478633f60f79").unwrap();

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
        PublicKey::from_hex("6451056817c90fb805bac8d224ddaa1e9808707d5783265c6c7d680d01983f78").unwrap(),
        PrivateKey::from_hex("f4760c0552d6352420b2187aa30b5b6f8e59e9de79486268a7970040dd04ce0f").unwrap(),
    );
    let coinbase_meta_sig = CommitmentSignature::new(
        Commitment::from_hex("d091215bef2cf35b4bedd404ae30091f068dd38de250aa953b7cd2ab12207b33").unwrap(),
        PrivateKey::from_hex("e20b5e6e9d6a42c0081d9108bbab7cd32406356f020b5c3077d664f7e95e840c").unwrap(),
        PrivateKey::from_hex("97777a4fcab5348a52270e9d885d8539a3272d17fb781a74ac8a66b701ff6e0f").unwrap(),
    );
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures {
            version: OutputFeaturesVersion::get_current_version(),
            output_type: OutputType::Coinbase,
            maturity: 3,
            metadata: Vec::new(),
            sidechain_features: None,
        },
        Commitment::from_hex("a05abd3009b42f0713e8bda9d33c0d12ef8ad048929ea1e6e62ed9423715305d").unwrap(),
        BulletRangeProof::from_hex("01acb432c02a7de7016df76edf94a97f19022a6c0b254cdf329f78dac57dba0e23c43f97851dd063f1388f9bbc2c9998c4446691404742b965643f12a6591c68545ccd912272d9fa8496109e84c15bb658baca78a619cee1403f7016b0b79f840ae0d2fb61d9d1832ee825fa54d943d9343eb4b48bf7be8b894291f3e63a4e7e6fd09e7bc033c1098c7ceec1ef0c99c5d24c9918ea301389300e0be5c62b848b35bc62a72b63f59c01ddfb80a9051e8f8d1da10d551a1cfc45f06a3e739b1ebf718e6808e3e04986401bbad703224c5d06107d83788c2586f4e51e5f0fd5071566e01335e1d2ef83655d8ae63ff03d00607469ee77c22e078398963ce2a1ed917ec630f533528c0c5dca366edd9a59317a23704c4a041ed8de774040e50ee7b576f4d3cffdcf2bd54f6968de9a68c7b7cd955064f7d3531ff1978e7dff692b9932eae7a05c70d7687cba4255da922acbd89c7424aaefacde949bcc6b4119dd8153026d599646e3bbc527ff3d807cc3ccac72d0d62aaf29671db66dfb27a269c55f684b0e0be9fa70d342b4dad548e31a6712ebb7205ac783caba96b852fc2ccc17007c71e786428023ced2203ab86b968373aa5ab22c719d603805035ed26cf8173e5f2d7d6871ebb0f16ca9fd606ad2c66fa740aacfe716f4b9c53a44109c9f259539cc5bad91ebd1af13cb2f18d233aba7a96ef08647f9896c8eed6eb1cbb40b8c2b80d644860a7457d7fbde4f42f04b2a2239619a06fa5247cb3874c7cac80ce70f6286732a0fc0c1f7631f7c800189f69bde41203746d2ca9fbf52e592d401").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("32a21d53836d8476458be02a9a591f7e37278383967ef42ded35f8d63f2f7c25").unwrap(),
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
        Commitment::from_hex("947f7949c54c831c2189a1208bbb9c77fda9fd246ff4948a9fad542b27b4d426").unwrap(),
        excess_sig,
        None,
    );
    let mut body = AggregateBody::new(vec![], vec![coinbase], vec![kernel]);
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("08 Aug 2022 10:00:00 +0200").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: vec![0; BLOCK_HASH_LENGTH],
            timestamp: timestamp.into(),
            output_mr: from_hex("ec3f1d12ebb1431132510b971f0747979a4304148775e4a1c483dd36ef127081").unwrap(),
            witness_mr: from_hex("1dcd2c3210813980c7d7523800555d6fc2db4698d75995329551b39c25a28c3b").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("0dd48e599a8d22e29f6ea80a88875761393d21546a72904b99428d54780e6078").unwrap(),
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
            o.verify_metadata_signature().unwrap();

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
        assert_eq!(block.block().body.kernels().len(), 1);
        assert_eq!(block.block().body.outputs().len(), 1);
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
