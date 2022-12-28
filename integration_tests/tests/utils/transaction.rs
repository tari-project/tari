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

use std::convert::TryFrom;

use rand::rngs::OsRng;
use tari_common_types::types::{ComAndPubSignature, PrivateKey, RangeProof};
use tari_core::{
    covenants::Covenant,
    transactions::{
        tari_amount::MicroTari,
        transaction_components::{
            EncryptedValue,
            OutputFeatures,
            Transaction,
            TransactionBuilder,
            TransactionInput,
            TransactionInputVersion,
            TransactionOutput,
            TransactionOutputVersion,
        },
        CryptoFactories,
    },
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey, SecretKey},
    range_proof::RangeProofService,
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
};
use tari_script::{ExecutionStack, TariScript};
use tari_utilities::ByteArray;

pub fn build_transaction_with_output(utxos: &[(u64, TransactionOutput)]) -> (u64, TransactionOutput, Transaction) {
    let inputs = utxos
        .iter()
        .map(|(_, u)| {
            TransactionInput::new_with_output_data(
                TransactionInputVersion::try_from(u.version.as_u8()).unwrap(),
                u.features.clone(),
                u.commitment.clone(),
                u.script.clone(),
                ExecutionStack::new(vec![]),
                ComAndPubSignature::default(),
                u.sender_offset_public_key.clone(),
                u.covenant.clone(),
                u.encrypted_value.clone(),
                u.minimum_value_promise,
            )
        })
        .collect::<Vec<_>>();
    let mut tx_builder = TransactionBuilder::new();

    for input in &inputs {
        tx_builder.add_input(input.clone());
    }

    let spendable_amount = utxos.iter().map(|x| x.0).sum();
    let output = build_output(spendable_amount);
    tx_builder.add_output(output.clone());

    let factories = CryptoFactories::default();
    let tx = tx_builder.build(&factories, None, 0).unwrap();

    (spendable_amount, output, tx)
}

pub fn build_output(spendable_amount: u64) -> TransactionOutput {
    let version = TransactionOutputVersion::V0;
    let features = OutputFeatures::default();
    let factories = CryptoFactories::default();
    let spending_key = PrivateKey::random(&mut OsRng);
    let commitment = factories.commitment.commit_value(&spending_key, spendable_amount);
    let proof = RangeProof::from_bytes(
        factories
            .range_proof
            .construct_proof(&spending_key, spendable_amount)
            .unwrap()
            .as_slice(),
    )
    .unwrap();
    let script = TariScript::default();
    let sender_offset_key = RistrettoSecretKey::random(&mut OsRng);
    let sender_offset_public_key = RistrettoPublicKey::from_secret_key(&sender_offset_key);
    let covenant = Covenant::default();
    let encrypted_value =
        EncryptedValue::encrypt_value(&spending_key, &commitment, MicroTari(spendable_amount)).unwrap();
    let minimum_value_promise = MicroTari(0u64);

    let metadata_signature = TransactionOutput::create_metadata_signature(
        TransactionOutputVersion::get_current_version(),
        spendable_amount.into(),
        &spending_key,
        &script,
        &features,
        &sender_offset_key,
        &covenant,
        &EncryptedValue::default(),
        minimum_value_promise,
    )
    .unwrap();

    TransactionOutput {
        version,
        features,
        commitment,
        proof,
        script,
        sender_offset_public_key,
        metadata_signature,
        covenant,
        encrypted_value,
        minimum_value_promise,
    }
}
