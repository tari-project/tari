//   Copyright 2022. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::default::Default;

use tari_common_types::types::{Commitment, PrivateKey, Signature};
use tari_core::transactions::{
    tari_amount::MicroTari,
    test_helpers::TestParams,
    transaction_components::{
        KernelBuilder,
        Transaction,
        TransactionBuilder,
        TransactionInput,
        TransactionKernel,
        TransactionKernelVersion,
        TransactionOutput,
        UnblindedOutput,
        UnblindedOutputBuilder,
    },
    transaction_protocol::TransactionMetadata,
    CryptoFactories,
};
use tari_crypto::{
    keys::PublicKey,
    ristretto::{
        pedersen::extended_commitment_factory::ExtendedPedersenCommitmentFactory,
        RistrettoPublicKey,
        RistrettoSecretKey,
    },
};
use tari_script::{inputs, script};

#[derive(Clone)]
struct TestTransactionBuilder {
    amount: MicroTari,
    factories: CryptoFactories,
    fee: MicroTari,
    inputs_max_height: u64,
    inputs: Vec<(TransactionInput, UnblindedOutput)>,
    keys: TestParams,
    lock_height: u64,
    output: Option<(TransactionOutput, UnblindedOutput)>,
}

impl TestTransactionBuilder {
    pub fn new() -> Self {
        Self {
            amount: MicroTari(0),
            factories: CryptoFactories::default(),
            fee: MicroTari(0),
            inputs_max_height: 0,
            inputs: vec![],
            keys: TestParams::new(),
            lock_height: 0,
            output: None,
        }
    }

    pub fn change_fee(&mut self, fee: MicroTari) -> &mut Self {
        self.fee = fee;
        self
    }

    pub fn update_inputs_max_height(&mut self, height: u64) -> &mut Self {
        self.inputs_max_height = height;
        self
    }

    fn update_amount(&mut self, amount: MicroTari) {
        self.amount += amount
    }

    pub fn add_input(&mut self, u: UnblindedOutput) -> &mut Self {
        self.update_amount(u.value);

        if u.features.maturity > self.inputs_max_height {
            self.update_inputs_max_height(u.features.maturity);
        }

        self.inputs.push((
            u.as_transaction_input(&ExtendedPedersenCommitmentFactory::default())
                .expect("The Unblinded output to convert to an Input"),
            u,
        ));

        self
    }

    pub fn build(mut self) -> (Transaction, UnblindedOutput) {
        self.create_non_recoverable_utxo();

        let (script_offset_pvt, offset, kernel) = &self.build_kernel();

        let output = self.output.clone().unwrap();

        let mut tx_builder = TransactionBuilder::new();
        tx_builder
            .add_inputs(&mut self.inputs.iter().map(|f| f.0.clone()).collect())
            .add_output(self.output.unwrap().0)
            .add_offset(offset.clone())
            .add_script_offset(script_offset_pvt.clone())
            .with_kernel(kernel.clone());

        let tx = tx_builder.build().unwrap();
        (tx, output.1)
    }

    pub fn build_kernel(&self) -> (PrivateKey, RistrettoSecretKey, TransactionKernel) {
        let input = &self.inputs[0].1.clone();
        let output = &self.output.clone().unwrap().1;

        let fee = self.fee;
        let nonce = PrivateKey::default() + self.keys.nonce.clone();
        let offset = PrivateKey::default() + self.keys.offset.clone();

        let script_offset_pvt = input.script_private_key.clone() - self.keys.sender_offset_private_key.clone();
        let excess_blinding_factor = output.spending_key.clone() - input.spending_key.clone();

        let tx_meta = TransactionMetadata::new(fee, self.lock_height);

        let public_nonce = PublicKey::from_secret_key(&nonce);
        let offset_blinding_factor = &excess_blinding_factor - &offset;
        let excess = PublicKey::from_secret_key(&offset_blinding_factor);
        let e = TransactionKernel::build_kernel_challenge_from_tx_meta(
            &TransactionKernelVersion::get_current_version(),
            &public_nonce,
            &excess,
            &tx_meta,
        );
        let k = offset_blinding_factor;
        let r = nonce;
        let s = Signature::sign_raw(&k, r, &e).unwrap();

        let kernel = KernelBuilder::new()
            .with_fee(self.fee)
            .with_lock_height(self.lock_height)
            .with_excess(&Commitment::from_public_key(&excess))
            .with_signature(&s)
            .build()
            .unwrap();

        (script_offset_pvt, offset, kernel)
    }

    fn calculate_spendable(&self) -> MicroTari {
        MicroTari(self.amount.0 - self.fee.0)
    }

    fn create_non_recoverable_utxo(&mut self) {
        let input_data: RistrettoPublicKey = PublicKey::from_secret_key(&self.keys.script_private_key);

        let mut builder = KeyManagerOutputBuilder::new(self.calculate_spendable(), self.keys.spend_key.clone())
            .with_features(Default::default())
            .with_script(script!(Nop))
            .with_script_private_key(self.keys.script_private_key.clone())
            .with_input_data(inputs!(input_data));
        builder.with_sender_offset_public_key(self.keys.sender_offset_public_key.clone());
        builder
            .sign_as_sender_and_receiver(&self.keys.sender_offset_private_key.clone())
            .expect("sign as sender and receiver");
        let unblinded = builder.try_build().expect("Get output from unblinded output");
        let utxo = unblinded
            .as_transaction_output(&self.factories)
            .expect("unblinded into output");

        self.output = Some((utxo, unblinded));
    }
}

pub fn build_transaction_with_output_and_fee(utxos: Vec<UnblindedOutput>, fee: u64) -> (Transaction, UnblindedOutput) {
    let mut builder = TestTransactionBuilder::new();
    for unblinded_output in utxos {
        builder.add_input(unblinded_output);
    }
    builder.change_fee(MicroTari(fee));

    builder.build()
}

pub fn build_transaction_with_output_and_lockheight(
    utxos: Vec<UnblindedOutput>,
    lockheight: u64,
) -> (Transaction, UnblindedOutput) {
    let mut builder = TestTransactionBuilder::new();
    for unblinded_output in utxos {
        builder.add_input(unblinded_output);
    }
    builder.lock_height = lockheight;

    builder.build()
}

pub fn build_transaction_with_output(utxos: Vec<UnblindedOutput>) -> (Transaction, UnblindedOutput) {
    let mut builder = TestTransactionBuilder::new();
    for unblinded_output in utxos {
        builder.add_input(unblinded_output);
    }

    builder.build()
}
