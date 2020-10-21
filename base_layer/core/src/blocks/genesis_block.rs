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

// This file is used to store the genesis block
use crate::{
    blocks::{block::Block, BlockHeader},
    proof_of_work::{PowAlgorithm, ProofOfWork},
};

use crate::transactions::{
    aggregated_body::AggregateBody,
    bullet_rangeproofs::BulletRangeProof,
    tari_amount::MicroTari,
    transaction::{KernelFeatures, OutputFeatures, OutputFlags, TransactionKernel, TransactionOutput},
    types::{Commitment, PrivateKey, PublicKey, Signature},
};
use tari_crypto::tari_utilities::{hash::Hashable, hex::*};

pub fn get_mainnet_genesis_block() -> Block {
    unimplemented!()
}

pub fn get_mainnet_block_hash() -> Vec<u8> {
    get_mainnet_genesis_block().hash()
}

pub fn get_mainnet_gen_header() -> BlockHeader {
    get_mainnet_genesis_block().header
}

/// This will get the rincewind gen block
pub fn get_rincewind_genesis_block() -> Block {
    // lets get the block
    let mut block = get_rincewind_genesis_block_raw();
    // Lets load in the rincewind faucet tx's
    let mut utxos = Vec::new();
    let file = include_str!("faucets/alphanet_faucet.json");
    for line in file.lines() {
        let utxo: TransactionOutput = serde_json::from_str(line).unwrap();
        utxos.push(utxo);
    }
    // fix headers to new mmr roots after adding utxos
    block.header.output_mr = from_hex("f9bfcc0bfae8f90991ea7cc9a625a411dd757cce088cbf740848570daa43daff").unwrap();
    block.header.range_proof_mr = from_hex("fbadbae2bf8c7289d77af52edc80490cb476a917abd0afeab8821913791b678f").unwrap();
    block.header.kernel_mr = from_hex("a40db2278709c3fb0e03044ca0f5090ffca616b708850d1437af4d584e17b97a").unwrap();
    block.body.add_outputs(&mut utxos);
    block
}

