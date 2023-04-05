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
use tari_common_types::types::{BulletRangeProof, Commitment, FixedHash, PrivateKey, PublicKey, Signature};
use tari_crypto::{signatures::CommitmentAndPublicKeySignature, tari_utilities::hex::*};
use tari_script::script;

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
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
            TransactionOutputVersion,
        },
    },
};

/// Returns the genesis block for the selected network.
pub fn get_genesis_block(network: Network) -> ChainBlock {
    use Network::{Dibbler, Esmeralda, Igor, LocalNet, MainNet, NextNet, Ridcully, StageNet, Stibbons, Weatherwax};
    match network {
        MainNet => get_mainnet_genesis_block(),
        StageNet => get_stagenet_genesis_block(),
        NextNet => get_nextnet_genesis_block(),
        Igor => get_igor_genesis_block(),
        Esmeralda => get_esmeralda_genesis_block(),
        LocalNet => get_esmeralda_genesis_block(),
        Dibbler => unimplemented!("Dibbler is longer supported"),
        Ridcully => unimplemented!("Ridcully is longer supported"),
        Stibbons => unimplemented!("Stibbons is longer supported"),
        Weatherwax => unimplemented!("Weatherwax longer supported"),
    }
}

pub fn get_stagenet_genesis_block() -> ChainBlock {
    let block = get_stagenet_genesis_block_raw();

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

fn get_stagenet_genesis_block_raw() -> Block {
    // Note: Use 'print_new_genesis_block_stagenet' in core/tests/helpers/block_builders.rs to generate the required
    // fields below - DO THIS ONCE ONLY AS IT GENERATES NEW RANDOM FIELDS
    let excess_sig = Signature::new(
        PublicKey::from_hex("d87ce1422fc23d82c06b135b52277cebe1025358fdfee4cae2893b25c19b7828").unwrap(),
        PrivateKey::from_hex("3e64e11e3b0b81046b1f4fb56bfcce5f7ff2e86bddbe4ab4f2d13235d7543707").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("a81361c40952ad0ce2f44241763812a749b81b04bdcec6158944bcc3d8790b1d").unwrap(),
        PublicKey::from_hex("76524b1c0d5b0a178685051aff743183573ed558cdfd79adbc474284f749ab57").unwrap(),
        PrivateKey::from_hex("7d28834c491c8e59b5d27744388ea676da1b8ddd509af805529f6537f0649907").unwrap(),
        PrivateKey::from_hex("64ebca6a4183b75a9c9c38819618fb3b4755cc80ef85ca83ddcc64e106af1002").unwrap(),
        PrivateKey::from_hex("e3e5d36ed2599f18ca875f14d403938a9d6408e38c790799a42a6182c4fbef00").unwrap(),
    );
    let extra = "Tokenized and connected";
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures::create_coinbase(360, Some(extra.as_bytes().to_vec())),
        Commitment::from_hex("982e1066148ee81eab6c5091aea9c2db6720bf527467e49b5a5b907d8f8b356c").unwrap(),
        BulletRangeProof::from_hex("015c29e512011a1d49e48d1e13c30be365f42c5637a5093fa42cbd41591e532013702cfe24c9fe28c9a8b8beed587fb8164837c231567d9d8d1b67421474f4eb099238bdb64e48b182f6b199fa6f3b34d873a579a29a86a27108ceeba12efafa1f789300500d6f388491a1e7a1483c8bafbe518d9254128ef208aa3fbb119c0712f670524c920daa06327dc8bcbc0ed3a4184ec691d43677f6c33fa0a952bf2d4d0a5f825dc383e89ff5d58b5b2b115c3b30cec96115a789595fe04e67d54f31710cfbbf5e05d56f62777c9cd96d531335288a37da53d67fc51eb8dfdf94f1af4c84ab76ffa97dcf60199e53a131510506c9ea44688114bc112ced41d3eda7a118542c4f0538afbfae5396b06bfdeb5136c8b881ed70f08d184f7d36e5c2a19f7a12e46d10d77eeec741ca7041e36dad0add61dca5ecf701071d095b9578130f101c4455f02e1cff31449403523e5a68fc9b7861e52f03cc113ff2fd03794e13340ad726b3880ef6c573f93c5e1c023c3b75741d7572bc94bb7811332b384e856d2eae81901d0b7ad2408e5eac6edb709e12afec090de89b3e5d4d5308f12b8a3a44dfe67fb027597ae823bc7d2f6a39b0a99713ce6f16e8d9247c706a3938296a141929ecd44ab804b4281a87aebb900359131b0d6d8bf0471b4b74354d3373389115c5971f5499f4830c50f9d8a92e2e467d3a37a0f86c7b8c810189876d2409cc7b4a54cc0c3d2045b8dfe09be5b8742f78f4dc6fa030a44a547671f083f2017c322722b8da5bbefdb56a2de43db3c7bbb9c47358d982dce0ce757e41ed140a").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("721205a842d2bce5bbc32c275b4f675c1d1087c1557d8775df7e596e7b29641c").unwrap(),
        coinbase_meta_sig,
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
        Commitment::from_hex("86bbb9891de4027fe1d97f27c20b85a839377f7c3cb7c7c922de3c8784aec737").unwrap(),
        excess_sig,
        None,
    );
    let mut body = AggregateBody::new(vec![], vec![coinbase], vec![kernel]);
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("14 Jun 2023 13:00:00 +0200").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: timestamp.into(),
            output_mr: FixedHash::from_hex("61f9287751dc57b026405b1da3b05da338d67a782877db26dfa89f9ea94d9c27").unwrap(),
            witness_mr: FixedHash::from_hex("5c44e634ef48c00a5b83a9f5690d61676366556bfe6efe94f308f486c529edd3")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("47dfdf2b2b860ff85b0ea1a1a8ac1a77ac436dec71d20674d1a9b87aa1db096e").unwrap(),
            kernel_mmr_size: 1,
            input_mr: FixedHash::zero(),
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
            validator_node_mr: FixedHash::from_hex("e1d55f91ecc7e435080ac2641280516a355a5ecbe231158987da217b5af30047")
                .unwrap(),
        },
        body,
    }
}

