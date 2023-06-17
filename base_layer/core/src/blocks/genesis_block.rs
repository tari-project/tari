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
            EncryptedData,
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

fn add_faucet_utxos_to_genesis_block(file: &str, block: &mut Block) {
    let mut utxos = Vec::new();
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
}

fn print_mr_values(block: &mut Block, print: bool) {
    if !print {
        return;
    }
    use std::convert::TryFrom;

    use croaring::Bitmap;

    use crate::{chain_storage::calculate_validator_node_mr, KernelMmr, MutableOutputMmr, WitnessMmr};

    let mut kernel_mmr = KernelMmr::new(Vec::new());
    for k in block.body.kernels() {
        println!("k: {}", k);
        kernel_mmr.push(k.hash().to_vec()).unwrap();
    }

    let mut witness_mmr = WitnessMmr::new(Vec::new());
    let mut output_mmr = MutableOutputMmr::new(Vec::new(), Bitmap::create()).unwrap();

    for o in block.body.outputs() {
        witness_mmr.push(o.witness_hash().to_vec()).unwrap();
        output_mmr.push(o.hash().to_vec()).unwrap();
    }
    let vn_mmr = calculate_validator_node_mr(&[]);

    block.header.kernel_mr = FixedHash::try_from(kernel_mmr.get_merkle_root().unwrap()).unwrap();
    block.header.witness_mr = FixedHash::try_from(witness_mmr.get_merkle_root().unwrap()).unwrap();
    block.header.output_mr = FixedHash::try_from(output_mmr.get_merkle_root().unwrap()).unwrap();
    block.header.validator_node_mr = FixedHash::try_from(vn_mmr).unwrap();
    println!();
    println!("kernel mr: {}", block.header.kernel_mr.to_hex());
    println!("witness mr: {}", block.header.witness_mr.to_hex());
    println!("output mr: {}", block.header.output_mr.to_hex());
    println!("vn mr: {}", block.header.validator_node_mr.to_hex());
}