pub fn get_rincewind_genesis_block_raw() -> Block {
    let sig = Signature::new(
        PublicKey::from_hex("82f5e603783cfe8b7d50ec1fefb7841398bffcadcb6102dae1f83b533f0aec41").unwrap(),
        PrivateKey::from_hex("05af349cb5618e636021ca66a3fd21067b6f9b159b75b7783985a534726fe509").unwrap(),
    );
    let mut body = AggregateBody::new(
        vec![],
        vec![TransactionOutput {
            features: OutputFeatures {
                flags: OutputFlags::COINBASE_OUTPUT,
                maturity: 60,
            },
            commitment: Commitment::from_hex(
                "feba9eeee21bb01aea86cfa52ea3c905647e3785040581dd9c1f6c89510e6548",
            )
                .unwrap(),
            proof: BulletRangeProof::from_hex("9aadf23887b4bf69f2743c773aabfa0c70a270971d2fc9ad123340a1b6a1d015783012da766307c628a84bc2453f28f0157d64943e95a59ea3e4d892ef66bd70d85026432df39294a3000ed243c63e3e041c234498149962b447e3c8d234631bf036cd4f0649347957795b28a683e3b6190f1a42b51e5debbef7d2cee4941416ad15d8080ab498d7cfdee64d62a8a3701986aecede76894602689862d4465c0160fd23d97cc4379e857725905d757154f45b01803d8562bf03c25217e02ba600628c90e81f43cf94dd10b4ebd9acd3ee388ec99e659a06162af7ca3f34c84d0ef23dccbabc307af93fadad74c7c9df9e896f1c61b4340b55cf9a270218420c40ce8fec166f51e187fb9e53083e514047ff9970627b3d29dadcd8c4555e61ec2e8c09b99a9de4ac15c8583522a647960a1dd476992787f69cc9a2bf3a302fb0426ad6a121be5beb98a621e9171718901a1e6ff81e73dde7d5bbe32251ddf363423262293b81d2cb40354c41ed317b3c06f2fbddfdc1cf07547a854a50416b8d5f4adc39f021a189a5f032d1d445a19bf57c921e34d3f7d6ff8227490d50391765ccf24b84649c88f5a2aece1936c638c0b44eb21a2d9e1c159b96061d674600139e9e09d7e07ed7cc0f2c172da4104568e58ae3ef47b0ab5e7aafac7fdb05ef3a229812c4013d8fb19334f0b3488ce9bf0fc280c565ebde6197c9060740ae5d1a808de889dbb123e26fcf1dd99501f99ba6b6057aa0b0ffd07cfdf0690a2a9c79ecff61da6318c81e9066d3fb5fbc3b102b3e1d586a8933c653887c37f24c29257a7d123fa43f962f073c62e6cb0b318743b9bf9fc9043c0f56a9164eb0666174bf76e5e4b4264a35ab25edeae69af5388d7bec4690ce67304812a44df7af9909033a8234c2a777cf66b48c326de09fb2df8b23477a05d33cb59c0c4aa43ca60c").unwrap(),
        }],
        vec![TransactionKernel {
            features: KernelFeatures::COINBASE_KERNEL,
            fee: MicroTari(0),
            lock_height: 0,
            meta_info: None,
            linked_kernel: None,
            excess: Commitment::from_hex(
                "d811169b90cf749d056416121ba34bf8b435e1c1549c446433c233289fc1372c",
            )
                .unwrap(),
            excess_sig: sig,
        }],
    );
    body.sort();
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            timestamp: 1_587_837_600.into(), // Saturday, 25 April 2020 18:00:00 GMT
            output_mr: from_hex("fab84d9d797c272b33011caa78718f93c3d5fc44c7d35bbf138613440fca2c79").unwrap(),
            range_proof_mr: from_hex("63a36ba139a884434702dffccec348b02ba886d3851a19732d8d111a54e17d56").unwrap(),
            kernel_mr: from_hex("b097af173dc852862f48af67aa57f48c47d20bc608d77b46a3018999bffba911").unwrap(),
            total_kernel_offset: PrivateKey::from_hex(
                "0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
            nonce: 0,
            pow: ProofOfWork {
                accumulated_monero_difficulty: 1.into(),
                accumulated_blake_difficulty: 1.into(),
                target_difficulty: 1.into(),
                pow_algo: PowAlgorithm::Blake,
                pow_data: vec![],
            },
        },
        body,
    }
}

pub fn get_rincewind_block_hash() -> Vec<u8> {
    get_rincewind_genesis_block().hash()
}

pub fn get_rincewind_gen_header() -> BlockHeader {
    get_rincewind_genesis_block().header
}

/// This will get the rincewind gen block
pub fn get_ridcully_genesis_block() -> Block {
    // lets get the block
    let block = get_ridcully_genesis_block_raw();
    // Lets load in the ridcully faucet tx's
    // TODO add faucets
    // let mut utxos = Vec::new();
    // let file = include_str!("faucets/alphanet_faucet.json");
    // for line in file.lines() {
    //     let utxo: TransactionOutput = serde_json::from_str(line).unwrap();
    //     utxos.push(utxo);
    // }
    // // fix headers to new mmr roots after adding utxos
    // block.header.output_mr = from_hex("f9bfcc0bfae8f90991ea7cc9a625a411dd757cce088cbf740848570daa43daff").unwrap();
    // block.header.range_proof_mr =
    // from_hex("fbadbae2bf8c7289d77af52edc80490cb476a917abd0afeab8821913791b678f").unwrap(); block.header.kernel_mr
    // = from_hex("a40db2278709c3fb0e03044ca0f5090ffca616b708850d1437af4d584e17b97a").unwrap(); block.body.
    // add_outputs(&mut utxos);
    block
}

