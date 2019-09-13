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
//

use crate::{
    blocks::{blockheader::BlockHeader, Block, BlockBuilder},
    fee::Fee,
    proof_of_work::Difficulty,
    tari_amount::MicroTari,
    transaction::{
        KernelBuilder,
        KernelFeatures,
        OutputFeatures,
        Transaction,
        TransactionInput,
        TransactionKernel,
        TransactionOutput,
        UnblindedOutput,
    },
    transaction_protocol::{
        build_challenge,
        sender::SenderTransactionProtocol,
        test_common::TestParams,
        TransactionMetadata,
    },
    types::{Commitment, PrivateKey, PublicKey, Signature, COMMITMENT_FACTORY, PROVER},
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    common::Blake256,
    keys::{PublicKey as PK, SecretKey},
    range_proof::RangeProofService,
};
use tari_utilities::hash::Hashable;

/// Create a random transaction input for the given amount and maturity period. The input and its unblinded
/// parameters are returned.
pub fn create_test_input(amount: MicroTari, maturity: u64) -> (TransactionInput, UnblindedOutput) {
    let mut rng = rand::OsRng::new().unwrap();
    let spending_key = PrivateKey::random(&mut rng);
    let commitment = COMMITMENT_FACTORY.commit(&spending_key, &PrivateKey::from(amount.clone()));
    let mut features = OutputFeatures::default();
    features.maturity = maturity;
    let input = TransactionInput::new(features.clone(), commitment);
    let unblinded_output = UnblindedOutput::new(amount, spending_key, Some(features));
    (input, unblinded_output)
}

/// Create an unconfirmed transaction for testing with a valid fee, unique access_sig, random inputs and outputs, the
/// transaction is only partially constructed
#[deprecated(note = "Use create_tx instead")]
pub fn create_test_tx(
    amount: MicroTari,
    fee_per_gram: MicroTari,
    lock_height: u64,
    input_count: u64,
    input_maturity: u64,
    output_count: u64,
) -> Transaction
{
    let mut rng = rand::OsRng::new().unwrap();
    let test_params = TestParams::new(&mut rng);
    let mut stx_builder = SenderTransactionProtocol::builder(0);
    stx_builder
        .with_lock_height(lock_height)
        .with_fee_per_gram(fee_per_gram)
        .with_offset(test_params.offset.clone())
        .with_private_nonce(test_params.nonce.clone())
        .with_change_secret(test_params.change_key.clone());

    let amount_per_input = amount / input_count;
    let amount_for_last_input = amount - amount_per_input * (input_count - 1);
    for i in 0..input_count {
        let input_amount = if i < input_count - 1 {
            amount_per_input
        } else {
            amount_for_last_input
        };
        let (utxo, input) = create_test_input(input_amount, input_maturity);
        stx_builder.with_input(utxo, input);
    }

    let estimated_fee = Fee::calculate(fee_per_gram, input_count as usize, output_count as usize);
    let amount_per_output = (amount - estimated_fee) / output_count;
    let amount_for_last_output = (amount - estimated_fee) - amount_per_output * (output_count - 1);
    for i in 0..output_count {
        let output_amount = if i < output_count - 1 {
            amount_per_output
        } else {
            amount_for_last_output
        };
        stx_builder.with_output(UnblindedOutput::new(
            output_amount.into(),
            test_params.spend_key.clone(),
            None,
        ));
    }

    let mut stx_protocol = stx_builder.build::<Blake256>(&PROVER, &COMMITMENT_FACTORY).unwrap();
    match stx_protocol.finalize(KernelFeatures::empty(), &PROVER, &COMMITMENT_FACTORY) {
        Ok(true) => (),
        Ok(false) => panic!("{:?}", stx_protocol.failure_reason()),
        Err(e) => panic!("{:?}", e),
    }
    stx_protocol.get_transaction().unwrap().clone()
}

