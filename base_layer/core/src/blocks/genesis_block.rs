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
use tari_crypto::{
    script::TariScript,
    tari_utilities::{hash::Hashable, hex::*},
};

use crate::{
    blocks::{block::Block, BlockHeader, BlockHeaderAccumulatedData, ChainBlock},
    covenants::Covenant,
    proof_of_work::{PowAlgorithm, ProofOfWork},
    transactions::{
        aggregated_body::AggregateBody,
        tari_amount::MicroTari,
        transaction_components::{
            KernelFeatures,
            OutputFeatures,
            OutputFeaturesVersion,
            OutputFlags,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
            TransactionOutputVersion,
        },
    },
};

const LATEST_BLOCK_VERSION: u16 = 2;

/// Returns the genesis block for the selected network.
pub fn get_genesis_block(network: Network) -> ChainBlock {
    use Network::*;
    match network {
        MainNet => get_mainnet_genesis_block(),
        Dibbler => get_dibbler_genesis_block(),
        Igor => get_igor_genesis_block(),
        LocalNet => get_dibbler_genesis_block(),
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
        PublicKey::from_hex("3431860a4f70ddd6748d759cf66179321809e1c120a97cbdbbf2c01af5c8802f").unwrap(),
        PrivateKey::from_hex("be1b1e7cd18210bfced717d39bebc2534b31274976fb141856d9ee2bfe571900").unwrap(),
    );
    let mut body = AggregateBody::new(
        vec![],
        vec![TransactionOutput::new_current_version(
            OutputFeatures {
                flags: OutputFlags::COINBASE_OUTPUT,
                maturity: 60,
                .. Default::default()
            },
            Commitment::from_hex(
                "ec928d4b05e5c8967beec2e8444831ac6a5cf8048f936cabb22a0479d592a55f",
            )
                .unwrap(),
            BulletRangeProof::from_hex("2e33c9c5e18c40d031012483654797bc811b4244dee8abe85b6d60587c1d7a682c1eb64c0ae85ae5f94f92c0ce73e39aa4769e4ba460ea0129575d7a862f343d12c8b44693ccaa7163a011b38ba0528e5d0daf88255eef81825b679aa8a36258fe07acd1616f010055001d5ec48dd1407eb5803c45fe02c42517882382a3c1251ea6fc11108d130be96666e235fa703d986cad8a2a662c61d91072d84dd2ef02bfce583225f765eabacddfe292a7329e4d3543c3763c54a92c5cd4355be57b022fab8d14598159d89e6cc9ab55791c5349f7a4a98746fd68ee6d406446f6ca0042cce361bfc837a3db1bc786c4fd48fededbe0b56e98b00f80db0672c67a6f5fda5b97026f38e3489016da811ccd6806c26fdf4f77e879e4488b016ea812e77a28773595799a3fe056ebddd3b5a97c18be2322ec71035988593a87ef4e62b70756c6b1c717c47a9c5151a754c16ce84defad030c0dcc5c86e982b71fb5158731029b8e425936c7e317f17c52ec3d3216dd922104b13a4e86b9f81bd59243567624aa6618db5a754201c87f27d7c40a3d08a1276ac98d09939ccfafed9413647b909ff15d6d3ed99dfb3a60eb360989028f0931b9cb5594c34afb046245f5ba3e16d4052dd3ee72bb6c4c065795f97d3815a5fba11e5e393363c5c3b6ba0a6144829e3984703eed29142dc51c99925a2ecd46de116d70001da63a1709bb21652c162c0da18c76d374edaf793ef4bf882afc15c72f7ab1e1c7a975aa1cc0fb2923829eb75dc90671851235bde9f3acff95fb99d4b323980029748968b2a82205047691a0d04314264304f56ded4a38bfe2413d10bd0feefe1bf38bda3c20561d734c24a0d967da12010ec8b89ff44aced35a40b67978362c7a43b43b4ca0817b09d3de258784f985eb36467bea04211d2066979850cef4fb18f92410d8b3754b07").unwrap(),
            // For genesis block: A default script can never be spent, intentionally
            TariScript::default(),
            // Script offset never checked for coinbase, thus can use default
            Default::default(),
            // For genesis block: Metadata signature will never be checked
            Default::default(),
            Covenant::default(),
        )],
        vec![TransactionKernel::new_current_version(
            KernelFeatures::COINBASE_KERNEL,
            MicroTari(0),
            0,
            Commitment::from_hex(
                "0e0e2189e3551f32798e41c06bfa4e812ba49cf5a8200b7f634e0e48df7c3b66",
            )
                .unwrap(),
            sig,
        )],
    );
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("31 Oct 2021 06:00:00 +0200").unwrap();
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: LATEST_BLOCK_VERSION,
            height: 0,
            prev_hash: vec![0; BLOCK_HASH_LENGTH],
            timestamp: timestamp.into(),
            output_mr: from_hex("144bc103c7374d0012544ea62cf6845ce35aa41d7186466b6f214faf4e55f78c").unwrap(),
            witness_mr: from_hex("4b65b5b7e494d45f761909f580fcc095054721162768bd6d6deddd0125124b29").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("38f4419ae0798d6fc393a206c03486680440c74e1b461fbf04b9a1341f04d32e").unwrap(),
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

pub fn get_dibbler_genesis_block() -> ChainBlock {
    let mut block = get_dibbler_genesis_block_raw();

    // add faucet utxos
    let mut utxos = Vec::new();
    let file = include_str!("faucets/dibbler_faucet.json");
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

    // use this code if you need to generate new Merkle roots
    // NB: `dibbler_genesis_sanity_check` must pass

    // use croaring::Bitmap;
    // use tari_common_types::types::HashDigest;
    // use tari_mmr::{MerkleMountainRange, MutableMmr};
    //
    // let mut kernel_mmr = MerkleMountainRange::<HashDigest, _>::new(Vec::new());
    // for k in block.body.kernels() {
    //     println!("k: {}", k);
    //     kernel_mmr.push(k.hash()).unwrap();
    // }
    //
    // let mut witness_mmr = MerkleMountainRange::<HashDigest, _>::new(Vec::new());
    // let mut output_mmr = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create()).unwrap();
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

    // hardcode the Merkle roots once they've been computed above
    block.header.kernel_mr = from_hex("5b91bebd33e18798e03e9c5d831d161ee9c3d12560f50b987e1a8c3ec53146df").unwrap();
    block.header.witness_mr = from_hex("11227f6ce9ff34349d7dcab606b633f55234d5c8a73696a68c6e9ddc7cd3bc40").unwrap();
    block.header.output_mr = from_hex("8904e47f6a390417d83d531ee12bcaa9dfbb85f64ed83d4665c2ed26092b3599").unwrap();

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

fn get_dibbler_genesis_block_raw() -> Block {
    // note: use print_new_genesis_block in block_builders.rs to generate the required fields below
    let excess_sig = Signature::new(
        PublicKey::from_hex("58c2fbf8d67f16c383e7f5335eeaca3d8fa0c2b8b8027db04511c75d8f2df211").unwrap(),
        PrivateKey::from_hex("4bb9c5d4504672cb15dce9447801a928f771f2a6f0c4fb9f9d87a3a0aaa18300").unwrap(),
    );
    let coinbase = TransactionOutput::new(
            TransactionOutputVersion::V0,
            OutputFeatures {
                version:OutputFeaturesVersion::V0,
                flags:OutputFlags::COINBASE_OUTPUT,
                maturity:60,
                metadata: Vec::new(),
                unique_id: None,
                parent_public_key: None,
                asset: None,
                mint_non_fungible: None,
                sidechain_checkpoint: None,
                committee_definition: None
            },
            Commitment::from_hex("e2b9ee8fdf05f9fa8fd7598d4568b539eef694e58cdae84c779140271a96d733 ").unwrap(),
            BulletRangeProof::from_hex("0c02c6d9bdbd1c21b29ee0f83bed597ed07f71a60f99ddcbc02550059c4c08020438f8fc25c69160dc6af81f84c037fc79b9f4a7baa93ab4d6ef1640356d9b575efe1f3f8b40f6c64d1a964aab215491f79738ccac1712d756607626442dda37ac30010dc663985153786cc53c865c01bfec186a803c1edb1a34efa3088ec221016a63cdbd2b58fa4c258bfbf0cff6b793ab62f5db0fb781f046effb5f4e7c0a956c2e042e3f0c16a72f20a624625fa6dc0b742e49e0158a50c8abde54834e04bb35baef0c258da30b738256549e3a2612ff89b4f6bfe82d16aa10b38daabe0df6b922717cb4b1604ab97a2a5efa4d325beb56c5419cff185d61e1a0fc9e374098bf4a10404d788141e2c77de222d68c14b421b62f300898c25487f491aff26be85e54c011e90cc96aff6b31993ce74233674fb150de929fbc259bcc7808a84432cf28bf83c2a0fbf2b47a6244fbafa02ca4f5c9d46c5380fe8eaed734f1d56e769e59800137900cb5905191bbb463cbcb4ea0a2073d716f18878ed4455d19426a2c1133bf703510bf0f1b3b70e9e5ee6fbb70a8e710fd0a4b8f37eacfdeef3df66e461f16ffdb270a7181505b1358f56578840bbfa284444c35160794f0294300ecb3fde701a3f5ed9234e4c196b93fd70633069eeb184ab53685b5324c963a7428094f0c7d4306b5da6ef5fb68d085c32adabe004bebcbf335ee8fc92e5e034edcb035872d08f139e9445539241ff9b9fbebbc0e7b248cbd97fa7c6f3d7823085893c9ced1685d69d2a7cf111f81e086927565c301d4e33639def1139bd5245a0ae9085d5ba10cdc1f89fc7a7fa95cc3aa11784ec40ebf57475ffb4f2b2042018e3dbe905ebd5d0ebe533f36f43f709110372c94258a59e53c9b319adca30c8e9f4f92d5937f994ff36a5bb38a15682187dc8734162f45e169a97a36fb5a05").unwrap(),
            // A default script can never be spent, intentionally
            TariScript::default(),
            // The Sender offset public key is not checked for coinbase outputs
            PublicKey::default(),
            // For genesis block: Metadata signature will never be checked
            ComSignature::default(),
            // Covenant
            Covenant::default()
        );
    let kernel = TransactionKernel::new(
        TransactionKernelVersion::V0,
        KernelFeatures::COINBASE_KERNEL,
        MicroTari(0),
        0,
        Commitment::from_hex("0e834379ebeff1bde0513800c73ac412afdadbb101fee7a97dc7ac5d1296017f").unwrap(),
        excess_sig,
    );
    let mut body = AggregateBody::new(vec![], vec![coinbase], vec![kernel]);
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("25 Jan 2022 16:00:00 +0200").unwrap();
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: LATEST_BLOCK_VERSION,
            height: 0,
            prev_hash: vec![0; BLOCK_HASH_LENGTH],
            timestamp: timestamp.into(),
            output_mr: from_hex("57437e643632204e465b553299980d98036150def829d7edf1b0e0aa5c279bdf").unwrap(),
            witness_mr: from_hex("ac6964128554a4e35f62772afe1131f135c8a3b995662e6d6dd99cc2e9f0159a").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("447c98c2566e08efa70ab7e7307022c0b37a0821120ff1e308a49caa5765accb").unwrap(),
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
    use tari_common_types::types::HashDigest;
    use tari_mmr::{MerkleMountainRange, MutableMmr};

    use super::*;
    use crate::{
        consensus::ConsensusManager,
        test_helpers::blockchain::create_new_blockchain_with_network,
        transactions::CryptoFactories,
        validation::{ChainBalanceValidator, FinalHorizonStateValidation},
    };

    #[test]
    fn dibbler_genesis_sanity_check() {
        let block = get_dibbler_genesis_block();
        assert_eq!(block.block().body.outputs().len(), 4001);

        let factories = CryptoFactories::default();
        assert!(block.block().body.outputs().iter().any(|o| o.is_coinbase()));
        for o in block.block().body.outputs() {
            o.verify_range_proof(&factories.range_proof).unwrap();
        }
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
        let mut kernel_mmr = MerkleMountainRange::<HashDigest, _>::new(Vec::new());
        for k in block.block().body.kernels() {
            kernel_mmr.push(k.hash()).unwrap();
        }

        let mut witness_mmr = MerkleMountainRange::<HashDigest, _>::new(Vec::new());
        let mut output_mmr = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create()).unwrap();

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

        let db = create_new_blockchain_with_network(Network::Dibbler);

        let lock = db.db_read_access().unwrap();
        ChainBalanceValidator::new(ConsensusManager::builder(Network::Dibbler).build(), Default::default())
            .validate(&*lock, 0, &utxo_sum, &kernel_sum)
            .unwrap();
    }
}
