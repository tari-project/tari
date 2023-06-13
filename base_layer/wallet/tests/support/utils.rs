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
use tari_common_types::types::{PrivateKey, PublicKey};
use tari_core::{
    test_helpers::TestKeyManager,
    transactions::{
        tari_amount::MicroTari,
        test_helpers::{create_key_manager_output_with_data, TestParams as TestParamsHelpers},
        transaction_components::{WalletOutput, OutputFeatures, TransactionInput},
    },
};
use tari_crypto::keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait};
use tari_script::script;

pub struct TestParams {
    pub spend_key: PrivateKey,
    pub change_spend_key: PrivateKey,
    pub offset: PrivateKey,
    pub nonce: PrivateKey,
    pub public_nonce: PublicKey,
}
impl TestParams {
    pub fn new<R: Rng + CryptoRng>(rng: &mut R) -> TestParams {
        let r = PrivateKey::random(rng);
        TestParams {
            spend_key: PrivateKey::random(rng),
            change_spend_key: PrivateKey::random(rng),
            offset: PrivateKey::random(rng),
            public_nonce: PublicKey::from_secret_key(&r),
            nonce: r,
        }
    }
}

pub async fn make_non_recoverable_input<R: Rng + CryptoRng>(
    _rng: &mut R,
    val: MicroTari,
    features: &OutputFeatures,
    key_manager: &TestKeyManager,
) -> (TransactionInput, WalletOutput) {
    let test_params = TestParamsHelpers::new(key_manager).await;
    let utxo = create_key_manager_output_with_data(script!(Nop), features.clone(), &test_params, val, key_manager)
        .await
        .unwrap();
    (
        utxo.as_transaction_input(key_manager)
            .await
            .expect("Should be able to make transaction input"),
        utxo,
    )
}

pub async fn make_input_with_features<R: Rng + CryptoRng>(
    _rng: &mut R,
    value: MicroTari,
    features: OutputFeatures,
    key_manager: &TestKeyManager,
) -> (TransactionInput, WalletOutput) {
    let test_params = TestParamsHelpers::new(key_manager).await;
    let utxo = create_key_manager_output_with_data(script!(Nop), features, &test_params, value, key_manager)
        .await
        .unwrap();
    (
        utxo.as_transaction_input(key_manager)
            .await
            .expect("Should be able to make transaction input"),
        utxo,
    )
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