/// Create an unconfirmed transaction for testing with a valid fee, unique access_sig, random inputs and outputs, the
/// transaction is only partially constructed
pub fn create_tx(
    amount: MicroTari,
    fee_per_gram: MicroTari,
    lock_height: u64,
    input_count: u64,
    input_maturity: u64,
    output_count: u64,
) -> (Transaction, Vec<UnblindedOutput>, Vec<UnblindedOutput>)
{
    let mut rng = rand::OsRng::new().unwrap();
    let test_params = TestParams::new(&mut rng);
    let mut stx_builder = SenderTransactionProtocol::builder(0);
    stx_builder
        .with_lock_height(lock_height)
        .with_fee_per_gram(fee_per_gram)
        .with_offset(test_params.offset.clone())
        .with_private_nonce(test_params.nonce.clone())
        .with_change_secret(test_params.change_key.clone());

    let mut unblinded_inputs = Vec::with_capacity(input_count as usize);
    let mut unblinded_outputs = Vec::with_capacity(output_count as usize);
    let amount_per_input = amount / input_count;
    for _ in 0..input_count - 1 {
        let (utxo, input) = create_test_input(amount_per_input, input_maturity);
        unblinded_inputs.push(input.clone());
        stx_builder.with_input(utxo, input);
    }
    let amount_for_last_input = amount - amount_per_input * (input_count - 1);
    let (utxo, input) = create_test_input(amount_for_last_input, input_maturity);
    unblinded_inputs.push(input.clone());
    stx_builder.with_input(utxo, input);

    let estimated_fee = Fee::calculate(fee_per_gram, input_count as usize, output_count as usize);
    let amount_per_output = (amount - estimated_fee) / output_count;
    let amount_for_last_output = (amount - estimated_fee) - amount_per_output * (output_count - 1);
    for i in 0..output_count {
        let output_amount = if i < output_count - 1 {
            amount_per_output
        } else {
            amount_for_last_output
        };
        let utxo = UnblindedOutput::new(output_amount.into(), test_params.spend_key.clone(), None);
        unblinded_outputs.push(utxo.clone());
        stx_builder.with_output(utxo);
    }

    let mut stx_protocol = stx_builder.build::<Blake256>(&PROVER, &COMMITMENT_FACTORY).unwrap();
    match stx_protocol.finalize(KernelFeatures::empty(), &PROVER, &COMMITMENT_FACTORY) {
        Ok(true) => (),
        Ok(false) => panic!("{:?}", stx_protocol.failure_reason()),
        Err(e) => panic!("{:?}", e),
    }
    (
        stx_protocol.get_transaction().unwrap().clone(),
        unblinded_inputs,
        unblinded_outputs,
    )
}

/// Spend the provided UTXOs by creating a new transaction that breaking the inputs up into equally sized outputs
pub fn create_test_tx_spending_utxos(
    fee_per_gram: MicroTari,
    lock_height: u64,
    utxos: Vec<(TransactionInput, UnblindedOutput)>,
    output_count: u64,
) -> (Transaction, Vec<UnblindedOutput>)
{
    let mut rng = rand::OsRng::new().unwrap();
    let test_params = TestParams::new(&mut rng);
    let mut stx_builder = SenderTransactionProtocol::builder(0);
    stx_builder
        .with_lock_height(lock_height)
        .with_fee_per_gram(fee_per_gram)
        .with_offset(test_params.offset.clone())
        .with_private_nonce(test_params.nonce.clone())
        .with_change_secret(test_params.change_key.clone());

    for (utxo, input) in &utxos {
        stx_builder.with_input(utxo.clone(), input.clone());
    }

    let input_count = utxos.len();
    let mut amount = MicroTari(0);
    utxos.iter().for_each(|(_, input)| amount += input.value);
    let estimated_fee = Fee::calculate(fee_per_gram, input_count as usize, output_count as usize);
    let amount_per_output = (amount - estimated_fee) / output_count;
    let amount_for_last_output = (amount - estimated_fee) - amount_per_output * (output_count - 1);
    let mut unblinded_outputs = Vec::with_capacity(output_count as usize);
    for i in 0..output_count {
        let output_amount = if i < output_count - 1 {
            amount_per_output
        } else {
            amount_for_last_output
        };
        let utxo = UnblindedOutput::new(output_amount.into(), test_params.spend_key.clone(), None);
        unblinded_outputs.push(utxo.clone());
        stx_builder.with_output(utxo);
    }

    let mut stx_protocol = stx_builder.build::<Blake256>(&PROVER, &COMMITMENT_FACTORY).unwrap();
    match stx_protocol.finalize(KernelFeatures::empty(), &PROVER, &COMMITMENT_FACTORY) {
        Ok(true) => (),
        Ok(false) => panic!("{:?}", stx_protocol.failure_reason()),
        Err(e) => panic!("{:?}", e),
    }
    let txn = stx_protocol.get_transaction().unwrap().clone();
    (txn, unblinded_outputs)
}

