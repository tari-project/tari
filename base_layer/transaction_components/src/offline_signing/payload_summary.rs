// Copyright 2025. The Tari Project
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

//! Human-readable summaries of offline-signing payloads.
//!
//! # Why this exists
//!
//! The `PayloadIntegritySignature` carried by a `Prepare*` result is produced with the wallet's **view key**. The view
//! key is a *shareable* key by design — it is handed to view-only wallets, auditors, exchanges and block explorers so
//! that they can see incoming payments. It follows that the integrity signature only proves that the payload was not
//! mangled by a party that has *no* view access; it proves nothing at all against anyone who holds the view key. Such
//! a party can build a payload that pays themselves, sign it with the view key, and the air-gapped signer will verify
//! it as authentic.
//!
//! The only defence that holds against a view-key holder is for the human operating the air-gapped signer to check
//! what is about to be signed. This module produces the summary that is shown to them, so that the rendering lives
//! next to the payload types and cannot drift out of sync with the fields that are actually signed.

use std::fmt::{Display, Formatter};

use tari_common_types::{tari_address::TariAddress, transaction::TxId};

use crate::{
    MicroMinotari,
    offline_signing::models::{OneSidedMultisigTransactionInfo, OneSidedTransactionInfo},
    transaction_components::MemoField,
};

/// A single recipient line in a [`PayloadSummary`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecipientSummary {
    pub address: TariAddress,
    pub amount: MicroMinotari,
    pub payment_id: MemoField,
}

/// The security-relevant contents of an offline-signing payload, in a form that can be shown to a human before any
/// spend key is used.
///
/// Every field here is covered by the canonical Borsh bytes that the integrity signature commits to, so a summary that
/// the operator approves is a summary of what will actually be signed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayloadSummary {
    pub tx_id: TxId,
    pub sender_address: TariAddress,
    pub payment_id: MemoField,
    pub recipients: Vec<RecipientSummary>,
    pub fee: MicroMinotari,
    pub fee_per_gram: MicroMinotari,
    /// The sum of all inputs the payload asks to spend.
    pub total_input_value: MicroMinotari,
    /// The sum of all recipient amounts.
    pub total_recipient_amount: MicroMinotari,
    pub num_inputs: usize,
    /// The multisig parties this payload pays into, if it is a multisig deposit.
    pub multisig_public_keys: Vec<String>,
    /// The party number this signer is acting as, if it is a multisig payload.
    pub multisig_party_number: Option<u8>,
}

impl PayloadSummary {
    /// Builds a summary of a one-sided payload.
    pub fn from_one_sided(tx_id: TxId, info: &OneSidedTransactionInfo) -> Self {
        Self {
            tx_id,
            sender_address: info.sender_address.clone(),
            payment_id: info.payment_id.clone(),
            recipients: info
                .recipients
                .iter()
                .map(|r| RecipientSummary {
                    address: r.address.clone(),
                    amount: r.amount,
                    payment_id: r.payment_id.clone(),
                })
                .collect(),
            fee: info.fee,
            fee_per_gram: info.fee_per_gram,
            // Saturating: a payload is attacker-controlled, and a summary must never panic before it can be shown
            total_input_value: info
                .inputs
                .iter()
                .fold(MicroMinotari::zero(), |acc, o| acc.saturating_add(o.value())),
            total_recipient_amount: info
                .recipients
                .iter()
                .fold(MicroMinotari::zero(), |acc, r| acc.saturating_add(r.amount)),
            num_inputs: info.inputs.len(),
            multisig_public_keys: Vec::new(),
            multisig_party_number: None,
        }
    }

    /// Builds a summary of a multisig payload.
    pub fn from_multisig(tx_id: TxId, info: &OneSidedMultisigTransactionInfo) -> Self {
        use tari_utilities::hex::Hex;

        let mut summary = Self::from_one_sided(tx_id, &info.base);
        summary.multisig_public_keys = info.public_keys.iter().map(|pk| pk.to_hex()).collect();
        summary.multisig_party_number = Some(info.party_number);
        summary
    }

    /// The total value leaving the wallet, i.e. everything paid out plus the fee. Anything left over from the inputs
    /// comes back as change.
    pub fn total_spend(&self) -> MicroMinotari {
        self.total_recipient_amount.saturating_add(self.fee)
    }

    /// The change that should return to the sender. `None` if the inputs do not cover the spend, which is itself a
    /// reason to refuse to sign.
    pub fn change(&self) -> Option<MicroMinotari> {
        self.total_input_value.checked_sub(self.total_spend())
    }
}

