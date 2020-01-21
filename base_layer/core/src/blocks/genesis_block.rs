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
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

// This file is used to store the genesis block
use crate::{
    blocks::{block::Block, BlockHeader},
    proof_of_work::{PowAlgorithm, ProofOfWork},
};

use tari_transactions::{
    aggregated_body::AggregateBody,
    bullet_rangeproofs::BulletRangeProof,
    tari_amount::MicroTari,
    transaction::{KernelFeatures, OutputFeatures, OutputFlags, TransactionKernel, TransactionOutput},
    types::{Commitment, PrivateKey, PublicKey, Signature},
};
use tari_utilities::{hash::Hashable, hex::*};

// TODO: see issue #1145
// The values contain in this block is temporary. They should be replaced by the actual values before test net.
pub fn get_genesis_block() -> Block {
    let sig = Signature::new(
        PublicKey::from_hex("82f5e603783cfe8b7d50ec1fefb7841398bffcadcb6102dae1f83b533f0aec41").unwrap(),
        PrivateKey::from_hex("05af349cb5618e636021ca66a3fd21067b6f9b159b75b7783985a534726fe509").unwrap(),
    );
    let mut body = AggregateBody::new(
        vec![],
        vec![TransactionOutput {
            features: OutputFeatures {
                flags: OutputFlags::COINBASE_OUTPUT,
                maturity: 1,
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
            timestamp: 1578296727.into(),
            output_mr: from_hex("fab84d9d797c272b33011caa78718f93c3d5fc44c7d35bbf138613440fca2c79").unwrap(),
            range_proof_mr: from_hex("63a36ba139a884434702dffccec348b02ba886d3851a19732d8d111a54e17d56").unwrap(),
            kernel_mr: from_hex("b097af173dc852862f48af67aa57f48c47d20bc608d77b46a3018999bffba911").unwrap(),
            total_kernel_offset: PrivateKey::from_hex(
                "0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
            nonce: 0,
            pow: ProofOfWork {
                accumulated_monero_difficulty: 0.into(),
                accumulated_blake_difficulty: 0.into(),
                pow_algo: PowAlgorithm::Blake,
                pow_data: vec![],
            },
        },
        body,
    }
}

pub fn get_gen_block_hash() -> Vec<u8> {
    get_genesis_block().hash()
}

pub fn get_gen_header() -> BlockHeader {
    get_genesis_block().header
}