pub fn get_nextnet_genesis_block() -> ChainBlock {
    let block = get_nextnet_genesis_block_raw();

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

fn get_nextnet_genesis_block_raw() -> Block {
    // Note: Use 'print_new_genesis_block_nextnet' in core/tests/helpers/block_builders.rs to generate the required
    // fields below - DO THIS ONCE ONLY AS IT GENERATES NEW RANDOM FIELDS
    let excess_sig = Signature::new(
        PublicKey::from_hex("a2e86b76b9a7500c69eab4d2f0827a7e0d39a8e1b8d9c52570d9675b8af00a3a").unwrap(),
        PrivateKey::from_hex("e0f9eef666d03301b21c6b6f03e2e174840f062572c73d2541ca01a35329110b").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("38b0e5f879d5cd74155087bebf9bfbf00a28f668431f3ca67a951e015ee71d30").unwrap(),
        PublicKey::from_hex("b441d1cf3bb8966139b7d66a49534e4ffa355cbb537910919fd0de25a7583e16").unwrap(),
        PrivateKey::from_hex("73b85743851a2c7cc738d56a2e694e70563ce7b81ebfba3c47a79b6e891d0303").unwrap(),
        PrivateKey::from_hex("3032edbc02f51ff388fa469391933205a8d79906325f03c770529fb726f13b0a").unwrap(),
        PrivateKey::from_hex("d0567c0baf6cfd259c3910841e1e0b96b302a14d34d17e3dde0ef1647f62ff07").unwrap(),
    );
    let extra = "Mathematical proof that something happened";
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures::create_coinbase(360, Some(extra.as_bytes().to_vec())),
        Commitment::from_hex("6aac1c6b736e98047d8fab8c7e17c241f4239f632ced08217cce3dcd0027690e").unwrap(),
        BulletRangeProof::from_hex("017aeea1deb4502a8647e1e894733d359dfa895494b4fdeb3ada77472cb899a438780b35117d176e9cc9cd6073a0f4cc5791e2da29b7eaa7d82e48fdcaa9d99c6eeea1eb36f85434b2c0b27511215b02b96b60b327ee98bac75bebe372d91e0a541a194d9bb4ccd4fa1e5f161021e72bd736f245460fbfbb52a73682f4c1344524d8eec41ae7ddac42cca9768d2a27c2915de142b00e6372de3e9a9b8417ccbf3568e191fadbd5dbbfd99600311f05694fe8ee322de050179baeb502e62d67960f10c543ac7bbd33cad4d2a5d2c21a080a9f565ef15a39c79017b3d9766ab63100b6762a5e41e4676093ef62b2123f2b600a9e5f190d9bd7f56953b15b0baeee34688a8141809cd84f45dce7534640b577148154de2e2b27f70e3aa38c70a1413998378316a582037843c4930542a62d6376362a376edc2d8770cd86021b47ab42a04640a4492bc2cbd7b61f7c9b1a59671103c0e1a033ec4476fa403bc420f05620b125b4d5ee97fc943a3fbee5a84840560fb80d44e942dc302d8f71c51a391ec42c37c4f0192fa0f5eafd30c080e7646d77de77f9591c0ff06e7d63bc109d58388e71e5c9098399f92f8d70e3a563d63df07acd48b6f7cb0c8759ac6ca05d321687cd31717669d6fc04a1008f761b8943bde10307279f78ebb15c1cb2c1403699a3fc626ae1fc5cb126a5c8abb160fbec14d871b09a7488248187c7deb95d0bff696133a06e9e4f29f76efe28fcc1345a547b249da9eadf008b0033aa516206b31010b3a18935147a91808f99fd1395a6723fa8c0e264243bed896188767808").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("26bcb4ad6da915c043061c084369eee80c9f621a7ba1165f942c447fd962494e").unwrap(),
        coinbase_meta_sig,
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
        Commitment::from_hex("3a27a38fc22d540fc3d239e3905220f890c5734bb404d94f8ac3e77553dcef05").unwrap(),
        excess_sig,
        None,
    );
    let mut body = AggregateBody::new(vec![], vec![coinbase], vec![kernel]);
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("14 Apr 2023 13:00:00 +0200").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: timestamp.into(),
            output_mr: FixedHash::from_hex("c1ee261a5ef09deee07b26f504e0af91de21dec8f187b9a20e5459b135d6a2f8").unwrap(),
            witness_mr: FixedHash::from_hex("6c161914626fa731eb7a9961895b6fad5222f09813e31d1f9ddbbdd709541148")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("7ace5bbe51e4cb7a029b9078cf496d91eace481345fadf96e0c9c8e7902b780c").unwrap(),
            kernel_mmr_size: 1,
            input_mr: FixedHash::zero(),
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
            validator_node_mr: FixedHash::from_hex("e1d55f91ecc7e435080ac2641280516a355a5ecbe231158987da217b5af30047")
                .unwrap(),
        },
        body,
    }
}

