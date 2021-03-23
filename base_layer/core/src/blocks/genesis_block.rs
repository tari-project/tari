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

use crate::{
    chain_storage::{BlockHeaderAccumulatedData, ChainBlock},
    transactions::{
        aggregated_body::AggregateBody,
        bullet_rangeproofs::BulletRangeProof,
        tari_amount::MicroTari,
        transaction::{KernelFeatures, OutputFeatures, OutputFlags, TransactionKernel, TransactionOutput},
        types::{Commitment, PrivateKey, PublicKey, Signature},
    },
};
use tari_crypto::{
    hash::blake2::Blake256,
    script::TariScript,
    tari_utilities::{hash::Hashable, hex::*},
};

pub fn get_mainnet_genesis_block() -> ChainBlock {
    unimplemented!()
}

pub fn get_mainnet_block_hash() -> Vec<u8> {
    get_mainnet_genesis_block().accumulated_data.hash
}

pub fn get_mainnet_gen_header() -> BlockHeader {
    get_mainnet_genesis_block().block.header
}

pub fn get_stibbons_genesis_block() -> ChainBlock {
    // lets get the block
    let mut block = get_stibbons_genesis_block_raw();
    // Lets load in the stibbons faucet tx's
    let mut utxos = Vec::new();
    let file = include_str!("faucets/stibbons_faucet.json");
    // last 2 lines are used for the kernel creation
    let mut kernel: Option<TransactionKernel> = None;
    let mut counter = 1;
    for line in file.lines() {
        if counter < 4001 {
            let utxo: TransactionOutput = serde_json::from_str(line).unwrap();
            utxos.push(utxo);
        } else {
            kernel = Some(serde_json::from_str(line).unwrap());
        }
        counter += 1;
    }
    // fix headers to new mmr roots after adding utxos
    block.header.output_mr = from_hex("a939fda2579fb0b6fd906111f61e37c5ea23eccd8b737eb7da517fde71a98078").unwrap();
    block.header.range_proof_mr = from_hex("90a557390ce185318375546cb1244ffda3bb62274cce591880e2d012c38b1755").unwrap();
    block.header.output_mmr_size += 4000;
    block.header.kernel_mr = from_hex("f5e08e66e9c0e5e3818d96a694f4f6eafd689f38cea2e52e771eab2cc7a3941a").unwrap();
    block.header.kernel_mmr_size += 1;
    block.body.add_outputs(&mut utxos);
    block.body.add_kernels(&mut vec![kernel.unwrap()]);
    let accumulated_data = BlockHeaderAccumulatedData {
        hash: block.hash(),
        total_kernel_offset: block.header.total_kernel_offset.clone(),
        achieved_difficulty: 1.into(),
        total_accumulated_difficulty: 1,
        accumulated_monero_difficulty: 1.into(),
        accumulated_blake_difficulty: 1.into(),
        target_difficulty: 1.into(),
    };
    ChainBlock {
        block,
        accumulated_data,
    }
}

