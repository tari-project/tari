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

use super::{header_iter::HeaderIter, ChainBalanceValidator, HeaderValidator};
use crate::{
    blocks::{BlockHeader, BlockHeaderValidationError},
    chain_storage::DbTransaction,
    consensus::{consensus_constants::ConsensusConstantsBuilder, ConsensusManagerBuilder, Network},
    proof_of_work::PowError,
    test_helpers::create_mem_db,
    transactions::{
        crypto::commitment::HomomorphicCommitmentFactory,
        fee::Fee,
        helpers::{create_random_signature_from_s_key, spend_utxos},
        tari_amount::uT,
        transaction::{KernelBuilder, KernelFeatures, TransactionKernel},
        types::{Commitment, CryptoFactories},
        OutputBuilder,
        OutputFeatures,
    },
    txn_schema,
    validation::{Validation, ValidationError},
};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, Hashable};
use tari_test_utils::unpack_enum;

#[test]
fn header_iter_empty_and_invalid_height() {
    let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet).build();
    let db = create_mem_db(&consensus_manager);

    let iter = HeaderIter::new(&db, 0, 10);
    let headers = iter.map(Result::unwrap).collect::<Vec<_>>();
    assert_eq!(headers.len(), 1);
    let genesis = consensus_manager.get_genesis_block();
    assert_eq!(&genesis.header, &headers[0]);

    // Invalid header height
    let iter = HeaderIter::new(&db, 1, 10);
    let headers = iter.collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(headers.len(), 1);
}

#[test]
fn header_iter_fetch_in_chunks() {
    let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet).build();
    let db = create_mem_db(&consensus_manager);
    let headers = (1..=15)
        .map(|i| {
            let mut header = BlockHeader::new(0);
            header.height = i;
            header
        })
        .collect::<Vec<_>>();
    db.insert_valid_headers(headers).unwrap();

    let iter = HeaderIter::new(&db, 11, 3);
    let headers = iter.map(Result::unwrap).collect::<Vec<_>>();
    assert_eq!(headers.len(), 12);
    let genesis = consensus_manager.get_genesis_block();
    assert_eq!(&genesis.header, &headers[0]);

    (1..=11).for_each(|i| {
        assert_eq!(headers[i].height, i as u64);
    })
}

#[test]
fn headers_validation() {
    let rules = ConsensusManagerBuilder::new(Network::LocalNet).build();
    let db = create_mem_db(&rules);
    let validator = HeaderValidator::new(db.clone(), rules.clone());

    let genesis = rules.get_genesis_block();
    validator.validate(&genesis.header).unwrap();

    let header = BlockHeader::from_previous(&genesis.header).unwrap();
    validator.validate(&header).unwrap();
    db.insert_valid_headers(vec![header.clone()]).unwrap();

    let header1 = header.clone();
    let mut prev_header = header;
    for _ in 0..3 {
        let header = BlockHeader::from_previous(&prev_header).unwrap();
        validator.validate(&header).unwrap();
        db.insert_valid_headers(vec![header.clone()]).unwrap();
        prev_header = header;
    }
    // Check that the genesis and header@1 are still valid
    validator.validate(&header1).unwrap();
    validator.validate(&genesis.header).unwrap();

    let mut header = BlockHeader::from_previous(&prev_header).unwrap();
    header.timestamp = EpochTime::now();
    header.pow.target_difficulty = 123456.into();
    let err = validator.validate(&header).unwrap_err();
    unpack_enum!(ValidationError::BlockHeaderError(err) = err);
    unpack_enum!(BlockHeaderValidationError::ProofOfWorkError(err) = err);
    unpack_enum!(PowError::InvalidTargetDifficulty = err);
    db.insert_valid_headers(vec![header.clone()]).unwrap();

    let mut header = BlockHeader::from_previous(&header).unwrap();
    header.timestamp = genesis.header.timestamp;
    let err = validator.validate(&header).unwrap_err();
    unpack_enum!(ValidationError::BlockHeaderError(err) = err);
    unpack_enum!(BlockHeaderValidationError::InvalidTimestamp = err);
}