pub fn get_ridcully_genesis_block_raw() -> Block {
    let sig = Signature::new(
        PublicKey::from_hex("3abaff058dede56934138bb0be37c271b94d250c0a99ee93c530324ac99c667a").unwrap(),
        PrivateKey::from_hex("7f93d02b23bf3613e1a05513ab729bed90269691f29dfe5af07f8164c254f009").unwrap(),
    );
    let mut body = AggregateBody::new(
        vec![],
        vec![TransactionOutput {
            features: OutputFeatures {
                flags: OutputFlags::COINBASE_OUTPUT,
                maturity: 60,
            },
            commitment: Commitment::from_hex(
                "d69bc7eccbe3b78a80ee4ebbf6936afd88c268db1573916180b3cc37e7966e75",
            )
                .unwrap(),
            proof: BulletRangeProof::from_hex("3a52dc09c729bd4a570a6255f0303670c4d642c1a3959788a3cf1668555ea130b25f16b9d7ae7c7def0aa445c458d064066797a031331f50b6b875da5c4ee0709e64bcabd209b76a1a897e265937b6af063ddba3e6b11a95a31fccb6e97a6b224ab4d1964858a8e50335696fdb205f99bf33ad656d3671a7386629c232ee047b68aebf4a155bd7b493586e39dff246465eff812502f2a6dad4170d32e1614f00f44fe42e0ac7380d6ae0b14ffcab9968d3b0863b7fe59d7516ce13f275f4d40ae335e7f5b5cddccb9198043131f47a9326a68f0b25d0a3db8c7444beaeb9f80da40dbeac6371d4a0b822a1e7bd11199ebd3937dc11da35f48624ee74faacb36414208c51e117e152ad1187207c6082ad8d1378e274d83ca0cdeb8103aeb9eb211a8b960859dd3e61488bcc13e8ef0bdcccb2ec30d7a8d0ff12c5fbf774ef2c164265722ad843ed4779f070c7d55b85e2410951f7dacb5be4dd269a6c6ee5a95e9cd2a66b1f71e95fe1571b78382ea6ced1add2cc30b4620a5310692a8b0936601e7ed76706d1bd770ac454057d577075b7804bd2eecb8283321299db09b6a529e23a4ca6dc1512e763c121ced26326ceb605840dbf1fe74729ec972faa744c3c3ab81bc2cd372890a20e8e7d7a95506d5fdf4414336efabdc5bb8623a8918f29f6ec3f4d89cc8ac47eae8b2d4bae21a81e206ca6b88c78fe06e715fea3bee870fc214919b8ffec2f7fb1fb3ae5648b0e3057890933b35beee75c7ebb1054d80d88b472d7bcf2fbe61b1384aacdcd975dbac45315192f51c611757f399f72c33e78baadeb34a27cd84f51f2dde1a2c70cf7644011e2a2a4c04b835582de15e931be091e841cb409992a623c29de75299aa826384df6874bfaf52ad3c834925307b874b935797fc8b75d5d0564e0e9527953f36fde52010a312b9a0825a5c9bd0e").unwrap(),
        }],
        vec![TransactionKernel {
            features: KernelFeatures::COINBASE_KERNEL,
            fee: MicroTari(0),
            lock_height: 0,
            meta_info: None,
            linked_kernel: None,
            excess: Commitment::from_hex(
                "4c8271842e2ed17adedd7e1e510e204c5069f83cda38327782a5da0e1cebb528",
            )
                .unwrap(),
            excess_sig: sig,
        }],
    );
    body.sort();
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            timestamp: 1_603_843_200.into(), // 10/28/2020 @ 12:00am (UTC) //Todo Swop this before reset
            // timestamp: 1_603_287_362.into(),
            output_mr: from_hex("511aaaed95b87d4ae5ddfa04f578ab6c402d5751263e842403f2dbf676c2edde").unwrap(),
            range_proof_mr: from_hex("4c2f3ab2013da57c2e4b9a284f236554154a5d76be81c744cb20df87d4c7f7fa").unwrap(),
            kernel_mr: from_hex("875f3c92cd17b4b634bc21fc578958f59717e8cfc517327d26ab996c0449a77a").unwrap(),
            total_kernel_offset: PrivateKey::from_hex(
                "0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
            nonce: 0,
            pow: ProofOfWork {
                accumulated_monero_difficulty: 1.into(),
                accumulated_blake_difficulty: 1.into(),
                target_difficulty: 1.into(),
                pow_algo: PowAlgorithm::Sha3,
                pow_data: vec![],
            },
        },
        body,
    }
}

