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
use tari_script::TariScript;

use crate::{
    MicroMinotari,
    fee::Fee,
    offline_signing::models::{OneSidedMultisigTransactionInfo, OneSidedTransactionInfo},
    transaction_components::{MemoField, OutputFeatures},
    weight::TransactionWeight,
};

/// A single recipient line in a [`PayloadSummary`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecipientSummary {
    pub address: TariAddress,
    pub amount: MicroMinotari,
    pub payment_id: MemoField,
}

/// A single directly-specified output in a [`PayloadSummary`].
///
/// These are outputs the payload hands to the signer ready-made, rather than ones the signer derives for a recipient.
/// They carry value away from the wallet exactly like a recipient does, but their destination is a raw script rather
/// than an address, so the script is shown verbatim: it is the only thing that says who can spend the output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputSummary {
    pub amount: MicroMinotari,
    pub script: TariScript,
    pub features: OutputFeatures,
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
    /// Outputs the payload specifies directly. No wallet flow produces these, so any entry here is worth the
    /// operator's full attention.
    pub outputs: Vec<OutputSummary>,
    pub fee: MicroMinotari,
    pub fee_per_gram: MicroMinotari,
    /// The sum of all inputs the payload asks to spend.
    pub total_input_value: MicroMinotari,
    /// The sum of all recipient amounts.
    pub total_recipient_amount: MicroMinotari,
    /// The sum of all directly-specified output amounts.
    pub total_output_amount: MicroMinotari,
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
            outputs: info
                .outputs
                .iter()
                .map(|o| OutputSummary {
                    amount: o.value(),
                    script: o.script().clone(),
                    features: o.features().clone(),
                    payment_id: o.payment_id().clone(),
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
            total_output_amount: info
                .outputs
                .iter()
                .fold(MicroMinotari::zero(), |acc, o| acc.saturating_add(o.value())),
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

    /// Whether the payload fixes the fee, or only sets the rate it will be charged at.
    ///
    /// The signer uses `fee_per_gram` whenever it is non-zero and ignores the `fee` field entirely, so a payload
    /// carrying a rate does not determine what will be charged: the final fee depends on the weight of a transaction
    /// that does not exist yet, because the recipient outputs and the change output are only added once the operator
    /// has approved. Every payload the wallet produces today is of this kind.
    pub fn fee_is_fixed(&self) -> bool {
        self.fee_per_gram == MicroMinotari::zero()
    }

    /// The fee, if the payload fixes one. `None` when it will be computed at signing time from `fee_per_gram`, in
    /// which case no honest figure can be shown for it here.
    pub fn fixed_fee(&self) -> Option<MicroMinotari> {
        if self.fee_is_fixed() { Some(self.fee) } else { None }
    }

    /// The total value leaving the wallet: everything paid out, plus the fee when the payload fixes one. Anything
    /// left over from the inputs comes back as change.
    ///
    /// This must cover every value-bearing field of the payload, not just the recipients: the builder charges
    /// directly-specified outputs to the same inputs, so leaving them out would overstate the change by exactly their
    /// total and hide that value from the operator.
    ///
    /// When the fee is not fixed it is excluded rather than guessed, and the rendering says so. A payload's `fee`
    /// field is not merely unreliable in that case, it is unused — the signer never reads it — so counting it would
    /// mean charging the operator for a number that will never be applied.
    pub fn total_spend(&self) -> MicroMinotari {
        let paid_out = self.total_recipient_amount.saturating_add(self.total_output_amount);
        match self.fixed_fee() {
            Some(fee) => paid_out.saturating_add(fee),
            None => paid_out,
        }
    }

    /// A lower bound on what a rate-based payload will be charged, or `None` when the fee is already fixed.
    ///
    /// Calculated from the current weight parameters, counting one kernel, the payload's inputs, and one output per
    /// recipient and directly-specified output plus a change output, with no allowance for features or scripts data.
    /// The real fee can only be larger. It is a floor for warning on, not a figure to present as the fee: see
    /// [`Self::fee_is_fixed`] for why no exact number exists at this point.
    pub fn minimum_fee(&self) -> Option<MicroMinotari> {
        if self.fee_is_fixed() {
            return None;
        }
        // One output per payee, plus change
        let num_outputs = self
            .recipients
            .len()
            .saturating_add(self.outputs.len())
            .saturating_add(1);
        Some(Fee::new(TransactionWeight::latest()).calculate(self.fee_per_gram, 1, self.num_inputs, num_outputs, 0))
    }

    /// Whether the inputs can still cover the spend once the fee is taken.
    ///
    /// [`Self::change`] is computed before the fee when the payload only sets a rate, so on its own it will report a
    /// healthy change figure for a payload that cannot actually be signed. This compares it against
    /// [`Self::minimum_fee`] so that shortfall is caught rather than shown as change.
    pub fn inputs_cover_the_fee(&self) -> bool {
        let Some(change) = self.change() else {
            return false;
        };
        match self.minimum_fee() {
            Some(minimum) => change >= minimum,
            None => true,
        }
    }

    /// The change that should return to the sender. `None` if the inputs do not cover the spend, which is itself a
    /// reason to refuse to sign.
    ///
    /// When [`Self::fee_is_fixed`] is false this is the change *before* the fee, since the fee is not yet known.
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
            writeln!(f, "  [{}] amount  : {}", i.saturating_add(1), recipient.amount)?;
            // Both forms are shown so the operator can check the address against whichever one they were given
            writeln!(f, "      address : {}", recipient.address.to_base58())?;
            writeln!(f, "      emoji   : {}", recipient.address)?;
            if !recipient.payment_id.is_empty() {
                writeln!(f, "      memo    : {}", recipient.payment_id)?;
            }
        }
        if !self.outputs.is_empty() {
            // Nothing a wallet does today puts an output here, so the operator is told plainly that this is not a
            // routine payment rather than being left to infer it from an unfamiliar line.
            writeln!(
                f,
                "Other outputs  : {} (NOT a normal payment - check these carefully)",
                self.outputs.len()
            )?;
            for (i, output) in self.outputs.iter().enumerate() {
                writeln!(f, "  [{}] amount  : {}", i.saturating_add(1), output.amount)?;
                // There is no address to show: whoever can satisfy this script owns the funds
                writeln!(f, "      script  : {}", output.script)?;
                writeln!(
                    f,
                    "      type    : {} (maturity {})",
                    output.features.output_type, output.features.maturity
                )?;
                if !output.payment_id.is_empty() {
                    writeln!(f, "      memo    : {}", output.payment_id)?;
                }
            }
        }
        if let Some(party_number) = self.multisig_party_number {
            writeln!(f, "Multisig party : {party_number}")?;
            for (i, pk) in self.multisig_public_keys.iter().enumerate() {
                writeln!(f, "  [{}] key     : {}", i.saturating_add(1), pk)?;
            }
        }
        // The fee is only a number if the payload fixes one. Otherwise it is a rate, and the amount it works out to
        // depends on outputs that do not exist until after this summary has been approved, so it is described rather
        // than guessed at, and the totals below are qualified to match.
        match self.fixed_fee() {
            Some(fee) => {
                writeln!(f, "Fee            : {fee}")?;
                writeln!(f, "Total spend    : {}", self.total_spend())?;
                match self.change() {
                    Some(change) => writeln!(f, "Change         : {change}")?,
                    None => writeln!(f, "Change         : INPUTS DO NOT COVER THE SPEND")?,
                }
            },
            None => {
                writeln!(
                    f,
                    "Fee            : {}/gram, charged when this is signed",
                    self.fee_per_gram
                )?;
                writeln!(f, "Total spend    : {} plus the fee", self.total_spend())?;
                match (self.change(), self.minimum_fee()) {
                    // The change is computed before the fee, so a payload can cover its outputs and still not cover
                    // the fee. Saying so is the whole point of this screen; showing the leftover as change is not.
                    (Some(change), Some(minimum)) if change < minimum => writeln!(
                        f,
                        "Change         : INPUTS DO NOT COVER THE SPEND ONCE THE FEE IS TAKEN (at least {minimum})"
                    )?,
                    (Some(change), Some(minimum)) => {
                        writeln!(f, "Change         : {change} less the fee (at least {minimum})")?
                    },
                    (Some(change), None) => writeln!(f, "Change         : {change} less the fee")?,
                    (None, _) => writeln!(f, "Change         : INPUTS DO NOT COVER THE SPEND")?,
                }
            },
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use tari_common_types::tari_address::{TariAddress, TariAddressFeatures};
    use tari_crypto::keys::{PublicKey, SecretKey};
    use tari_script::{ExecutionStack, push_pubkey_script};

    use super::*;
    use crate::{
        key_manager::{KeyManager, TariKeyId, TransactionKeyManagerInterface},
        offline_signing::models::PaymentRecipient,
        transaction_components::{EncryptedData, WalletOutput, covenants::Covenant},
    };

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
    fn it_sums_recipients() {
        let summary = PayloadSummary::from_one_sided(TxId::from(42u64), &info(&[1000, 2500]));
        assert_eq!(summary.total_recipient_amount, MicroMinotari::from(3500));
        assert_eq!(summary.recipients.len(), 2);
        assert_eq!(summary.tx_id, TxId::from(42u64));
        // The payload sets a rate, so the signer will ignore its `fee` field and work the fee out itself
        assert!(!summary.fee_is_fixed());
        assert_eq!(summary.fixed_fee(), None);
        assert_eq!(summary.total_spend(), MicroMinotari::from(3500));
    }

    /// A rate is not a fee. The signer ignores `fee` whenever `fee_per_gram` is set, and the amount that will
    /// actually be charged depends on outputs the signer adds after the operator has approved, so the summary says
    /// so instead of printing a figure it cannot stand behind. See GHSA-q6gv-vm66-c4wr.
    #[test]
    fn it_does_not_present_a_rate_as_a_settled_fee() {
        let mut info = info(&[1000]);
        info.inputs = vec![];
        let summary = PayloadSummary::from_one_sided(TxId::from(1u64), &info);
        let rendered = summary.to_string();

        assert!(
            rendered.contains("20 µT/gram, charged when this is signed"),
            "the rate must be shown as a rate:\n{rendered}"
        );
        assert!(
            !rendered.contains("Fee            : 0 µT"),
            "an unset fee field must never be rendered as a zero fee:\n{rendered}"
        );
        assert!(
            rendered.contains("plus the fee"),
            "the total must say the fee comes on top:\n{rendered}"
        );
    }

    #[test]
    fn it_sums_recipients_and_a_fixed_fee() {
        let mut info = info(&[1000, 2500]);
        // With no rate set, the signer charges exactly the payload's fee, so it can be shown as a settled figure
        info.fee_per_gram = MicroMinotari::zero();
        let summary = PayloadSummary::from_one_sided(TxId::from(42u64), &info);

        assert!(summary.fee_is_fixed());
        assert_eq!(summary.fixed_fee(), Some(MicroMinotari::from(100)));
        assert_eq!(summary.total_spend(), MicroMinotari::from(3600));

        let rendered = summary.to_string();
        assert!(rendered.contains("Fee            : 100 µT"), "{rendered}");
        assert!(!rendered.contains("plus the fee"), "{rendered}");
    }

    #[test]
    fn it_reports_when_inputs_do_not_cover_the_spend() {
        let summary = PayloadSummary::from_one_sided(TxId::from(1u64), &info(&[1000]));
        // No inputs were supplied, so the spend cannot be covered
        assert!(summary.change().is_none());
        assert!(summary.to_string().contains("INPUTS DO NOT COVER THE SPEND"));
    }

    /// An output the payload specifies directly must be visible to the operator and charged to the spend, because
    /// the builder will charge it to the same inputs either way. See GHSA-q6gv-vm66-c4wr.
    #[test]
    fn it_renders_and_charges_directly_specified_outputs() {
        let key_manager = KeyManager::new_random().unwrap();
        let destination = KeyManager::new_random().unwrap().get_spend_key().pub_key;
        let amount = MicroMinotari::from(900_000);
        let output = WalletOutput::new(
            Default::default(),
            amount,
            TariKeyId::Zero,
            OutputFeatures::default(),
            push_pubkey_script(&destination),
            ExecutionStack::default(),
            TariKeyId::Zero,
            Default::default(),
            Default::default(),
            0,
            Covenant::default(),
            EncryptedData::default(),
            MicroMinotari::zero(),
            MemoField::new_empty(),
            &key_manager,
        )
        .unwrap();

        let mut info = info(&[1000]);
        info.outputs = vec![output];

        let summary = PayloadSummary::from_one_sided(TxId::from(1u64), &info);
        assert_eq!(summary.outputs.len(), 1);
        assert_eq!(summary.total_output_amount, amount);
        // recipients + directly-specified outputs; the fee is a rate here, so it is charged on top
        assert_eq!(summary.total_spend(), MicroMinotari::from(1000 + 900_000));

        let rendered = summary.to_string();
        assert!(
            rendered.contains("900000 µT"),
            "the output's value must be shown:\n{rendered}"
        );
        assert!(
            rendered.contains(&push_pubkey_script(&destination).to_string()),
            "the script that owns the output must be shown:\n{rendered}"
        );
    }

    /// Every value-bearing field of the payload has to reach the operator. A field that is signed but not rendered is
    /// the whole of GHSA-q6gv-vm66-c4wr, so it is asserted directly rather than left to the individual tests above.
    #[test]
    fn it_accounts_for_every_value_bearing_field() {
        let key_manager = KeyManager::new_random().unwrap();
        let output = |value: u64| {
            WalletOutput::new(
                Default::default(),
                MicroMinotari::from(value),
                TariKeyId::Zero,
                OutputFeatures::default(),
                push_pubkey_script(&key_manager.get_spend_key().pub_key),
                ExecutionStack::default(),
                TariKeyId::Zero,
                Default::default(),
                Default::default(),
                0,
                Covenant::default(),
                EncryptedData::default(),
                MicroMinotari::zero(),
                MemoField::new_empty(),
                &key_manager,
            )
            .unwrap()
        };

        let mut info = info(&[1000, 2500]);
        info.inputs = vec![output(1_000_000)];
        info.outputs = vec![output(300), output(700)];

        let summary = PayloadSummary::from_one_sided(TxId::from(1u64), &info);

        // Everything the payload spends: recipients and directly-specified outputs. The fee is a rate here, so it
        // is charged on top of this and the rendering says as much.
        let expected_spend = MicroMinotari::from(1000 + 2500 + 300 + 700);
        assert_eq!(summary.total_spend(), expected_spend);
        assert_eq!(
            summary.change(),
            Some(MicroMinotari::from(1_000_000) - expected_spend),
            "the change shown must be what is left after every value-bearing field"
        );
    }

    /// A payload whose inputs cover the outputs but not the fee cannot be signed, and must not be rendered as though
    /// it leaves change behind.
    #[test]
    fn it_warns_when_the_inputs_will_not_cover_the_fee() {
        let key_manager = KeyManager::new_random().unwrap();
        let input = |value: u64| {
            WalletOutput::new(
                Default::default(),
                MicroMinotari::from(value),
                TariKeyId::Zero,
                OutputFeatures::default(),
                push_pubkey_script(&key_manager.get_spend_key().pub_key),
                ExecutionStack::default(),
                TariKeyId::Zero,
                Default::default(),
                Default::default(),
                0,
                Covenant::default(),
                EncryptedData::default(),
                MicroMinotari::zero(),
                MemoField::new_empty(),
                &key_manager,
            )
            .unwrap()
        };

        let mut info = info(&[1000]);
        // Covers the recipient with a single µT to spare, nowhere near the fee at 20 µT/gram
        info.inputs = vec![input(1001)];

        let summary = PayloadSummary::from_one_sided(TxId::from(1u64), &info);
        let minimum = summary.minimum_fee().expect("a rate-based payload has a fee floor");
        assert!(minimum > MicroMinotari::from(1));
        assert_eq!(summary.change(), Some(MicroMinotari::from(1)), "change before the fee");
        assert!(!summary.inputs_cover_the_fee());

        let rendered = summary.to_string();
        assert!(
            rendered.contains("INPUTS DO NOT COVER THE SPEND ONCE THE FEE IS TAKEN"),
            "the shortfall must be called out rather than shown as change:\n{rendered}"
        );

        // With enough to cover it, the bound is shown alongside the change instead
        info.inputs = vec![input(1_000_000)];
        let summary = PayloadSummary::from_one_sided(TxId::from(1u64), &info);
        assert!(summary.inputs_cover_the_fee());
        let rendered = summary.to_string();
        assert!(rendered.contains("less the fee (at least"), "{rendered}");
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