#[test]
fn chain_balance_validation() {
    let factories = CryptoFactories::default();
    let consensus_manager = ConsensusManagerBuilder::new(Network::Ridcully).build();
    let mut genesis = consensus_manager.get_genesis_block();
    let faucet_value = 5000 * uT;
    let faucet_utxo = OutputBuilder::new()
        .with_value(faucet_value)
        .build(&factories.commitment)
        .unwrap();
    let faucet_key = faucet_utxo.spending_key().clone();
    let faucet_utxo = faucet_utxo.as_transaction_output(&factories).unwrap();
    let faucet_hash = faucet_utxo.hash();
    let (pk, sig) = create_random_signature_from_s_key(faucet_key.clone(), 0.into(), 0);
    let excess = Commitment::from_public_key(&pk);
    let kernel = TransactionKernel {
        features: KernelFeatures::empty(),
        fee: 0 * uT,
        lock_height: 0,
        excess,
        excess_sig: sig,
    };
    genesis.body.add_output(faucet_utxo);
    genesis.body.add_kernels(&mut vec![kernel]);
    let total_faucet = faucet_value + consensus_manager.consensus_constants(0).faucet_value();
    let constants = ConsensusConstantsBuilder::new(Network::LocalNet)
        .with_consensus_constants(consensus_manager.consensus_constants(0).clone())
        .with_faucet_value(total_faucet)
        .build();
    // Create a LocalNet consensus manager that uses rincewind consensus constants and has a custom rincewind genesis
    // block that contains an extra faucet utxo
    let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet)
        .with_block(genesis.clone())
        .with_consensus_constants(constants)
        .build();

    let db = create_mem_db(&consensus_manager);

    let validator = ChainBalanceValidator::new(db.clone(), consensus_manager.clone(), factories.clone());
    // Validate the genesis state
    validator.validate(&0).unwrap();

    //---------------------------------- Add a new coinbase and header --------------------------------------------//
    let mut txn = DbTransaction::new();
    let coinbase_value = consensus_manager.emission_schedule().block_reward(1);
    let coinbase = OutputBuilder::new()
        .with_value(coinbase_value)
        .with_features(OutputFeatures::create_coinbase(1))
        .build(&factories.commitment)
        .unwrap();
    let sig = sign!(coinbase).unwrap();
    let excess = factories.commitment.commit_value(coinbase.blinding_factor(), 0);
    let coinbase_key = coinbase.spending_key().clone();
    let coinbase = coinbase.as_transaction_output(&factories).unwrap();
    let coinbase_hash = coinbase.hash();
    txn.insert_utxo(coinbase);
    let kernel = KernelBuilder::new()
        .with_signature(&sig)
        .with_excess(&excess)
        .with_features(KernelFeatures::COINBASE_KERNEL)
        .build()
        .unwrap();
    txn.insert_kernel(kernel);

    let header1 = BlockHeader::from_previous(&genesis.header).unwrap();
    txn.insert_header(header1.clone());
    db.commit(txn).unwrap();

    validator.validate(&1).unwrap();

    //---------------------------------- Spend coinbase from h=1 ----------------------------------//
    let mut txn = DbTransaction::new();

    txn.spend_utxo(coinbase_hash);

    let output = OutputBuilder::new()
        .with_value(coinbase_value)
        .with_spending_key(coinbase_key)
        .build(&factories.commitment)
        .unwrap();
    let fee = Fee::calculate(25 * uT, 1, 1, 2);
    let schema = txn_schema!(from: vec![output], to: vec![coinbase_value - fee], fee: 25 * uT);
    let (tx, _, params) = spend_utxos(schema);
    for utxo in tx.body.outputs() {
        txn.insert_utxo(utxo.clone());
    }
    for kernel in tx.body.kernels() {
        txn.insert_kernel(kernel.clone());
    }

    let v = consensus_manager.emission_schedule().block_reward(2) + fee;
    let coinbase = OutputBuilder::new()
        .with_value(v)
        .with_features(OutputFeatures::create_coinbase(1))
        .build(&factories.commitment)
        .unwrap();
    let sig = sign!(coinbase).unwrap();
    let excess = factories.commitment.commit_value(coinbase.blinding_factor(), 0);
    let kernel = KernelBuilder::new()
        .with_signature(&sig)
        .with_excess(&excess)
        .with_features(KernelFeatures::COINBASE_KERNEL)
        .build()
        .unwrap();
    txn.insert_kernel(kernel);
    let coinbase = coinbase.as_transaction_output(&factories).unwrap();
    txn.insert_utxo(coinbase);
    let mut header2 = BlockHeader::from_previous(&header1).unwrap();
    header2.total_kernel_offset = params.offset;
    txn.insert_header(header2.clone());
    db.commit(txn).unwrap();

    validator.validate(&2).unwrap();

    //---------------------------------- Spend faucet UTXO --------------------------------------------//
    let mut txn = DbTransaction::new();

    txn.spend_utxo(faucet_hash);

    let output = OutputBuilder::new()
        .with_value(faucet_value)
        .with_spending_key(faucet_key)
        .build(&factories.commitment)
        .unwrap();
    let fee = Fee::calculate(25 * uT, 1, 1, 2);
    let schema = txn_schema!(from: vec![output], to: vec![faucet_value - fee], fee: 25 * uT);
    let (tx, _, params) = spend_utxos(schema);
    for utxo in tx.body.outputs() {
        txn.insert_utxo(utxo.clone());
    }
    for kernel in tx.body.kernels() {
        txn.insert_kernel(kernel.clone());
    }

    let v = consensus_manager.emission_schedule().block_reward(3) + fee;
    let coinbase = OutputBuilder::new()
        .with_value(v)
        .with_features(OutputFeatures::create_coinbase(1))
        .build(&factories.commitment)
        .unwrap();
    let sig = sign!(coinbase).unwrap();
    let excess = factories.commitment.commit_value(coinbase.blinding_factor(), 0);
    let kernel = KernelBuilder::new()
        .with_signature(&sig)
        .with_excess(&excess)
        .with_features(KernelFeatures::COINBASE_KERNEL)
        .build()
        .unwrap();
    txn.insert_kernel(kernel);
    let coinbase = coinbase.as_transaction_output(&factories).unwrap();
    txn.insert_utxo(coinbase);
    let mut header3 = BlockHeader::from_previous(&header2).unwrap();
    header3.total_kernel_offset = params.offset;
    txn.insert_header(header3.clone());
    db.commit(txn).unwrap();

    validator.validate(&3).unwrap();

    //---------------------------------- Try to inflate --------------------------------------------//
    let mut txn = DbTransaction::new();

    let v = consensus_manager.emission_schedule().block_reward(4) + 1 * uT;
    let coinbase = OutputBuilder::new()
        .with_value(v)
        .with_features(OutputFeatures::create_coinbase(1))
        .build(&factories.commitment)
        .unwrap();
    let sig = sign!(coinbase).unwrap();
    let excess = factories.commitment.commit_value(coinbase.blinding_factor(), 0);
    let kernel = KernelBuilder::new()
        .with_signature(&sig)
        .with_excess(&excess)
        .with_features(KernelFeatures::COINBASE_KERNEL)
        .build()
        .unwrap();
    txn.insert_kernel(kernel);
    let coinbase = coinbase.as_transaction_output(&factories).unwrap();
    txn.insert_utxo(coinbase);
    let header4 = BlockHeader::from_previous(&header3).unwrap();
    txn.insert_header(header4);
    db.commit(txn).unwrap();

    validator.validate(&4).unwrap_err();
}