pub fn get_ridcully_block_hash() -> Vec<u8> {
    get_ridcully_genesis_block().hash()
}

pub fn get_ridcully_gen_header() -> BlockHeader {
    get_ridcully_genesis_block().header
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::transactions::types::CryptoFactories;

    #[test]
    fn rincewind_genesis_sanity_check() {
        let block = get_rincewind_genesis_block();
        println!("{}", &block);
        assert_eq!(block.body.outputs().len(), 4001);

        let factories = CryptoFactories::default();
        let coinbase = block.body.outputs().first().unwrap();
        assert!(coinbase.is_coinbase());
        coinbase.verify_range_proof(&factories.range_proof).unwrap();
        assert_eq!(block.body.kernels().len(), 1);
        for kernel in block.body.kernels() {
            kernel.verify_signature().unwrap();
        }

        let coinbase_kernel = block.body.kernels().first().unwrap();
        assert!(coinbase_kernel.features.contains(KernelFeatures::COINBASE_KERNEL));
        // I am leaving this here as a reminder. This the value is also changed as the emission curve has been swopped
        // from a floating point on to a integer one. BUG: Mainnet value was committed to in the Rincewind
        // let coinbase = block.body.outputs().iter().find(|o| o.is_coinbase()).unwrap();
        // genesis block coinbase let consensus_manager =
        // ConsensusManagerBuilder::new(Network::MainNet).build(); let supply =
        // consensus_manager.emission_schedule().supply_at_block(0); let expected_kernel =
        //     &coinbase.commitment - &factories.commitment.commit_value(&PrivateKey::default(), supply.into());
        // assert_eq!(coinbase_kernel.excess, expected_kernel);
    }

    #[test]
    fn ridcully_genesis_sanity_check() {
        let block = get_ridcully_genesis_block();
        assert_eq!(block.body.outputs().len(), 1);

        let factories = CryptoFactories::default();
        let coinbase = block.body.outputs().first().unwrap();
        assert!(coinbase.is_coinbase());
        coinbase.verify_range_proof(&factories.range_proof).unwrap();
        assert_eq!(block.body.kernels().len(), 1);
        for kernel in block.body.kernels() {
            kernel.verify_signature().unwrap();
        }

        let coinbase_kernel = block.body.kernels().first().unwrap();
        assert!(coinbase_kernel.features.contains(KernelFeatures::COINBASE_KERNEL));
    }

    #[test]
    fn load_rincewind_mmrs() {
        let block = get_rincewind_genesis_block();
        let output_mr = from_hex("f9bfcc0bfae8f90991ea7cc9a625a411dd757cce088cbf740848570daa43daff").unwrap();
        let range_proof_mr = from_hex("fbadbae2bf8c7289d77af52edc80490cb476a917abd0afeab8821913791b678f").unwrap();
        let kernel_mr = from_hex("a40db2278709c3fb0e03044ca0f5090ffca616b708850d1437af4d584e17b97a").unwrap();

        assert_eq!(output_mr, block.header.output_mr);
        assert_eq!(range_proof_mr, block.header.range_proof_mr);
        assert_eq!(kernel_mr, block.header.kernel_mr);
    }
}