pub fn get_mainnet_genesis_block() -> ChainBlock {
    unimplemented!()
}

pub fn get_igor_genesis_block() -> ChainBlock {
    // lets get the block
    let mut block = get_igor_genesis_block_raw();

    // Add faucet utxos
    // NB! Update 'consensus_constants.rs/pub fn igor()/ConsensusConstants {faucet_value: ?}' with total value
    let mut utxos = Vec::new();
    let file = include_str!("faucets/igor_faucet.json");
    let mut counter = 1;
    let lines_count = file.lines().count();
    for line in file.lines() {
        if counter < lines_count {
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

    // // // Use this code if you need to generate new Merkle roots
    // // // NB: `igor_genesis_sanity_check` must pass
    //
    // use std::convert::TryFrom;
    //
    // use croaring::Bitmap;
    //
    // use crate::{chain_storage::calculate_validator_node_mr, KernelMmr, MutableOutputMmr, WitnessMmr};
    //
    // let mut kernel_mmr = KernelMmr::new(Vec::new());
    // for k in block.body.kernels() {
    //     println!("k: {}", k);
    //     kernel_mmr.push(k.hash().to_vec()).unwrap();
    // }
    //
    // let mut witness_mmr = WitnessMmr::new(Vec::new());
    // let mut output_mmr = MutableOutputMmr::new(Vec::new(), Bitmap::create()).unwrap();
    //
    // for o in block.body.outputs() {
    //     witness_mmr.push(o.witness_hash().to_vec()).unwrap();
    //     output_mmr.push(o.hash().to_vec()).unwrap();
    // }
    // let vn_mmr = calculate_validator_node_mr(&[]);
    //
    // block.header.kernel_mr = FixedHash::try_from(kernel_mmr.get_merkle_root().unwrap()).unwrap();
    // block.header.witness_mr = FixedHash::try_from(witness_mmr.get_merkle_root().unwrap()).unwrap();
    // block.header.output_mr = FixedHash::try_from(output_mmr.get_merkle_root().unwrap()).unwrap();
    // block.header.validator_node_mr = FixedHash::try_from(vn_mmr).unwrap();
    // println!("kernel mr: {}", block.header.kernel_mr.to_hex());
    // println!("witness mr: {}", block.header.witness_mr.to_hex());
    // println!("output mr: {}", block.header.output_mr.to_hex());
    // println!("vn mr: {}", block.header.validator_node_mr.to_hex());

    // Hardcode the Merkle roots once they've been computed above
    block.header.kernel_mr =
        FixedHash::from_hex("fe12b384142ae39c1a3b31f033c28e8cee13329c3090d30fb6feb256e352a7c0").unwrap();
    block.header.witness_mr =
        FixedHash::from_hex("32293a4022d0e5091a6d7efb532e93edb298b4d15b8c4b519f9d5b2405748b52").unwrap();
    block.header.output_mr =
        FixedHash::from_hex("c0fc5b1871d2208122297f99d4d32e750b1e947fb60af7dde9ff431315ce56df").unwrap();
    block.header.validator_node_mr =
        FixedHash::from_hex("e1d55f91ecc7e435080ac2641280516a355a5ecbe231158987da217b5af30047").unwrap();

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
    // Note: Use 'print_new_genesis_block_igor' in core/tests/helpers/block_builders.rs to generate the required fields
    // below - DO THIS ONCE ONLY AS IT GENERATES NEW RANDOM FIELDS
    let extra = "Hello, Igor";
    let excess_sig = Signature::new(
        PublicKey::from_hex("280b26ea8bb952b173d51bf86580c92c910d30bcadb469823bc9369326a0c705").unwrap(),
        PrivateKey::from_hex("49f653d7dd5effeb8de9ad9cb975536c44cba3ba025c732e433450f8dd714a07").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("d252d95ac1b59cda85d6f1c3635d578bddbd4cc7ec1530728cc087ed61606860").unwrap(),
        PublicKey::from_hex("6ebb7267c0b7f0475d40c2e2c34bbc801b742f1d17aff3840cdedad7fbe56f18").unwrap(),
        PrivateKey::from_hex("82c34aeeea3434e5b6f444d7503b6341ab1b8731d5e09f28f58be5904dc57608").unwrap(),
        PrivateKey::from_hex("9839a4f80a63a3448d1275d83ef3a3768053c82be4b2833179d93df297c84a07").unwrap(),
        PrivateKey::from_hex("1b5cd36e529208d6439aa184b9f278682344dda180e67ec7fac6bd806194b50f").unwrap(),
    );

    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures::create_coinbase(6, Some(extra.as_bytes().to_vec())),
        Commitment::from_hex("aaa625805404339d2e4cdc8ea22073b0633f9b22ef049d98b488f5f1dfd23f07").unwrap(),
        BulletRangeProof::from_hex("013aca7480dee5525ed6eb60650ebac4b023b7035e72397ec6faf5ce6b216e4839d6b6d35451239a62bccd5a85e4f7edab6213021e2ee11c7432f361a009c845587ce418a60386e1fd6155f6a48b2e37f8c1458ecc54e690c823ee78aa7c114f13cca5b13c8353d191ca1b83d1aaa164424ff6cbfed43079fa7e2d966f7cf7ab745254c4083a832ddb214747c937e60952b3ba19d71517ccbe0c0386f921d2786762b4f37eeabaeb7e6637d6077285e8fb2f1d91fe075f01a8617c3c21b261a9546c8624069e34dcdb3cf5f7dcb7ba1ede5d2bb1002099e3dec6a3e49fc67fd55546c86171ad6998738e04aee879dddcc94a98fbed289be8d8ac9cb304b694e204daa57fb6d6cc1449279a2caa0ef0482529479ca51e157ca8fdfebf6021100c35a8160c70d6603e9184c7ede8bcba85580b245dd7acfb45ba53eafb3827fbe578162cc78e3727a4e7e841b2d6845d17a8419e8e97abed4269ad26c02fd8aec53b0a447eb9bb328d15864c88d9f51b2b5ff9c8ba812bfeed484104f6f0606c811988eb7a0ac409c61a30827f757a70f237392fc73557d18eefd25f5f28763d761e4ea3579927ef81cbc0f5df8bdc1c1c812aa5bd5299f1bc87a96fa52a346fe93c50e4a99080015704d6393efaf2dd630760f42bd92d0f726665ed6c272339840155b1e5e5dcc4e6223fea60b090a149fee9329c3c0e72167ec1b8dce146ff60079351f0c82fe11b20853618f81e7027af60713a84a14414459281dcc0a4b3f503118ed182442b6e5fdb3e7e690ab53238d80a1fedbc118332fefce98920a0a70c").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("06a12cf13dbe2c455912128bf36975a421ec2bcbdf5fae8a7f22d172e4371673").unwrap(),
        coinbase_meta_sig,
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
        Commitment::from_hex("a4545e2eedf020d9d0dfef3a8571e27525437e88c037a52dc5254fc7e8e0cc2d").unwrap(),
        excess_sig,
        None,
    );
    let mut body = AggregateBody::new(vec![], vec![coinbase], vec![kernel]);
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("29 Mar 2023 06:00:00 +0200").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: timestamp.into(),
            output_mr: FixedHash::from_hex("380139dfd2dfaadd4a17805c50e645585da2e7cca1425dabc456df0f8ef62a1a").unwrap(),
            witness_mr: FixedHash::from_hex("1ae98f1730a1473e3bd56e5878a567e0c818c1c9becefb20d33507abdcf2f567")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("a623c3b5e6e75c2cfffca535709bf059bf474cfa660787ad6cecbeb6b14ae8ea").unwrap(),
            kernel_mmr_size: 1,
            validator_node_mr: FixedHash::from_hex("e1d55f91ecc7e435080ac2641280516a355a5ecbe231158987da217b5af30047")
                .unwrap(),
            input_mr: FixedHash::zero(),
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
    let block = get_esmeralda_genesis_block_raw();

    // leaving this code here if we want to add faucets back again the future
    // // Add faucet utxos
    // // NB! Update 'consensus_constants.rs/pub fn esmeralda()/ConsensusConstants {faucet_value: ?}' with total value
    // let mut utxos = Vec::new();
    // let file = include_str!("faucets/esmeralda_faucet.json");
    // let mut counter = 1;
    // for line in file.lines() {
    //     if counter < 4001 {
    //         let utxo: TransactionOutput = serde_json::from_str(line).unwrap();
    //         utxos.push(utxo);
    //     } else {
    //         block.body.add_kernel(serde_json::from_str(line).unwrap());
    //         block.header.kernel_mmr_size += 1;
    //     }
    //     counter += 1;
    // }
    // block.header.output_mmr_size += utxos.len() as u64;
    // block.body.add_outputs(&mut utxos);
    // block.body.sort();

    // Use this code if you need to generate new Merkle roots
    // NB: `esmeralda_genesis_sanity_check` must pass
    //
    // use croaring::Bitmap;
    // use std::convert::TryFrom;
    // use crate::{chain_storage::calculate_validator_node_mr, KernelMmr, MutableOutputMmr, WitnessMmr};
    // use crate::transactions::transaction_components::OutputType;

    // let mut kernel_mmr = KernelMmr::new(Vec::new());
    // for k in block.body.kernels() {
    //     println!("k: {}", k);
    //     kernel_mmr.push(k.hash().to_vec()).unwrap();
    // }

    // let mut witness_mmr = WitnessMmr::new(Vec::new());
    // let mut output_mmr = MutableOutputMmr::new(Vec::new(), Bitmap::create()).unwrap();

    // for o in block.body.outputs() {
    //     witness_mmr.push(o.witness_hash().to_vec()).unwrap();
    //     output_mmr.push(o.hash().to_vec()).unwrap();
    // if matches!(o.features.output_type, OutputType::ValidatorNodeRegistration) {
    //     let reg = o
    //         .features
    //         .sidechain_feature
    //         .as_ref()
    //         .and_then(|f| f.validator_node_registration())
    //         .unwrap();
    //     vn_mmr.push(reg.derive_shard_key(block.hash()).to_vec()).unwrap();
    // }
    // }

    // let vn_mmr = calculate_validator_node_mr(&[]).unwrap();

    // block.header.kernel_mr = FixedHash::try_from(kernel_mmr.get_merkle_root().unwrap()).unwrap();
    // block.header.witness_mr = FixedHash::try_from(witness_mmr.get_merkle_root().unwrap()).unwrap();
    // block.header.output_mr = FixedHash::try_from(output_mmr.get_merkle_root().unwrap()).unwrap();
    // block.header.validator_node_mr = FixedHash::try_from(vn_mmr.get_merkle_root().unwrap()).unwrap();
    // println!("kernel mr: {}", block.header.kernel_mr.to_hex());
    // println!("witness mr: {}", block.header.witness_mr.to_hex());
    // println!("output mr: {}", block.header.output_mr.to_hex());
    // println!("vn mr: {}", block.header.validator_node_mr.to_hex());

    // Hardcode the Merkle roots once they've been computed above
    // block.header.kernel_mr =
    //     FixedHash::from_hex("49bec44ce879f529523c593d2f533fffdc2823512d673e78e1bb6b2c28d9fcf5").unwrap();
    // block.header.witness_mr =
    //     FixedHash::from_hex("8e6bb075239bf307e311f497d35c12c77c4563f218c156895e6630a7d9633de3").unwrap();
    // block.header.output_mr =
    //     FixedHash::from_hex("163304b3fe0f9072170db341945854bf88c8e23e23ecaac3ed86b9231b20e16f").unwrap();
    // block.header.validator_node_mr =
    //     FixedHash::from_hex("e1d55f91ecc7e435080ac2641280516a355a5ecbe231158987da217b5af30047").unwrap();

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
    // Note: Use 'print_new_genesis_block_esmeralda' in core/tests/helpers/block_builders.rs to generate the required
    // fields below - DO THIS ONCE ONLY AS IT GENERATES NEW RANDOM FIELDS
    let excess_sig = Signature::new(
        PublicKey::from_hex("02fc8b9ed73a7c30927a5f07f75fbb3bdbf5969d17b4d7bf4596c0af8a824361").unwrap(),
        PrivateKey::from_hex("17f6a1e485a977bc46d5a72063f065f81ffe889b5e9b9681d69218cbc36c9f06").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("449d2db3cf368ce68797ee1cd1bce33ac6581a35ea0c7da786f12bde446a454c").unwrap(),
        PublicKey::from_hex("8eae110198e7c8ebf05fc722e2be456e7c4bb2854231f8020f1f3ce1e585ed68").unwrap(),
        PrivateKey::from_hex("20d306112590fbc217c85fb2285b79d5bc74a23fa278a101ee2a0e42b589850b").unwrap(),
        PrivateKey::from_hex("3c7e99f66fc396d3907dc08d36eb2ed0d714015b30dc1485d1596ad7a6323700").unwrap(),
        PrivateKey::from_hex("456181db33315172b12a7515dcce6240e42d30a758abc5b3643d40ceefe41d09").unwrap(),
    );

    let extra = "Queues happen to other people";
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures::create_coinbase(6, Some(extra.as_bytes().to_vec())),
        Commitment::from_hex("f89516425a158cefdd9f0efb623fb96a491e7b79896765b6a0d9f83f321c9509").unwrap(),
        BulletRangeProof::from_hex("011efee1941f357e92fc0cbe496cd733748690ab627fe8d24961c3faaf34848239accdf30bc35c977a532ede18391ff228ae755e5a3ab866ea3534ee6c6fb0461220495125c4c00d6e86c011dfec67b4ac4d9401d5cc85d78756a0eae51a21945cc8f146833a99e1374f5f97220a5d5cd9fe9406453068dbcd13413c5a5eecc01d56ee1fd328fec34c71aafdba2d69bfde3ab57007bf9fb17355b774752fc1a41f4ecf06255ee7b280d09989a58caa263d6fc139549c274cd964fc8f3081203c6576612a853fd59777c9afa5f97b8aca1137d7b18ebe7edcfdde1371bb79820b7a924e9d87600aa5233cf908c7ac1e8bc2b225244d74ac5059edcf0d0945cfc47dee4736837e829c945359e1c10fdaa49adc74e02e340ae26660c78de120e58d35a41395381ad7ad3dc9d85d666ba5cc5b3737c06f2024033cae9fbe076b43d3210043d912375874e6e205f01b9e588185108fbfd678cc01cfb29bcf0a259c85285e70c17df4e460b53c4cc5cae49e55f2eb0d3438c17652dd7e00e95686fae87c44ed39b11ce1b08f8cefe13bee3158d3dea5a12b63e909a3cda8831b4077a629586a331006eb8712a780d1d8d847c0663eee9683426f745373d0aee89b2d091822bcfd69856275de121f34bdcd392d846592a0d9869e1d311c7440cf11c2787c781e347dfc57a177e21db5566d9e1962b20958922ed8411d9e377c8c26d5e507c2befd30fd2931a3bfdf26e7643d62797fbd6eb5eb6505ab8bc326ebfcea7e09ec0278df8a332a59d038616f9368bf9664e7487d0271204decc8ccdcf8902a09").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("ace2a671dabb56145c7a945fa4a4810a2f9bb7fae335986a9a88a4cee1ffe03c").unwrap(),
        coinbase_meta_sig,
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
        Commitment::from_hex("d0c30f23a31e6452f87d1bfde729a61be138ea031011a3e1e7ccc7c682a3ff08").unwrap(),
        excess_sig,
        None,
    );
    let mut body = AggregateBody::new(vec![], vec![coinbase], vec![kernel]);
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("31 Mar 2023 14:00:00 +0200").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: timestamp.into(),
            output_mr: FixedHash::from_hex("3f88b304c52c1efc089410e167bf5ef01f671b7e289e8ee7dc5711adf2751c56").unwrap(),
            witness_mr: FixedHash::from_hex("ff088c196f8f617439ba9d309b8dc2463d0799904887d47873a3ea554b4211b2")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("80056d85dd0b419fcb63427b96df706ae32d81a4ddef27bf1ea3c0198425abeb").unwrap(),
            kernel_mmr_size: 1,
            validator_node_mr: FixedHash::from_hex("e1d55f91ecc7e435080ac2641280516a355a5ecbe231158987da217b5af30047")
                .unwrap(),
            input_mr: FixedHash::zero(),
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
    use std::convert::TryFrom;

    use croaring::Bitmap;
    use curve25519_dalek::scalar::Scalar;
    use tari_common_types::{
        epoch::VnEpoch,
        types::{Commitment, PrivateKey},
    };
    use tari_crypto::{hash::blake2::Blake256, hash_domain, hashing::DomainSeparatedHasher};
    use tari_utilities::ByteArray;

    use super::*;
    use crate::{
        chain_storage::calculate_validator_node_mr,
        consensus::ConsensusManager,
        test_helpers::blockchain::create_new_blockchain_with_network,
        transactions::{
            transaction_components::{transaction_output::batch_verify_range_proofs, OutputType},
            CryptoFactories,
        },
        validation::{ChainBalanceValidator, FinalHorizonStateValidation},
        KernelMmr,
        MutableOutputMmr,
        WitnessMmr,
    };

    #[test]
    fn stagenet_genesis_sanity_check() {
        // Note: Generate new data for `pub fn get_stagenet_genesis_block()` and `fn get_stagenet_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_stagenet_genesis_block();
        check_block(Network::StageNet, &block, 1, 1, false);
    }

    #[test]
    fn nextnet_genesis_sanity_check() {
        // Note: Generate new data for `pub fn get_nextnet_genesis_block()` and `fn get_stagenet_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_nextnet_genesis_block();
        check_block(Network::NextNet, &block, 1, 1, false);
    }

    #[test]
    fn esmeralda_genesis_sanity_check() {
        // Note: Generate new data for `pub fn get_esmeralda_genesis_block()` and `fn get_esmeralda_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_esmeralda_genesis_block();
        check_block(Network::Esmeralda, &block, 1, 1, false);
    }

    #[test]
    fn igor_genesis_sanity_check() {
        // Note: Generate new data for `pub fn get_igor_genesis_block()` and `fn get_igor_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_igor_genesis_block();
        check_block(Network::Igor, &block, 5527, 2, true);
    }

    fn check_block(
        network: Network,
        block: &ChainBlock,
        expected_outputs: usize,
        expected_kernels: usize,
        alternate_range_proof_check: bool,
    ) {
        assert!(block.block().body.inputs().is_empty());
        assert_eq!(block.block().body.kernels().len(), expected_kernels);
        assert_eq!(block.block().body.outputs().len(), expected_outputs);

        let factories = CryptoFactories::default();
        assert!(block.block().body.outputs().iter().any(|o| o.is_coinbase()));
        let outputs = block.block().body.outputs().iter().collect::<Vec<_>>();
        if alternate_range_proof_check {
            do_alternate_range_proof_check(&factories, &outputs);
        } else {
            batch_verify_range_proofs(&factories.range_proof, &outputs).unwrap();
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
        let mut kernel_mmr = KernelMmr::new(Vec::new());
        for k in block.block().body.kernels() {
            kernel_mmr.push(k.hash().to_vec()).unwrap();
        }

        let mut witness_mmr = WitnessMmr::new(Vec::new());
        let mut output_mmr = MutableOutputMmr::new(Vec::new(), Bitmap::create()).unwrap();
        let mut vn_nodes = Vec::new();
        for o in block.block().body.outputs() {
            o.verify_metadata_signature().unwrap();
            witness_mmr.push(o.witness_hash().to_vec()).unwrap();
            output_mmr.push(o.hash().to_vec()).unwrap();
            if matches!(o.features.output_type, OutputType::ValidatorNodeRegistration) {
                let reg = o
                    .features
                    .sidechain_feature
                    .as_ref()
                    .and_then(|f| f.validator_node_registration())
                    .unwrap();
                vn_nodes.push((
                    reg.public_key().clone(),
                    reg.derive_shard_key(None, VnEpoch(0), VnEpoch(0), block.hash()),
                ));
            }
        }

        assert_eq!(kernel_mmr.get_merkle_root().unwrap(), block.header().kernel_mr,);
        assert_eq!(witness_mmr.get_merkle_root().unwrap(), block.header().witness_mr,);
        assert_eq!(output_mmr.get_merkle_root().unwrap(), block.header().output_mr,);
        assert_eq!(calculate_validator_node_mr(&vn_nodes), block.header().validator_node_mr,);

        // Check that the faucet UTXOs balance (the faucet_value consensus constant is set correctly and faucet kernel
        // is correct)

        let utxo_sum = block.block().body.outputs().iter().map(|o| &o.commitment).sum();
        let kernel_sum = block.block().body.kernels().iter().map(|k| &k.excess).sum();

        let db = create_new_blockchain_with_network(Network::Igor);

        let lock = db.db_read_access().unwrap();
        ChainBalanceValidator::new(ConsensusManager::builder(network).build(), Default::default())
            .validate(&*lock, 0, &utxo_sum, &kernel_sum, &Commitment::default())
            .unwrap();
    }

    fn receiver_commit_nonce_a(commitment: &Commitment) -> PrivateKey {
        hash_domain!(
            CommitNonceHashDomain,
            "com.tari-genesis-tools.multi_party_utxo.receiver_commit_nonce_a",
            0
        );
        let hasher_bytes = DomainSeparatedHasher::<Blake256, CommitNonceHashDomain>::new()
            .chain(commitment.as_bytes())
            .finalize();
        PrivateKey::from_bytes(hasher_bytes.as_ref()).expect("blake256 hash will succeed")
    }

    // For the faucet, the value of the commitment can be bound into the metadata signature using a deterministic nonce
    fn do_alternate_range_proof_check(factories: &CryptoFactories, outputs: &[&TransactionOutput]) {
        for output_ref in outputs {
            let output = (*output_ref).clone();
            if output.features.output_type == OutputType::Standard {
                let commit_nonce_a = receiver_commit_nonce_a(&output.commitment);
                let e_bytes = TransactionOutput::build_metadata_signature_challenge(
                    TransactionOutputVersion::get_current_version(),
                    &output.script,
                    &output.features,
                    &output.sender_offset_public_key,
                    output.metadata_signature.ephemeral_commitment(),
                    output.metadata_signature.ephemeral_pubkey(),
                    &output.commitment,
                    &output.covenant,
                    &output.encrypted_value,
                    output.minimum_value_promise,
                );
                let e = PrivateKey::from_bytes(&e_bytes).unwrap();
                let value_as_private_key = PrivateKey::from(output.minimum_value_promise.as_u64());
                assert_eq!(
                    output.metadata_signature.u_a().to_hex(),
                    (commit_nonce_a.clone() + e.clone() * value_as_private_key).to_hex()
                );

                // Demonstrating how to extract the value (Note: For the proof, inverting the scalar with multiplication
                // is less efficient than multiplication only as above)
                let metadata_signature_u_a_scalar =
                    Scalar::from_bits(<[u8; 32]>::try_from(output.metadata_signature.u_a().as_bytes()).unwrap());
                let commit_nonce_a_scalar = Scalar::from_bits(<[u8; 32]>::try_from(commit_nonce_a.as_bytes()).unwrap());
                let e_scalar = Scalar::from_bits(<[u8; 32]>::try_from(e.as_bytes()).unwrap());
                let value_scalar = (metadata_signature_u_a_scalar - commit_nonce_a_scalar) * e_scalar.invert();
                assert_eq!(value_scalar, Scalar::from(output.minimum_value_promise.as_u64()));
                let mut value_bytes = [0u8; 8];
                value_bytes.copy_from_slice(&value_scalar.to_bytes()[0..8]);
                let value = u64::from_le_bytes(value_bytes);
                assert_eq!(value, output.minimum_value_promise.as_u64());
            } else {
                // Normal range proof verification for the coinbase output
                assert!(output.verify_range_proof(factories.range_proof.as_ref()).is_ok());
            }
        }
    }
}
