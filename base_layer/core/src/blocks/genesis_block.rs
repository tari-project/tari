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
        PublicKey::from_hex("e487276e01039473094f492f20b707d3805399f799fc9d4ad77c648f2a5c0d18").unwrap(),
        PrivateKey::from_hex("544bea13bfa55f26f3cbdf986b0f7fff4a056316b252531896577d201f362704").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("b60caad21427512b85ccc38a455419481ff4f45253b611374c68d8e82533ee66").unwrap(),
        PublicKey::from_hex("76dfffcfa2c67c1b140ee04cf04fc271df193bdb4914e0b163b2507504f28b0f").unwrap(),
        PrivateKey::from_hex("81631e30518430fd864a010fdba2f08f63c63f974972de179e0c9d7bdfb0d000").unwrap(),
        PrivateKey::from_hex("be45ce74a588437453c4dc27ed5cc368c422cd6d86f4f180d66a6fd20a458d07").unwrap(),
        PrivateKey::from_hex("43227a2310450b9adcc9d3d0ebdfa8eb6f260e6d7a8e7e5ad9b8230916333104").unwrap(),
    );
    let extra = "Tokenized and connected";
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures::create_coinbase(360, Some(extra.as_bytes().to_vec())),
        Commitment::from_hex("002354d7fb5a0e173a7351ebd2ea169998b891542c2995c36621ab30c7e89004").unwrap(),
        BulletRangeProof::from_hex("01c24948f13b2dd0ebfb0a3e0697d882d05eef5744f7da5703d6525ffb9e34f556803ea04a2cbe040161ac805a0c3f598f3ad5744e313a175ac6fa2b52a80464499aa29a326ffbee4e3ce54d2f16528be30be1179ae6b8711e2602201d3193bc33ca5fd6812b94c16c50137c42935d856300313facdcc77493783522a7d945546a98f6a76a47c1546260e5b42d5a37be51f5d796568bc725e5aedc190bcbedc8506ed3a1b5e79490d62d10cf79e1440682988acc22d26a81ee5c591831652f967a4026f82b935ffed77e7f53636a9c91df6ccf292e97930da2e149d8c38e5c39301cff35fe5f81a1ee8ccd2f1897726a5530f47b8fc09f7ea63452d546a9fa1130d002041a6a8ac64f4239d8bcda752679df132b4c60a7ed14042ad5a8430eb4075e7d9eefcddb7155d1a8cd077ec567f49679567cdacdda7d6ccd63ca7c3f69232095ad1ff86338cf02a388a5614f563fac0d1cf5916663b2e5a1e1cf5d7bdb70a60231e9582ac23432bfc527daaba93806973fe0cf6261556fa75bc25e2bd653b4cf47727b54b163716be622b8e9e59e931425b154f98360c9d152c50856e34c0a1146ca2ba7f79720e4303b6e4f4f0b87f268b8c9ef36f0c8499fbd58f79129de3b5bb0c3e40fb5e21e1f01c03b3b2c01c973a142fe3385c7bc07c846c2a8002f50e43d544635ce42f031d046a0030b3209d42e4975988a54625eac0ecf6e009064acaeeba32ba641fa8b026891f518e7c5c2c9971e988064033841bbffb200fc6aecb9c24fb6f0ea6f382fa30fb01e55ca552ec38150218e0336ed6ce65407").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("bc94d6502fa7c1dde829ca5a8d2672784454dc6c42e4f62f5ce83eccbe63b038").unwrap(),
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
        Commitment::from_hex("02c0544ac33b2baba6da92a13a7b54b454edaacaa5b76eaf561aacf078142471").unwrap(),
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
            output_mr: FixedHash::from_hex("a00d1c95378f383cb53f2fda0658b680d15b530ffb251b72d5fdf0af386ae775").unwrap(),
            witness_mr: FixedHash::from_hex("945bad8a49c1c74501002ae227a9282a2148c793355c1c2517c07e5c15972d50")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("ad201fe393372af2313f5c55f491c9d4e864a7a796401453ef52eaf2cd042132").unwrap(),
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
        PublicKey::from_hex("c2e01e3c69a7b9f664dcf6069bf0ac5ddba7ec24f7b37628649355cabefdd66a").unwrap(),
        PrivateKey::from_hex("079988886b259373587a3d4f9abd6b4b7e3d483a6bc6fd67810291b9c5402d01").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("3a2704f98f7a2f29d01c4f313955616f70a2afd54cff6addeb0d696e8a4b5e76").unwrap(),
        PublicKey::from_hex("a25019606afab39eec37dca1b3ed928831116ab9761459cff91768377677f741").unwrap(),
        PrivateKey::from_hex("3f047693eab2e424bbd41fdeea7c99cdf6fdcdccb6d679e621f95dca24029603").unwrap(),
        PrivateKey::from_hex("58e625717af1fd2cf8634fda866fc13e0bd7b32dae0b85ed2597ec0191828a0f").unwrap(),
        PrivateKey::from_hex("71cec1b930693191db40c3f62e5544b0ecba43686d79d61f1537cbbfa8ffd90c").unwrap(),
    );
    let extra = "Mathematical proof that something happened";
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures::create_coinbase(360, Some(extra.as_bytes().to_vec())),
        Commitment::from_hex("dc06fb061ce57f7a785e970e3ac3f61be38cb18a094d6f56d46ff237aaf0dd29").unwrap(),
        BulletRangeProof::from_hex("01262ef222ceaa3ad01b29d4e2c860226c55f098da593ece40f2f288217ce3d02458bdd4c7e0ccfdf95860fa0495c93a8e4c19cbc267c00fe1820a0bc1b23fc85a4ab1dba8d11e3bf22fe2836545baabd73cc01c35d36e04d6cbe6e26a8ae9227274f27ee2003ad9b42a0014f2d39c9090c7c4815a9a67755d623bbbd21e66d80918e62b4687dcab350b1c10c12426e62dc7f739ac2bc6c0336ddac37478da2c5e2c7bc0202212780a89c5184f4ab19a2a2562c1b81c2f8433524907a00e075a19b02e537dd72eb8889ac6a0ccefb621ace5ddc4d0c3457cb089c7ed84ba1acb0c1e957f739c6aa6fb866082fa9ef139556b19356a3bc1f9181c55c49723dfbf62a4ddf90b37b8ae76e245a9eaba87ebe4cb238f1b172ce09483ee635b2acad910524ef872385bdc18c20f4c72d55249d46dd8097b2d36d710962c85b71afb412fceb9a3e6d53712885c280abcfed3ff1468226f6bbcb3e1a314753b4db08a9032985fff2b819053b5bc44e464e9557242b0a45bcf1db2df17bd18e4d3d89b15684408e0051aacc69785eed91c7112fbcd2a5daaa0d7fd7084a430e48a9921531120f0395139b2950fe03dc24ccde6683aae2f6496b399c8e14703f9d042939f4b9c9f6168cdc6e4f0ef696f207b13284d104664af825fda2d2efca4f47872ee3c1c3140655a2215c0b3aa6696e59d950be6a69832a82f959ac9b2bc94cb0101003620089f16577e5281152441def452891e2145f7972a0213d8972d74bf5625087dba65abdab63be8def59bf9ac50be3c94986dac852983ece8e9d2a201f72603").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("b6ec474ed5106296da315ae066739042597ba35094a6ae152a33bdd117661b2e").unwrap(),
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
        Commitment::from_hex("e43e1ebfd31a86ed27ed9167522b31108e4117806ce12a1ede06e02e67da5f1c").unwrap(),
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
            output_mr: FixedHash::from_hex("dbd70217d48910988e34a677528d49cec3c0cb091ba9edfb41f5320b565d314c").unwrap(),
            witness_mr: FixedHash::from_hex("381e281523a39da58158a47e901ed78c6a86da1b913c25102b92deb369e718eb")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("dfe55fd7137908009c05ec37e5c6e4b9d773e6cc1ca8f4c83c976ca118355c6b").unwrap(),
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
        PublicKey::from_hex("a66c29df39770371a56c3dce3decd49adf5b3d5319627a440bb5c48b62b2005b").unwrap(),
        PrivateKey::from_hex("9a0fc16882bd549214e68aaa4a5a47ea21e5e99a474c84c8bb64fc117637220d").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("beaddcc1459880fdab9f3c3a03abacbafe33a78c650ae3bb95685165f0e3db10").unwrap(),
        PublicKey::from_hex("7871b48a91ec49100ba7071585135dc82d6335b614abbccd3a5fbaff43d69d4c").unwrap(),
        PrivateKey::from_hex("e3adaefec54cefd6d931527bcda2d3720b41e8cc9cdbeee17f39ae1d10bb0e05").unwrap(),
        PrivateKey::from_hex("47589f298e0de0a6e48ffaaf72845ae04377c209fa418a57ac9340f696a2e803").unwrap(),
        PrivateKey::from_hex("40fe12bc2ed537c3244fa79071e47b028320dda2def354f47f2366913359e003").unwrap(),
    );
    let mut body = AggregateBody::new(
        vec![],
        vec![TransactionOutput::new_current_version(
            OutputFeatures {
                output_type: OutputType::Coinbase,
                maturity: 6,
                .. Default::default()
            },
            Commitment::from_hex(
                "6ac550a45e40f5f16beca09add1c08873b36035940daa995552ea3264a87b675",
            )
                .unwrap(),
            BulletRangeProof::from_hex("01363492c73b3da970ced46ae6c7591a86868c1a654be75c0ab7637c883663171b945311a5006031603bb8133f64256b8c57a4cae76d716fdb8db3ae290076b01a700df03607b69a1236107a740f2d42014a669ab4a50939ad47506a1630c3cb1aae556112bb0d1e556055e7059593a0fe4bc349bfddd107d0adb3258fc4968d3becc022c12e86fd7d46ce0db535ff34531fee4f441c297af40e400596c806486168facbde74897cae6985117a1fed3534d21929efbd58ff96b86e2a8893fc911260290c173f10cb5619f94e5d831371ca056e4caee38cbd25da045b94df077b6df005c2f1a607b3c445f2153bcc2e1474a7b10fff7526948ac353cdc81f79c90aa890e3767f7d259ba25ab462291c6fa30121ccde7d108d87a2d9566b2ee0322538ae434c76ab887f41ae2bd876b3ee749d34cdfbf62329154bac5801259e1c5516b4dbc54a1a246a8d80f551ca4680ef3fafb2b67157f7781e078403950c8d765ca63a7ee210c55846108d7fc2d2a768c994cd1124a2414cb6395ca74f749214c493c7abe36359ca5801b7ab20a8c3df799918f322d3edbfd7f88bfe57cc4e46f897259518bfbfdce39d583f6ab14655ba0d9f7b4740630b3dc41cc4e5b37f457621183253f8afe526b75a5f7b3e1a26c1bc1dd42d7d9bbd1e019f71d03caf15817065aeec5721d95e150714fae102b01875979a8a15c7128f39918affc708070b900aeba890d3118fad1bd9557591e74d6a49f82f5dd7311ace7b4b3d44bd0bddf4ccbb00c6c37c8c641aeaf1616a6bd56a5d455055ed004e26739eb4b9e700").unwrap(),
            // For genesis block: A default script can never be spent, intentionally
            script!(Nop),
            // Script offset never checked for coinbase, thus can use default
            PublicKey::from_hex("88c4d5dafaeb529d70d594cdb43a0fad2842a6cfdc47ac579b09b2e6478d3a4f").unwrap(),
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
                "9667d7e1ee479e44bded85b918ead8f1793c28706cb1ff8da864dd961a2d302a",
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
            output_mr: FixedHash::from_hex("77ff3d39cc9039db3d3306e37b02df75775c663c49a4df3b4b61876074c6e7c3").unwrap(),
            witness_mr: FixedHash::from_hex("35ca82655ce41604e1134f4af5c56d350c54640f40eaaf6703feccb132c2342d")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("c41e75a1e1160f61ffadcce354638704dd554f75dc3953a150d4f847a4e7ee21").unwrap(),
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
        PublicKey::from_hex("7aaa799353095d15ab06583470c4db0e31ba6bfe59a2e3677f221196595ddd6b").unwrap(),
        PrivateKey::from_hex("bed3f9b21969d09f1e5629fdbac0fef080adf765b756f948b055321ba1d2d70e").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("98666f86a7b7770d38b647034d1c2d715b43763e4b27a6a65cf15b02fd98ab18").unwrap(),
        PublicKey::from_hex("562c9aca618e1f8fcea4dd3b235cd0ec5c889afb71b5a0bf6423741130243d54").unwrap(),
        PrivateKey::from_hex("cdcecd5d349a280ea3b6e14d2cfa6e7dd397a93aaeb27dc68f0148716490a007").unwrap(),
        PrivateKey::from_hex("445f7b6021ca8629cbebaba27a760a88a2c585c959a6dc8999ea1188d37dbe05").unwrap(),
        PrivateKey::from_hex("7d028ce82ac03da42a53091625b34a4721309d7847df21957ae8326e75bebc07").unwrap(),
    );

    let extra = "Queues happen to other people";
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures::create_coinbase(6, Some(extra.as_bytes().to_vec())),
        Commitment::from_hex("f0c938bc737a151e3ae1bf348a3bc66412540bb1a23900dd57ac31980346883a").unwrap(),
        BulletRangeProof::from_hex("015cb801ab624235886946e2602031d3f29e09d7474be8069b0f0e6d4db86ca16924eca5837589d68f2d2f7aea3293dd584f4eb4f7e1e2489aba61e4aa852858195225a91c896b059c8941b1ca51007e9b53e5401e0e4467c229a1f4e7bb95c702f613ac2f31b63e58359b7982aa7411f9d783352958f84a01452f03f97bc8e33ec07bee5d34a8f9731c2a616f5907d446fc9d16a41eea1af93a379233d360f972ecfb8d37814c2f1bd802cfb4a90e71ac6e1e250c64e8a3b2cd371093b17c2758fca57563e56038666ecd6341ba2399831949ee30c69598ca3fbbde5ae43ca46860b228cde154dee1e55304ecb17ebd02748538a275dadb53e44c358c2856821b9820378b86f881f1aefc037813b7ff7b426790e6bb621a762a4eee24d8bd5b01f8c6b05e7b8637ebed1ef1d5490c678d4c20aab97647251a2d737897e2bbb02664c88674b356aaf7368e4672b026703c01e3d285e80dc72bc9a929cca568a737d03c2d51f7120716d12dd2025119b7f2b5d3489701ea3183acfa2dd57868e52bb650a9b479d371af9643ee1d96ac5796d5769e3d6c961551c60cf2d78d82b025ac3f3b048c8d076556bac3980d4842b5e01d2d42684841eee1fef807dda138360a9424027cc2667d58f2b5d30cf62aea94c4a74a0cd79236f126bf0547c7ec4fc80750e334a10e2925d94421fd6eeb17ce289afd71dcabc02c5e495ab714e5016cc8e513ae01e7bd10089f1ec2e77db854b1c076d1e78739fb4210ecc6c211012813cc433a6a8426da45a46b3dc3719c6d36c338aa622cff68ff5438b9c5ab02").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("eab7c49459e1ae57d58ddacbe0f4c8c6c44db179d0f33f0ab6c12113b27ab209").unwrap(),
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
        Commitment::from_hex("7a21c0bbac8bbc8fa1d5203689ad7bac46864069781185cdcd61cd2f4cdee626").unwrap(),
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
            output_mr: FixedHash::from_hex("28f37c501656939c50630c1f671b4a1e96efe7240e06fe64ff2cfb018448def2").unwrap(),
            witness_mr: FixedHash::from_hex("6766ce4df95b98263042accecdc8e56ccb567730c88785cf7d2b7bbb0bd33191")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("11c45631c382fb4fbabcb285a1da130fc10edebebafbf6425e12b2ada0fe550a").unwrap(),
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
        transactions::{transaction_components::transaction_output::batch_verify_range_proofs, CryptoFactories},
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
