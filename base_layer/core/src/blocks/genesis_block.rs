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
        transaction::{KernelFeatures, OutputFeatures, OutputFlags, TransactionKernel, TransactionOutput},
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
        LocalNet => get_igor_genesis_block(),
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
        PublicKey::from_hex("f2139d1cdbcfa670bbb60d4d03d9d50b0a522e674b11280e8064f6dc30e84133").unwrap(),
        PrivateKey::from_hex("3ff7522d9a744ebf99c7b6664c0e2c8c64d2a7b902a98b78964766f9f7f2b107").unwrap(),
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
                "fadafb12de96d90042dcbf839985aadb7ae88baa3446d5c6a17937ef2b36783e",
            )
                .unwrap(),
            BulletRangeProof::from_hex("845c947cbf23683f6ff6a56d0aa55fca14a618f7476d4e29348c5cbadf2bb062b8da701a0f058eb69c88492895c3f034db194f6d1b2d29ea83c1a68cbdd19a3f90ae080cfd0315bb20cd05a462c4e06e708b015da1d70c0f87e8c7413b579008e43a6c8dc1edb72b0b67612e897d251ec55798184ff35c80d18262e98034677b73f2dcc7ae25c9119900aadaf04a16068bf57b9e8b9bb694331750dc8acc6102b8961be183419dce2f96c48ced9892e4cdb091dcda0d6a0bb4ed94fc0c63ca065f25ce1e560504d49970bcaac007f33368f15ffa0dd3f56bf799b66fa684fe0fbeb882aee4a6fe05a3ca7c488a6ba22779a42f0f5d875175b8ebc517dd49df20b4f04f027b7d22b7c62cb93727f35c18a0b776d95fac4ff5405d6ed3dbb7613152178cecea4b712aa6e6701804ded71d94cf67de2e86ae401499b39de81b7344185c9eb3bd570ac6121143a690f118d9413abb894729b6b3e057f4771b2c2204285151a56695257992f2b0331f27066270718b37ab472c339d2560c1f6559f3c4ce31ec7f7e2acdbebb1715951d8177283a1ccc2f393ce292956de5db4afde419c0264d5cc4758e6e2c07b730ad43819f3761658d63794cc8071b30f9d7cd622bece4f086b0ca6a04fee888856084543a99848f06334acf48cace58e5ef8c85412017c400b4ec92481ba6d745915aef40531db73d1d84d07d7fce25737629e0fc4ee71e7d505bfd382e362cd1ac03a67c93b8f20cb4285ce240cf1e000d48332ba32e713d6cdf6266449a0a156241f7b1b36753f46f1ecb8b1836625508c5f31bc7ebc1d7cd634272be02cc109bf86983a0591bf00bacea1287233fc12324846398be07d44e8e14bd78cd548415f6de60b5a0c43a84ac29f6a8ac0b1b748dd07a8a4124625e1055b5f5b19da79c319b6e465ca5df0eb70cb4e3dc399891ce90b").unwrap(),
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
                "f472cc347a1006b7390f9c93b3c62fba334fd99f6c9c1daf9302646cd4781f61",
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
            output_mr: from_hex("dcc44f39b65e5e1e526887e7d56f7b85e2ea44bd29bc5bc195e6e015d19e1c06").unwrap(),
            witness_mr: from_hex("e4d7dab49a66358379a901b9a36c10f070aa9d7bdc8ae752947b6fc4e55d255f").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("589bc62ac5d9139f921c68b8075c32d8d130024acaf3196d1d6a89df601e2bcf").unwrap(),
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
            // kernel = Some(serde_json::from_str(line).unwrap());
        }
        counter += 1;
    }
    block.body.add_outputs(&mut utxos);
    block.header.output_mmr_size += 4000;

    // use this code if you need to generate new Merkle roots

    // use croaring::Bitmap;
    // use tari_common_types::types::HashDigest;
    // use tari_mmr::{MerkleMountainRange, MutableMmr};

    // let mut kernel_mmr = MerkleMountainRange::<HashDigest, _>::new(Vec::new());
    // for k in block.body.kernels() {
    //     // println!("k: {}", k);
    //     kernel_mmr.push(k.hash()).unwrap();
    // }

    // let mut witness_mmr = MerkleMountainRange::<HashDigest, _>::new(Vec::new());
    // let mut output_mmr = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create()).unwrap();

    // for o in block.body.outputs() {
    //     witness_mmr.push(o.witness_hash()).unwrap();
    //     output_mmr.push(o.hash()).unwrap();
    // }

    // block.header.kernel_mr = kernel_mmr.get_merkle_root().unwrap();
    // block.header.witness_mr = witness_mmr.get_merkle_root().unwrap();
    // block.header.output_mr = output_mmr.get_merkle_root().unwrap();
    // println!("kernel mr: {}", block.header.kernel_mr.to_hex());
    // println!("witness mr: {}", block.header.witness_mr.to_hex());
    // println!("output mr: {}", block.header.output_mr.to_hex());

    // hardcode the Merkle roots once they've been computed above
    block.header.kernel_mr = from_hex("cc0075fe75737041afa1142727c4b1b4ee076c0521dd39dc4d6a660c6b3eff21").unwrap();
    block.header.witness_mr = from_hex("4c1e237c34df49281a468f78dfc46796e89e2520fc94a7bf467a2516c64ec9d9").unwrap();
    block.header.output_mr = from_hex("28f897a7d31e1db72cd737b67cbdaa53f5f77f547237642fef9e9c65f9ecf855").unwrap();

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
        PublicKey::from_hex("2882c40752f21f0afb6c981940b3152bdf9821bea5e981ba7782fb447892aa20").unwrap(),
        PrivateKey::from_hex("40c035eacd3f5fae17c6197f6507e3e7397e79bf61b88990f5ce8dc0272f290b").unwrap(),
    );
    let mut body = AggregateBody::new(
        vec![],
        vec![TransactionOutput::new_current_version(
            OutputFeatures {
                flags: OutputFlags::COINBASE_OUTPUT,
                maturity: 60,
                ..Default::default()
            },
            Commitment::from_hex("ec92a1eab4d41599347347eba54d54bf8e2cd10017dfc527d9a8783e0eecc802 ").unwrap(),
            BulletRangeProof::from_hex("8e151bfdaba56a728af5b64cfc7f6d91af848b2f70c54ea04f580217ae2c3d3db6126d2ed9f0d888cd85660edcdb4dfa2afca152073d56c5c23169d6bfbad112cac94feea4c647bdd1e778257dc034df98d456467ba59ac35ee3bc4fb1d3e47874a5afd0e7caca1219bb6df3c71811ce7d35f3c556bb10532bafd30fcb26e918cce8679367bcd557ce6d477249950a0b959aee007f4fcc7809bb940ae58551037199fb0ec98b14ad002f865a1064a94ce9e08012dac9042669eaa5c7967edc0c5ad37db36e49faa42c705f8bb466d98f0b2cf4e8aa05aa8af78e320033b5e80cf6f4ea06df854202dc57a63623ba51cd351135cc6f0b2e2b9d6a3a7059d0c3082e82a57e2dff33741737c6efce5ff30df1e42c7fc123ac43a1605fbeeae82d13c0a5402662630cba18f4f20feabf6bdfae9e326e78bec7d0569622aa46a9c94be8c972e3a5f0acadc5096df2f325600829870b43cdc508c057598ce848d3845500a026f23641434cc75900fb7c7b8d55f40d590e483e9b0c936eb8c9f82cd37078882db6dd99154673e6ed49ba3b2a2ef777002714288a610a31d469d810d65cde1e17b4b5f6891f7d1b9e042e7daf84e093739ab04fe7f27bee55e85aa3295c406d8af0c7ccbb8546d2e4c80950c8e3874cd1a4b6ea9d4a66f60d1949809826281d7afd8dfc3e099a97d8f31c933a50a8818ca61c544b076e847a92c8ec6709a4f384940d1b546d39eff6a0b5f713f5ada842c399e98f19304f8747a2b3b8778037146aac39080b3406afb75fc62aa8c90124c00bba9bc8e9804a246f6edf0dd0c700cc752c6797aba74a9a9025875a7299390691ead47bcb0b801f1485af15b1ca9b49b9460d7b82e44f55bfea73d5b4233756da29e2033437480d1e9b830352bd78e75211a01aaa84b429b38876b2b777e41c0304e9362b596123aa474500").unwrap(),
            // For genesis block: A default script can never be spent, intentionally
            TariScript::default(),
            // Script offset never checked for coinbase, thus can use default
            Default::default(),
            // For genesis block: Metadata signature will never be checked
            Default::default(),
            Default::default()
        )],
        vec![TransactionKernel::new_current_version(
             KernelFeatures::COINBASE_KERNEL,
             MicroTari(0),
            0,
            Commitment::from_hex("aef7b6f76c5510a0adaa0458160f73e38691d1bb55e0e76d5dc9f27978527b1b").unwrap(),
            excess_sig,
        )],
    );
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("21 Jan 2022 00:00:00 +0200").unwrap();
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: LATEST_BLOCK_VERSION,
            height: 0,
            prev_hash: vec![0; BLOCK_HASH_LENGTH],
            timestamp: timestamp.into(),
            output_mr: from_hex("68c5269487e748bf941c18fe0e875fe13e94df7aa9da4c5e51a5d4d1ca8dca74").unwrap(),
            witness_mr: from_hex("33be75c7bee4f6c678ecfc9be46ede28f7b1db8a0b0c0bd708a4035886bd8c0a").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("593eb6923d9c4337bb50239dd09be7fcba40fc412c0d599bdc8d3e1679e27a42").unwrap(),
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
    use super::*;
    use crate::transactions::CryptoFactories;

    #[test]
    fn dibbler_genesis_sanity_check() {
        let block = get_dibbler_genesis_block();
        assert_eq!(block.block().body.outputs().len(), 4001);

        let factories = CryptoFactories::default();
        let coinbase = block.block().body.outputs().first().unwrap();
        assert!(coinbase.is_coinbase());
        coinbase.verify_range_proof(&factories.range_proof).unwrap();
        assert_eq!(block.block().body.kernels().len(), 1);
        for kernel in block.block().body.kernels() {
            kernel.verify_signature().unwrap();
        }

        let coinbase_kernel = block.block().body.kernels().first().unwrap();
        assert!(coinbase_kernel.features.contains(KernelFeatures::COINBASE_KERNEL));
    }
}
