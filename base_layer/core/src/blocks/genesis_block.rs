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
    signatures::CommitmentSignature,
    tari_utilities::{hash::Hashable, hex::*},
};
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
            OutputFeaturesVersion,
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
    use Network::{Dibbler, Esmeralda, Igor, LocalNet, MainNet, Ridcully, Stibbons, Weatherwax};
    match network {
        MainNet => get_mainnet_genesis_block(),
        Igor => get_igor_genesis_block(),
        Esmeralda => get_esmeralda_genesis_block(),
        LocalNet => get_esmeralda_genesis_block(),
        Dibbler => unimplemented!("Dibbler is longer supported"),
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

    // Use this code to generate the correct header mr fields for igor if the gen block changes.

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
        PublicKey::from_hex("90e9cd591ed69e325fbdf0b36fd4aeab8b1917542ff94c32b0e38896827d1943").unwrap(),
        PrivateKey::from_hex("cbf10a5453e09990ecb22a55935acf2d6ad10237dde94fca56495ae2fead110b").unwrap(),
    );
    let coinbase_meta_sig = CommitmentSignature::new(
        Commitment::from_hex("1abc2df9fe3c6a3e68ef0e7219dd3dd617646e67857c39f4fcef7ee044191272").unwrap(),
        PrivateKey::from_hex("fdbef6a0e7560d555c0c5cef216cae1549dbb43c9e0747ec42551f5efb318105").unwrap(),
        PrivateKey::from_hex("1d317a93fd9aaad4ba172b0be0e7b2676287e68fb47cd0b8e6613506253be601").unwrap(),
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
                "1805b11acc21796deeccecde810583fcf2d850bea46bce6beab6a76459eeeb26",
            )
                .unwrap(),
            BulletRangeProof::from_hex("01369dfde898b40805d6a8964f3f8aff6d1bb17beaf71da91793fe097c0ee8b03930abd4672a4a90dd3610110d6e152c5105de7fdc50c582f810dc2be235dee61dfce10d7b487004f5023956fc346569be554c8d2a43c71f7d5fc83c2019aac07ae8f432cd6f18e98c2ce49908e4212e4f91b99c71bb87e4efe845096c87497c15189394092ce32b6a2f06765cc2124c6a07adc28f629cb26905c0da0d70ab112a301018acda9bdee5165662bb8afe3b43a4d373110086366bc18fd90683a4336b5a01008e174c134ceea61c04fe14cd16bce83bd36219410a5367e95a32460d42a89205c892870300a237652ed1d9d4c824c1eefc3f1da648bc22db11efc75b5e7cd8b709e1f89d53e81ff1267b896a74fb7f3870bbb413f5fdbfc34b66652b2fbcce1794db2416489b1b57ccbb034c4b88772368d8d9397726d9e52d40b2736a221ae6348790b58320df517cbd603f1f27ccf11980af0689ed529cad70631171823574480731971e546240e00f5358ac5bae24588a873e7649c97be62381b965c01f1372cd1eeff2048a5e0363c0ecf227ffbf1591b51f8f1a963b0db2266b00229ec70db46ed31812f13a0d1c432b71b8e7d0cdd9f293590b4bdb7ebe93e90fde0173afce419de1ff4af3cef277130e271c4a4417fff0d5d60e5483ccc7e236b1347e47063037252ae063f12ce903bc7947aedfcd70aa85a7ef628f31d9fe076508dba9f8b2091876b2f5795f68bbdd267ceb5d6b4ddfe3bf225b54fedd79001acda500080212469bd0e99c4721fda2363b16915c97c5bf8b1f8d4de8be760d").unwrap(),
            // For genesis block: A default script can never be spent, intentionally
            script!(Nop),
            // Script offset never checked for coinbase, thus can use default
            PublicKey::from_hex("3ec3bc4ab31d3266bec5e7ed977bb766ab9b1d8951547ae7f653dc6109334e38").unwrap(),
            // For genesis block: Metadata signature will never be checked
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
                "660d5125fee9784c7c4d7ae5c533591418bf081a3ff21097f0bf9157af5aa91a",
            )
                .unwrap(),
            sig,None

        )],
    );
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("08 Aug 2022 10:00:00 +0200").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: vec![0; BLOCK_HASH_LENGTH],
            timestamp: timestamp.into(),
            output_mr: from_hex("331daafb9d16ab1b11a20dabb2ece9e609dfdef752793562173ed238d4e52d7f").unwrap(),
            witness_mr: from_hex("33af10f6bd6e0cc06e1f2def2e0f61371a9ff62ef0cdee47ed76caa6ce3778dc").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("1830a7f405c7d50c386322e30d0de71d075ac11ec590c77e2826b7e65f0ed935").unwrap(),
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

