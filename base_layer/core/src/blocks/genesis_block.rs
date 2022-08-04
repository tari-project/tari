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
    use Network::{Esmeralda,Dibbler, Igor, LocalNet, MainNet, Ridcully, Stibbons, Weatherwax};
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
        PublicKey::from_hex("4488793b42f196272fa83f0b3d44ce4726d80318972ab1136b409ab7c744ae37,").unwrap(),
        PrivateKey::from_hex("c540beec61c57af5812398a23bf25478296437868dadae5d3254ef711c04b30f").unwrap(),
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
                "4007211a1c6cc2f9992ce23769c02a4e9f37170765527935dd3f331e6ca04d73",
            )
                .unwrap(),
            BulletRangeProof::from_hex("01da5f3db36c55ab6b99ebe3b204c5dfa78d0fa6eff4f7004b0d38109b7f447846aaa196d260f39b7b74e459ce1cc2adc5756329dd95ecabe68a14faa58312de38a4cd867f683ffb5e1ed8f2135c627f977c62249aba80b5f6a1e2e88aba7dbe7226f6fbe21ac82732ad5ef136224bce5910406a69dd8425e4508ddd0deff45b1a1aa4d298765b9bd603a29b60409f2f607cb6e910c8f5050ada662a281361435682f12ed51403872be454367a0dc0bb55a84cdf7328aee944a5ee22f831fedd00c0a7539faf15866770a1f6a5fbda5fe7030626508fa9e450f061191411290c3dbc6f4ee66e9fffde0520d309738f3251aaf3c46765df25c00cf5e4c35a50ed2e9483ec9d989c4043ede9405ed148258ddfbd157e6802234c112b35613b843a2570c08e71267df17c5e7fea0658d43a4b6971bc7b9229b433bac2cff0db1ca51ec23b6fe3423875cb5384116c90924125300130269ef644368aea5fc27715686ef6ece57f0089124c9711c73ad66bf6ffd82bd163297275ae52503ff772a5a523b0d09653b7f6df144ffabed39e8c7c56ff21b2296d480737ec8a688462dc54214a8aa8b9c0c3b3db82bd084ed95dd5609dcdd8a44d196928395cc1b1a9bac749eaa55a2c6bb8ac6cdb97dd0d9602fde08eac62c77bd61d27ee02c92f2cf6e373bb6257cdd436eca805c72c39a2e57d10316832b1a27a15c101d7fab0e872d10de295e55507309d60c653a3114acf8845c2568cb165d97bc6526308eae4551a0f02009c5bb2ec89d268007f148155c897ebbc4d3a5fa929b51948bb2d263c0001").unwrap(),
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
                "58f6c7b149e49eac2a51498d0027a98c7f8115c3d808ca03717b0837303d614a",
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
            output_mr: from_hex("8c50b1b393d50f72140746cfef314612bf2d832cbb8a4af39df7ff70023f2632").unwrap(),
            witness_mr: from_hex("35950652ecf2fa8d600fa99becd7ceae9474f2f351e2c94fd7989c9bbc81c9ff").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("9196491fe5659ced84b894ed1ee859400a051b9321b9d3ba54dba499fb7397d7").unwrap(),
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
    block.header.kernel_mr = from_hex("300a7ef7bdbfa27058b11302ce9700a08eff7c7d91e94f75c21f3b4482905b96").unwrap();
    block.header.witness_mr = from_hex("4ecd3bb4a3f6bd620d037b255a32a47258c62f12a8e67f2ffefd9e9bb84a1118").unwrap();
    block.header.output_mr = from_hex("66ac81147988d4856de58e16538c48138f2acab3a060efb749de74e8d779bcf4").unwrap();

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
        PublicKey::from_hex("9669e4f6b0ce11025c0e324334df0740da6bb4e65822ec0fd8a54553101f246a").unwrap(),
        PrivateKey::from_hex("81d95524c319eb50bbafed95f5147ab772fbdfc023434361b5310f75723e3100").unwrap(),
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
        Commitment::from_hex("6a7c60dd96da6c4c4e7f9198a7dee85351d40896d5cd6cd715dece6078cbf058").unwrap(),
        BulletRangeProof::from_hex("017ceea5bd4bf50040be924148b2298d4d460ba2af8ab0f4c12fbbc2a59147cd7830d3e45a5efb3b22dbcb42592176720836a1dbc3142dd37289dd6ff3c72eb768c6de833d80acd763c639eac60214f9d68f222d07c6a1ba583b34eae005b6512928a16af409992efeb98b1e86b43212c91de8933b3742afd33118587f3e5f6f4cbeee3dd6fb2ecd6086cc6aedb9414fd261de9bce728ee6f746b6e37b99a5f85428c298b5c741879a77758fccc1149c39a526cba1a2f2f4ec9a443c6159d3687a323dd8ac4493c67ed307318ca08ae2b805f399e43d13e25ccd31a3a67cf724594828b99a759efab02194118801056d0b898b234bfda3364e69444360ccc61a0c8e864bb3d077213ee0264217fc21971dfe3861eec053daa51b2ccce0ad6bbc3e22271d13cc9371b8f5da16b63e5ac91ea335cbe4adf9cef355fe44746d7d5c47c63dab7453fb6b752368e7653fd7f0e8938250e018a541da29e0cab39452950bc6ce6a3cfa12af48e2f607c94fa1cf980d3a89af39e26db10403f9d61c357a12dc27b9a9970d02dc8e7afda8972ddbaf9778a142b6551489510efc5b73b6a674e4b1cd35a522fccdee96e3bfff02b95ba2f9f9206fabff1bcb8eef975d61d6137008d063d3f0ef7b1479777a9188e3dd3a8f8397ecd9d379e8947b39c99f7b12afbe93dc4929b9e8f11151ff7e1f29162642d0bcec3add8a1d11012aea75e3092a518154383fe36eea80e57bf450f81e02bd14d019db6d84986d07d57fd2330b7a60886c36068bf3b3cf90d082904277f7be83a7e59b0d89b2bc792223fef208").unwrap(),
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
        Commitment::from_hex("4618a4b84050124d07af8d8afd0b83fc46e17b2781421c7f9ed0acb6ceaa9943").unwrap(),
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
            version: 3,
            height: 0,
            prev_hash: vec![0; BLOCK_HASH_LENGTH],
            timestamp: timestamp.into(),
            output_mr: from_hex("9295f004c2e42bd31e1704225bd7f13c4dbd734690540fc79effa18ac515afd2").unwrap(),
            witness_mr: from_hex("6e9d24328e76d2492e48654fd010eed295086bfff117a097a4ebc70e6a11bf8a").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("299fe7e82670a3e2d15ca4a316c5c4d23abcc0a9fe2fb3bd675af335304051a5").unwrap(),
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
        ChainBalanceValidator::new(ConsensusManager::builder(Network::Esmeralda).build(), Default::default())
            .validate(&*lock, 0, &utxo_sum, &kernel_sum, &Commitment::default())
            .unwrap();
    }
}
