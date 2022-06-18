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
        PublicKey::from_hex("241ccf966ae42c350fca8848cf31f999054125b88b057a25742f91ceb9aac777").unwrap(),
        PrivateKey::from_hex("e7c7308ab867afd781ee7051be2c7fd7abd5f580935fad32219bfffb8b56640e").unwrap(),
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
                "d097d496c74f8542d02fa0fe1a7bf8861548f895fae1dbf7c95a746f06b1f004",
            )
                .unwrap(),
            BulletRangeProof::from_hex("013a682cfb99bc6d72899d79dd94b7f6738d9120417e7779517259668ab9e09c2198e213732818f52fe5eb5f0100b6446ac02e6527189d8309c13e2441c9749f0caad4621f700d5f23d6f31980e1269d3dd9dadcff4a27b6b77e78e1308554f62f602aba7e370d2ffa3c46a0b4f46c44501c1ac7fc44f3c8d9de2ff50268366740dcccea7aacc754cd7dfde918703ce512b899e68a323c064ba31a0008d16ea83ea669c4507405bcc8c29195c0f479d2b35afe15a05a72ab84b9180873b25f2908f4e286f6860bb17245634587d563164fee262d037568564bf774388e72fe6c4ad83a6fd4c1abc4d195090c14c9c6aade00e696aab5115d8e55848936dfe50e6ec4e87cd4366eae7191976f0a2f30576538a91412466dc97192e92f693a77ba272ca7d6794487c681642529fe3123cb52adaa7872ed279e0b52d4c93d07ffd00e5cdc36b5ca5fb70f0f69105aee5151833ab85bbc9dc86c609cdeb17921fc8e3f744fe41f1adb00f1395c8665b840fc2e7349bf74e03f429567c48c09e9be8f1462d4e0daa5c56ab51b10503b7f295c94c4c5a83c3fd2928c519dced92acac12eecef5fb95981f1cdccef7f3736c5b01ea87e2928ae5530eb5dccb762f319f9466638ecca082374cd9039fb4cbc3e297f424a91898c4daed2cf2c84942526854bdee2cf022cbec5750f0527d4bd117bb0972e5e8f4c5371b33a185012eb67590dec6d5eadc8b952970b8f7340c14b885753fc4edf466108e7ce2a42760bb5e304dd36c60e270fa26827f578ffc90e0538149461d9ed03240c66d69e9ca1e19d0c").unwrap(),
            // For genesis block: A default script can never be spent, intentionally
            TariScript::default(),
            // Script offset never checked for coinbase, thus can use default
            Default::default(),
            // For genesis block: Metadata signature will never be checked
            Default::default(),
            Covenant::default(),
            EncryptedValue::default(),
        )],
        vec![TransactionKernel::new_current_version(
            KernelFeatures::COINBASE_KERNEL,
            MicroTari(0),
            0,
            Commitment::from_hex(
                "0cdfb637574c7b163095077e099a6599a63c9b8201781d2cfee170c08d8ad605",
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
            output_mr: from_hex("d321d92a6f321444eb0e4805bbd341b4f7dd321797ac051b18f396c769c93ad7").unwrap(),
            witness_mr: from_hex("1d1dac0c3b712dc3fc824e63969731258c5f5f62ea6c23dca0581681bc56ddf3").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("b3231893fa5c497c9bd53999b740db78f67e0b8f3d0278e98350ae963d8bfdbc").unwrap(),
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
    block.header.kernel_mr = from_hex("c2c248c913cc352c8e0bc3e556e749d6dc9c141c3be21c907de07d9666c9bce1").unwrap();
    block.header.witness_mr = from_hex("8c76b516f156eea99c9ef8b999428b5a0b5955336dcd69626d06d4b677e61fde").unwrap();
    block.header.output_mr = from_hex("f59571be572410f108d5415a1e14e7ef02fad872cffe8d52b0b074dc32761f17").unwrap();

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
        PublicKey::from_hex("48b45d1a3c9d8daec78d14b228d3d718c4066a774df5f114f8e6a46d9a4c0232").unwrap(),
        PrivateKey::from_hex("dcfc85add0315d11ea21e3d65cb848f43ffc176cc207e6ff25d55ecde5dc7908").unwrap(),
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
            Commitment::from_hex("82123f9b9118c29a407d36d3896f188bef34545ffcdcb0d927d39cafc04fcb69").unwrap(),
            BulletRangeProof::from_hex("01e2db56bf110a57743363419f0cac81f8fe321d3313cca346af48b0ad8be8035a48be82a4f3a4c5bf60771cec692b39cbc8d8283b3598758ad8d6a946dc08482d98d185702776b53fc22a915fc520376c58fbea716035b340a39d173c5621b4491e3d646968947ed1971e139605c041c8fefe6dca7a92a9d16d66bc7774ed045c1cc264b2338a2c803523f0c39b2e85e9fe79de2de934d77219a54b93f2aa2571e4c29332b89c64f105154f87c90a293a176573ee3577d152821178099e241c25c061f0ef48fd719f7e8c5b0d935d6fd38570bb284c7adeb2637d0b9fc3690766fc1863d8128a58f913c146a471ce3ee80e1c46800309062671de046070fd642c12949afde0678aacc80ebde85075f92b0910a1c2ea43cb56fe4bef4827cfe01b725edb09aa07d25742afb107dfa4655d20f7a1a75bf3b8e936f7c2cb86a6b90bca4643a7d4da44ef1f7cf8cc4e70c3825162db4a585567fe86c171f77e4ad87e66d04e70553a39470717149a9f9b5915f1e9481cd1bb62a6666653cf0bfaf12f0ea61f616a37f26a56ced1279e3493f79be9ed59129a223c8f9af3ed0b90587fdcae2975b1740426f927be31af91efc60b84bbf2c26633045f75521e194524716085e1ecd85cce1cb903f2072de422371fea666997ddef80878e0da0ad4cf00c768dc8b8749cdf0682eba1ff91577076bce5ad5acc7182ab59185e37961b5f05ba67589a853817188dd61d169eab692e6ef70a03b9b0ed1f8a630733ba626c08dc5496462b338ab9a2d52ca17e55bc671c6558d4c17f1a14e4daa8ce62cac907").unwrap(),
            // A default script can never be spent, intentionally
            TariScript::default(),
            // The Sender offset public key is not checked for coinbase outputs
            PublicKey::default(),
            // For genesis block: Metadata signature will never be checked
            ComSignature::default(),
            // Covenant
            Covenant::default(),
            EncryptedValue::default(),
        );
    let kernel = TransactionKernel::new(
        TransactionKernelVersion::V0,
        KernelFeatures::COINBASE_KERNEL,
        MicroTari(0),
        0,
        Commitment::from_hex("da7779147cb54b086651fad2916e8d8fbf7a9c72d036b94c2b5d23900b990f44").unwrap(),
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
            output_mr: from_hex("e0724cd00725c8150ad7d1a20fbc2e17176b1a9f568f97a063569e6785bfb8f4").unwrap(),
            witness_mr: from_hex("8b959e4ae1e5d817cd4926579c53713089aa679bec49881aeb9a543a71ff5b2d").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("ea5435af163bbec819c2558aa36616e8bf54c3c66bdbbe96708e44324c134d79").unwrap(),
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
        // Note: Generate new data for `pub fn get_dibbler_genesis_block()` and `fn get_dibbler_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
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
