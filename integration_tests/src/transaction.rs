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

use tari_core::{
    borsh::SerializedSize,
    covenants::Covenant,
    transactions::{
        key_manager::TariKeyId,
        tari_amount::MicroTari,
        test_helpers::{create_transaction_with, TestKeyManager, TestParams},
        transaction_components::{
            OutputFeatures,
            Transaction,
            TransactionInput,
            TransactionOutput,
            WalletOutput,
            WalletOutputBuilder,
        },
        weight::TransactionWeight,
    },
};
use tari_script::{inputs, script, TariScript};

#[derive(Clone)]
struct TestTransactionBuilder {
    amount: MicroTari,
    fee_per_gram: MicroTari,
    inputs_max_height: u64,
    inputs: Vec<(TransactionInput, WalletOutput)>,
    keys: TestParams,
    lock_height: u64,
    output: Option<(TransactionOutput, WalletOutput, TariKeyId)>,
}

impl TestTransactionBuilder {
    pub async fn new(key_manager: &TestKeyManager) -> Self {
        Self {
            amount: MicroTari(0),
            fee_per_gram: MicroTari(1),
            inputs_max_height: 0,
            inputs: vec![],
            keys: TestParams::new(key_manager).await,
            lock_height: 0,
            output: None,
        }
    }

    pub fn fee_per_gram(&mut self, fee: MicroTari) -> &mut Self {
        self.fee_per_gram = fee;
        self
    }

    pub fn update_inputs_max_height(&mut self, height: u64) -> &mut Self {
        self.inputs_max_height = height;
        self
    }

    fn update_amount(&mut self, amount: MicroTari) {
        self.amount += amount
    }

    pub async fn add_input(&mut self, u: WalletOutput, key_manager: &TestKeyManager) -> &mut Self {
        self.update_amount(u.value);

        if u.features.maturity > self.inputs_max_height {
            self.update_inputs_max_height(u.features.maturity);
        }

        self.inputs.push((
            u.to_transaction_input(key_manager)
                .await
                .expect("The wallet output to convert to an Input"),
            u,
        ));

        self
    }

    pub async fn build(mut self, key_manager: &TestKeyManager) -> (Transaction, WalletOutput) {
        self.create_utxo(key_manager, self.inputs.len()).await;

        let inputs = self.inputs.iter().map(|f| f.1.clone()).collect();
        let outputs = vec![(self.output.clone().unwrap().1, self.output.clone().unwrap().2)];
        let tx = create_transaction_with(self.lock_height, self.fee_per_gram, inputs, outputs, key_manager).await;

        (tx, self.output.clone().unwrap().1)
    }

    async fn create_utxo(&mut self, key_manager: &TestKeyManager, num_inputs: usize) {
        let script = script!(Nop);
        let features = OutputFeatures::default();
        let covenant = Covenant::default();
        let value = self.amount - self.estimate_fee(num_inputs, features.clone(), script.clone(), covenant.clone());
        let builder = WalletOutputBuilder::new(value, self.keys.spend_key_id.clone())
            .with_features(features)
            .with_script(script)
            .with_script_key(self.keys.script_key_id.clone())
            .with_input_data(inputs!(self.keys.script_key_pk.clone()))
            .with_sender_offset_public_key(self.keys.sender_offset_key_pk.clone())
            .sign_as_sender_and_receiver(key_manager, &self.keys.sender_offset_key_id.clone())
            .await
            .expect("sign as sender and receiver");
        let wallet_output = builder
            .try_build(key_manager)
            .await
            .expect("Get output from wallet output");
        let utxo = wallet_output
            .to_transaction_output(key_manager)
            .await
            .expect("wallet into output");

        self.output = Some((utxo, wallet_output, self.keys.sender_offset_key_id.clone()));
    }

    fn estimate_fee(
        &self,
        num_inputs: usize,
        features: OutputFeatures,
        script: TariScript,
        covenant: Covenant,
    ) -> std::io::Result<MicroTari> {
        let features_and_scripts_bytes =
            features.get_serialized_size()? + script.get_serialized_size()? + covenant.get_serialized_size()?;
        let weights = TransactionWeight::v1();
        let fee = self.fee_per_gram.0 * weights.calculate(1, num_inputs, 1 + 1, features_and_scripts_bytes);
        Ok(MicroTari(fee))
    }
}

pub async fn build_transaction_with_output_and_fee_per_gram(
    utxos: Vec<WalletOutput>,
    fee_per_gram: u64,
    key_manager: &TestKeyManager,
) -> (Transaction, WalletOutput) {
    let mut builder = TestTransactionBuilder::new(key_manager).await;
    for wallet_output in utxos {
        builder.add_input(wallet_output, key_manager).await;
    }
    builder.fee_per_gram(MicroTari(fee_per_gram));

    builder.build(key_manager).await
}

pub async fn build_transaction_with_output_and_lockheight(
    utxos: Vec<WalletOutput>,
    lockheight: u64,
    key_manager: &TestKeyManager,
) -> (Transaction, WalletOutput) {
    let mut builder = TestTransactionBuilder::new(key_manager).await;
    for wallet_output in utxos {
        builder.add_input(wallet_output, key_manager).await;
    }
    builder.lock_height = lockheight;

    builder.build(key_manager).await
}

pub async fn build_transaction_with_output(
    utxos: Vec<WalletOutput>,
    key_manager: &TestKeyManager,
) -> (Transaction, WalletOutput) {
    let mut builder = TestTransactionBuilder::new(key_manager).await;
    for wallet_output in utxos {
        builder.add_input(wallet_output, key_manager).await;
    }

    builder.build(key_manager).await
}
