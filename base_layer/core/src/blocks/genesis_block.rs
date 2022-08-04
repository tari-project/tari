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
            sig,None

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
    block.header.kernel_mr = from_hex("1e9d127e43a0f708baa66b37434efd5ec9ab0ed6f59814c444524c116b633cf0").unwrap();
    block.header.witness_mr = from_hex("4d62bcba745348a1120c36cd13cb903ec2737c2e43870464523123e9a262ba70").unwrap();
    block.header.output_mr = from_hex("ff286f4e2768b6ee035be599d96c1c76e3df678daa79f8efc359e3883bfd349b").unwrap();

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
    // Note: Use print_new_genesis_block in core/tests/helpers/block_builders.rs to generate the required fields below
    let excess_sig = Signature::new(
        PublicKey::from_hex("2058a2ed3c8f477bc16a498fe9737b20d867e50dac08ee7c4ed65eca5a838c1b").unwrap(),
        PrivateKey::from_hex("2e0b4bef10a55913c75cd67b65554b78895794020a056e654f696efe19d0e80e").unwrap(),
    );
    let coinbase = TransactionOutput::new(
            TransactionOutputVersion::get_current_version(),
            OutputFeatures {
                version: OutputFeaturesVersion::get_current_version(),
                output_type: OutputType::Coinbase,
                maturity: 60,
                metadata: Vec::new(),
                unique_id: None,
                sidechain_features: None,
                parent_public_key: None,
                asset: None,
                mint_non_fungible: None,
                sidechain_checkpoint: None,
                committee_definition: None,
            },
            Commitment::from_hex("f44fc4dd2b91f99908ff06da02fd639593011509c088bff91d73fc0734f48604").unwrap(),
            BulletRangeProof::from_hex("01fa927f9cdaeb0f0c570b464aa530d6289ffcc1c3903d363d98789bfdc6d049609005709e0fa2fe11e730c0e1011826023ead9664ba829ee42b7ecff8be12ab065e9d777bf4ad7b2ba9af1210f1b6fc34efca689730410260fd522dffa8b8447d040bc6bac46745236b53ea18c5edc812192e74da203a2dc91335fea59bb4085a16a662adf71a5fd07021c933970546911a00840641736d23448a06438088894f70e209fb4f01bc81207b6df7e082c0d598b01f23d3dd911c98066c0de489b173940af3a5afe0f875889f848cf86227e3a003be8898057c4285c1bab4f6aa5c53820a39773699b634a4ee7a6101f009748f96aaa22fb4b54b421511d0df0bea64f2a008629489d69f4ad4015cd2b8ed939f989fc608c4e0064b250b46a74c6d13742adcbef132cc76fa37f58dd9cd66d3b9155c0dbaf3981133872c7115ce3f12706388b065c85977402e21a08711d02346350709fbf9a1cc7dac23156c3ae773fa7b0b6db2dd37bd141fc337082a6370242110073df7743ac952ec9d883bea15d6f766631685a345d49ee81fc5b0ed8cc6683f70b5b723a71a6907a70b84bd53c02e30691bf3cd2c6b80e86a9c53d931c7505ad7bbba8778fd7bffad39e9e7054c513e9563fc53d49bb42f3649a166e517302b3b051679499f442921b4641f58045f02fe9b87e4d033690e5c8d7ffc0f16ebcf857c22d008f706364dc5f92b0a5fca0a0ac7179953f65f02e04165291ed4cf616a035fc186fa8cb23259b6ef0214f68405a8e10ada64cbf6ac04ccc00aeb5e6b9e56c9fd899c2504a9dd6b7300").unwrap(),
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
        Commitment::from_hex("8ebec2a50f69f3b7ce31148dccb9622189b102a0a7e1c983768ccaf1232c2c7e").unwrap(),
        excess_sig,
        None,
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
            output_mr: from_hex("f33e9318ea222e7a9b8a081ff7271ebe52dafb8c96ea48c0a8f26ae3beae40d7").unwrap(),
            witness_mr: from_hex("37167af608a7545424d8948f390b36b078b952120256ccf5c76cb62787060c99").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("ad1732305f06f562a56829c81ba14e499784aa92923c54464e93225f3794bd71").unwrap(),
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

        // todo replace this back in with new esmarelda gen block
        // for kernel in block.block().body.kernels() {
        // kernel.verify_signature().unwrap();
        // }
        // we only validate the coinbase, aggregated faucet kernel signature is invalid.
        block.block().body.kernels()[0].verify_signature().unwrap();

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

        let db = create_new_blockchain_with_network(Network::Dibbler);

        let lock = db.db_read_access().unwrap();
        ChainBalanceValidator::new(ConsensusManager::builder(Network::Dibbler).build(), Default::default())
            .validate(&*lock, 0, &utxo_sum, &kernel_sum, &Commitment::default())
            .unwrap();
    }
}