impl Display for PayloadSummary {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Transaction ID : {}", self.tx_id)?;
        writeln!(f, "From           : {}", self.sender_address.to_base58())?;
        if !self.payment_id.is_empty() {
            writeln!(f, "Payment ID     : {}", self.payment_id)?;
        }
        writeln!(
            f,
            "Inputs         : {} totalling {}",
            self.num_inputs, self.total_input_value
        )?;
        writeln!(f, "Recipients     : {}", self.recipients.len())?;
        for (i, recipient) in self.recipients.iter().enumerate() {
            writeln!(f, "  [{}] amount  : {}", i + 1, recipient.amount)?;
            // Both forms are shown so the operator can check the address against whichever one they were given
            writeln!(f, "      address : {}", recipient.address.to_base58())?;
            writeln!(f, "      emoji   : {}", recipient.address)?;
            if !recipient.payment_id.is_empty() {
                writeln!(f, "      memo    : {}", recipient.payment_id)?;
            }
        }
        if let Some(party_number) = self.multisig_party_number {
            writeln!(f, "Multisig party : {party_number}")?;
            for (i, pk) in self.multisig_public_keys.iter().enumerate() {
                writeln!(f, "  [{}] key     : {}", i + 1, pk)?;
            }
        }
        writeln!(f, "Fee            : {} (at {}/gram)", self.fee, self.fee_per_gram)?;
        writeln!(f, "Total spend    : {}", self.total_spend())?;
        match self.change() {
            Some(change) => writeln!(f, "Change         : {change}")?,
            None => writeln!(f, "Change         : INPUTS DO NOT COVER THE SPEND")?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use tari_common_types::tari_address::{TariAddress, TariAddressFeatures};
    use tari_crypto::keys::{PublicKey, SecretKey};

    use super::*;
    use crate::{offline_signing::models::PaymentRecipient, transaction_components::OutputFeatures};

    fn address() -> TariAddress {
        use tari_common::configuration::Network;
        use tari_common_types::types::{CompressedPublicKey, PrivateKey, UncompressedPublicKey};

        let view = UncompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng()));
        let spend = UncompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng()));
        TariAddress::new_dual_address(
            CompressedPublicKey::new_from_pk(view),
            CompressedPublicKey::new_from_pk(spend),
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap()
    }

    fn info(amounts: &[u64]) -> OneSidedTransactionInfo {
        OneSidedTransactionInfo {
            payment_id: MemoField::new_empty(),
            recipients: amounts
                .iter()
                .map(|a| PaymentRecipient {
                    amount: MicroMinotari::from(*a),
                    output_features: OutputFeatures::default(),
                    address: address(),
                    payment_id: MemoField::new_empty(),
                })
                .collect(),
            inputs: vec![],
            outputs: vec![],
            fee: MicroMinotari::from(100),
            fee_per_gram: MicroMinotari::from(20),
            sender_address: address(),
        }
    }

    #[test]
    fn it_sums_recipients_and_fee() {
        let summary = PayloadSummary::from_one_sided(TxId::from(42u64), &info(&[1000, 2500]));
        assert_eq!(summary.total_recipient_amount, MicroMinotari::from(3500));
        assert_eq!(summary.total_spend(), MicroMinotari::from(3600));
        assert_eq!(summary.recipients.len(), 2);
        assert_eq!(summary.tx_id, TxId::from(42u64));
    }

    #[test]
    fn it_reports_when_inputs_do_not_cover_the_spend() {
        let summary = PayloadSummary::from_one_sided(TxId::from(1u64), &info(&[1000]));
        // No inputs were supplied, so the spend cannot be covered
        assert!(summary.change().is_none());
        assert!(summary.to_string().contains("INPUTS DO NOT COVER THE SPEND"));
    }

    #[test]
    fn it_renders_every_recipient_address_and_amount() {
        let info = info(&[1000, 2500]);
        let summary = PayloadSummary::from_one_sided(TxId::from(7u64), &info);
        let rendered = summary.to_string();
        for recipient in &info.recipients {
            assert!(
                rendered.contains(&recipient.address.to_base58()),
                "recipient address missing from summary:\n{rendered}"
            );
        }
        assert!(rendered.contains("1000 µT"), "amount missing from summary:\n{rendered}");
        assert!(rendered.contains("2500 µT"), "amount missing from summary:\n{rendered}");
    }
}
