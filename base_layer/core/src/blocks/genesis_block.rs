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
    use Network::{Dibbler, Igor, LocalNet, MainNet, Ridcully, Stibbons, Weatherwax};
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
        PublicKey::from_hex("aa8ae9b04d87a8e864ff8dfb1d37dc1b7ea0e7b8ec314c0d4172e228b7b6966f").unwrap(),
        PrivateKey::from_hex("fff382bba64848ed7a15b97be2b9e3e7f0d8752b551ea794d08070f53e044309").unwrap(),
    );
    let mut body = AggregateBody::new(
        vec![],
        vec![TransactionOutput::new_current_version(
            OutputFeatures {
                output_type: OutputType::Coinbase,
                maturity: 60,
                .. Default::default()
            },
            Commitment::from_hex(
                "e29d3838b35425b4bd2dc51bc1b6761652b08302c942a14c9557991c83a0cc75",
            )
                .unwrap(),
            BulletRangeProof::from_hex("01800de4e1880d93a37ba64c3ea89d4a0ae80e2eb13246fce934afb1137b76dc11e63844aa27ecea714e8dbc2ce493b570d400021e56e06f7a1c6282bf6691c36b1ccb13c90f9ae3c1d465b15df837f7aa80dc1588a710b025bfdf7e0f644dd11980546fc6818b5d743bccfb445dfd204595acbf12b55fa48af94c2d0c440d2a0d5a504ef2f187153d9858f36c70220a4664e14c63ba5eb61b5f29f5919ee55b04382997b9a5da0a9ecf73a87bdd62df5e940cc6920203ed052e9e5a8593d895045e060ad5ccdc09589764df8548b74aac78638aaec2f5b5a6a8059a655a014051e2de4a7cec5f1cea4eed03e284d8b4016bba1c01df28366048675b69edc9a111aad822eb2480f60f8a47e69a8dbe5da81d6fce2db1f8693a19ef7bd3874aef6c66808cf659248d2c1318ea10a3879d90c1e72e35f4ab2fa54a96cace0750cc0c243db3b9ea0d4689710e9b5333f8a2afe1498d858563d854ded157ac8edc387b34c17264fa732073f0cc84f3059b21ce56a36e365e9c8870f8cd04ab6954691784548aa91291866e22f63e075b7d6cd3e716b2a00ca54b8a97fe2d7849c11150b452eaf1e0e6c0d2dc47784cb40f9b3760fef866de67ea0fe6806bb1f9157f7286572aad2df06898c60812b296f7fb32780869f05df1317baf5061688b2d6a0616ace1b57ba77fd77550909beaa1aed0be964fc074d7fea017de081258c1da0febeca59e34cdf2f88c48052d8cfe8d504eb0bea91d95ad229cb37a06b1e5e20339aa4d5a6e2899a99e0ec40f83152839da49db9ade865ca9de305bb7f20d4d0c").unwrap(),
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
                "9474ba70976e2fa06f970bb83f7d0a4d4b45e6e29f834847b659d32102f90b51",
            )
                .unwrap(),
            sig,
        )],
    );
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("24 Jun 2022 10:00:00 +0200").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 3,
            height: 0,
            prev_hash: vec![0; BLOCK_HASH_LENGTH],
            timestamp: timestamp.into(),
            output_mr: from_hex("3cf274cea5e77c5b259f0bf0e63b2d4dcc1eaa0bb96c1497524f71ff84430815").unwrap(),
            witness_mr: from_hex("0897676242cdb559647e12a2b416c518d4a5b737d66d72eb31cf24cc025700ad").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("d6db311096294e468177f294c4398275e843278274ba97a4e7d01f1a90cab86d").unwrap(),
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

    // Add faucet utxos
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

    // Use this code if you need to generate new Merkle roots
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

    // Hardcode the Merkle roots once they've been computed above
    block.header.kernel_mr = from_hex("8bec1140bfac718ab3acd8a5e19c1bb28e0e4a57663c2fc7e8c7155cc355aac3").unwrap();
    block.header.witness_mr = from_hex("1df4a4200338686763c784187f7077148986e088586cf4839147a3f56adc4af6").unwrap();
    block.header.output_mr = from_hex("f9616ca84e798022f638546e6ce372d1344eee56e5cf47ba7e2bf58b5e28bf45").unwrap();

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
    // Note: Use print_new_genesis_block in block_builders.rs to generate the required fields below
    let excess_sig = Signature::new(
        PublicKey::from_hex("024008ec92ab04b039fcdef2d20e4a7a72f5088797cc16855d30b91d5cfbcb16").unwrap(),
        PrivateKey::from_hex("5d37ce54fe8beeff5330cfca82997878f1263d76331114a9030a383bcdc9e901").unwrap(),
    );
    let coinbase = TransactionOutput::new(
            TransactionOutputVersion::get_current_version(),
            OutputFeatures {
                version: OutputFeaturesVersion::get_current_version(),
                output_type: OutputType::Coinbase,
                maturity: 60,
                recovery_byte: 0,
                metadata: Vec::new(),
                unique_id: None,
                sidechain_features: None,
                parent_public_key: None,
                asset: None,
                mint_non_fungible: None,
                sidechain_checkpoint: None,
                committee_definition: None,
            },
            Commitment::from_hex("b699aba9a294d2e654bcd076cc2a6f8fb4ea5de880615a7e536267199da71c0f").unwrap(),
            BulletRangeProof::from_hex("01ee67e6b49742e37a1db8649728f96d59e8f0568a28fbf6f98768db0084681a2776f0f43b1593b4aa18dc0d6d5648e25c44a20224ca5df8196928472720140a4356f7d057b873997de9b163f3377f8c96b061ccdeb0df3c7a2375ceeb8984af3104ff189b0f6f2ce1d774e0a2b48beb8f83d5484650084812cb0d47d3c0bc297790e40d9a5e8e03cc53cf9198ea7408984f663c7f24d9407b0603d7088d3dcd37d295b8350602cbd25e591d7a2db4693357f01af104079d2741dcb5c60424f7255c814e9d1f808ae6f40983c006a94012827c6c485f9fa1fe5fa0e3db8af34c4502ce691fbb08b06adb7c9ebfea63c968fb5995accbbcf3cbaef364c56cf1551646dc4cd6f2f062614daa7b957f1e163c0306e00d2fb055381c8182a63dd2d65cda0bd869da7c6a8b1b564a9376b1d46d40624ebb728443d34c4a0722fbcc152d4a5a1a19e35795b5283985958fd324525fe2f3ad7d8b799cd6ecb4811a8da42290e13d02454adfdcd9f0cc1b65fd8d1e10f6327e57537ebbc5515b0fc176c447ecbd1aebce5cd14a591572a73a3e4a2d964a96458dfa97da7efc0e8297e7f62400d264d2367621433dad037da65b48c568a920ed34645fb9efb6f0c47c27235e8c6750e139177bc2c911ceb40e5fa85359aa6389e404c6ba2e01fcd40cdee41b72f351a29627333cbec5d5842bcb092a6c23180dac2eb04420c09deaae05830b75836ab9c8a9a0991ed29b17eded69e024c9a7e4850d7e1ea275f7c7530a12080467f938e7fc72a59be21603d1a271d11f60f6370a850ce553a97284e4a0740f").unwrap(),
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
        Commitment::from_hex("0cff7e89fa0468aa68f777cf600ae6f9e46fdc6e4e33540077e7303e8929295c").unwrap(),
        excess_sig,
    );
    let mut body = AggregateBody::new(vec![], vec![coinbase], vec![kernel]);
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("24 Jun 2022 10:00:00 +0200").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 2,
            height: 0,
            prev_hash: vec![0; BLOCK_HASH_LENGTH],
            timestamp: timestamp.into(),
            output_mr: from_hex("cfe91b83e0d8b5190671e9db7cf3129cb163b2812b862776bcd7f42aee58eecf").unwrap(),
            witness_mr: from_hex("71a1fdcf3da037f786e3874b0f49a7720b35b978cbc78d284f20d140317f89bb").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("55bb9a3369ede6c4e04bab54dd4f2345531e559fc6d72d9f62adad1d49898c15").unwrap(),
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
        transactions::{transaction_components::transaction_output::batch_verify_range_proofs, CryptoFactories},
        validation::{ChainBalanceValidator, FinalHorizonStateValidation},
    };

    #[test]
    fn dibbler_genesis_sanity_check() {
        // Note: Generate new data for `pub fn get_dibbler_genesis_block()` and `fn get_dibbler_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_dibbler_genesis_block();
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
