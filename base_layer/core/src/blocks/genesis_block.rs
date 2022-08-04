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
    block.header.kernel_mr = from_hex("af55d39195d0f2bc16558e3e79e91fe65f52519189a14e842a39ac6bcb7170ae").unwrap();
    block.header.witness_mr = from_hex("a2f1e88886a3e8ecf8966625588d846bd236b85ac6b361acb7aed70b7287e99b").unwrap();
    block.header.output_mr = from_hex("c9e4382a60e6f190eb21aeb815d7449be27fe24b27867db798635c49ed134a5c").unwrap();

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
        PublicKey::from_hex("0646f943fcfab97b981d259e1da31f170b9119234d35e235d88bf9d4f53fbd61").unwrap(),
        PrivateKey::from_hex("aceea89fe16c6bcb2c188dd6ec519d89a035544419ec465feb129b1f67749c0d").unwrap(),
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
            Commitment::from_hex("3682a4cfc556c0b5102065bfbb21fcd3b62b26ebad28d90f9d6720e1cea31d2c").unwrap(),
            BulletRangeProof::from_hex("01d202a095c27dc9e19ffd8456ac85dc45c9ff7505d84a37af6c8a3b572b97531f98e40484332d968e000451c3e8b14e7c9704a15905564e49e10ac909df52dd2d8467a19c9f51f74ff16c98dfd97e5f22146a7d8a4eef280050c9729a0d2b1b0cce1cfe8050440b01362bd486485f7155f04ff1e885e5b5e594dbe91add2564015c0ba23e9faea20df2396d1cdd7a1c784f40945b0205a69e814520c7202a335e76516965be5a78d126b510b8b73da2adb82b350c2a32d86780b74a00da873d2748991cb0a13206620f5a12aa849e0f3ab030ed6e769d9ba725cacd464955e54f360ddddf79a86da74ace814b5c4cccd3c76b985733d91803024f38a62ab43244f2ae4ea7631a7779c879d27815094e200fb0b36769b855d0934cd061a0ca05162aecccc847b80c4d305e54f855d4a7bec5d4f8f3618fcabef44e9aecf2a3b37bc0ead352597ee7a38cc401c4471c53e1889e1affe6f9ae964cce719604296e0310f61f241b42260720bec94bb6e514dd9a94cdc2e8d8dd4377e9c805d324b4265413aa79caf926a27b7182ca8222a9e80024a878eee84b34c4c2422f3aabb44072c8f1a7a1fad46fb4d1c474c4d9dabfebfc73dd0c3c51b5942d6d78223faa0dcae2007c9eeee04d7cca47ee230980e8a32637f39ce3d4526f3e49a7907ef63b9cf3fac169e5db0ad9b1c1b898814aea6568457922271e1428e9bfa273a94006d77f15bf981dcb2a0c70bdc63f86241159b97d463f7fd0d3ef581c727fe1210bc3d0509596dbede6d84f6e199498c97bc3e497553bac19673c5055384e3c3f02").unwrap(),
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
        Commitment::from_hex("b050c0aa325f70666b83f1636423f724f3886bbaff11179a76be0df47829bf73").unwrap(),
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
            output_mr: from_hex("bb866666548a998c82d14746b730b929f2ee0074d8d1652261dd6e751f9e821c").unwrap(),
            witness_mr: from_hex("1a0f889a52e089e909bd2a39a9ac185b0645d0e0125e4a38eec76314ca455ad6").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("7be2dfbaf3a4892bed506ed606edc6dd4f09eba0f75d1260c82864e50c2d888c").unwrap(),
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