pub fn get_esmeralda_genesis_block() -> ChainBlock {
    let mut block = get_esmeralda_genesis_block_raw();

    // Add faucet utxos
    let mut utxos = Vec::new();
    let file = include_str!("faucets/esmeralda_faucet.json");
    let mut counter = 1;
    for line in file.lines() {
        if counter < 4001 {
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

    // Use this code if you need to generate new Merkle roots
    // NB: `esmerlada_genesis_sanity_check` must pass

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
    // block.header.kernel_mr = kernel_mmr.get_merkle_root().unwrap();
    // block.header.witness_mr = witness_mmr.get_merkle_root().unwrap();
    // block.header.output_mr = output_mmr.get_merkle_root().unwrap();
    // println!("kernel mr: {}", block.header.kernel_mr.to_hex());
    // println!("witness mr: {}", block.header.witness_mr.to_hex());
    // println!("output mr: {}", block.header.output_mr.to_hex());

    // Hardcode the Merkle roots once they've been computed above
    block.header.kernel_mr = from_hex("a3ebc99443f546cadea6146ec12a841fc3d51d2c322029273d6bb44d9879d81a").unwrap();
    block.header.witness_mr = from_hex("2ccdd0aeb1ff7b41ce0a6f1f3123a71138d705e9abf36b684271decca8e20855").unwrap();
    block.header.output_mr = from_hex("b15724ad00693d13e574909af476fd980ea0ba183462b2389442043d785a3f0d").unwrap();

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
    // Note: Use print_new_genesis_block_esme in core/tests/helpers/block_builders.rs to generate the required fields
    // below
    let excess_sig = Signature::new(
        PublicKey::from_hex("b4e82e14ab54f7fe025a832160ae589fbd2ccd8daa66d04e0e3c058b23c5d339").unwrap(),
        PrivateKey::from_hex("c40cd89e2433f8ace92f544d291a8cc65686c2512cc6e1d01f68cf8856f3630e").unwrap(),
    );
    let coinbase_meta_sig = CommitmentSignature::new(
        Commitment::from_hex("1875cb95dc182a7186259af680b9ea4c2443850de37fac23928cbd5130d0af0d").unwrap(),
        PrivateKey::from_hex("2f9d7e2a3e7ca12ed91e8e24c159d3274e871407fe72f220faab47fa46bde904").unwrap(),
        PrivateKey::from_hex("39864d07c8c1134ce515a56ed56f35bddebb803bc944c0b9f0f588b5f19d3602").unwrap(),
    );
    let coinbase = TransactionOutput::new(
        TransactionOutputVersion::get_current_version(),
        OutputFeatures {
            version: OutputFeaturesVersion::get_current_version(),
            output_type: OutputType::Coinbase,
            maturity: 3,
            metadata: Vec::new(),
            unique_id: None,
            sidechain_features: None,
            parent_public_key: None,
            asset: None,
            mint_non_fungible: None,
            sidechain_checkpoint: None,
            committee_definition: None,
        },
        Commitment::from_hex("00d6e2617fc58804c6c1723df20ad3941f66aecbf079f205a9ad5ba9c84b7d5b").unwrap(),
        BulletRangeProof::from_hex("01b04258b14f2093e796290ba4e011e71745d38c6d59d47eee509f33fff8c0c467e23d95553482c3e48382f068841e8bc40238e53886a38148ab05ea0bcbe35e12e075c2b3ec2ed7a81d72b31c5a7c79c391bc5aec841917f01fa656220baac6370a1458732f50c0bb16b18058c52a0fb6dfdce745078f318055f66eb81d911b365032bb9672cce7d4fddaefadabbc35cd3b16782012ccbfedc774650eda66df53fe97c1c3b69feadd141865bfd218a20d7d9e1032957bcb87f28ad984b9a81b45b8869314bd39d7fec662270b5988b5e247b5c37f54132b37af115a61f40fef2cc2d696ca7413fb2a037dae574fbbd854d6fa4cec956088975ae2450e98a19c3492098ac1efe95c4b73deef206c03394a25679d03e89e74a626bec9b6a5c7104b5ca7e152119dc08b393238eeca34388d7364dcc6e13ffb1f57b3c038219a7f4b8043a6378e8310d2c6f229f716b438064d7b359717010118bf8860f7ae61a422104a73bd4c5d36538b5112dd0beb293b463bd3d15f4c847a8c848c7369bd395dfefd18ab6d1cf6e41f02916caa3659580149d31c33c1c4c2fb6645cef528a4704865d3f905fc12bba1c2469e3379999ad63c1266e73a7b9726ac99cc3700d5325aab127d74c4b025977ef9d2fcea26be519c1b2e8727e58ac943684e35e38c7d92217c8b2981a423a77849b94ec05d6b89700937916be5e31c74d8ba854af0023b6770b68962ca7e94123a1e8b13cef5844d0f8405ca89c0dbd56c6d1f96bc070077afcf72f55eb8375444f9d038bcb8b88ded2cb7abb7fb22d021581191b205").unwrap(),
        // A default script can never be spent, intentionally
        script!(Nop),
        // The Sender offset public key is not checked for coinbase outputs
        PublicKey::from_hex("263adc1c006b85ac62df5e82e25274655ade2680ee6372e9291af031b0a3c025").unwrap(),
        // For genesis block: Metadata signature will never be checked
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
        Commitment::from_hex("8e81612b46e718305fc18c1a1058450ff1799ec22fc538b29f065026791c5c05").unwrap(),
        excess_sig,
        None,
    );
    let mut body = AggregateBody::new(vec![], vec![coinbase], vec![kernel]);
    body.sort();
    // set genesis timestamp
    let genesis = DateTime::parse_from_rfc2822("08 Aug 2022 10:00:00 +0200").unwrap();
    #[allow(clippy::cast_sign_loss)]
    let timestamp = genesis.timestamp() as u64;
    Block {
        header: BlockHeader {
            version: 0,
            height: 0,
            prev_hash: vec![0; BLOCK_HASH_LENGTH],
            timestamp: timestamp.into(),
            output_mr: from_hex("867e4bfa4a5731652d49a7cd32a6ddcacc3473b8b8f446f6d7366aa671eb3842").unwrap(),
            witness_mr: from_hex("36034e90dbf56b54fcc63fb26a6200a15b9fdb4957834d7de42ee3ada1aaaa00").unwrap(),
            output_mmr_size: 1,
            kernel_mr: from_hex("8ecc03eba1c5ee9ba7e5069c723e74587535eccbc06ce9149f6d4a0469900ffa").unwrap(),
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

    use croaring::Bitmap;
    use tari_common_types::types::Commitment;

    use super::*;
    use crate::{
        consensus::ConsensusManager,
        test_helpers::blockchain::create_new_blockchain_with_network,
        transactions::{transaction_components::transaction_output::batch_verify_range_proofs, CryptoFactories},
        validation::{ChainBalanceValidator, FinalHorizonStateValidation},
        KernelMmr,
        MutableOutputMmr,
        WitnessMmr,
    };

    #[test]
    fn esmeralda_genesis_sanity_check() {
        // Note: Generate new data for `pub fn get_esmeralda_genesis_block()` and `fn get_esmeralda_genesis_block_raw()`
        // if consensus values change, e.g. new faucet or other
        let block = get_esmeralda_genesis_block();
        assert_eq!(block.block().body.outputs().len(), 4001);

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
            kernel_mmr.push(k.hash()).unwrap();
        }

        let mut witness_mmr = WitnessMmr::new(Vec::new());
        let mut output_mmr = MutableOutputMmr::new(Vec::new(), Bitmap::create()).unwrap();

        for o in block.block().body.outputs() {
            o.verify_metadata_signature().unwrap();

            witness_mmr.push(o.witness_hash()).unwrap();
            output_mmr.push(o.hash()).unwrap();
        }

        assert_eq!(kernel_mmr.get_merkle_root().unwrap(), block.header().kernel_mr);
        assert_eq!(witness_mmr.get_merkle_root().unwrap(), block.header().witness_mr);
        assert_eq!(output_mmr.get_merkle_root().unwrap(), block.header().output_mr);

        // Check that the faucet UTXOs balance (the faucet_value consensus constant is set correctly and faucet kernel
        // is correct)

        let utxo_sum = block.block().body.outputs().iter().map(|o| &o.commitment).sum();
        let kernel_sum = block.block().body.kernels().iter().map(|k| &k.excess).sum();

        let db = create_new_blockchain_with_network(Network::Esmeralda);

        let lock = db.db_read_access().unwrap();
        ChainBalanceValidator::new(
            ConsensusManager::builder(Network::Esmeralda).build(),
            Default::default(),
        )
        .validate(&*lock, 0, &utxo_sum, &kernel_sum, &Commitment::default())
        .unwrap();
    }

    #[test]
    fn igor_genesis_sanity_check() {
        let block = get_igor_genesis_block();
        assert_eq!(block.block().body.outputs().len(), 1);

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
            kernel_mmr.push(k.hash()).unwrap();
        }

        let mut witness_mmr = WitnessMmr::new(Vec::new());
        let mut output_mmr = MutableOutputMmr::new(Vec::new(), Bitmap::create()).unwrap();
        assert_eq!(block.block().body.kernels().len(), 1);
        assert_eq!(block.block().body.outputs().len(), 1);
        for o in block.block().body.outputs() {
            witness_mmr.push(o.witness_hash()).unwrap();
            output_mmr.push(o.hash()).unwrap();
        }

        assert_eq!(kernel_mmr.get_merkle_root().unwrap(), block.header().kernel_mr);
        assert_eq!(witness_mmr.get_merkle_root().unwrap(), block.header().witness_mr);
        assert_eq!(output_mmr.get_merkle_root().unwrap(), block.header().output_mr);

        // Check that the faucet UTXOs balance (the faucet_value consensus constant is set correctly and faucet kernel
        // is correct)

        let utxo_sum = block.block().body.outputs().iter().map(|o| &o.commitment).sum();
        let kernel_sum = block.block().body.kernels().iter().map(|k| &k.excess).sum();

        let db = create_new_blockchain_with_network(Network::Igor);

        let lock = db.db_read_access().unwrap();
        ChainBalanceValidator::new(ConsensusManager::builder(Network::Igor).build(), Default::default())
            .validate(&*lock, 0, &utxo_sum, &kernel_sum, &Commitment::default())
            .unwrap();
    }
}