pub fn get_stagenet_genesis_block() -> ChainBlock {
    let mut block = get_stagenet_genesis_block_raw();

    // Add faucet utxos - enable/disable as required
    let add_faucet_utxos = false;
    if add_faucet_utxos {
        // NB! Update 'consensus_constants.rs/pub fn igor()/ConsensusConstants {faucet_value: ?}' with total value
        // NB: `stagenet_genesis_sanity_check` must pass
        let file_contents = include_str!("faucets/esmeralda_faucet.json"); // TODO: Update when required
        add_faucet_utxos_to_genesis_block(file_contents, &mut block);
        // Enable print only if you need to generate new Merkle roots, then disable it again
        let print_values = false;
        print_mr_values(&mut block, print_values);

        // Hardcode the Merkle roots once they've been computed above
        block.header.kernel_mr = FixedHash::from_hex("TODO: Update when required").unwrap();
        block.header.witness_mr = FixedHash::from_hex("TODO: Update when required").unwrap();
        block.header.output_mr = FixedHash::from_hex("TODO: Update when required").unwrap();
        block.header.validator_node_mr = FixedHash::from_hex("TODO: Update when required").unwrap();
    }

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
    let extra = "Tokenized and connected";
    let excess_sig = Signature::new(
        PublicKey::from_hex("f28c11d6ed6a2e276de79714ec0175b07d2177c6c6228c6f6f335123a7a39b47").unwrap(),
        PrivateKey::from_hex("790453728c6ef87ecc266ceed0a0f00296488a50fbea5000c4da742c82e81a0b").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("1a0b40a10f9773d784d4cc6131097e9ed64615362ec45cecc007c244652bd300").unwrap(),
        PublicKey::from_hex("0286fe7855950b756b86514f9196c4dae76c4dd693cd82b9e09f7d079c1d6262").unwrap(),
        PrivateKey::from_hex("070e40a5f89e6187cc87b2941db22ba20ec93e8b901caf8622c1e693b3f23809").unwrap(),
        PrivateKey::from_hex("15a1690f3e78b13db0371a0c37234c25b98f40e6f732c6632b697e44cd90f50e").unwrap(),
        PrivateKey::from_hex("371aa7e360af28245e335a74936badd2261677055a13d72f42844bfb7ff6b508").unwrap(),
    );
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures::create_coinbase(360, Some(extra.as_bytes().to_vec())),
        Commitment::from_hex("daa15a31930cbc4b210beda5687d8492d92ae4cb04960d0dd196fac46a321f28").unwrap(),
        Some(BulletRangeProof::from_hex("01a80824c26f9078cdfb069ba056bec7a9a3fe1ee76a656151c1805e4c21fa531e1c7b94b29509e4b732ca17baf834a1db46baa6dd1b34eb13d54b3ab619c00a0390a30ca92b3a2669afc110f79f6ee9085e44ce56b1beb0e6a779e071b721021fbc3d1c3abcfbfdbfa60e517220415ca5fb746b02d579eceda538e737d1d9a2091cbb218e80f3fd0dcd3ba576f06ca1e2e7df418b3ab09e6f3d10a8480868d261c24bd27fea2dbcd21e5235e64601e985c31eee8f2b8768de310a29b5bc133d5bee105d5e72066e429af0ac2fab04c9b50af7b83994bedca5e39c7e7517c30a0d9473070b888aaa89a1e9177703fc3de96bf70b5d9144c8cc37abf2289f0c9e08f26d75328573cc3661b4ec4dbc0d6bd612d6b9d96278b978a4e5a4c38e760012d0d08d587c06cffa551d84feaca7995be4bbdf7f223a8b99198731bba9ac1444f43898c38489049fbcee90c14aa6e1752d61b68e8b974cc2f928f2a76d97ed570a894b59e30a61f7b222f031c1aa9953e7461ef965e4111149a6ec19f60b1866a6615fa483be75810bcd2eeec0b36ddc64e7e4b4c9ce42f216dd3b39900e7e271c113ac38374599050ded9ba2a5a34424628199aa154254e9a9298361a846408eeda0bf5047c5fa529862257259fbf1e5a9bf43c89057960483e8368e450036edd809b4d9e59a869310be894dabccdab7d956a99e9a113da22d4e1bfb80fd202e747b5dbf90c7bac9a70bdd3f69a1854a7d266c6f3e052923e10254ebd467f086b81e4c86f098ac3783e15f3eaa7b7e40970ea68c3e005e7ac329b8baac93703").unwrap()),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("2ab00cb5e477e4c7a0cbdd830f203a7e5d7e3ea8d015cf46f84e6c699ac96d0e").unwrap(),
        coinbase_meta_sig,
        // Covenant
        Covenant::default(),
        EncryptedData::default(),
        // Genesis blocks don't need to prove a minimum value
        MicroTari::zero(),
    );
    let kernel = TransactionKernel::new(
        TransactionKernelVersion::V0,
        KernelFeatures::COINBASE_KERNEL,
        MicroTari(0),
        0,
        Commitment::from_hex("2e812cf4e0d2e880577c7f22f39df9eb8ada3ed73150556dfc870d704a16bb4d").unwrap(),
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
            output_mr: FixedHash::from_hex("7e1ae76183d2bee553d047176ba8e2103a279621328e9b644bfc0d91b0434de2").unwrap(),
            witness_mr: FixedHash::from_hex("b71f8eadc3556245e3673d685bbeafa55b378754e7dc7af527fd35594e5c3ac2")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("4f7c30b3fbb3fc3cfc9c544753d9766bfed0aa0ba4d2f8c4792d8565d051ca48").unwrap(),
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

pub fn get_nextnet_genesis_block() -> ChainBlock {
    let mut block = get_nextnet_genesis_block_raw();

    // Add faucet utxos - enable/disable as required
    let add_faucet_utxos = false;
    if add_faucet_utxos {
        // NB! Update 'consensus_constants.rs/pub fn igor()/ConsensusConstants {faucet_value: ?}' with total value
        // NB: `nextnet_genesis_sanity_check` must pass
        let file_contents = include_str!("faucets/esmeralda_faucet.json"); // TODO: Update when required
        add_faucet_utxos_to_genesis_block(file_contents, &mut block);
        // Enable print only if you need to generate new Merkle roots, then disable it again
        let print_values = false;
        print_mr_values(&mut block, print_values);

        // Hardcode the Merkle roots once they've been computed above
        block.header.kernel_mr = FixedHash::from_hex("TODO: Update when required").unwrap();
        block.header.witness_mr = FixedHash::from_hex("TODO: Update when required").unwrap();
        block.header.output_mr = FixedHash::from_hex("TODO: Update when required").unwrap();
        block.header.validator_node_mr = FixedHash::from_hex("TODO: Update when required").unwrap();
    }

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
    let extra = "Mathematical proof that something happened";
    let excess_sig = Signature::new(
        PublicKey::from_hex("a077240803d68e34b234ba72faffa2c6945ec354d95ec867cda47803f97b0020").unwrap(),
        PrivateKey::from_hex("8f1ad8d5cbdd593a2e8b34db7df8a0306cbd7c66a87fff399e9180f94a0b8600").unwrap(),
    );
    let coinbase_meta_sig = CommitmentAndPublicKeySignature::new(
        Commitment::from_hex("b8d0c8708cbd6c0d7bb305aaed1885a1f80f7aefd5220de27f8f2531f3256a5c").unwrap(),
        PublicKey::from_hex("808481273eec485d2fbc81f52e0056ff39a6808addce49a73bf1cfc764adf549").unwrap(),
        PrivateKey::from_hex("defc37b9c7ccae0a2492ea1a447df0a52af07bf12bf441eecf7c29b764aaf008").unwrap(),
        PrivateKey::from_hex("038d021e853270d297fd0101c9c45d71427716722e7d9873e3ecbbb253bf6301").unwrap(),
        PrivateKey::from_hex("b88f8c6cc795d435a309ca6918459ee9b63a881666b1ba312a697c6980f2980b").unwrap(),
    );
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures::create_coinbase(360, Some(extra.as_bytes().to_vec())),
        Commitment::from_hex("b24439eb7c41d94e871f80de8acf4b5fee0a9f3b9ccd22315c2052de7fab3659").unwrap(),
        Some(BulletRangeProof::from_hex("018c9edb0a5d121d93f1f4b98e1a70a53842572350e987117b6b61f80795412d039c19e95433220d9552bb99120edbedfb762eeb850362576a340f2ac2de950647922dcb8c1d6b44809aa447428b849a3b55fca603604c3bc1f50a124eb33d5f1ee601ba7c34deae9624fa2124e65535cac54e2bd70f6a56276536ef3f7989a33754a45023cdc7a7409b5a90ad3f79d95eee53f0109863b00b91fe1a61a1152511541b323dbfabf543b5fe2aab8475580f7c790c231544aa2d8b07a20b12a77003526d36e8bd57948fd08cc2dc9f17dcfcb22798262b8f55e1020a03892b3b522008fb01489ea7d65dada109864faa4fc3a12853d3d58d1889361af6c6641b317db68ff4e20cf1d7c6d2b0d4ad21aaac9f6f9d8688fb6ca3e0767dbfb62daec94cf63550c11ace2073e546b25ae5fd24a1623ae1fe738b8b7933e4b69cf8280f0b222da9f3e709d071e525f7b17ec0a68546fb5d9ed84ec4ad36915af05abb3545425014d25559fa3dc58d0b50efb5a20285bc80dee8ce108a03777b21fe04572e5caef6f9f4479b1765f45288f7d1b4aedc747a1921c834a4d7c53671c222ea394438955d2f11957d9e81c2b62660350b012e6d8a06a7968e225bc625fd6828292c059832cdce869b193f8370fe4c6250b6f58ff5d43bce7b129ab65974fa4c256c0e24f53947b06bf921f901dc9ef4d910c03b85be85d6f675495b8acd19cc0bf98f81e41eb3aa1b3e6edf45a0916491979a5242709a4176c72f53b4cd2cc700f52bc519d3cd4af86fd63a64a027b5e4c0ad642bce0d00ded542fc6c49608f0d").unwrap()),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("528e71d34210aad1bbc32eaeda40b147fda5c1efcc620fe64983606937276d29").unwrap(),
        coinbase_meta_sig,
        // Covenant
        Covenant::default(),
        EncryptedData::default(),
        // Genesis blocks don't need to prove a minimum value
        MicroTari::zero(),
    );
    let kernel = TransactionKernel::new(
        TransactionKernelVersion::V0,
        KernelFeatures::COINBASE_KERNEL,
        MicroTari(0),
        0,
        Commitment::from_hex("b276b36f13263126c226f2dc19f825284ba5d5218ea9da8a3b7015eff1b5c522").unwrap(),
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
            output_mr: FixedHash::from_hex("2d7b1c9794a55c9d183668710c915e5a8fe051b8867a2043b37c94000289c2a3").unwrap(),
            witness_mr: FixedHash::from_hex("1247e8f2fbfaea797b80893a7df918ccd05f1b38559665f5e432ab303e93ca77")
                .unwrap(),
            output_mmr_size: 1,
            kernel_mr: FixedHash::from_hex("2786456622eda4fc857115c4d77b895ed8115b054353af9b382abe9c204e9ebb").unwrap(),
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

pub fn get_mainnet_genesis_block() -> ChainBlock {
    unimplemented!()
}

pub fn get_igor_genesis_block() -> ChainBlock {
    // lets get the block
    let mut block = get_igor_genesis_block_raw();

    // Add faucet utxos - enable/disable as required
    let add_faucet_utxos = true;
    if add_faucet_utxos {
        // NB! Update 'consensus_constants.rs/pub fn igor()/ConsensusConstants {faucet_value: ?}' with total value
        // NB: `igor_genesis_sanity_check` must pass
        let file_contents = include_str!("faucets/igor_faucet.json");
        add_faucet_utxos_to_genesis_block(file_contents, &mut block);
        // Enable print only if you need to generate new Merkle roots, then disable it again
        let print_values = false;
        print_mr_values(&mut block, print_values);

        // Hardcode the Merkle roots once they've been computed above
        block.header.kernel_mr =
            FixedHash::from_hex("ad494884dabf1337a678625613d016d55c0d6a968c86a5ed57fd3099c207368b").unwrap();
        block.header.witness_mr =
            FixedHash::from_hex("a2d553743332a6af82cf40ed4a0faad89156ae62b181672634e2b90c3c759403").unwrap();
        block.header.output_mr =
            FixedHash::from_hex("87bf1cdcca897a3b2f77ab1051a875de5a4ceaebd740bd664ba94fe6dd6e1c00").unwrap();
        block.header.validator_node_mr =
            FixedHash::from_hex("e1d55f91ecc7e435080ac2641280516a355a5ecbe231158987da217b5af30047").unwrap();
    }

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

    let mut body = AggregateBody::new(vec![], vec![], vec![]);
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("15 Jun 2023 14:00:00 +0200").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: timestamp.into(),
            output_mr: FixedHash::from_hex("0000000000000000000000000000000000000000000000000000000000000000").unwrap(),
            witness_mr: FixedHash::from_hex("0000000000000000000000000000000000000000000000000000000000000000")
                .unwrap(),
            output_mmr_size: 0,
            kernel_mr: FixedHash::from_hex("0000000000000000000000000000000000000000000000000000000000000000").unwrap(),
            kernel_mmr_size: 0,
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
    // lets get the block
    let mut block = get_esmeralda_genesis_block_raw();

    // Add faucet utxos - enable/disable as required
    let add_faucet_utxos = true;
    if add_faucet_utxos {
        // NB! Update 'consensus_constants.rs/pub fn esmeralda()/ConsensusConstants {faucet_value: ?}' with total value
        // NB: `esmeralda_genesis_sanity_check` must pass
        let file_contents = include_str!("faucets/esmeralda_faucet.json");
        add_faucet_utxos_to_genesis_block(file_contents, &mut block);
        // Enable print only if you need to generate new Merkle roots, then disable it again
        let print_values = false;
        print_mr_values(&mut block, print_values);

        // Hardcode the Merkle roots once they've been computed above
        block.header.kernel_mr =
            FixedHash::from_hex("8c93eba80af538d89004df33e6d9f52fbd542f2a0e56887bdf1e0b8397e515a3").unwrap();
        block.header.witness_mr =
            FixedHash::from_hex("d45b5ce1ff99cb4ccdd38747d4dca7ac1d702224064b79f494e5430422bd7a31").unwrap();
        block.header.output_mr =
            FixedHash::from_hex("961bf351744baf355c5522f2ae7890f9c1588e37c694b53742abea3c3b2d5155").unwrap();
        block.header.validator_node_mr =
            FixedHash::from_hex("e1d55f91ecc7e435080ac2641280516a355a5ecbe231158987da217b5af30047").unwrap();
    }

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
    let mut body = AggregateBody::new(vec![], vec![], vec![]);
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("15 Jun 2023 14:00:00 +0200").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: timestamp.into(),
            output_mr: FixedHash::from_hex("0000000000000000000000000000000000000000000000000000000000000000").unwrap(),
            witness_mr: FixedHash::from_hex("0000000000000000000000000000000000000000000000000000000000000000")
                .unwrap(),
            output_mmr_size: 0,
            kernel_mr: FixedHash::from_hex("0000000000000000000000000000000000000000000000000000000000000000").unwrap(),
            kernel_mmr_size: 0,
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
    use croaring::Bitmap;
    use tari_common_types::{epoch::VnEpoch, types::Commitment};
    use tari_utilities::ByteArray;

    use super::*;
    use crate::{
        chain_storage::calculate_validator_node_mr,
        consensus::{ConsensusManager, ConsensusManagerBuilder},
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
        check_block(Network::Esmeralda, &block, 4965, 1);
    }

    #[test]
    fn igor_genesis_sanity_check() {
        // Note: Generate new data for `pub fn get_igor_genesis_block()` and `fn get_igor_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_igor_genesis_block();
        check_block(Network::Igor, &block, 5526, 1);
    }

    fn check_block(network: Network, block: &ChainBlock, expected_outputs: usize, expected_kernels: usize) {
        assert!(block.block().body.inputs().is_empty());
        assert_eq!(block.block().body.kernels().len(), expected_kernels);
        assert_eq!(block.block().body.outputs().len(), expected_outputs);

        let factories = CryptoFactories::default();
        let consensus_manager: ConsensusManager = ConsensusManagerBuilder::new(network).build();
        let some_output_is_coinbase = block.block().body.outputs().iter().any(|o| o.is_coinbase());
        if consensus_manager.have_genesis_coinbase() {
            assert!(some_output_is_coinbase);
        } else {
            assert!(!some_output_is_coinbase);
        }
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
        let some_kernel_contains_coinbase_features = block
            .block()
            .body
            .kernels()
            .iter()
            .any(|k| k.features.contains(KernelFeatures::COINBASE_KERNEL));
        if consensus_manager.have_genesis_coinbase() {
            assert!(some_kernel_contains_coinbase_features);
        } else {
            assert!(!some_kernel_contains_coinbase_features);
        }

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
