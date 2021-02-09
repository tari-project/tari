//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    blocks::BlockHeader,
    consensus::{ConsensusManagerBuilder, Network},
    test_helpers::blockchain::create_store_with_consensus,
    validation::header_iter::HeaderIter,
};

#[test]
fn header_iter_empty_and_invalid_height() {
    let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet).build();
    let db = create_store_with_consensus(&consensus_manager);

    let iter = HeaderIter::new(&db, 0, 10);
    let headers = iter.map(Result::unwrap).collect::<Vec<_>>();
    assert_eq!(headers.len(), 1);
    let genesis = consensus_manager.get_genesis_block();
    assert_eq!(&genesis.block.header, &headers[0]);

    // Invalid header height
    let iter = HeaderIter::new(&db, 1, 10);
    let headers = iter.collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(headers.len(), 1);
}

#[test]
fn header_iter_fetch_in_chunks() {
    let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet).build();
    let db = create_store_with_consensus(&consensus_manager);
    let headers = (1..=15).fold(vec![db.fetch_chain_header(0).unwrap()], |mut acc, i| {
        let prev = acc.last().unwrap();
        let mut header = BlockHeader::new(0);
        header.height = i;
        header.prev_hash = prev.hash().clone();
        // These have to be unique
        header.kernel_mmr_size = 2 + i;
        header.output_mmr_size = 4001 + i;

        let chain_header = db.create_chain_header_if_valid(header, prev).unwrap();
        acc.push(chain_header);
        acc
    });
    db.insert_valid_headers(
        headers
            .into_iter()
            .skip(1)
            .map(|chain_header| (chain_header.header, chain_header.accumulated_data))
            .collect(),
    )
    .unwrap();

    let iter = HeaderIter::new(&db, 11, 3);
    let headers = iter.map(Result::unwrap).collect::<Vec<_>>();
    assert_eq!(headers.len(), 12);
    let genesis = consensus_manager.get_genesis_block();
    assert_eq!(&genesis.block.header, &headers[0]);

    (1..=11).for_each(|i| {
        assert_eq!(headers[i].height, i as u64);
    })
}

