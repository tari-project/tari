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

use crate::support::utils::{make_input, TestParams};
use rand::RngCore;
use std::{thread, time::Duration};
use tari_core::{
    fee::Fee,
    tari_amount::MicroTari,
    transaction::{KernelFeatures, OutputFeatures, TransactionOutput, UnblindedOutput},
    transaction_protocol::single_receiver::SingleReceiverTransactionProtocol,
    types::{PrivateKey, PublicKey, RangeProof, COMMITMENT_FACTORY, PROVER},
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey as PublicKeyTrait, SecretKey},
    range_proof::RangeProofService,
};
use tari_utilities::ByteArray;
use tari_wallet::output_manager_service::{error::OutputManagerError, output_manager_service::OutputManagerService};

#[test]
fn sending_transaction_and_confirmation() {
    let mut rng = rand::OsRng::new().unwrap();
    let (secret_key, _public_key) = PublicKey::random_keypair(&mut rng);

    let mut oms = OutputManagerService::new(secret_key, "".to_string(), 0);

    let (_ti, uo) = make_input(&mut rng.clone(), MicroTari::from(100 + rng.next_u64() % 1000));
    oms.add_output(uo.clone()).unwrap();
    assert_eq!(oms.add_output(uo), Err(OutputManagerError::DuplicateOutput));
    let num_outputs = 20;
    for _i in 0..num_outputs {
        let (_ti, uo) = make_input(&mut rng.clone(), MicroTari::from(100 + rng.next_u64() % 1000));
        oms.add_output(uo).unwrap();
    }

    let mut stp = oms
        .prepare_transaction_to_send(MicroTari::from(1000), MicroTari::from(20), None)
        .unwrap();

    let sender_tx_id = stp.get_tx_id().unwrap();
    let mut num_change = 0;
    // Is there change? Unlikely not to be but the random amounts MIGHT produce a no change output situation
    if stp.get_amount_to_self().unwrap() > MicroTari::from(0) {
        let pt = oms.pending_transactions();
        assert_eq!(pt.len(), 1);
        assert_eq!(
            pt.get(&sender_tx_id).unwrap().outputs_to_be_received[0].value,
            stp.get_amount_to_self().unwrap()
        );
        num_change = 1;
    }

    let msg = stp.build_single_round_message().unwrap();

    let b = TestParams::new(&mut rng);

    let recv_info = SingleReceiverTransactionProtocol::create(
        &msg,
        b.nonce,
        b.spend_key,
        OutputFeatures::empty(),
        &PROVER,
        &COMMITMENT_FACTORY,
    )
    .unwrap();

    stp.add_single_recipient_info(recv_info.clone(), &PROVER).unwrap();

    stp.finalize(KernelFeatures::empty(), &PROVER, &COMMITMENT_FACTORY)
        .unwrap();

    let tx = stp.get_transaction().unwrap();

    oms.confirm_sent_transaction(sender_tx_id, &tx.body.inputs, &tx.body.outputs)
        .unwrap();

    assert_eq!(oms.pending_transactions().len(), 0);
    assert_eq!(oms.spent_outputs().len(), tx.body.inputs.len());
    assert_eq!(
        oms.unspent_outputs().len(),
        num_outputs + 1 - oms.spent_outputs().len() + num_change
    );
}

#[test]
fn send_not_enough_funds() {
    let mut rng = rand::OsRng::new().unwrap();
    let (secret_key, _public_key) = PublicKey::random_keypair(&mut rng);

    let mut oms = OutputManagerService::new(secret_key, "".to_string(), 0);

    let num_outputs = 20;
    for _i in 0..num_outputs {
        let (_ti, uo) = make_input(&mut rng.clone(), MicroTari::from(100 + rng.next_u64() % 1000));
        oms.add_output(uo).unwrap();
    }

    match oms.prepare_transaction_to_send(MicroTari::from(num_outputs * 2000), MicroTari::from(20), None) {
        Err(OutputManagerError::NotEnoughFunds) => assert!(true),
        _ => assert!(false),
    }
}

#[test]
fn send_no_change() {
    let mut rng = rand::OsRng::new().unwrap();
    let (secret_key, _public_key) = PublicKey::random_keypair(&mut rng);

    let mut oms = OutputManagerService::new(secret_key, "".to_string(), 0);
    let fee_per_gram = MicroTari::from(20);
    let fee_without_change = Fee::calculate(fee_per_gram, 2, 1);
    let key1 = PrivateKey::random(&mut rng);
    let value1 = 500;
    oms.add_output(UnblindedOutput::new(MicroTari::from(value1), key1, None))
        .unwrap();
    let key2 = PrivateKey::random(&mut rng);
    let value2 = 800;
    oms.add_output(UnblindedOutput::new(MicroTari::from(value2), key2, None))
        .unwrap();

    let mut stp = oms
        .prepare_transaction_to_send(
            MicroTari::from(value1 + value2) - fee_without_change,
            MicroTari::from(20),
            None,
        )
        .unwrap();

    let sender_tx_id = stp.get_tx_id().unwrap();
    assert_eq!(stp.get_amount_to_self().unwrap(), MicroTari::from(0));
    assert_eq!(oms.pending_transactions().len(), 1);

    let msg = stp.build_single_round_message().unwrap();

    let b = TestParams::new(&mut rng);

    let recv_info = SingleReceiverTransactionProtocol::create(
        &msg,
        b.nonce,
        b.spend_key,
        OutputFeatures::empty(),
        &PROVER,
        &COMMITMENT_FACTORY,
    )
    .unwrap();

    stp.add_single_recipient_info(recv_info.clone(), &PROVER).unwrap();

    stp.finalize(KernelFeatures::empty(), &PROVER, &COMMITMENT_FACTORY)
        .unwrap();

    let tx = stp.get_transaction().unwrap();

    oms.confirm_sent_transaction(sender_tx_id, &tx.body.inputs, &tx.body.outputs)
        .unwrap();

    assert_eq!(oms.pending_transactions().len(), 0);
    assert_eq!(oms.spent_outputs().len(), tx.body.inputs.len());
    assert_eq!(oms.unspent_outputs().len(), 0);
}

