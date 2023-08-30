//  Copyright 2019 The Tari Project
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

use rand::{CryptoRng, Rng};
use tari_core::{
    covenants::Covenant,
    transactions::{
        key_manager::TransactionKeyManagerInterface,
        tari_amount::MicroMinotari,
        test_helpers::{create_wallet_output_with_data, TestKeyManager, TestParams},
        transaction_components::{
            OutputFeatures,
            RangeProofType,
            TransactionOutput,
            TransactionOutputVersion,
            WalletOutput,
        },
        transaction_protocol::sender::TransactionSenderMessage,
    },
};
use tari_key_manager::key_manager_service::KeyManagerInterface;
use tari_script::{inputs, script, TariScript};

pub async fn make_input<R: Rng + CryptoRng>(
    _rng: &mut R,
    val: MicroMinotari,
    features: &OutputFeatures,
    key_manager: &TestKeyManager,
) -> WalletOutput {
    let test_params = TestParams::new(key_manager).await;
    create_wallet_output_with_data(TariScript::default(), features.clone(), &test_params, val, key_manager)
        .await
        .unwrap()
}

pub async fn create_wallet_output_from_sender_data(
    info: &TransactionSenderMessage,
    key_manager: &TestKeyManager,
) -> WalletOutput {
    let test_params = TestParams::new(key_manager).await;
    let sender_data = info.single().unwrap();
    let public_script_key = key_manager
        .get_public_key_at_key_id(&test_params.script_key_id)
        .await
        .unwrap();
    let encrypted_data = key_manager
        .encrypt_data_for_recovery(&test_params.spend_key_id, None, sender_data.amount.as_u64())
        .await
        .unwrap();
    let mut utxo = WalletOutput::new(
        TransactionOutputVersion::get_current_version(),
        sender_data.amount,
        test_params.spend_key_id.clone(),
        sender_data.features.clone(),
        sender_data.script.clone(),
        inputs!(public_script_key),
        test_params.script_key_id.clone(),
        sender_data.sender_offset_public_key.clone(),
        Default::default(),
        0,
        Covenant::default(),
        encrypted_data,
        MicroMinotari::zero(),
        key_manager,
    )
    .await
    .unwrap();
    let output_message = TransactionOutput::metadata_signature_message(&utxo);
    utxo.metadata_signature = key_manager
        .get_receiver_partial_metadata_signature(
            &test_params.spend_key_id,
            &sender_data.amount.into(),
            &sender_data.sender_offset_public_key,
            &sender_data.ephemeral_public_nonce,
            &TransactionOutputVersion::get_current_version(),
            &output_message,
            RangeProofType::BulletProofPlus,
        )
        .await
        .unwrap();
    utxo
}

pub async fn make_input_with_features<R: Rng + CryptoRng>(
    _rng: &mut R,
    value: MicroMinotari,
    features: OutputFeatures,
    key_manager: &TestKeyManager,
) -> WalletOutput {
    let test_params = TestParams::new(key_manager).await;
    create_wallet_output_with_data(script!(Nop), features, &test_params, value, key_manager)
        .await
        .unwrap()
}

/// This macro unlocks a Mutex or RwLock. If the lock is
/// poisoned (i.e. panic while unlocked) the last value
/// before the panic is used.
macro_rules! acquire_lock {
    ($e:expr, $m:ident) => {
        match $e.$m() {
            Ok(lock) => lock,
            Err(poisoned) => {
                log::warn!(target: "wallet", "Lock has been POISONED and will be silently recovered");
                poisoned.into_inner()
            },
        }
    };
    ($e:expr) => {
        acquire_lock!($e, lock)
    };
}
