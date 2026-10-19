//  Copyright 2022, The Tari Project
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

// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use tari_common_types::tari_address::TariAddress;
use tari_transaction_components::{key_manager::RecoveryProvenance, transaction_components::WalletOutput};

pub mod handle;
pub mod initializer;
pub mod service;
mod utxo_scanner_task;
pub mod uxto_scanner_service_builder;

pub const RECOVERY_KEY: &str = "recovery_data";

/// The source address recorded for an imported output. Only a coinbase or a self-authored memo yields our own
/// address; an AddressAndData sender address is returned as-is and is recorded as a claim (see
/// `CompletedTransaction::source_address_claimed`).
pub fn imported_output_source_address(
    output: &WalletOutput,
    provenance: RecoveryProvenance,
    own_one_sided_address: &TariAddress,
) -> TariAddress {
    if output.is_coinbase() {
        // It's a coinbase, so we know we mined it (we do mining with cold wallets).
        return own_one_sided_address.clone();
    }
    if let Some(address) = output.payment_id().claimed_sender_address() {
        return address;
    }
    if output.payment_id().is_transaction_info() && provenance.memo_is_self_authored() {
        return own_one_sided_address.clone();
    }
    TariAddress::default()
}

#[cfg(test)]
mod test {
    use tari_common::configuration::Network;
    use tari_common_types::{
        tari_address::TariAddress,
        types::{CompressedPublicKey, PrivateKey},
    };
    use tari_crypto::keys::SecretKey;
    use tari_transaction_components::{
        MicroMinotari,
        key_manager::{KeyManager, RecoveryProvenance},
        test_helpers::{TestParams, UtxoTestParams},
        transaction_components::{MemoField, OutputFeatures, RangeProofType, WalletOutput, memo_field::TxType},
    };

    use super::imported_output_source_address;

    fn random_address() -> TariAddress {
        TariAddress::new_dual_address_with_default_features(
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
            Network::LocalNet,
        )
        .unwrap()
    }

    fn output_with(features: OutputFeatures, memo: MemoField) -> WalletOutput {
        let key_manager = KeyManager::new_random().unwrap();
        TestParams::new(&key_manager)
            .create_output(
                UtxoTestParams {
                    value: MicroMinotari::from(1_000),
                    features,
                    payment_id: memo,
                    ..Default::default()
                },
                &key_manager,
            )
            .unwrap()
    }

    #[test]
    fn imported_source_address_follows_recovery_kind() {
        let own = random_address();
        let sender = random_address();
        let both = [RecoveryProvenance::OwnViewKey, RecoveryProvenance::DiffieHellman];

        let coinbase = output_with(
            OutputFeatures::create_coinbase(0, None, RangeProofType::BulletProofPlus),
            MemoField::new_empty(),
        );
        for provenance in both {
            assert_eq!(imported_output_source_address(&coinbase, provenance, &own), own);
        }

        let address_and_data = output_with(
            OutputFeatures::default(),
            MemoField::new_address_and_data(sender.clone(), 0.into(), true, TxType::PaymentToOther, vec![]).unwrap(),
        );
        for provenance in both {
            assert_eq!(
                imported_output_source_address(&address_and_data, provenance, &own),
                sender
            );
        }

        let transaction_info = output_with(
            OutputFeatures::default(),
            MemoField::new_transaction_info(
                sender.clone(),
                5_000.into(),
                10.into(),
                false,
                TxType::PaymentToOther,
                vec![],
                vec![],
            )
            .unwrap(),
        );
        assert_eq!(
            imported_output_source_address(&transaction_info, RecoveryProvenance::OwnViewKey, &own),
            own
        );
        assert_eq!(
            imported_output_source_address(&transaction_info, RecoveryProvenance::DiffieHellman, &own),
            TariAddress::default()
        );

        let empty = output_with(OutputFeatures::default(), MemoField::new_empty());
        for provenance in both {
            assert_eq!(
                imported_output_source_address(&empty, provenance, &own),
                TariAddress::default()
            );
        }
    }
}