#[test]
#[ignore]
// TODO: Fix this test with the new DB structure
fn chain_balance_validation() {
    // let factories = CryptoFactories::default();
    // let consensus_manager = ConsensusManagerBuilder::new(Network::Ridcully).build();
    // let mut genesis = consensus_manager.get_genesis_block();
    // let faucet_value = 5000 * uT;
    // let (faucet_utxo, faucet_key) = create_utxo(faucet_value, &factories, None);
    // let (pk, sig) = create_random_signature_from_s_key(faucet_key.clone(), 0.into(), 0);
    // let excess = Commitment::from_public_key(&pk);
    // let kernel = TransactionKernel {
    //     features: KernelFeatures::empty(),
    //     fee: 0 * uT,
    //     lock_height: 0,
    //     excess,
    //     excess_sig: sig,
    // };
    // let _faucet_hash = faucet_utxo.hash();
    // genesis.body.add_output(faucet_utxo);
    // genesis.body.add_kernels(&mut vec![kernel]);
    // let total_faucet = faucet_value + consensus_manager.consensus_constants(0).faucet_value();
    // let constants = ConsensusConstantsBuilder::new(Network::LocalNet)
    //     .with_consensus_constants(consensus_manager.consensus_constants(0).clone())
    //     .with_faucet_value(total_faucet)
    //     .build();
    // // Create a LocalNet consensus manager that uses rincewind consensus constants and has a custom rincewind genesis
    // // block that contains an extra faucet utxo
    // let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet)
    //     .with_block(genesis.clone())
    //     .with_consensus_constants(constants)
    //     .build();
    //
    // let db = create_store_with_consensus(&consensus_manager);
    //
    // let validator = ChainBalanceValidator::new(db.clone(), consensus_manager.clone(), factories.clone());
    // // Validate the genesis state
    // validator.validate(&genesis.header).unwrap();
    //
    // //---------------------------------- Add a new coinbase and header --------------------------------------------//
    // let mut txn = DbTransaction::new();
    // let coinbase_value = consensus_manager.emission_schedule().block_reward(1);
    // let (coinbase, coinbase_key) = create_utxo(coinbase_value, &factories, Some(OutputFeatures::create_coinbase(1)));
    // let _coinbase_hash = coinbase.hash();
    // let (pk, sig) = create_random_signature_from_s_key(coinbase_key.clone(), 0.into(), 0);
    // let excess = Commitment::from_public_key(&pk);
    // let kernel = KernelBuilder::new()
    //     .with_signature(&sig)
    //     .with_excess(&excess)
    //     .with_features(KernelFeatures::COINBASE_KERNEL)
    //     .build()
    //     .unwrap();
    //
    // let header1 = BlockHeader::from_previous(&genesis.header).unwrap();
    // txn.insert_header(header1.clone());
    //
    // let mut mmr_position = 0;
    // let mut mmr_leaf_index = 0;
    //
    // txn.insert_kernel(kernel, header1.hash(), mmr_position);
    // txn.insert_utxo(coinbase.clone(), header1.hash(), mmr_leaf_index);
    //
    // db.commit(txn).unwrap();
    //
    // validator.validate(&header1).unwrap();
    //
    // //---------------------------------- Spend coinbase from h=1 ----------------------------------//
    // let mut txn = DbTransaction::new();
    //
    // // txn.spend_utxo(coinbase_hash);
    //
    // let output = UnblindedOutput::new(coinbase_value, coinbase_key, None);
    // let fee = Fee::calculate(25 * uT, 1, 1, 2);
    // let schema = txn_schema!(from: vec![output], to: vec![coinbase_value - fee], fee: 25 * uT);
    // let (tx, _, params) = spend_utxos(schema);
    //
    // let v = consensus_manager.emission_schedule().block_reward(2) + fee;
    // let (coinbase, key) = create_utxo(v, &factories, Some(OutputFeatures::create_coinbase(1)));
    // let (pk, sig) = create_random_signature_from_s_key(key, 0.into(), 0);
    // let excess = Commitment::from_public_key(&pk);
    // let kernel = KernelBuilder::new()
    //     .with_signature(&sig)
    //     .with_excess(&excess)
    //     .with_features(KernelFeatures::COINBASE_KERNEL)
    //     .build()
    //     .unwrap();
    //
    // let mut header2 = BlockHeader::from_previous(&header1).unwrap();
    // header2.total_kernel_offset = params.offset;
    // txn.insert_header(header2.clone());
    //
    // let header2_hash = header2.hash();
    // mmr_leaf_index += 1;
    // txn.insert_utxo(coinbase.clone(), header2_hash.clone(), mmr_leaf_index);
    // for utxo in tx.body.outputs() {
    //     mmr_leaf_index += 1;
    //     txn.insert_utxo(utxo.clone(), header2_hash.clone(), mmr_leaf_index);
    // }
    // mmr_position += 1;
    // txn.insert_kernel(kernel, header2_hash.clone(), mmr_position);
    // for kernel in tx.body.kernels() {
    //     mmr_position += 1;
    //     txn.insert_kernel(kernel.clone(), header2_hash.clone(), mmr_position);
    // }
    //
    // db.commit(txn).unwrap();
    //
    // validator.validate(&header2).unwrap();
    //
    // //---------------------------------- Spend faucet UTXO --------------------------------------------//
    // let mut txn = DbTransaction::new();
    //
    // // txn.spend_utxo(faucet_hash);
    //
    // let output = UnblindedOutput::new(faucet_value, faucet_key, None);
    // let fee = Fee::calculate(25 * uT, 1, 1, 2);
    // let schema = txn_schema!(from: vec![output], to: vec![faucet_value - fee], fee: 25 * uT);
    // let (tx, _, params) = spend_utxos(schema);
    //
    // let v = consensus_manager.emission_schedule().block_reward(3) + fee;
    // let (coinbase, key) = create_utxo(v, &factories, Some(OutputFeatures::create_coinbase(1)));
    // let (pk, sig) = create_random_signature_from_s_key(key, 0.into(), 0);
    // let excess = Commitment::from_public_key(&pk);
    // let kernel = KernelBuilder::new()
    //     .with_signature(&sig)
    //     .with_excess(&excess)
    //     .with_features(KernelFeatures::COINBASE_KERNEL)
    //     .build()
    //     .unwrap();
    //
    // let mut header3 = BlockHeader::from_previous(&header2).unwrap();
    // header3.total_kernel_offset = params.offset;
    // txn.insert_header(header3.clone());
    //
    // let header3_hash = header3.hash();
    // mmr_leaf_index += 1;
    // txn.insert_utxo(coinbase.clone(), header3_hash.clone(), mmr_leaf_index);
    // for utxo in tx.body.outputs() {
    //     mmr_leaf_index += 1;
    //     txn.insert_utxo(utxo.clone(), header3_hash.clone(), mmr_leaf_index);
    // }
    //
    // mmr_position += 1;
    // txn.insert_kernel(kernel, header3_hash.clone(), mmr_position);
    // for kernel in tx.body.kernels() {
    //     mmr_position += 1;
    //     txn.insert_kernel(kernel.clone(), header3_hash.clone(), mmr_position);
    // }
    // db.commit(txn).unwrap();
    //
    // validator.validate(&header3).unwrap();
    //
    // //---------------------------------- Try to inflate --------------------------------------------//
    // let mut txn = DbTransaction::new();
    //
    // let v = consensus_manager.emission_schedule().block_reward(4) + 1 * uT;
    // let (coinbase, key) = create_utxo(v, &factories, Some(OutputFeatures::create_coinbase(1)));
    // let (pk, sig) = create_random_signature_from_s_key(key, 0.into(), 0);
    // let excess = Commitment::from_public_key(&pk);
    // let kernel = KernelBuilder::new()
    //     .with_signature(&sig)
    //     .with_excess(&excess)
    //     .with_features(KernelFeatures::COINBASE_KERNEL)
    //     .build()
    //     .unwrap();
    //
    // let header4 = BlockHeader::from_previous(&header3).unwrap();
    // txn.insert_header(header4.clone());
    // let header4_hash = header4.hash();
    //
    // mmr_leaf_index += 1;
    // txn.insert_utxo(coinbase.clone(), header4_hash.clone(), mmr_leaf_index);
    // mmr_position += 1;
    // txn.insert_kernel(kernel, header4_hash.clone(), mmr_position);
    //
    // db.commit(txn).unwrap();
    //
    // validator.validate(&header4).unwrap_err();
    unimplemented!();
}
