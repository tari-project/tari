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
    use Network::{Dibbler, Esmeralda, Igor, LocalNet, MainNet, Ridcully, StageNet, Stibbons, Weatherwax};
    match network {
        MainNet => get_mainnet_genesis_block(),
        StageNet => get_stagenet_genesis_block(),
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
    // Note: Use print_new_genesis_block_stagenet in core/tests/helpers/block_builders.rs to generate the required
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
        PublicKey::from_hex("da609d92fc846f204be4241d89070ab86fc4e553b2385416930cf528292ac508").unwrap(),
        PrivateKey::from_hex("70a6ca2a91b4e5d2c2001ddf77024f0c41f3ad1c1f481cd5d6b99614d3848004").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("1eff5625b089f02b1581cf8425f79810eb4a126fe4168bb8a5f95bbe49c28f42").unwrap(),
        PublicKey::from_hex("3c5c492002a4d91248f14ec85f4132d148f13383b228119ab0c436943542811e").unwrap(),
        PrivateKey::from_hex("4d5e19876b797234aef50de91436f56ea4be96b9169c4250740430e471f7670e").unwrap(),
        PrivateKey::from_hex("dfbe8a47a7614e42d6825d12ba068c26bfecc23a7017f96a94eb37f576531d07").unwrap(),
        PrivateKey::from_hex("523ef297343fe3a1fc69a422da85dcb9144768be2debc8d65f3cbb20d673510d").unwrap(),
    );
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures::create_coinbase(6, None),
        Commitment::from_hex("88f274d57be82db8fc4d3a2a246e31333df49c3b5cc042dcde5b3a9cae704963").unwrap(),
        BulletRangeProof::from_hex("01a225006336a27b35ed120cc25865278d44ee0b95e5fddaed9ab9253d32c8383cb4bb0458a1fca5e43308a1a6c8353dee1fb203c08bd35fb761d8b5d3edcb222cb8ba2bb19da42156a6cbb6e52c4ff9bafebe8651b0f747f0492c9814f5b8b534708eb83e4ece000135a3d9d724c57030a08172b439b85f0787f032f42accc433bc25c48449790dd94c447e2493034e834be5d76b3a2c571cd84a6dca0ed80359f6c8cd8c86a81e365f6b61fc9f37b70a96f046ebf05b3162e1ce31c9d39fee6f88a7804e98edb2c5bb985c43c54052b06b8276a9e7a56a75a5252f6bf53c5c69584e4c7348b00493fef802aae382533ca37dfa6081bb58a975ca21fa178d7b6b9e8abaa923270306c2e21a5c4ea6e8a9ee601be86f767e93ee0d30333c15b2799e5fba2c86a6b3a4a1d23801b4a9bbfcd2d888465b208a29e7f206986065471a2c86b69d889fd0c189e0217b668587270beb80cae15d5e5d0aa3558838506b29baabc9bd7b3aa51bde16ff9f9c7685c4921e1f4779e73c354a9286d0c78fa7306eceb6a089f8e27d9f250ecd87b55379e0f20ed5f26ee70b1d593aff3c30cd2ede759fba64dc7f307bee9a91c4085ee49ec40315d90d0d82998c084c5c177654ae09cb1b9b3bc302c6e4516dcc5eb965086a8edab1f019c09d2c194a5b148968eef94a5303e6aa0a6711d3ad186da96f4e9a502a6ed3e2545fcb6002e0f06604aa0aaf0681b272d5072fc2143ad4f2330e3883b68bf9f60fa0754b6e9992940646e422dac53da12ef0db5c684366f2a3ccbe34903c3e6a9660cfa227ee0e190a").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("0ad98e0eab0d736a52b607c1136bed6c4c0a6b47331b6dee22fadf8b6fa41011").unwrap(),
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
        Commitment::from_hex("5a29c8dc112fc66e5c564ad56d6b7ab14c72150da138e12140059a58bcea1708").unwrap(),
        excess_sig,
        None,
    );
    let mut body = AggregateBody::new(vec![], vec![coinbase], vec![kernel]);
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
            output_mr: FixedHash::from_hex("e1fd2948352b3ad9e22dafc1b6646e712726e50245ff19cec5b9bf16e0800cd6").unwrap(),
            witness_mr: FixedHash::from_hex("82a3355bcf36cc540c2dea003d32fad17c4295ba45fbfc7584dc3837107b4422")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("7a284d1212fc20ea144c7d04d29e81e7e841730ec8050181dc85418ae236682a").unwrap(),
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
        assert_eq!(
            calculate_validator_node_mr(&vn_nodes).unwrap(),
            block.header().validator_node_mr,
        );

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
