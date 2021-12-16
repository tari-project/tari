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
        Ridcully => unimplemented!("No longer supported"),
        Stibbons => unimplemented!("No longer supported"),
        Weatherwax => unimplemented!("No longer supported"),
        LocalNet => get_igor_genesis_block(),
        Dibbler => get_dibbler_genesis_block(),
        Igor => get_igor_genesis_block(),
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
        vec![TransactionOutput {
            features: OutputFeatures {
                flags: OutputFlags::COINBASE_OUTPUT,
                maturity: 60,
               .. Default::default()
            },
            commitment: Commitment::from_hex(
                "fadafb12de96d90042dcbf839985aadb7ae88baa3446d5c6a17937ef2b36783e",
            )
                .unwrap(),
            proof: BulletRangeProof::from_hex("845c947cbf23683f6ff6a56d0aa55fca14a618f7476d4e29348c5cbadf2bb062b8da701a0f058eb69c88492895c3f034db194f6d1b2d29ea83c1a68cbdd19a3f90ae080cfd0315bb20cd05a462c4e06e708b015da1d70c0f87e8c7413b579008e43a6c8dc1edb72b0b67612e897d251ec55798184ff35c80d18262e98034677b73f2dcc7ae25c9119900aadaf04a16068bf57b9e8b9bb694331750dc8acc6102b8961be183419dce2f96c48ced9892e4cdb091dcda0d6a0bb4ed94fc0c63ca065f25ce1e560504d49970bcaac007f33368f15ffa0dd3f56bf799b66fa684fe0fbeb882aee4a6fe05a3ca7c488a6ba22779a42f0f5d875175b8ebc517dd49df20b4f04f027b7d22b7c62cb93727f35c18a0b776d95fac4ff5405d6ed3dbb7613152178cecea4b712aa6e6701804ded71d94cf67de2e86ae401499b39de81b7344185c9eb3bd570ac6121143a690f118d9413abb894729b6b3e057f4771b2c2204285151a56695257992f2b0331f27066270718b37ab472c339d2560c1f6559f3c4ce31ec7f7e2acdbebb1715951d8177283a1ccc2f393ce292956de5db4afde419c0264d5cc4758e6e2c07b730ad43819f3761658d63794cc8071b30f9d7cd622bece4f086b0ca6a04fee888856084543a99848f06334acf48cace58e5ef8c85412017c400b4ec92481ba6d745915aef40531db73d1d84d07d7fce25737629e0fc4ee71e7d505bfd382e362cd1ac03a67c93b8f20cb4285ce240cf1e000d48332ba32e713d6cdf6266449a0a156241f7b1b36753f46f1ecb8b1836625508c5f31bc7ebc1d7cd634272be02cc109bf86983a0591bf00bacea1287233fc12324846398be07d44e8e14bd78cd548415f6de60b5a0c43a84ac29f6a8ac0b1b748dd07a8a4124625e1055b5f5b19da79c319b6e465ca5df0eb70cb4e3dc399891ce90b").unwrap(),
            // For genesis block: A default script can never be spent, intentionally
            script: TariScript::default(),
            // Script offset never checked for coinbase, thus can use default
            sender_offset_public_key: Default::default(),
            // For genesis block: Metadata signature will never be checked
            metadata_signature: Default::default(),
            covenant: Covenant::default(),
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
    body.sort(1);
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
    // lets get the block
    let block = get_dibbler_genesis_block_raw();

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
    let sig = Signature::new(
        PublicKey::from_hex("6e6b47d8f6c654367858204b4277d854f78e0267baaf26cd513813ee23c16969").unwrap(),
        PrivateKey::from_hex("92a310f7bbb351850e599ac6719d530625643fad74cb2902ab4adaf9207ff60d").unwrap(),
    );
    let mut body = AggregateBody::new(
        vec![],
        vec![TransactionOutput {
            features: OutputFeatures {
                flags: OutputFlags::COINBASE_OUTPUT,
                maturity: 60,
                ..Default::default()
            },
            commitment: Commitment::from_hex(
                "c44428c3c707009befb074fb6db679049fd1a6c9b0466b281945c12670e8d706",
            )
                .unwrap(),
            proof: BulletRangeProof::from_hex("72101d8e581bb12bf153bed6eb321dd151e94e5d966e7545504b2f73a2cfda43bce0d87a9c0b4008762d30edaa4574b787824614b2f150e6456b609a7c2640603cc06570b44c60eeec2b88dc43a2aea484f357ef14ecb078a1f80f40b57d3812beab3a6fe23cacb3644b16967fdfd390daa6443fc03f711b001e9fe551f9f979db76189f4e17ed648bcfa419abd455bb5ec40e1801302d8cb870a3fbe4ad760a7dece961bff85989a75216f24c342c266f8819df33805e1df3dab7e6b8451007c58192a9bc3e660324309e0c34a3a67bab78156cc2443c7718a351136905a80af0268b08fb015b2b32cd6a9f6d839716cf549303c6c34a9db0f92e015a32414bfa5c680cf51439c9810a0783a51cc6176e52bddfd98bad7897742e9c107732791a5047022ec1f4e3c7569214e79cb1903131e43c2a2bd537c0ef04904e667a46961a5961340e17b38ac511176adfb0f94dd3b112589ef35b42c2745dd1d9ed5acadfd3226c51fed42f71c0cacbfefd05abe5d7dc7f799d7f2147c7803762cb1bc0da30fa9eca2e9748f9a55e19fb5ffad19865d370850a98bb165649c7a2814d147718ae8e58d126b7acb3500ce871d709083917873a8618ef04d364376ac142721c6d86c8b4a8d45cbf62310c18656ff950302a6fffc56ca1160e6496be756f40769bfc259043904d9d2c94bb9d69d842dc0bea5dcd708c1d96e47dba856d5cb46f0c69f67a02ec8da5ed5cc96d5f4a375059c2f38607da525e51453040d6647870a4ab6a7656cb5e2161412d1cb5284ba2abe7a21966f6ef3c615a9e4507711880fdb192277d1255318a4a5a9ff3b55a861bcadbee41e9fa653515a24bef2d6adf2db066c59c89bcc8b131dacb20f3f7390922a8a69c7105bb53e204f5e009eed3f5838b8d95e6cd1f00721c21f48419597ece1611b83c6b53abc2dd1b1804").unwrap(),
            // For genesis block: A default script can never be spent, intentionally
            script: TariScript::default(),
            // Script offset never checked for coinbase, thus can use default
            sender_offset_public_key: Default::default(),
            // For genesis block: Metadata signature will never be checked
            metadata_signature: Default::default(),
            covenant: Default::default()
        }],
        vec![TransactionKernel {
            features: KernelFeatures::COINBASE_KERNEL,
            fee: MicroTari(0),
            lock_height: 0,
            excess: Commitment::from_hex("d0d8c0f6ba4913fd6f30595e0d7268e108cfbc8ae4a602dc6a33daf67fca434e").unwrap(),
            excess_sig: sig,
        }],
    );
    body.sort(2);
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("31 Oct 2021 00:00:00 +0200").unwrap();
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: LATEST_BLOCK_VERSION,
            height: 0,
            prev_hash: vec![0; BLOCK_HASH_LENGTH],
            timestamp: timestamp.into(),
            output_mr: from_hex("aba0712711944acb920b549c196973c7cccdb38725eb595dbc5da5b33ce0220a").unwrap(),
            witness_mr: from_hex("a84f23daa5f85af99f97fb2b13262d61f09e1059a625fb479c1926d2c63948bc").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("12624ca9347f092db4fb832f122221d2a2514978ecf44def0438f53001253367").unwrap(),
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
        assert_eq!(block.block().body.outputs().len(), 1);

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
