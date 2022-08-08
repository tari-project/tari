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
        PublicKey::from_hex("12f32084748379badbd3dd52426b329b6363fb7423b111bf7f7ae2295c0a5803").unwrap(),
        PrivateKey::from_hex("ebd416ca6022826c4765339aef656e6d0b31d07d63364fd9a4acddc7ebf3a700").unwrap(),
    );
    let coinbase_meta_sig = CommitmentSignature::new(
        Commitment::from_hex("464de7d3e6ac0e67c2cb3959a886ef62b676bd95713daa4fa8b1f1b605700a5e").unwrap(),
        PrivateKey::from_hex("ec90eb6a67481ada4f123b1bb7952608d7e796ba4914fb8f372aa39dac5ad20a").unwrap(),
        PrivateKey::from_hex("5b0493bec6d74c43fa82bc36afa94f9d47edead04b1112a5ac63cfd98eaeb501").unwrap(),
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
                "d0bfb79757abaf8cf3ba1824af5f9e8acaadb69192a6e6c5dd2ec47f9b3f2809",
            )
                .unwrap(),
            BulletRangeProof::from_hex("0144941a0046c02bc3c874379f3fef45965679e0e204628484a6348138106dbc5d1e17287d9a5c00b821c77658aafb34ed1a6774a56740850e68c9301fba290a555c14d6614d29273b4d0826647fb587c2441d96e852ce3b1ee2807095e4e18922caf103730a0307dee152b322e656881e03640dc6b938a071b34a5813c2d7400874f0abcfa572fe13bf45608161b3dfb058eaef9c2d833b407f3ed4c5da9f37562a68e798f3e927e4d7d9ab470dd56dc5ddb1857d275898f0e3687a832a51e33892e9c5afb3730dd923b09cb8801d0b10761c7a3dffe70f44857d6665ff6b127814fc438484e3a19189c26e9d2daf8dbec132086c36d282e1e62f515d85af130a9e347d897c25fd49996468e014e9c3b80fe9592e229ad0bd71938b4e1398683c26a232f027418b0f1edffbc17f3ec9caf7862a1648625056c9dd510a4b8fc632862e74cc8d4378a1c0cc9aef6013bedd73db79c469a3ef9a45aa724f123bd04abe50f76f0d6b65676998d0759338838cd00e36c8961da94b790cbb55801b8e395a5c8f20a1528c0e57ef8d1004037f309713a819b79fa819fab0cdcdac924b474a3bf7c8925cbcc519b286dedd3e10825d8b7580d0ee5c5acc0f3a4a0629fe21a246cfe07a3719cd4db2c3fd8bab50bc19934b7a935664741a5a3be190111546a9706c99114cdca067bfa24d831fe43b7910e6cf8fdf1395949386a603e27401719b8315b68aec3a2f889a036b62595b02f5094780afcfa3643dd755a6b64e0069b4fe5b96108e7c48c57836717bae0dec6f63c0585f10aec90a3d97551c6702").unwrap(),
            // For genesis block: A default script can never be spent, intentionally
            script!(Nop),
            // Script offset never checked for coinbase, thus can use default
            PublicKey::from_hex("c41fd076155e15035e26109c3d5a674d640ca778da9f88c05205c6db76b0a35e").unwrap(),
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
                "705a54b17c56e7a48d97b1fe1cc1fb7c0cf0da2e2cf24059e11989c533520f49",
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
            output_mr: from_hex("25dc765bab606150c6fe006d973535dfa895893df37d24d1b88f903c58a939ce").unwrap(),
            witness_mr: from_hex("3d7b50fa362a1dd010b5f7f58389adf12d11255ac453b18fb8b91cf4a78c565f").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("17fff18a59f38188fe7a8e2c2f69c14c13838e2166eddc01f741b95999c4d7b5").unwrap(),
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
    block.header.kernel_mr = from_hex("06beaa77e5ccc73ed92aa7945988d0cf72a351f580f6501732c982a12b7b14ac").unwrap();
    block.header.witness_mr = from_hex("aea6634a08c7637135a9ff099532de9cdd2ae6a7f9969e59047c6f18cf79d945").unwrap();
    block.header.output_mr = from_hex("b44dfaf83390bfd52c9dfad18855d4eca294ed2c36213a2e11ab66448d1e10ba").unwrap();

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
        PublicKey::from_hex("fc33235905e81e76708c7420a1a885be6411c76132a07462a9f0ca8dd714d108").unwrap(),
        PrivateKey::from_hex("dc503f7f28d56f9f4e71196c1c977c47581aebf17a5b3aa3c96edeee17df360d").unwrap(),
    );
    let coinbase_meta_sig = CommitmentSignature::new(
        Commitment::from_hex("70e5d9c282bb5aa9c4984a5dc23fd7aa5f5eca7b9d43bae84a484a4ab6495046").unwrap(),
        PrivateKey::from_hex("da5942187deaa76738d393a27c75d9c28c747899b21720d261f671f459801307").unwrap(),
        PrivateKey::from_hex("66d427c1e1266f969a3fb0bb001dad021684c39e4b65a4ceda2d2e1382a53300").unwrap(),
    );
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures {
            version: OutputFeaturesVersion::get_current_version(),
            output_type: OutputType::Coinbase,
            maturity: 3,
            metadata: Vec::new(),
            unique_id: None,
            sidechain_features: None,
            parent_public_key: None,
            asset: None,
            mint_non_fungible: None,
            sidechain_checkpoint: None,
            committee_definition: None,
        },
        Commitment::from_hex("2ee8be354f52193678ed5edc86cb2ec2382598f68bad1038d858a4823d83f120").unwrap(),
        BulletRangeProof::from_hex("01743b0b7bb056f75921533681cd9f7292e618fc02cf156e3732cd6f58863fe829480a2c0c36f222eb59dbdc06d8d62c231163cf80a30d9fba9d230bf96e8d5946e05fe995852ec0034c2dfae2c9ccd21a10c24a7268ed8a24fa440aac5abc4e0c1a0947c658456a7ac432ffd44cbe202a8f6c93dabb7a3754caeefd8e6e03ad6d3c58c1d86d36d9d33aa8ddfcbdbe6bb50472e421b43861f3e0520b6c0cec9a3e2410f74b5628d20516a61fb44544d409768febb37499eb5f8c307daa87187c569ca61a6440c36163f8d81dc0ac47564215c6547501c1d3b2114784abf1a4137ce64bd611d2df58838e0dc6f53de037b4d01b3b8f6cb316574af870767b8a9867c49f5080ed291380c268c47dae34705e25dbe52070b7f3a61c242010d967465cc058f7b9f938040dfd485639a7378a3fa6e8be1d86e1bed81dab46e58c82d5040057fd622b1e552248a184f12e9620c85607a3236e738b50916aa13427268f5202af4973f98e55621a936981baee4cb8bcb0b885c4694d643d95ec0545f46d6858c00937475382bf0a49b2471caad9a7424b5775599218ea51db3b56bebd9f7d882873eadf26d93533d810547fb23f5468f152f80aa8eb92e649d1e7818f726de8e2e334b12f14d0537c015c3df6fc0cd96429c11a83e5685fc754855a6d420f7e7b5b5893d906ab29bc96ca70d639231d91cf5df3d60623c693337adecaa905b2be74aee2eeeb4933116a4c1f1f62f43063858c82aa1e59d72ade21767df10a607a72835b640adc3de7d178bc302596cef8d6c80dfc18a55809c6f02399a20f").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("4e173863c5845030a4e210d586043d5bda166d2b83bb221547a5f83a0ea29637").unwrap(),
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
        Commitment::from_hex("ac127a73fe2b93d5daf250bcebbfc04bae24118bf024868d7802ff36bfc74d6a").unwrap(),
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
            output_mr: from_hex("5797a5c7db67010b9e3e515c62e7f812397013705f90e65d71e16c25b04f3746").unwrap(),
            witness_mr: from_hex("b124bc9cea40c57fd97e2ab85c405c006c546c758d75dee5d8fd7630655ef330").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("8fa8517a3ee0f1cfc82e460a3a9ff441ed2c194f4a3c990ae2488ce6af783ebe").unwrap(),
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