#[test]
fn send_not_enough_for_change() {
    let mut rng = rand::OsRng::new().unwrap();
    let (secret_key, _public_key) = PublicKey::random_keypair(&mut rng);

    let mut oms = OutputManagerService::new(secret_key, "".to_string(), 0);
    let fee_per_gram = MicroTari::from(20);
    let fee_without_change = Fee::calculate(fee_per_gram, 2, 1);
    let key1 = PrivateKey::random(&mut rng);
    let value1 = 500;
    oms.add_output(UnblindedOutput::new(MicroTari::from(value1), key1, None))
        .unwrap();
    let key2 = PrivateKey::random(&mut rng);
    let value2 = 800;
    oms.add_output(UnblindedOutput::new(MicroTari::from(value2), key2, None))
        .unwrap();

    match oms.prepare_transaction_to_send(
        MicroTari::from(value1 + value2 + 1) - fee_without_change,
        MicroTari::from(20),
        None,
    ) {
        Err(OutputManagerError::NotEnoughFunds) => assert!(true),
        _ => assert!(false),
    }
}

#[test]
fn receiving_and_confirmation() {
    let mut rng = rand::OsRng::new().unwrap();
    let (secret_key, _public_key) = PublicKey::random_keypair(&mut rng);

    let mut oms = OutputManagerService::new(secret_key, "".to_string(), 0);
    let value = MicroTari::from(5000);
    let recv_key = oms.get_recipient_spending_key(1, value).unwrap();
    assert_eq!(oms.unspent_outputs().len(), 0);
    assert_eq!(oms.pending_transactions().len(), 1);

    let commitment = COMMITMENT_FACTORY.commit(&recv_key, &value.into());
    let rr = PROVER.construct_proof(&recv_key, value.into()).unwrap();
    let output = TransactionOutput::new(
        OutputFeatures::COINBASE_OUTPUT,
        commitment,
        RangeProof::from_bytes(&rr).unwrap(),
    );

    oms.confirm_received_transaction_output(1, &output).unwrap();

    assert_eq!(oms.pending_transactions().len(), 0);
    assert_eq!(oms.unspent_outputs().len(), 1);
}

#[test]
fn cancel_transaction() {
    let mut rng = rand::OsRng::new().unwrap();
    let (secret_key, _public_key) = PublicKey::random_keypair(&mut rng);

    let mut oms = OutputManagerService::new(secret_key, "".to_string(), 0);

    let num_outputs = 20;
    for _i in 0..num_outputs {
        let (_ti, uo) = make_input(&mut rng.clone(), MicroTari::from(100 + rng.next_u64() % 1000));
        oms.add_output(uo).unwrap();
    }
    let stp = oms
        .prepare_transaction_to_send(MicroTari::from(1000), MicroTari::from(20), None)
        .unwrap();

    assert_eq!(
        oms.cancel_transaction(1),
        Err(OutputManagerError::PendingTransactionNotFound)
    );

    oms.cancel_transaction(stp.get_tx_id().unwrap()).unwrap();

    assert_eq!(oms.unspent_outputs().len(), num_outputs);
}

#[test]
fn timeout_transaction() {
    let mut rng = rand::OsRng::new().unwrap();
    let (secret_key, _public_key) = PublicKey::random_keypair(&mut rng);

    let mut oms = OutputManagerService::new(secret_key, "".to_string(), 0);

    let num_outputs = 20;
    for _i in 0..num_outputs {
        let (_ti, uo) = make_input(&mut rng.clone(), MicroTari::from(100 + rng.next_u64() % 1000));
        oms.add_output(uo).unwrap();
    }
    let _stp = oms
        .prepare_transaction_to_send(MicroTari::from(1000), MicroTari::from(20), None)
        .unwrap();

    let remaining_outputs = oms.unspent_outputs().len();

    thread::sleep(Duration::from_millis(2));

    oms.timeout_pending_transactions(chrono::Duration::milliseconds(10))
        .unwrap();

    assert_eq!(oms.unspent_outputs().len(), remaining_outputs);

    oms.timeout_pending_transactions(chrono::Duration::milliseconds(1))
        .unwrap();

    assert_eq!(oms.unspent_outputs().len(), num_outputs);
}
