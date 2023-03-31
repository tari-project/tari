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
    // Note: Use print_new_genesis_block_nextnet in core/tests/helpers/block_builders.rs to generate the required
    // fields below
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
    let genesis = DateTime::parse_from_rfc2822("8 Feb 2023 13:00:00 -0800").unwrap();
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
    // Note: Use print_new_genesis_block_stagenet in core/tests/helpers/block_builders.rs to generate the required
    // fields below
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
    let genesis = DateTime::parse_from_rfc2822("24 Feb 2023 12:15:00 +0100").unwrap();
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
    let block = get_igor_genesis_block_raw();

    // Use this code to generate the correct header mr fields for igor if the gen block changes.
    //
    // use croaring::Bitmap;
    //
    // use crate::{KernelMmr, MutableOutputMmr, WitnessMmr};
    //
    // let mut kernel_mmr = KernelMmr::new(Vec::new());
    // for k in block.body.kernels() {
    //     println!("k: {}", k);
    //     kernel_mmr.push(k.hash()).unwrap();
    // }
    //
    // let mut witness_mmr = WitnessMmr::new(Vec::new());
    // let mut output_mmr = MutableOutputMmr::new(Vec::new(), Bitmap::create()).unwrap();
    //
    // for o in block.body.outputs() {
    //     witness_mmr.push(o.witness_hash()).unwrap();
    //     output_mmr.push(o.hash()).unwrap();
    // }
    //
    // println!("kernel mr: {}", kernel_mmr.get_merkle_root().unwrap().to_hex());
    // println!("witness mr: {}", witness_mmr.get_merkle_root().unwrap().to_hex());
    // println!("output mr: {}", output_mmr.get_merkle_root().unwrap().to_hex());

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
    // Note: Use print_new_genesis_block_igor in core/tests/helpers/block_builders.rs to generate the required fields
    // below
    let sig = Signature::new(
        PublicKey::from_hex("e28d427eca88f76a07b3710ecc3cdc1c19a39b6936dfed3e36f2c94e1c74811b").unwrap(),
        PrivateKey::from_hex("2749e866be9b231778952922268d18843eace6a76844b3c7579750f7898d8f09").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("54caee18216617c0bd1c35a39c3bb6c1158e61cf5083fedc5c50aecab5802121").unwrap(),
        PublicKey::from_hex("d2ad84c5bb638ac21280da8de5b8c682107c4e8ce1412b8e102de549e510af53").unwrap(),
        PrivateKey::from_hex("a930bb55c4a04aa86e16d8711bd9b72fb4791bfc31676442d0886bde01addf0a").unwrap(),
        PrivateKey::from_hex("6ff89c404b71748fe49d8d6c9aedcbb6af6c7c0f6c9128bf94edfa0b6185c50d").unwrap(),
        PrivateKey::from_hex("d6d4578a3640754b88ef7c7de59d83d16fad30dd7ddce344c5d249a3b015b404").unwrap(),
    );
    let extra = "Hello, Igor";
    let mut body = AggregateBody::new(
        vec![],
        vec![TransactionOutput::new_current_version(
            OutputFeatures::create_coinbase(6, Some(extra.as_bytes().to_vec())),
            Commitment::from_hex(
                "ae0335c9afec7ee8e423feed1605092a7ba2c05dae2c7be1830acd8c9e9d1733",
            )
                .unwrap(),
            BulletRangeProof::from_hex("015eb52c05ba445ceab516b9f941c8832194ae348c2c34228949ab3435d84b6470961464e4d85e3c25e9d9a65344ce67607bad7a65e598700c504380831ae82c24ccd091caf279d9b878eaf501c81c231586f1073883b94734c2387b5aa5ba5f150846cd7ae07a9949f17ad4443292d081e41ce5797c884035987c8de94cf9e44d4ec38e200fa315cb2e79fa6a5892a576000e4a2c712b08e6f9adbe5a9fd67959a21eb58b418cb2ac5cd5ae60ef985a9c15c189c4646ff3aeeb065ca46c8f8c41542473c909f9875dc2f8e9c9691bbfad5c383825c935743992069f37d995727644123853a2f812093e41cd85440de0d6500d8fe399352600fae2ebce14b1fd103266e4354ca3abd9de79f68631d0966ceaca723f1049e9f06ad13cb669998f52d8c1ddbebf307a83d6feffb756d5ff060ab630aa6d96737e01e2af881362a128029bcb0d1b1d015c617f17dbc9df150dce09804d5ea35325f7931e8218700e48882528b1d54845603f6e02f2b7bb08bad9261f19394df4ac9d86f27879619d469453c88ded79a558fddca8523f51d01f346a1e327f1489aaea54dd61bac8071ed46830dbbbc5f2469f799dccb926d5af5bf5f06f6f5e3aee40aad3268ea2902a6258d3db4c3438d4d3b4c88d2128784af7ddd260fd446a602bb196b489c5cc6f4e1963f68cdd16753d1d55674193fe446f9848e939b0de24e0b2ec11ef9f1e044bdaa138934aaf40fb3f4af45ac65873043f67f9a2ef07edb21615e0db44f808baa1fcac60ae0249acd1c1f548daaa79a8f3344c69dc5543f1e63003a9825203").unwrap(),
            // For genesis block: A default script can never be spent, intentionally
            script!(Nop),
            // Script offset never checked for coinbase, thus can use default
            PublicKey::from_hex("d0c6e428e32ca2626dd66d328056acac0812e5e697652535e285c4b04f394c38").unwrap(),
            coinbase_meta_sig,
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
                "0acc815177030858527ab0eaa991d7aeaaaa8363c0e75fbd210d1209f08bdc0b",
            )
                .unwrap(),
            sig,None

        )],
    );
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("30 Nov 2022 16:00:00 +0100").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: timestamp.into(),
            output_mr: FixedHash::from_hex("d8bcf0f897d1dcca3bf479a3801f1637734967d54badfc74dbce761ae5ed97a1").unwrap(),
            witness_mr: FixedHash::from_hex("287e36690ed5008619417ebd4342e797fc717aeb06d004b10051cc439f2c521f")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("70d32b50d61845d1624e16e367de7315434ee6ffed600e660a3ebc71e36e125f").unwrap(),
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

