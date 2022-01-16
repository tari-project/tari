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
    block.header.kernel_mr = from_hex("bc02a300e902fd1bd337042ee0b35100a1e302ca0888c969cef7390ceff0e87b").unwrap();
    block.header.witness_mr = from_hex("9bd974132a2496e90ac2100b883c568f53142b793f7116d061bb556f15e920b4").unwrap();
    block.header.output_mr = from_hex("5ccc7a4dbacece4b9e496d2ffede904e44da25db3dc410a10d2180ad2a37fb0e").unwrap();

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
        PublicKey::from_hex("e66438f4055514850c89a90a5741c412aecc74298f8f0d130fa08778375f4b53").unwrap(),
        PrivateKey::from_hex("65c35e9bdc6c6d43c7f80cbc2be838198e951da07f86e6f0e56978541455370d").unwrap(),
    );
    let mut body = AggregateBody::new(
        vec![],
        vec![TransactionOutput::new_current_version(
            OutputFeatures {
                flags: OutputFlags::COINBASE_OUTPUT,
                maturity: 60,
                ..Default::default()
            },
            Commitment::from_hex("e6ce76ce2f6ca4278222b4ee556344cd0becb14532b4c57c7c40c248d61abd54").unwrap(),
            BulletRangeProof::from_hex("a853f198f5bf92849373c545aee14c1fd9cd4fb8b1b525ee539efa2123706f354034f2931b7d5e940aaa42b89da1e088ff9b196cbf108c57b3df788ad0b6893324fddfb16bbba09e3acb8d99626ef0ea0c595189ea5dcef33d650ad398369a6e74fb0dd74ea43198c4d834abfb40dcaba87731b18bdb95115a8bfd288d23610fd8a50bc8d124fb80b57b56f79dad937b37de46797697baa6cc358f78a0d2e400b0c172ff8da82775b3ec591e365fc098114d1fddf57dadb2f74d7fd4aae1a107a5b52f025d6d1035bb128a45a08c79435d7d7c22b0ea4d06d28e8ab11a21d8050ec55213562c863e7c773f8823a67f5c44fde70dd74c0ac79f9fdaab8a9176604cac8c6ee3110e9a8aa6b87dd3fa2312d2323fad631807f1992aa1b6bfb3043d84eeb33e2cc9383919ad4417db1768eaa0218d2683af097c0a544b417c3eea7ef4a299c3ddc989406a145fd9e6b935bb2797fb9b30ff8f79d0bcebdb17204f5cb6d0b027c7b05e0419cfa7810b16600498d2bf6526448d7f8be43eb048265a1d985a240641d01cb626fd0266da240c7c8b1cd7f74b3474a7c025998255e01f00521558ab4c74bee24c7dbd4fafb37438fceb907b0c0b034e15078bd6efa09e428cb0d61c31fa32174b0f0954b8f7fdb119b3c401b5e1b7d784cbc2a4ba73504136d4c7c992a19016a8841dd82107fca3f61aec7978ea78d55a4a8f14f9932175de1a2ca9effd29cb96e72c69bb1ef9980bac8d72122c23dcf348f7fb68017f7b04a1ae47a300a2a418642a807d19b310c60e2f3f9778ec2f5338b72b9f26a40234f44c78a5a8883daa88e3f1beecf7caa48c19003075766df4dfc5bca85a784214b34f89f4f63fb3e1da3ee7686ac3ae61607572f9a623768d8c43f2a129a80c4b50bdaa99729aba7218d5ef04f6f01cd49479d27e9f10a096aa3d5d89c16008").unwrap(),
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
            Commitment::from_hex("28a51f144c09c4b4e108a7e3b8653788e8237b1aa8dd264c8eba043b07566530").unwrap(),
            excess_sig,
        )],
    );
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("07 Jan 2022 00:00:00 +0200").unwrap();
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: LATEST_BLOCK_VERSION,
            height: 0,
            prev_hash: vec![0; BLOCK_HASH_LENGTH],
            timestamp: timestamp.into(),
            output_mr: from_hex("1ebe4c1198422ab3acf032d3d9df0ef2a4252630780805703994d63e12265c18").unwrap(),
            witness_mr: from_hex("d0e6b97ac7b9ef4b15704d52d919a239709f5732416b27e2331aa160de318ace").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("4d8e291a61004b3a54be756bd9e68fffb43a9025cc78a355ddd586e4d134660b").unwrap(),
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
