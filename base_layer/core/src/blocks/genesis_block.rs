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
        PublicKey::from_hex("c6792ec52679002e7f73f2c3aee29d73eceb1e957a0d0d2d5b21f41250248557").unwrap(),
        PrivateKey::from_hex("0b6f9a5b26223790b6063705ffc6e99bd880e2279f0f1412c3a8acc8e564df01").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("1e7e738325f215a3b77b66675b2734ea7e0a7580e3e551383a4c0df35e965a17").unwrap(),
        PublicKey::from_hex("bc8613b66ef92b2ab59678720c4568de3ae62cc6bd7f62a2f3c3c201eb639c0b").unwrap(),
        PrivateKey::from_hex("c2bdc95822edcd9e25595e8b3c88f5328505c0c800f9ba051f23959305145a04").unwrap(),
        PrivateKey::from_hex("5de74e5e5cca8a61c3573416afd62d13d8275ad467f6e154140ab8ba34773a07").unwrap(),
        PrivateKey::from_hex("1a4bfc44d24cb949637ec65377e6570aaae28b5d2baef0fbe56f69ef30028c08").unwrap(),
    );
    let extra = "Tokenized and connected";
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures::create_coinbase(360, Some(extra.as_bytes().to_vec())),
        Commitment::from_hex("424a61d31d8baf69dd73bdcdc38b6871a0775005da0022974deba78b1fe36b05").unwrap(),
        BulletRangeProof::from_hex("01209cb650398bec0617f0d839123a5b3a9ad0ee164719df28a8357fd0a3932e76121b38442f9bfd0077e006737edc8fe0ae5fb4b9c11f0d4b69ebbad76a0c1313fefe0c63fb9209c8dfe3ccde6772f400ea8e6a59b4599c6ac5b18451145d6e1cf6986d4f143cfb996854ecdf07d2066da68203f56940988ed143a759b9026934487fca88adff6c52a60c184ef92729503f63f6e06d92f9862bbab8693bcfa6681e17ad7af1c66d728e908bf77f7bd07b8692f8e6c2fe006678df9d15f7d5774050410c0204a1e1ebcf5f707f6f474f0c453b1817c3c1a4d100916a148f1aff5dfa41e8e8b236bc37f8457bced78d23e219af7efcc18f95529f38153c2969f217e89edcfdfa0536db6013241d3699dfdf044a518bc7c874b3cf4a21f11a32201206ed486adbf15a9f2ac06b4a10162052de935c3962f0bbf88985177ce40273721cc72ea4e4da21e99a5c407bb49569feefaf945bb5257621cc38027c6aa54f26a075c71d0ba92b092423fbb8e4c490db677a18faebd3b43c03750e5b664d255aa68042a3ea5434ba535e36d5442af61dd5e3e5cf6be15f424fd79cb7a19ef97dd407fc9cc8b18989e048af2ff5d7b4164ae3f179ce8bcb49a2e282fa240cda7a2e954f6e3736144cf10dda9dbe3f67b7f65c6fcbd14bdec5beacf95e4ab88d107b5053e07e313fa9fe404ed5940929f46734115a6ec913a00f507f9e7f4e5a038d624b791fb31e25b18df6c08cab7250ba1b2ff57717b81249fdac1d9d59480a10499d4a7c0aa2775a5221b5af713611e37de90abd1330407761a5b5af7e750d").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("c6eab0ab955ed7de005fe005f5cfc5100c41cced9a320a0d35793970cfee2b6d").unwrap(),
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
        Commitment::from_hex("f60e1d339fe3cff0be827b274fd03ffe5abdef9c02d3c4af0636819d2cfab519").unwrap(),
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
            output_mr: FixedHash::from_hex("1fa7fbf2ba1de4d57329c0052582cab968686a2df28ffa20ae36b3c549215c83").unwrap(),
            witness_mr: FixedHash::from_hex("24c685e2daa6d438dfa6df674ebb8d36f0572438e64cf3c36767072bba436839")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("efe013a63663c8c9eeff0cf0c8235d2ec224f11609a2eb549be294474b2b417c").unwrap(),
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
        PublicKey::from_hex("ae2a7fbad3dc4499bffeb8b343290a6f090a66ad840a4720c7c728633ad0a830").unwrap(),
        PrivateKey::from_hex("2986978cf82ac9cd7b41f2b7e82e440603d9e22e42bc33ce1bb761ee6e43270c").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("8ea2b2e94c992f053491be4b000ef286202b1cb656cb90245f5a029702d52814").unwrap(),
        PublicKey::from_hex("987a827cd2e9e685a211e21cd58cd0713b98f64dfc51a275e6255cef116aad47").unwrap(),
        PrivateKey::from_hex("91814da5122e1fbcd39cf746d893acbb4acf3bee647527c4135ce2d47cfec20d").unwrap(),
        PrivateKey::from_hex("9ffe9506bf50d66e348581447608880367215cea638264a6b0957ce53bee0106").unwrap(),
        PrivateKey::from_hex("953a49690cd92e75c69d02c66cbb0470548cbc163759fd9a72e36ce3d224ae01").unwrap(),
    );
    let extra = "Mathematical proof that something happened";
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures::create_coinbase(360, Some(extra.as_bytes().to_vec())),
        Commitment::from_hex("14eb303345367eb5030d2db37cc85285f1ed5b4e929212dcd51b7c944d11e802").unwrap(),
        BulletRangeProof::from_hex("0112e733462fbd423e959066f35a63154bb4a418168d7050c02f6aa870d13fa0058270f430c63114020128401d35f0d01c9024706d640cc71efc125b2142c5c9245820d0891165995e04f359924ae8ff7398ddf4562bc6f157bc0d7d65dfca91087abf4e801d406de64c0bb686855ae5a6947e9850f718a93f1ade69d8e4429e1cca919560d7e9b29d6d4778eccb5b67056e7e4211e1dc64be3e240a4901756e7cba2c0e4cb15b63c045cd891340ee01ef05e83659c56ca87b922144e2a24bd163b845a18ad169b87c45cc0bfc5859a796b7de92f6ce0c4c30f578ab15898a6c77de84409871331cbbbca82af8092d6e505deede3f1a97f70c48baa5bff0ee2c75fc8e787d4a9d416dcbf703b5085ffb304288740042442dd1452ac7ab3d7ac36f3ef43a4ca62f8452e58633b2974cce0c3c741b062a06d969249e95eff94a0f163c493cc7586648d3ce239066a741e8f57780a1b03a40bc2788a247e685d8475b58ec29e0184c369057c779fdbe0956ebaca092b7d4244fe4d2ad4a382eb7a12dea6c86f36bd6b670d2fa645b965e71938f233719819ebc7f5aab82acf5c9395a7046a218461862dbde88ce60ddfe624e3ebf4de8115b3319a85310c730b02238c21dc8630ae4713a5c2d8ce162caa1623c8026d2811a4d452d4b18fab21b635d97de6e9ee7384d81a3a989ae118adeab51443b54ddc679de9c856215f018e8009847b60c10d886fea42283d719f07e6ab6b9a76c3bb31354ad8657d9a24ae90c26fc0a8b296525e68aa209911f629f033eb64cfe854a03991ce04a0f8668e10b").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("90b8e8e1daaff378e186b1ffd9af9a92b0b648b309a43316af8728c0b2d66778").unwrap(),
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
        Commitment::from_hex("0073ba987f8517aace96808834567202d16c5775899388193ba270990f1aee52").unwrap(),
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
            output_mr: FixedHash::from_hex("e57a52c71e813d49d61e766e4f4a99eacbfb2fdd0165def3fa1e6824428016f9").unwrap(),
            witness_mr: FixedHash::from_hex("f79fd613a5663b58b15cc1a111ccfc59ce3a51e1914737992d2b38fbcda98ee1")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("e483a8f3fcee0dd5932150d920f7220cf3cf05c4af44b30d2db20c4f434ce599").unwrap(),
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
        PublicKey::from_hex("ca53d0649b4f49ce13c5aa25759317a1530794c2021418e4b6f66b520d394c6e").unwrap(),
        PrivateKey::from_hex("d64b5758b336a63dd6419afe9793595d5d6071ddf65da9c67d00d280acda1b06").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("0e05ae72616e0c084e87316f4a79c20c68e6ba7962593c5adfcce88ef6d3c664").unwrap(),
        PublicKey::from_hex("a4a8e9c3b3bfcad508f234bb92cd4bb4687f313fb2e809f1e76b7266c6bf1473").unwrap(),
        PrivateKey::from_hex("22666d7e9f4a9f02a422c5c85c34b21238420b2715e69bf34321dd9fc5fa6b0e").unwrap(),
        PrivateKey::from_hex("1c91624face419d2d384f4702c5cc5c88cbc6ad9b9deec6b80be74592fff9f0f").unwrap(),
        PrivateKey::from_hex("f7a80f871d69f4346e5f111ec156ed325f4c64512f3aecfb4b7f5625cbb0a103").unwrap(),
    );
    let extra = "Hello, Igor";
    let mut body = AggregateBody::new(
        vec![],
        vec![TransactionOutput::new_current_version(
            OutputFeatures::create_coinbase(6, Some(extra.as_bytes().to_vec())),
            Commitment::from_hex(
                "aa07de9710522be8505b94e95770251a9887601a7fe59b6c6aed58d17be21239",
            )
                .unwrap(),
            BulletRangeProof::from_hex("01e8b495608956bf4108ce44cf0b2de030d20a884f332014067dbe4bfa9bad5068f4854c1397d39493e80078ae7c4984718ca42ec043d0622b4dad6ca08daace71422fe5bd215aeaa0836e9ea8b9e03eb7557cb2948e8d337ad8778e6ea9e88e7d3c8ec9f4b4435acbbedea4e5a9209bc5f20c6a245516b7cd6cca5dbcf62601752ed428f74449f7031680b69c9da3b23315fdfa90753e9cefcef2427fecb1701d46719ac15bf06729f16251a484676c24d4e17789dab08285397ecd5b78f9094d4a60022e7711c97cadd13a546de2b150839e6d9d6314d1259ee02a9d0762a8292857aa5eaeec3a8b16c1696263d1feb05435a8430c7d7258f421be48b12934368cde5c8da6abb59591658f7927149e0bdc8c5e69e2a14ef9f7883b72a704ba038c18f489d09a29148fc460f98d69b715a8028c14cff5f872eb08464e24a68041948436926ac41e01bfa537b10305acf79978bc88c9700cddc25e95620b5e9f04442c04e1bd427dfdd6dd8832930e527e071ac282f680a1363a6b11dafd5f2703e0fa3f9d90fa514ea5b4a4d8a76942c9a489a28be5a18db0a2933a4353667e45c6fc21917dce8d24656f9730bed4cba37d677c4aa4b3908e5f10c89fd9bdf036ee946027b9c503b971b4bdf454a450b03ddcd48c462c6801aa7e113c087bce30d34d17a52bd8af792450ec08487d0d0d24a076e684d1ed9d2b684f59a41e510921b3397c593d155b61968ab60579a84b549a3f8eeca0f25c0e38e72342e7950a298c35c07513e44a1f9856118e00d3973e35e723894320c23b4d3b4f13255009").unwrap(),
            // For genesis block: A default script can never be spent, intentionally
            script!(Nop),
            // Script offset never checked for coinbase, thus can use default
            PublicKey::from_hex("08725a7cb79a8ef1fae7b3c51e1a196bfe71397170c7d3873326523dcb19df35").unwrap(),
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
                "7a45da0c1b48f749a28f43ffdce648f2c5d4067855f7f90b2446f89c6d0bcb04",
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
            output_mr: FixedHash::from_hex("0162f1842dbfd6eab11777192b172e38742740b6cb24cb6d4fd8a83670ab4b90").unwrap(),
            witness_mr: FixedHash::from_hex("c0756d0d5ecfde4da4a2a4cefc630868ccc766791381ee5daf3fbecb16f04d65")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("3da75454a57a84eb1c46ab019fa68c4ea89e882347bbc4ff9ddb8ab4a4640c31").unwrap(),
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
        PublicKey::from_hex("c42ca89794d5a0595d84e357225dbddbbdd18bd9686b77018bd94c1e3b665104").unwrap(),
        PrivateKey::from_hex("8cd51e8aa90df57645c3c681bddc35f63589e26a5e19555676e6a8fdff01d50d").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("b83b50fd8382c3eff983757439b0790c2b9e0d43d9c3a1cea411ba3052e9c419").unwrap(),
        PublicKey::from_hex("8acb6d2786c6aaa4bb1823e275a1ce6f08764bc5da89ac57893d7ed0f1d6b638").unwrap(),
        PrivateKey::from_hex("56c629927cbc688c57904e832274a82b695598df6dd6568880e7cf0f6582f40c").unwrap(),
        PrivateKey::from_hex("9df06fc573595f82ce943331e7eb573e74fb936efe903ba691dcc705b333c502").unwrap(),
        PrivateKey::from_hex("58be250059de4df41f3622018dd175b4621a6e03468b86588ab6d2bf3abec008").unwrap(),
    );

    let extra = "Queues happen to other people";
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures::create_coinbase(6, Some(extra.as_bytes().to_vec())),
        Commitment::from_hex("ae98d49d8535094c19933d61b73ebc366ada515b9a5f4432b48af74c2041bd08").unwrap(),
        BulletRangeProof::from_hex("0190e7ea4ea05181577b3ed8fb20872ae592dcc1ede0164fd31b78387ec41c0913aa39232fd2bdb7ccbc0dfe386573ad4f8ecc8918d017b724636a81494e2989483c5c19a997fcd5767496d11659d7551bb8d8b9122cae706481e2d3d955f4fc27aa96549e0151c63c382982f626f6e789d4e1b8f6a573057a7028efdeb70f214b22f81edf5268b8efa9eadad51d557c2efd20e5ea3c22f4d7633c383372137b1bb43ffa4205dd244870b3fb2e95fe6d66085b5851047d39ee1c60314242bf1e7546402287d114f5d3a804af1b77e744bba057bab9ecc11d8e1882fa47c4914a39dc2e5362e4e7ef75f139cd8358d9efb3f9a715466e127ee9e2d6b1116e8a474beea7dc8b7e4f1e898376e83fcda7b10a0cc3ab4e96463cc8aa950055fbf28415ee62e7e1d66964c75f8f5df5978444cc86a0f6eabfcf8c3f5a4257a0eabd652680f3b7ff8010f6749260b842494cd253a0c2aab6cf9e204fe37d6e56ed89bf14e8a88237a9b3c597d6250f81eb63a755cd0534d32e1e539382d5d32f6fddee59da0c2e9840ed119711ad2bcc3b85f76eea12c1e9254a5360246630e34c1d484ce08357e64459b7d70649c4ea57ce9fefddab35309c9de9526f98434dad7c8e0f9c882aa012946324d7254bf7f338bc6aa51fae602dd84db8831a043302ed447ee643c707705df95623f09bab776cd386d82681eb1ad09dfc18cbb37f40d9c20c0c9145ef6071aaf90f93028b10507d4045db3e74ba74a31da559fb52fa08f1036bcb1bfd6e5d43fd790646a3e5614a36ad9d6c61f25fa7a3e44d8347a06f640d").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("74832eb9ac1a1c477360e5e075871ccdecf2bc4e20c0a676985a9ab14cce161d").unwrap(),
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
        Commitment::from_hex("d286dc52e40b89e9a1ae6a8e6f89f4dea6ebc7e01fc042add1a09b9b9e9f9e70").unwrap(),
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
            output_mr: FixedHash::from_hex("2c16f7bc19ed1fdab23b191dc1911b4e67f4df8c1ad4c2cbdfbe8038154f08a6").unwrap(),
            witness_mr: FixedHash::from_hex("4527ef21891afdb11b28afdc7d2dcbca6c13101b0a2e9d4224856bfa81264377")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("281e60f775745b930496ab4b4a67462ab3e68c050752524637c9045a47d3fb68").unwrap(),
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
