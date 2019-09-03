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
    blocks::{aggregated_body::AggregateBody, blockheader::BlockHeader, Block},
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

/// Create a test input UTXO for a transaction with its unblinded output set with a specified maturity.
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

pub fn create_test_tx_spending_utxos(
    fee_per_gram: MicroTari,
    lock_height: u64,
    utxos: Vec<(TransactionInput, UnblindedOutput)>,
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

    for (utxo, input) in &utxos {
        stx_builder.with_input(utxo.clone(), input.clone());
    }

    let input_count = utxos.len();
    let mut amount = MicroTari(0);
    utxos.iter().for_each(|(_, input)| amount += input.value);
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

/// Create a partially constructed block using the provided set of transactions
pub fn create_test_block(block_height: u64, prev_block: Option<Block>, transactions: Vec<Transaction>) -> Block {
    let mut header = BlockHeader::new(0);
    header.height = block_height;
    if let Some(block) = prev_block {
        header.prev_hash = block.hash();
        header.total_difficulty = block.header.total_difficulty + Difficulty::from(1);
    }
    let mut body = AggregateBody::empty();
    transactions.iter().for_each(|tx| {
        body.kernels.push(tx.body.kernels[0].clone());
        body.inputs.append(&mut tx.body.inputs.clone());
        body.outputs.append(&mut tx.body.outputs.clone());
    });

    Block { header, body }
}

/// Create a partially constructed utxo set using the outputs of a test block
pub fn extract_outputs_as_inputs(utxos: &mut Vec<TransactionInput>, published_block: &Block) {
    for output in &published_block.body.outputs {
        let input = TransactionInput::from(output.clone());
        if !utxos.contains(&input) {
            utxos.push(input);
        }
    }
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