pub fn get_esmeralda_genesis_block() -> ChainBlock {
    let block = get_esmeralda_genesis_block_raw();

    // leaving this code here if we want to add faucets back again the future
    // Add faucet utxos
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
    // Note: Use print_new_genesis_block_esmeralda in core/tests/helpers/block_builders.rs to generate the required
    // fields below
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
    let genesis = DateTime::parse_from_rfc2822("15 Mar 2023 10:00:00 +0200").unwrap();
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

#[cfg(test)]
mod test {

    use croaring::Bitmap;
    use tari_common_types::{epoch::VnEpoch, types::Commitment};

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
        check_block(Network::StageNet, &block, 1, 1);
    }

    #[test]
    fn nextnet_genesis_sanity_check() {
        // Note: Generate new data for `pub fn get_nextnet_genesis_block()` and `fn get_stagenet_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_nextnet_genesis_block();
        check_block(Network::NextNet, &block, 1, 1);
    }

    #[test]
    fn esmeralda_genesis_sanity_check() {
        // Note: Generate new data for `pub fn get_esmeralda_genesis_block()` and `fn get_esmeralda_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_esmeralda_genesis_block();
        check_block(Network::Esmeralda, &block, 1, 1);
    }

    #[test]
    fn igor_genesis_sanity_check() {
        let block = get_igor_genesis_block();
        check_block(Network::Igor, &block, 1, 1);
    }

    fn check_block(network: Network, block: &ChainBlock, expected_outputs: usize, expected_kernels: usize) {
        assert!(block.block().body.inputs().is_empty());
        assert_eq!(block.block().body.kernels().len(), expected_kernels);
        assert_eq!(block.block().body.outputs().len(), expected_outputs);

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
}