/// Create a transaction kernel with the given fee, using random keys to generate the signature
pub fn create_test_kernel(fee: MicroTari, lock_height: u64) -> TransactionKernel {
    let (excess, s) = create_random_signature(fee, lock_height);
    KernelBuilder::new()
        .with_fee(fee)
        .with_lock_height(lock_height)
        .with_excess(&Commitment::from_public_key(&excess))
        .with_signature(&s)
        .build()
        .unwrap()
}

/// Create a genesis block returning it with the spending key for the coinbase utxo
///
/// Right now this function does not use consensus rules to generate the block. The coinbase output has an arbitrary
/// value, and the maturity is zero.
pub fn create_genesis_block() -> (Block, UnblindedOutput) {
    let mut header = BlockHeader::new(0);
    header.total_difficulty = Difficulty::from(1);
    let value = MicroTari::from(100_000_000);
    let excess = Commitment::from_public_key(&PublicKey::default());
    let (utxo, key) = create_utxo(value);
    let (_pk, sig) = create_random_signature(0.into(), 0);
    let kernel = KernelBuilder::new()
        .with_signature(&sig)
        .with_excess(&excess)
        .with_features(KernelFeatures::COINBASE_KERNEL)
        .build()
        .unwrap();
    let block = BlockBuilder::new()
        .with_header(header)
        .with_coinbase_utxo(utxo, kernel)
        .build();
    // TODO right now it matters not that it's not flagged as a Coinbase output
    let output = UnblindedOutput::new(value, key, None);
    (block, output)
}

/// Create a partially constructed block using the provided set of transactions
/// TODO - as we move to creating ever more correct blocks in tests, maybe we should deprecate this method since there
/// is chain_block, or rename it to `create_orphan_block` and drop the prev_block argument
pub fn create_test_block(block_height: u64, prev_block: Option<Block>, transactions: Vec<Transaction>) -> Block {
    let mut header = BlockHeader::new(0);
    header.height = block_height;
    if let Some(block) = prev_block {
        header.prev_hash = block.hash();
        header.total_difficulty = block.header.total_difficulty + Difficulty::from(1);
    }
    BlockBuilder::new()
        .with_header(header)
        .with_transactions(transactions)
        .build()
}

/// Create a new block using the provided transactions that adds to the blockchain given in `prev_block`
pub fn chain_block(prev_block: &Block, transactions: Vec<Transaction>) -> Block {
    let mut header = BlockHeader::from_previous(&prev_block.header);
    header.total_difficulty = prev_block.header.total_difficulty + Difficulty::from(1);

    BlockBuilder::new()
        .with_header(header)
        .with_transactions(transactions)
        .build()
}

/// Generate a random signature, returning the public key (excess) and the signature.
pub fn create_random_signature(fee: MicroTari, lock_height: u64) -> (PublicKey, Signature) {
    let mut rng = rand::OsRng::new().unwrap();
    let r = SecretKey::random(&mut rng);
    let (k, p) = PublicKey::random_keypair(&mut rng);
    let tx_meta = TransactionMetadata { fee, lock_height };
    let e = build_challenge(&PublicKey::from_secret_key(&r), &tx_meta);
    (p, Signature::sign(k, r, &e).unwrap())
}

/// A convenience struct for a set of public-private keys and a public-private nonce
pub struct TestKeySet {
    k: PrivateKey,
    pk: PublicKey,
    r: PrivateKey,
    pr: PublicKey,
}

/// Generate a new random key set. The key set includes
/// * a public-private keypair (k, pk)
/// * a public-private nonce keypair (r, pr)
pub fn generate_keys() -> TestKeySet {
    let mut rng = rand::OsRng::new().unwrap();
    let (k, pk) = PublicKey::random_keypair(&mut rng);
    let (r, pr) = PublicKey::random_keypair(&mut rng);
    TestKeySet { k, pk, r, pr }
}

/// Create a new UTXO for the specified value and return the output and spending key
pub fn create_utxo(value: MicroTari) -> (TransactionOutput, PrivateKey) {
    let keys = generate_keys();
    let commitment = COMMITMENT_FACTORY.commit_value(&keys.k, value.into());
    let proof = PROVER.construct_proof(&keys.k, value.into()).unwrap();
    let utxo = TransactionOutput::new(OutputFeatures::default(), commitment, proof.into());
    (utxo, keys.k)
}