#[allow(deprecated)]
pub fn get_stibbons_genesis_block_raw() -> Block {
    let sig = Signature::new(
        PublicKey::from_hex("f2139d1cdbcfa670bbb60d4d03d9d50b0a522e674b11280e8064f6dc30e84133").unwrap(),
        PrivateKey::from_hex("3ff7522d9a744ebf99c7b6664c0e2c8c64d2a7b902a98b78964766f9f7f2b107").unwrap(),
    );
    let mut body = AggregateBody::new(
        vec![],
        vec![TransactionOutput {
            features: OutputFeatures {
                flags: OutputFlags::COINBASE_OUTPUT,
                maturity: 60,
            },
            commitment: Commitment::from_hex(
                "fadafb12de96d90042dcbf839985aadb7ae88baa3446d5c6a17937ef2b36783e",
            )
                .unwrap(),
            proof: BulletRangeProof::from_hex("845c947cbf23683f6ff6a56d0aa55fca14a618f7476d4e29348c5cbadf2bb062b8da701a0f058eb69c88492895c3f034db194f6d1b2d29ea83c1a68cbdd19a3f90ae080cfd0315bb20cd05a462c4e06e708b015da1d70c0f87e8c7413b579008e43a6c8dc1edb72b0b67612e897d251ec55798184ff35c80d18262e98034677b73f2dcc7ae25c9119900aadaf04a16068bf57b9e8b9bb694331750dc8acc6102b8961be183419dce2f96c48ced9892e4cdb091dcda0d6a0bb4ed94fc0c63ca065f25ce1e560504d49970bcaac007f33368f15ffa0dd3f56bf799b66fa684fe0fbeb882aee4a6fe05a3ca7c488a6ba22779a42f0f5d875175b8ebc517dd49df20b4f04f027b7d22b7c62cb93727f35c18a0b776d95fac4ff5405d6ed3dbb7613152178cecea4b712aa6e6701804ded71d94cf67de2e86ae401499b39de81b7344185c9eb3bd570ac6121143a690f118d9413abb894729b6b3e057f4771b2c2204285151a56695257992f2b0331f27066270718b37ab472c339d2560c1f6559f3c4ce31ec7f7e2acdbebb1715951d8177283a1ccc2f393ce292956de5db4afde419c0264d5cc4758e6e2c07b730ad43819f3761658d63794cc8071b30f9d7cd622bece4f086b0ca6a04fee888856084543a99848f06334acf48cace58e5ef8c85412017c400b4ec92481ba6d745915aef40531db73d1d84d07d7fce25737629e0fc4ee71e7d505bfd382e362cd1ac03a67c93b8f20cb4285ce240cf1e000d48332ba32e713d6cdf6266449a0a156241f7b1b36753f46f1ecb8b1836625508c5f31bc7ebc1d7cd634272be02cc109bf86983a0591bf00bacea1287233fc12324846398be07d44e8e14bd78cd548415f6de60b5a0c43a84ac29f6a8ac0b1b748dd07a8a4124625e1055b5f5b19da79c319b6e465ca5df0eb70cb4e3dc399891ce90b").unwrap(),
            // TODO: Replace default script hash ?
            script_hash: TariScript::default().as_hash::<Blake256>().unwrap().to_vec(),
            // TODO: Replace default offset public key
            script_offset_public_key: Default::default()
        }],
        vec![TransactionKernel {
            features: KernelFeatures::COINBASE_KERNEL,
            fee: MicroTari(0),
            lock_height: 0,
            excess: Commitment::from_hex(
                "f472cc347a1006b7390f9c93b3c62fba334fd99f6c9c1daf9302646cd4781f61",
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
            timestamp: 1_611_835_200.into(), // 28/01/2021 @ 12:00pm (UTC)
            output_mr: from_hex("dcc44f39b65e5e1e526887e7d56f7b85e2ea44bd29bc5bc195e6e015d19e1c06").unwrap(),
            range_proof_mr: from_hex("e4d7dab49a66358379a901b9a36c10f070aa9d7bdc8ae752947b6fc4e55d255f").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("589bc62ac5d9139f921c68b8075c32d8d130024acaf3196d1d6a89df601e2bcf").unwrap(),
            kernel_mmr_size: 1,
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

/// This will get the ridcully gen block
pub fn get_ridcully_genesis_block() -> ChainBlock {
    // lets get the block
    let mut block = get_ridcully_genesis_block_raw();
    // Lets load in the ridcully faucet tx's
    let mut utxos = Vec::new();
    let file = include_str!("faucets/ridcully_faucet.json");
    // last 2 lines are used for the kernel creation
    let mut kernel: Option<TransactionKernel> = None;
    let mut counter = 1;
    for line in file.lines() {
        if counter < 4001 {
            let utxo: TransactionOutput = serde_json::from_str(line).unwrap();
            utxos.push(utxo);
        } else {
            kernel = Some(serde_json::from_str(line).unwrap());
        }
        counter += 1;
    }
    // fix headers to new mmr roots after adding utxos
    block.header.output_mr = from_hex("a939fda2579fb0b6fd906111f61e37c5ea23eccd8b737eb7da517fde71a98078").unwrap();
    block.header.range_proof_mr = from_hex("90a557390ce185318375546cb1244ffda3bb62274cce591880e2d012c38b1755").unwrap();
    block.header.output_mmr_size += 4000;
    block.header.kernel_mr = from_hex("f5e08e66e9c0e5e3818d96a694f4f6eafd689f38cea2e52e771eab2cc7a3941a").unwrap();
    block.header.kernel_mmr_size += 1;
    block.body.add_outputs(&mut utxos);
    block.body.add_kernels(&mut vec![kernel.unwrap()]);
    let accumulated_data = BlockHeaderAccumulatedData {
        hash: block.hash(),
        total_kernel_offset: block.header.total_kernel_offset.clone(),
        achieved_difficulty: 1.into(),
        total_accumulated_difficulty: 1,
        accumulated_monero_difficulty: 1.into(),
        accumulated_blake_difficulty: 1.into(),
        target_difficulty: 1.into(),
    };
    ChainBlock {
        block,
        accumulated_data,
    }
}

#[allow(deprecated)]
pub fn get_ridcully_genesis_block_raw() -> Block {
    let sig = Signature::new(
        PublicKey::from_hex("f2139d1cdbcfa670bbb60d4d03d9d50b0a522e674b11280e8064f6dc30e84133").unwrap(),
        PrivateKey::from_hex("3ff7522d9a744ebf99c7b6664c0e2c8c64d2a7b902a98b78964766f9f7f2b107").unwrap(),
    );
    let mut body = AggregateBody::new(
        vec![],
        vec![TransactionOutput {
            features: OutputFeatures {
                flags: OutputFlags::COINBASE_OUTPUT,
                maturity: 60,
            },
            commitment: Commitment::from_hex(
                "fadafb12de96d90042dcbf839985aadb7ae88baa3446d5c6a17937ef2b36783e",
            )
                .unwrap(),
            proof: BulletRangeProof::from_hex("845c947cbf23683f6ff6a56d0aa55fca14a618f7476d4e29348c5cbadf2bb062b8da701a0f058eb69c88492895c3f034db194f6d1b2d29ea83c1a68cbdd19a3f90ae080cfd0315bb20cd05a462c4e06e708b015da1d70c0f87e8c7413b579008e43a6c8dc1edb72b0b67612e897d251ec55798184ff35c80d18262e98034677b73f2dcc7ae25c9119900aadaf04a16068bf57b9e8b9bb694331750dc8acc6102b8961be183419dce2f96c48ced9892e4cdb091dcda0d6a0bb4ed94fc0c63ca065f25ce1e560504d49970bcaac007f33368f15ffa0dd3f56bf799b66fa684fe0fbeb882aee4a6fe05a3ca7c488a6ba22779a42f0f5d875175b8ebc517dd49df20b4f04f027b7d22b7c62cb93727f35c18a0b776d95fac4ff5405d6ed3dbb7613152178cecea4b712aa6e6701804ded71d94cf67de2e86ae401499b39de81b7344185c9eb3bd570ac6121143a690f118d9413abb894729b6b3e057f4771b2c2204285151a56695257992f2b0331f27066270718b37ab472c339d2560c1f6559f3c4ce31ec7f7e2acdbebb1715951d8177283a1ccc2f393ce292956de5db4afde419c0264d5cc4758e6e2c07b730ad43819f3761658d63794cc8071b30f9d7cd622bece4f086b0ca6a04fee888856084543a99848f06334acf48cace58e5ef8c85412017c400b4ec92481ba6d745915aef40531db73d1d84d07d7fce25737629e0fc4ee71e7d505bfd382e362cd1ac03a67c93b8f20cb4285ce240cf1e000d48332ba32e713d6cdf6266449a0a156241f7b1b36753f46f1ecb8b1836625508c5f31bc7ebc1d7cd634272be02cc109bf86983a0591bf00bacea1287233fc12324846398be07d44e8e14bd78cd548415f6de60b5a0c43a84ac29f6a8ac0b1b748dd07a8a4124625e1055b5f5b19da79c319b6e465ca5df0eb70cb4e3dc399891ce90b").unwrap(),
            // TODO: Replace default script hash ?
            script_hash: TariScript::default().as_hash::<Blake256>().unwrap().to_vec(),
            // TODO: Replace default offset public key
            script_offset_public_key: Default::default()
        }],
        vec![TransactionKernel {
            features: KernelFeatures::COINBASE_KERNEL,
            fee: MicroTari(0),
            lock_height: 0,
            excess: Commitment::from_hex(
                "f472cc347a1006b7390f9c93b3c62fba334fd99f6c9c1daf9302646cd4781f61",
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
            timestamp: 1_603_843_200.into(), // 10/28/2020 @ 12:00am (UTC)
            output_mr: from_hex("dcc44f39b65e5e1e526887e7d56f7b85e2ea44bd29bc5bc195e6e015d19e1c06").unwrap(),
            range_proof_mr: from_hex("e4d7dab49a66358379a901b9a36c10f070aa9d7bdc8ae752947b6fc4e55d255f").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("589bc62ac5d9139f921c68b8075c32d8d130024acaf3196d1d6a89df601e2bcf").unwrap(),
            kernel_mmr_size: 1,
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

pub fn get_ridcully_block_hash() -> Vec<u8> {
    get_ridcully_genesis_block().accumulated_data.hash
}

pub fn get_stibbons_block_hash() -> Vec<u8> {
    get_stibbons_genesis_block().accumulated_data.hash
}

pub fn get_ridcully_gen_header() -> BlockHeader {
    get_ridcully_genesis_block().block.header
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::transactions::types::CryptoFactories;

    #[test]
    fn ridcully_genesis_sanity_check() {
        let block = get_ridcully_genesis_block();
        assert_eq!(block.block.body.outputs().len(), 4001);

        let factories = CryptoFactories::default();
        let coinbase = block.block.body.outputs().first().unwrap();
        assert!(coinbase.is_coinbase());
        coinbase.verify_range_proof(&factories.range_proof).unwrap();
        assert_eq!(block.block.body.kernels().len(), 2);
        for kernel in block.block.body.kernels() {
            kernel.verify_signature().unwrap();
        }

        let coinbase_kernel = block.block.body.kernels().first().unwrap();
        assert!(coinbase_kernel.features.contains(KernelFeatures::COINBASE_KERNEL));
    }
}
