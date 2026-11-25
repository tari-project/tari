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
//! Unless the online wallet has imported a payload key and the signer requires one (see
//! [`crate::offline_signing::PayloadKeyPolicy`]), the `PayloadIntegritySignature` carried by a `Prepare*` result is
//! produced with the wallet's **view key**. The view key is a *shareable* key by design — it is handed to view-only
//! wallets, auditors, exchanges and block explorers so that they can see incoming payments. It follows that the
//! integrity signature only proves that the payload was not mangled by a party that has *no* view access; it proves
//! nothing at all against anyone who holds the view key. Such a party can build a payload that pays themselves, sign it
//! with the view key, and the air-gapped signer will verify it as authentic.
//!
//! The only defence that holds against a view-key holder is for the human operating the air-gapped signer to check
//! what is about to be signed. This module produces the summary that is shown to them, so that the rendering lives
//! next to the payload types and cannot drift out of sync with the fields that are actually signed. A
//! [`PayloadSummary`] owns the payload it describes, so the summary shown is always of the payload that is signed.
//!
//! The type system enforces the confirmation: the `sign_locked_*` functions only accept a [`ConfirmedPayload`], and
//! the only way to get one is to write a [`PayloadSummary`] out with [`present_payload_summary`] and pass the
//! resulting [`Confirmation`] to [`PayloadSummary::confirm`].

use std::{
    fmt::{Display, Formatter},
    io::{BufRead, Write},
    ops::Deref,
    sync::atomic::{AtomicU64, Ordering},
};

use blake2::Blake2b;
use digest::{Digest, consts::U32};
use log::info;
use tari_common_types::{tari_address::TariAddress, transaction::TxId, types::FixedHash};
use tari_crypto::hashing::DomainSeparatedHasher;
use tari_hashing::OfflineSigningPayloadHashDomain;
use tari_script::TariScript;
use tari_utilities::hex::Hex;

use crate::{
    MicroMinotari,
    fee::Fee,
    helpers::terminal_safe::TerminalSafe,
    key_manager::TariKeyId,
    offline_signing::{
        models::{
            ConfirmationKind,
            HasVersion,
            OneSidedMultisigTransactionInfo,
            OneSidedTransactionInfo,
            PayloadIntegritySignature,
            PrepareDepositMultisigTransactionResult,
            PrepareOneSidedTransactionForSigningResult,
            PrepareWithdrawMultisigTransactionResult,
            SigningReceipt,
            borsh_canonical_multisig,
            borsh_canonical_one_sided,
            has_payload_auth_fields,
        },
        offline_signer::{check_payload_expiry, unix_now},
    },
    transaction_components::{
        MemoField,
        OutputFeatures,
        OutputType,
        RangeProofType,
        TransactionError,
        WalletOutput,
        covenants::Covenant,
    },
    weight::TransactionWeight,
};

pub(crate) const LOG_TARGET: &str = "c::tx::offline_signing";
const MEMO_INDENT: &str = "      | ";
const MEMO_LINE_WIDTH: usize = 48;
const RECAP_HEADER: &str = "---------------------------- Recap ----------------------------";
const RECAP_FOOTER: &str = "---------------------------------------------------------------";

/// `secs` (unix time) as `YYYY-MM-DD HH:MM:SS UTC`.
pub(crate) fn format_unix_time(secs: u64) -> String {
    i64::try_from(secs)
        .ok()
        .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0))
        .map_or_else(
            || format!("{secs} (unix time)"),
            |t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        )
}

mod sealed {
    pub trait Sealed {}
}

/// A payload that can be signed offline. Implemented only for the three `Prepare*` results.
pub trait OfflinePayload: sealed::Sealed + HasVersion + Clone + std::fmt::Debug + Send + Sync + 'static {
    /// The canonical bytes the payload signature covers.
    fn canonical_bytes(&self) -> Result<Vec<u8>, TransactionError>;
    fn tx_id(&self) -> TxId;
    fn payload_key_id(&self) -> Option<&TariKeyId>;
    fn expires_at(&self) -> u64;
    fn payload_signature(&self) -> &PayloadIntegritySignature;
}

macro_rules! impl_offline_payload {
    ($payload:ty, $canonical:ident) => {
        impl sealed::Sealed for $payload {}
        impl OfflinePayload for $payload {
            fn canonical_bytes(&self) -> Result<Vec<u8>, TransactionError> {
                $canonical(
                    &self.version,
                    self.tx_id,
                    &self.info,
                    self.payload_key_id.as_ref(),
                    self.expires_at,
                )
            }

            fn tx_id(&self) -> TxId {
                self.tx_id
            }

            fn payload_key_id(&self) -> Option<&TariKeyId> {
                self.payload_key_id.as_ref()
            }

            fn expires_at(&self) -> u64 {
                self.expires_at
            }

            fn payload_signature(&self) -> &PayloadIntegritySignature {
                &self.payload_signature
            }
        }
    };
}

impl_offline_payload!(PrepareOneSidedTransactionForSigningResult, borsh_canonical_one_sided);
impl_offline_payload!(PrepareDepositMultisigTransactionResult, borsh_canonical_multisig);
impl_offline_payload!(PrepareWithdrawMultisigTransactionResult, borsh_canonical_one_sided);

/// Which kind of payload a summary describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadKind {
    OneSided,
    MultisigDeposit,
    MultisigWithdrawal,
}

impl Display for PayloadKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PayloadKind::OneSided => write!(f, "one-sided payment"),
            PayloadKind::MultisigDeposit => write!(f, "multisig deposit"),
            PayloadKind::MultisigWithdrawal => write!(f, "multisig withdrawal"),
        }
    }
}

/// A single recipient line in a [`PayloadDetails`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecipientSummary {
    pub address: TariAddress,
    pub amount: MicroMinotari,
    pub features: OutputFeatures,
    /// The memo the signed output will carry.
    pub payment_id: MemoField,
    /// The payload's own memo for this recipient differs from `payment_id` and is not used.
    pub payload_memo_differs: bool,
}

/// A single directly-specified output in a [`PayloadDetails`].
///
/// These are outputs the payload hands to the signer ready-made, rather than ones the signer derives for a recipient.
/// They carry value away from the wallet exactly like a recipient does, but their destination is a raw script rather
/// than an address, so the script is shown verbatim: it is the only thing that says who can spend the output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputSummary {
    pub amount: MicroMinotari,
    pub script: TariScript,
    pub features: OutputFeatures,
    pub covenant: Covenant,
    pub payment_id: MemoField,
}

impl OutputSummary {
    /// [`unusual_features`], plus a covenant if the output carries one.
    pub fn unusual(&self) -> Vec<String> {
        let mut reasons = unusual_features(&self.features);
        if !self.covenant.is_empty() {
            reasons.push("carries a covenant: it restricts how this output can be spent later".to_string());
        }
        reasons
    }
}

/// Reasons why output features differ from those of a normal payment, one per non-default feature.
pub fn unusual_features(features: &OutputFeatures) -> Vec<String> {
    let mut reasons = Vec::new();
    if features.maturity > 0 {
        reasons.push(format!("cannot be spent before block {}", features.maturity));
    }
    if features.output_type != OutputType::Standard {
        reasons.push(format!(
            "output type {} is not a standard payment",
            features.output_type
        ));
    }
    if features.range_proof_type == RangeProofType::RevealedValue {
        reasons.push("amount is revealed on chain (RevealedValue)".to_string());
    }
    if features.sidechain_feature.is_some() {
        reasons.push("carries sidechain data".to_string());
    }
    if !features.coinbase_extra.is_empty() {
        reasons.push("carries coinbase extra data".to_string());
    }
    reasons
}

/// The security-relevant contents of an offline-signing payload, in a form that can be shown to a human before any
/// spend key is used.
///
/// Every field here is covered by the canonical Borsh bytes that the integrity signature commits to, so a summary that
/// the operator approves is a summary of what will actually be signed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayloadDetails {
    pub kind: PayloadKind,
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
    /// The number of signatures (m of n) the multisig output requires, if it is a multisig deposit.
    pub multisig_required_signatures: Option<u8>,
    /// The index of the payload key the payload claims to be signed with, or `None` for the view key.
    pub payload_key_index: Option<u64>,
    /// When the signer stops accepting the payload (unix seconds), or `None` for a version 5 payload.
    pub expires_at: Option<u64>,
}

impl PayloadDetails {
    /// Builds the details of a one-sided payload.
    pub(crate) fn one_sided(kind: PayloadKind, tx_id: TxId, info: &OneSidedTransactionInfo) -> Self {
        Self {
            kind,
            tx_id,
            sender_address: info.sender_address.clone(),
            payment_id: info.payment_id.clone(),
            recipients: info
                .recipients
                .iter()
                .map(|r| RecipientSummary {
                    address: r.address.clone(),
                    amount: r.amount,
                    features: r.output_features.clone(),
                    payment_id: r.payment_id.clone(),
                    payload_memo_differs: false,
                })
                .collect(),
            outputs: info
                .outputs
                .iter()
                .map(|o| OutputSummary {
                    amount: o.value(),
                    script: o.script().clone(),
                    features: o.features().clone(),
                    covenant: o.covenant().clone(),
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
            multisig_required_signatures: None,
            payload_key_index: None,
            expires_at: None,
        }
    }

    /// Records which key the payload claims to be signed with and when it expires.
    fn with_payload_auth<P: OfflinePayload>(mut self, payload: &P) -> Self {
        // Anything other than `PayloadAuth` is refused when the payload is parsed
        if let Some(TariKeyId::PayloadAuth { index }) = payload.payload_key_id() {
            self.payload_key_index = Some(*index);
        }
        self.expires_at = has_payload_auth_fields(payload.get_version()).then_some(payload.expires_at());
        self
    }

    fn write_signing_key_and_expiry(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.payload_key_index {
            Some(index) => writeln!(f, "Signed with    : payload key #{index}")?,
            None => {
                writeln!(f, "Signed with    : view key (shared key)")?;
                writeln!(
                    f,
                    "WARNING        : signed with the shared view key - anyone holding the view key could have \
                     prepared this payload"
                )?;
            },
        }
        let Some(expires_at) = self.expires_at else {
            return writeln!(f, "Expires        : never (version 5 payload)");
        };
        let left = match expires_at.checked_sub(unix_now()) {
            Some(secs) => format!("{}h {:02}m {:02}s left", secs / 3600, (secs % 3600) / 60, secs % 60),
            None => "EXPIRED".to_string(),
        };
        writeln!(f, "Expires        : {} ({left})", format_unix_time(expires_at))
    }

    /// Builds the details of a multisig deposit payload.
    pub(crate) fn multisig(tx_id: TxId, info: &OneSidedMultisigTransactionInfo) -> Self {
        let mut details = Self::one_sided(PayloadKind::MultisigDeposit, tx_id, &info.base);
        details.multisig_public_keys = info.public_keys.iter().map(|pk| pk.to_hex()).collect();
        // `party_number` holds the threshold m
        details.multisig_required_signatures = Some(info.party_number);
        details.use_top_level_memo_for_recipients();
        details
    }

    /// Multisig signers put the top-level memo on the recipient output, so that is the memo shown for it.
    fn use_top_level_memo_for_recipients(&mut self) {
        for recipient in &mut self.recipients {
            recipient.payload_memo_differs = recipient.payment_id != self.payment_id;
            recipient.payment_id = self.payment_id.clone();
        }
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

    /// The "Total spend" and "Change" lines, shared by the fee block and the recap so the two cannot disagree.
    fn spend_and_change_lines(&self) -> (String, String) {
        let total_spend = self.total_spend();
        // The fee is only a number if the payload fixes one. Otherwise it is a rate, and the amount it works out to
        // depends on outputs that do not exist until after this summary has been approved, so it is described rather
        // than guessed at, and the totals are qualified to match.
        if self.fee_is_fixed() {
            let change = match self.change() {
                Some(change) => format!("Change         : {change}"),
                None => "Change         : INPUTS DO NOT COVER THE SPEND".to_string(),
            };
            return (format!("Total spend    : {total_spend}"), change);
        }
        let change = match (self.change(), self.minimum_fee()) {
            // The change is computed before the fee, so a payload can cover its outputs and still not cover the fee.
            // Saying so is the whole point of this screen; showing the leftover as change is not.
            (Some(change), Some(minimum)) if change < minimum => {
                format!("Change         : INPUTS DO NOT COVER THE SPEND ONCE THE FEE IS TAKEN (at least {minimum})")
            },
            (Some(change), Some(minimum)) => format!("Change         : {change} less the fee (at least {minimum})"),
            (Some(change), None) => format!("Change         : {change} less the fee"),
            (None, _) => "Change         : INPUTS DO NOT COVER THE SPEND".to_string(),
        };
        (format!("Total spend    : {total_spend} plus the fee"), change)
    }

    fn write_memos(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let has_memos = !self.payment_id.is_empty() ||
            self.recipients.iter().any(|r| !r.payment_id.is_empty()) ||
            self.outputs.iter().any(|o| !o.payment_id.is_empty());
        if !has_memos {
            return Ok(());
        }
        writeln!(f, "Memos (sender-supplied text, control characters shown escaped):")?;
        if !self.payment_id.is_empty() {
            write_memo(f, "  payment ID     :", &self.payment_id)?;
        }
        for (i, recipient) in self.recipients.iter().enumerate() {
            if !recipient.payment_id.is_empty() {
                write_memo(
                    f,
                    &format!("  recipient [{}]:", i.saturating_add(1)),
                    &recipient.payment_id,
                )?;
            }
        }
        for (i, output) in self.outputs.iter().enumerate() {
            if !output.payment_id.is_empty() {
                write_memo(
                    f,
                    &format!("  output [{}]   :", i.saturating_add(1)),
                    &output.payment_id,
                )?;
            }
        }
        Ok(())
    }

    /// The fixed closing block: every destination, the multisig keys and the totals again, after all memo text.
    fn write_recap(&self, f: &mut Formatter<'_>, total_spend: &str, change: &str) -> std::fmt::Result {
        const UNUSUAL: &str = " (WARNING: unusual output features, see above)";
        writeln!(f, "{RECAP_HEADER}")?;
        for (i, recipient) in self.recipients.iter().enumerate() {
            let warning = if unusual_features(&recipient.features).is_empty() {
                ""
            } else {
                UNUSUAL
            };
            writeln!(
                f,
                "  [{}] {} -> {}{}{}",
                i.saturating_add(1),
                recipient.amount,
                recipient.address.to_base58(),
                warning,
                if recipient.payload_memo_differs {
                    " (WARNING: recipient memo differs, see above)"
                } else {
                    ""
                }
            )?;
        }
        for (i, output) in self.outputs.iter().enumerate() {
            let warning = if output.unusual().is_empty() { "" } else { UNUSUAL };
            writeln!(
                f,
                "  output [{}] {} -> script {} (NOT a normal payment){}",
                i.saturating_add(1),
                output.amount,
                TerminalSafe(&output.script),
                warning
            )?;
        }
        if let Some(required) = self.multisig_required_signatures {
            writeln!(
                f,
                "  Required signatures {} of {}",
                required,
                self.multisig_public_keys.len()
            )?;
            for (i, pk) in self.multisig_public_keys.iter().enumerate() {
                writeln!(f, "    key [{}] {}", i.saturating_add(1), TerminalSafe(pk))?;
            }
        }
        writeln!(f, "  {total_spend}")?;
        writeln!(f, "  {change}")?;
        writeln!(f, "{RECAP_FOOTER}")
    }
}

/// Writes a memo's escaped text in short indented lines, so it cannot soft-wrap back to column 0 on a terminal of 80
/// columns or more. Escape sequences are kept whole, and non-ASCII characters count as two columns.
fn write_memo(f: &mut Formatter<'_>, label: &str, memo: &MemoField) -> std::fmt::Result {
    writeln!(f, "{label}")?;
    let text = memo.to_string();
    let mut chars = text.chars();
    let mut line = String::new();
    let mut width = 0usize;
    while let Some(c) = chars.next() {
        // The memo is already escaped, so every backslash starts an escape sequence
        let mut unit = String::from(c);
        if c == '\\' {
            match chars.next() {
                Some('u') => {
                    unit.push('u');
                    for n in chars.by_ref() {
                        unit.push(n);
                        if n == '}' {
                            break;
                        }
                    }
                },
                Some('x') => {
                    unit.push('x');
                    unit.extend(chars.by_ref().take(2));
                },
                Some(n) => unit.push(n),
                None => {},
            }
        }
        let unit_width = unit
            .chars()
            .map(|c| if c.is_ascii() { 1usize } else { 2 })
            .sum::<usize>();
        if width > 0 && width.saturating_add(unit_width) > MEMO_LINE_WIDTH {
            writeln!(f, "{MEMO_INDENT}{line}")?;
            line.clear();
            width = 0;
        }
        line.push_str(&unit);
        width = width.saturating_add(unit_width);
    }
    if width > 0 {
        writeln!(f, "{MEMO_INDENT}{line}")?;
    }
    Ok(())
}

fn write_features(f: &mut Formatter<'_>, features: &OutputFeatures, reasons: &[String]) -> std::fmt::Result {
    writeln!(
        f,
        "      features: type {}, maturity {}, range proof {}",
        features.output_type, features.maturity, features.range_proof_type
    )?;
    if !reasons.is_empty() {
        writeln!(f, "      WARNING : {}", reasons.join("; "))?;
    }
    Ok(())
}

impl Display for PayloadDetails {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Payload        : {}", self.kind)?;
        writeln!(f, "Transaction ID : {}", self.tx_id)?;
        self.write_signing_key_and_expiry(f)?;
        writeln!(
            f,
            "From           : {} (as stated by the payload, not checked)",
            self.sender_address.to_base58()
        )?;
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
            writeln!(f, "      emoji   : {}", TerminalSafe(&recipient.address))?;
            write_features(f, &recipient.features, &unusual_features(&recipient.features))?;
            if recipient.payload_memo_differs {
                writeln!(
                    f,
                    "      WARNING : the payload's recipient memo differs from the memo that will be put on the \
                     output; the output memo is the one listed under payment ID"
                )?;
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
                writeln!(f, "      script  : {}", TerminalSafe(&output.script))?;
                if !output.covenant.is_empty() {
                    writeln!(
                        f,
                        "      Covenant: {}",
                        TerminalSafe(&output.covenant.to_bytes().to_hex())
                    )?;
                }
                write_features(f, &output.features, &output.unusual())?;
            }
        }
        if let Some(required) = self.multisig_required_signatures {
            writeln!(
                f,
                "Required signatures (m of n) : {} of {}",
                required,
                self.multisig_public_keys.len()
            )?;
            for (i, pk) in self.multisig_public_keys.iter().enumerate() {
                writeln!(f, "  [{}] key     : {}", i.saturating_add(1), TerminalSafe(pk))?;
            }
        }
        match self.fixed_fee() {
            Some(fee) => writeln!(f, "Fee            : {fee}")?,
            None => writeln!(
                f,
                "Fee            : {}/gram, charged when this is signed",
                self.fee_per_gram
            )?,
        }
        let (total_spend, change) = self.spend_and_change_lines();
        writeln!(f, "{total_spend}")?;
        writeln!(f, "{change}")?;

        // Sender-supplied text goes after every line it could imitate, and nothing from a memo follows this block
        self.write_memos(f)?;

        self.write_recap(f, &total_spend, &change)
    }
}

static NEXT_SUMMARY_ID: AtomicU64 = AtomicU64::new(0);

/// A summary of an offline-signing payload that owns the payload it describes.
///
/// It derefs to [`PayloadDetails`] for read-only access to what will be shown.
#[derive(Debug)]
pub struct PayloadSummary<P: OfflinePayload> {
    details: PayloadDetails,
    payload: P,
    id: u64,
}

impl<P: OfflinePayload> PayloadSummary<P> {
    fn new(details: PayloadDetails, payload: P) -> Self {
        Self {
            details: details.with_payload_auth(&payload),
            payload,
            id: NEXT_SUMMARY_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Turns an approved [`Confirmation`] of this summary into a payload that can be signed.
    pub fn confirm(self, decision: Confirmation) -> Result<ConfirmedPayload<P>, PayloadNotConfirmed> {
        if decision.summary_id != self.id {
            return Err(PayloadNotConfirmed::DifferentSummary);
        }
        if !decision.approved {
            return Err(PayloadNotConfirmed::Declined);
        }
        Ok(ConfirmedPayload {
            payload: self.payload,
            confirmation: decision.kind,
            payload_hash: decision.payload_hash,
        })
    }

    /// Domain-separated Blake2b-256 of the payload's canonical bytes, the bytes its integrity signature covers.
    pub fn payload_hash(&self) -> Result<FixedHash, TransactionError> {
        let hasher =
            DomainSeparatedHasher::<Blake2b<U32>, OfflineSigningPayloadHashDomain>::new_with_label("payload_hash")
                .chain_update(self.payload.canonical_bytes()?);
        Ok(FixedHash::from(Digest::finalize(hasher)))
    }

    /// Gives the payload back without signing it.
    pub fn into_payload(self) -> P {
        self.payload
    }
}

impl PayloadSummary<PrepareOneSidedTransactionForSigningResult> {
    pub fn from_one_sided(request: PrepareOneSidedTransactionForSigningResult) -> Self {
        let details = PayloadDetails::one_sided(PayloadKind::OneSided, request.tx_id, &request.info);
        Self::new(details, request)
    }
}

impl PayloadSummary<PrepareDepositMultisigTransactionResult> {
    pub fn from_multisig(request: PrepareDepositMultisigTransactionResult) -> Self {
        let details = PayloadDetails::multisig(request.tx_id, &request.info);
        Self::new(details, request)
    }
}

impl PayloadSummary<PrepareWithdrawMultisigTransactionResult> {
    pub fn from_withdraw_multisig(request: PrepareWithdrawMultisigTransactionResult) -> Self {
        let mut details = PayloadDetails::one_sided(PayloadKind::MultisigWithdrawal, request.tx_id, &request.info);
        details.use_top_level_memo_for_recipients();
        Self::new(details, request)
    }
}

impl<P: OfflinePayload> Deref for PayloadSummary<P> {
    type Target = PayloadDetails;

    fn deref(&self) -> &PayloadDetails {
        &self.details
    }
}

impl<P: OfflinePayload> Display for PayloadSummary<P> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.details.fmt(f)
    }
}

/// The operator's decision on one presented [`PayloadSummary`]. Only [`present_payload_summary`] produces one.
#[derive(Debug)]
pub struct Confirmation {
    summary_id: u64,
    approved: bool,
    kind: ConfirmationKind,
    payload_hash: FixedHash,
}

impl Confirmation {
    pub fn is_approved(&self) -> bool {
        self.approved
    }
}

/// Why a [`PayloadSummary`] could not be turned into a [`ConfirmedPayload`].
#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum PayloadNotConfirmed {
    #[error("The transaction was not confirmed. Nothing was signed.")]
    Declined,
    #[error("The confirmation was given for a different transaction summary. Nothing was signed.")]
    DifferentSummary,
}

/// A payload whose summary has been presented and confirmed. The `sign_locked_*` functions only accept this.
///
/// A raw payload cannot be signed:
/// ```compile_fail
/// use tari_common::configuration::Network;
/// use tari_transaction_components::{
///     consensus::ConsensusConstants,
///     key_manager::KeyManager,
///     offline_signing::{
///         PayloadKeyPolicy,
///         models::PrepareOneSidedTransactionForSigningResult,
///         sign_locked_transaction,
///     },
/// };
///
/// fn sign(km: &KeyManager, cc: ConsensusConstants, net: Network, prepared: PrepareOneSidedTransactionForSigningResult) {
///     let _ = sign_locked_transaction(km, cc, net, prepared, &PayloadKeyPolicy::default());
/// }
/// ```
///
/// and a `ConfirmedPayload` cannot be built outside this module:
/// ```compile_fail,E0451
/// use tari_common_types::types::FixedHash;
/// use tari_transaction_components::offline_signing::{
///     ConfirmedPayload,
///     models::{ConfirmationKind, PrepareOneSidedTransactionForSigningResult},
/// };
///
/// fn wrap(payload: PrepareOneSidedTransactionForSigningResult) -> ConfirmedPayload<PrepareOneSidedTransactionForSigningResult> {
///     ConfirmedPayload { payload, confirmation: ConfirmationKind::Prompt, payload_hash: FixedHash::default() }
/// }
/// ```
#[derive(Clone)]
pub struct ConfirmedPayload<P: OfflinePayload> {
    payload: P,
    confirmation: ConfirmationKind,
    payload_hash: FixedHash,
}

impl<P: OfflinePayload> ConfirmedPayload<P> {
    pub fn payload(&self) -> &P {
        &self.payload
    }

    /// A receipt for signing this payload now.
    pub(crate) fn receipt(&self) -> SigningReceipt {
        SigningReceipt {
            tx_id: self.payload.tx_id(),
            payload_hash: self.payload_hash,
            confirmation: self.confirmation,
            signed_at: unix_now(),
        }
    }

    pub(crate) fn into_payload(self) -> P {
        self.payload
    }
}

impl<P: OfflinePayload> std::fmt::Debug for ConfirmedPayload<P> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfirmedPayload")
            .field("payload", &self.payload)
            .field("confirmation", &self.confirmation)
            .field("payload_hash", &self.payload_hash)
            .finish()
    }
}

impl ConfirmedPayload<PrepareWithdrawMultisigTransactionResult> {
    /// Sets each input's `script_key_id` from `f`. Touches nothing else; the script key id is
    /// not shown in the summary, and the integrity signature and the signer's payload key-id
    /// allowlist (`check_payload_key_ids`) still run over the result inside `sign_locked_*`.
    pub fn set_input_script_key_ids<E>(
        &mut self,
        mut f: impl FnMut(&WalletOutput) -> Result<TariKeyId, E>,
    ) -> Result<(), E> {
        for o in &mut self.payload.info.inputs {
            let k = f(o)?;
            o.set_script_key_id(k);
        }
        Ok(())
    }
}

/// How [`present_payload_summary`] gets the operator's decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmationMode {
    /// Ask y/N on `input`.
    Interactive,
    /// There is no one to ask: write the summary, log a record (transaction id, decision and payload hash), then
    /// approve.
    NonInteractive,
}

/// Writes `summary` to `out` and returns the operator's decision on it. An expired payload is refused with an error
/// before anything is written.
///
/// In [`ConfirmationMode::Interactive`] the operator is asked to type `y` or `yes`; anything else, including end of
/// input, declines. In [`ConfirmationMode::NonInteractive`] the confirmation is approved without reading `input`.
/// Either way the decision is logged with the transaction id and payload hash; the summary itself is not, as it
/// carries addresses, amounts and memo text.
pub fn present_payload_summary<P: OfflinePayload>(
    summary: &PayloadSummary<P>,
    mode: ConfirmationMode,
    out: &mut dyn Write,
    input: &mut dyn BufRead,
) -> std::io::Result<Confirmation> {
    check_payload_expiry(summary.expires_at, unix_now()).map_err(|e| std::io::Error::other(e.to_string()))?;
    let payload_hash = summary
        .payload_hash()
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    write!(
        out,
        "\nThe following transaction is about to be signed with your spend key:\n\n{summary}\n"
    )?;
    let (approved, kind, decision) = match mode {
        ConfirmationMode::NonInteractive => {
            // Only claim a log record when a logger will actually write one
            if log::log_enabled!(target: LOG_TARGET, log::Level::Info) {
                writeln!(
                    out,
                    "Signing without a prompt (non-interactive mode or no terminal). A record (transaction id and \
                     payload hash) has been logged."
                )?;
            } else {
                writeln!(out, "Signing without a prompt (non-interactive mode or no terminal).")?;
            }
            (true, ConfirmationKind::NoPrompt, "signed without a prompt")
        },
        ConfirmationMode::Interactive => {
            write!(out, "Sign this transaction? [y/N]: ")?;
            out.flush()?;
            let mut answer = String::new();
            input.read_line(&mut answer)?;
            let answer = answer.trim().to_ascii_lowercase();
            let approved = answer == "y" || answer == "yes";
            let decision = if approved { "approved" } else { "declined" };
            (approved, ConfirmationKind::Prompt, decision)
        },
    };
    info!(
        target: LOG_TARGET,
        "Offline payload {} ({}) {decision}, payload hash {payload_hash}", summary.tx_id, summary.kind
    );
    out.flush()?;
    Ok(Confirmation {
        summary_id: summary.id,
        approved,
        kind,
        payload_hash,
    })
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use tari_common_types::tari_address::{TariAddress, TariAddressFeatures};
    use tari_crypto::keys::{PublicKey, SecretKey};
    use tari_script::{ExecutionStack, push_pubkey_script};

    use super::*;
    use crate::{
        helpers::terminal_safe::needs_escape,
        key_manager::{KeyManager, TariKeyId, TransactionKeyManagerInterface},
        offline_signing::models::{PayloadIntegritySignature, PaymentRecipient, get_latest_version},
        transaction_components::{EncryptedData, WalletOutput, covenants::Covenant, memo_field::TxType},
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
        let summary = PayloadDetails::one_sided(PayloadKind::OneSided, TxId::from(42u64), &info(&[1000, 2500]));
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
        let summary = PayloadDetails::one_sided(PayloadKind::OneSided, TxId::from(1u64), &info);
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
        let summary = PayloadDetails::one_sided(PayloadKind::OneSided, TxId::from(42u64), &info);

        assert!(summary.fee_is_fixed());
        assert_eq!(summary.fixed_fee(), Some(MicroMinotari::from(100)));
        assert_eq!(summary.total_spend(), MicroMinotari::from(3600));

        let rendered = summary.to_string();
        assert!(rendered.contains("Fee            : 100 µT"), "{rendered}");
        assert!(!rendered.contains("plus the fee"), "{rendered}");
    }

    #[test]
    fn it_reports_when_inputs_do_not_cover_the_spend() {
        let summary = PayloadDetails::one_sided(PayloadKind::OneSided, TxId::from(1u64), &info(&[1000]));
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
            key_manager.get_random_key(None, None).unwrap().key_id,
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

        let summary = PayloadDetails::one_sided(PayloadKind::OneSided, TxId::from(1u64), &info);
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
                key_manager.get_random_key(None, None).unwrap().key_id,
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

        let summary = PayloadDetails::one_sided(PayloadKind::OneSided, TxId::from(1u64), &info);

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
                key_manager.get_random_key(None, None).unwrap().key_id,
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

        let summary = PayloadDetails::one_sided(PayloadKind::OneSided, TxId::from(1u64), &info);
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
        let summary = PayloadDetails::one_sided(PayloadKind::OneSided, TxId::from(1u64), &info);
        assert!(summary.inputs_cover_the_fee());
        let rendered = summary.to_string();
        assert!(rendered.contains("less the fee (at least"), "{rendered}");
    }

    #[test]
    fn it_renders_every_recipient_address_and_amount() {
        let info = info(&[1000, 2500]);
        let summary = PayloadDetails::one_sided(PayloadKind::OneSided, TxId::from(7u64), &info);
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

    fn one_sided_payload(info: OneSidedTransactionInfo) -> PrepareOneSidedTransactionForSigningResult {
        PrepareOneSidedTransactionForSigningResult {
            version: get_latest_version(),
            tx_id: TxId::from(1u64),
            info,
            payload_signature: PayloadIntegritySignature {
                signature: Default::default(),
            },
            payload_key_id: None,
            expires_at: unix_now().saturating_add(3600),
        }
    }

    #[test]
    fn summary_names_the_signing_key_and_the_expiry() {
        let mut payload = one_sided_payload(info(&[1000]));
        payload.payload_key_id = Some(TariKeyId::PayloadAuth { index: 3 });
        payload.expires_at = unix_now().saturating_add(3600);
        let rendered = PayloadSummary::from_one_sided(payload.clone()).to_string();
        assert!(rendered.contains("Signed with    : payload key #3"), "{rendered}");
        assert!(!rendered.contains("shared view key"), "{rendered}");
        assert!(
            rendered.contains(&format!(
                "Expires        : {} (0h 59m",
                format_unix_time(payload.expires_at)
            )) || rendered.contains(&format!(
                "Expires        : {} (1h 00m",
                format_unix_time(payload.expires_at)
            )),
            "{rendered}"
        );
        assert!(rendered.contains(" UTC ("), "{rendered}");

        payload.payload_key_id = None;
        let rendered = PayloadSummary::from_one_sided(payload).to_string();
        assert!(
            rendered.contains("Signed with    : view key (shared key)"),
            "{rendered}"
        );
        assert!(
            rendered.contains("WARNING        : signed with the shared view key"),
            "{rendered}"
        );
    }

    #[test]
    fn summary_of_a_version_5_payload_has_no_expiry() {
        let mut payload = one_sided_payload(info(&[1000]));
        payload.version = semver::Version::new(5, 0, 0);
        payload.expires_at = 0;
        let summary = PayloadSummary::from_one_sided(payload);
        assert_eq!(summary.expires_at, None);
        assert!(
            summary
                .to_string()
                .contains("Expires        : never (version 5 payload)")
        );
    }

    #[test]
    fn an_expired_payload_is_refused_before_its_summary_is_presented() {
        let mut payload = one_sided_payload(info(&[1000]));
        payload.expires_at = unix_now().saturating_sub(1);
        let summary = PayloadSummary::from_one_sided(payload);
        let mut out = Vec::new();
        let err = present_payload_summary(
            &summary,
            ConfirmationMode::NonInteractive,
            &mut out,
            &mut std::io::empty(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("expired"), "{err}");
        assert!(out.is_empty());
    }

    #[test]
    fn unix_time_is_formatted_in_utc() {
        assert_eq!(format_unix_time(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(format_unix_time(1_700_000_000), "2023-11-14 22:13:20 UTC");
    }

    fn output_with_memo(key_manager: &KeyManager, value: u64, memo: MemoField) -> WalletOutput {
        WalletOutput::new(
            Default::default(),
            MicroMinotari::from(value),
            key_manager.get_random_key(None, None).unwrap().key_id,
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
            memo,
            key_manager,
        )
        .unwrap()
    }

    #[test]
    fn payload_summary_prints_recap_last() {
        let memo_text = "Change         : 1 µT\nRecap";
        let memo = || MemoField::new_open(memo_text.as_bytes().to_vec(), TxType::PaymentToOther).unwrap();
        let key_manager = KeyManager::new_random().unwrap();
        let mut info = info(&[1000]);
        info.payment_id = memo();
        info.recipients[0].payment_id = memo();
        info.inputs = vec![output_with_memo(&key_manager, 1_000_000, MemoField::new_empty())];
        info.outputs = vec![output_with_memo(&key_manager, 300, memo())];

        let rendered = PayloadSummary::from_one_sided(one_sided_payload(info)).to_string();
        // Rejoin the wrapped memo lines so the memo text can be found whole
        let rendered = rendered.replace(&format!("\n{MEMO_INDENT}"), "");
        let recap = rendered.find("---- Recap ----").expect("recap header");
        let escaped_memo = "Change         : 1 µT\\nRecap";
        let occurrences: Vec<usize> = rendered.match_indices(escaped_memo).map(|(i, _)| i).collect();
        assert_eq!(occurrences.len(), 3, "{rendered}");
        assert!(occurrences.iter().all(|i| *i < recap), "{rendered}");
        let after_header = &rendered[recap + "---- Recap ----".len()..];
        assert!(!after_header.contains("1 µT\\n"), "{rendered}");
        assert!(!after_header.contains("Recap"), "{rendered}");
        assert_eq!(rendered.lines().last(), Some(RECAP_FOOTER));
        assert!(rendered.ends_with(&format!("{RECAP_FOOTER}\n")));
    }

    #[test]
    fn payload_summary_shows_recipient_features() {
        let mut info = info(&[1000, 2000]);
        info.recipients[0].output_features.maturity = 1_000_000;
        info.recipients[0].output_features.range_proof_type = RangeProofType::RevealedValue;
        info.recipients[0].output_features.output_type = OutputType::Burn;

        let rendered = PayloadSummary::from_one_sided(one_sided_payload(info)).to_string();
        assert!(
            rendered.contains("features: type Burn, maturity 1000000, range proof RevealedValue"),
            "{rendered}"
        );
        let warnings: Vec<&str> = rendered.lines().filter(|l| l.contains("WARNING :")).collect();
        assert_eq!(
            warnings.len(),
            1,
            "only the unusual recipient is warned about:\n{rendered}"
        );
        assert!(
            warnings[0].contains("cannot be spent before block 1000000"),
            "{rendered}"
        );
        assert!(
            warnings[0].contains("output type Burn is not a standard payment"),
            "{rendered}"
        );
        assert!(
            warnings[0].contains("amount is revealed on chain (RevealedValue)"),
            "{rendered}"
        );
        assert!(
            rendered.contains("features: type Standard, maturity 0, range proof BulletProofPlus"),
            "{rendered}"
        );

        let recap = &rendered[rendered.find("---- Recap ----").unwrap()..];
        assert_eq!(
            recap.matches("WARNING: unusual output features").count(),
            1,
            "{rendered}"
        );
        assert!(
            recap
                .lines()
                .nth(1)
                .unwrap()
                .contains("WARNING: unusual output features"),
            "{rendered}"
        );
    }

    #[test]
    fn payload_summary_escapes_memo_text() {
        let memo = MemoField::new_open(b"\x1b[2J\r\x07\x00\xc2\x9b".to_vec(), TxType::PaymentToOther).unwrap();
        let mut info = info(&[1000]);
        info.payment_id = memo.clone();
        info.recipients[0].payment_id = memo;

        let rendered = PayloadSummary::from_one_sided(one_sided_payload(info)).to_string();
        assert!(
            rendered.replace(&format!("\n{MEMO_INDENT}"), "").contains("\\u{1b}[2J"),
            "{rendered}"
        );
        assert!(
            !rendered.chars().any(|c| c < ' ' && c != '\n'),
            "raw control characters in:\n{rendered}"
        );
    }

    #[test]
    fn payload_summary_recap_flags_other_outputs() {
        let key_manager = KeyManager::new_random().unwrap();
        let features = OutputFeatures {
            maturity: 50,
            ..Default::default()
        };
        let output = WalletOutput::new(
            Default::default(),
            MicroMinotari::from(300),
            key_manager.get_random_key(None, None).unwrap().key_id,
            features,
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
        .unwrap();
        let mut info = info(&[1000]);
        info.outputs = vec![output];

        let rendered = PayloadSummary::from_one_sided(one_sided_payload(info)).to_string();
        let recap = &rendered[rendered.find("---- Recap ----").unwrap()..];
        let line = recap
            .lines()
            .find(|l| l.starts_with("  output [1] "))
            .expect("other output in recap");
        assert!(line.contains("(NOT a normal payment)"), "{rendered}");
        assert!(
            line.ends_with("(WARNING: unusual output features, see above)"),
            "{rendered}"
        );
    }

    fn output_with_covenant(key_manager: &KeyManager, covenant: Covenant) -> WalletOutput {
        WalletOutput::new(
            Default::default(),
            MicroMinotari::from(300),
            key_manager.get_random_key(None, None).unwrap().key_id,
            OutputFeatures::default(),
            push_pubkey_script(&key_manager.get_spend_key().pub_key),
            ExecutionStack::default(),
            TariKeyId::Zero,
            Default::default(),
            Default::default(),
            0,
            covenant,
            EncryptedData::default(),
            MicroMinotari::zero(),
            MemoField::new_empty(),
            key_manager,
        )
        .unwrap()
    }

    #[test]
    fn payload_summary_shows_an_output_covenant_escaped() {
        // Control and bidi characters inside the covenant's own data
        let covenant = crate::covenant!(field_eq(
            @field::features,
            @bytes("\x1b[2J\r\u{202e}".as_bytes().to_vec().try_into().unwrap())
        ))
        .unwrap();
        let key_manager = KeyManager::new_random().unwrap();
        let mut with_covenant = info(&[1000]);
        with_covenant.outputs = vec![output_with_covenant(&key_manager, covenant.clone())];

        let rendered = PayloadSummary::from_one_sided(one_sided_payload(with_covenant)).to_string();
        let lines: Vec<&str> = rendered.lines().collect();
        let script = lines.iter().position(|l| l.starts_with("      script  : ")).unwrap();
        assert_eq!(
            lines[script.saturating_add(1)],
            format!("      Covenant: {}", covenant.to_bytes().to_hex()),
            "{rendered}"
        );
        assert!(
            lines[script.saturating_add(3)].starts_with("      WARNING : ") &&
                lines[script.saturating_add(3)]
                    .ends_with("carries a covenant: it restricts how this output can be spent later"),
            "{rendered}"
        );
        let recap = &rendered[rendered.find("---- Recap ----").unwrap()..];
        let line = recap.lines().find(|l| l.starts_with("  output [1] ")).unwrap();
        assert!(
            line.ends_with("(WARNING: unusual output features, see above)"),
            "{rendered}"
        );
        assert!(
            !rendered.chars().any(|c| c != '\n' && needs_escape(c) && c != '\\'),
            "raw control characters in:\n{rendered}"
        );

        // No covenant, no line and no warning
        let mut info = info(&[1000]);
        info.outputs = vec![output_with_covenant(&key_manager, Covenant::default())];
        let rendered = PayloadSummary::from_one_sided(one_sided_payload(info)).to_string();
        assert!(!rendered.contains("Covenant"), "{rendered}");
        assert!(!rendered.contains("WARNING :"), "{rendered}");
        assert!(!rendered.contains("unusual output features"), "{rendered}");
    }

    #[test]
    fn payload_summary_wraps_memo_text() {
        let mut text = " ".repeat(200);
        text.push_str("---- Recap ----");
        let memo = MemoField::new_open(text.into_bytes(), TxType::PaymentToOther).unwrap();
        let mut info = info(&[1000]);
        info.payment_id = memo.clone();
        info.recipients[0].payment_id = memo;

        let rendered = PayloadSummary::from_one_sided(one_sided_payload(info)).to_string();
        for line in rendered.lines() {
            if line.starts_with('-') || line.starts_with('[') {
                assert!(line == RECAP_HEADER || line == RECAP_FOOTER, "{line:?} in:\n{rendered}");
            }
        }
        let memos_start = rendered.find("Memos (").unwrap();
        let recap_start = rendered.find(RECAP_HEADER).unwrap();
        let memo_lines: Vec<&str> = rendered[memos_start..recap_start].lines().skip(1).collect();
        assert!(memo_lines.len() > 4, "{rendered}");
        for line in memo_lines {
            assert!(
                line == "  payment ID     :" || line == "  recipient [1]:" || line.starts_with(MEMO_INDENT),
                "{line:?} in:\n{rendered}"
            );
            assert!(line.chars().count() <= MEMO_INDENT.len() + MEMO_LINE_WIDTH, "{line:?}");
        }
    }

    #[test]
    fn memo_wrapping_keeps_escape_sequences_whole() {
        let bytes: Vec<u8> = [0x1bu8; 30].iter().chain([0xffu8; 30].iter()).copied().collect();
        let mut info = info(&[1000]);
        info.payment_id = MemoField::new_open(bytes, TxType::PaymentToOther).unwrap();
        let rendered = PayloadSummary::from_one_sided(one_sided_payload(info)).to_string();
        let memo_lines: Vec<&str> = rendered.lines().filter(|l| l.starts_with(MEMO_INDENT)).collect();
        assert!(memo_lines.len() > 2, "{rendered}");
        for line in memo_lines {
            // Only whole escape sequences on each line: removing them leaves nothing behind
            let rest = line[MEMO_INDENT.len()..]
                .replace("type(PaymentToOther), data(", "")
                .replace("\\u{1b}", "")
                .replace("\\xff", "")
                .replace(')', "");
            assert!(rest.is_empty(), "{line:?} in:\n{rendered}");
        }
    }

    #[test]
    fn multisig_summary_shows_the_output_memo() {
        let memo = |text: &str| MemoField::new_open(text.as_bytes().to_vec(), TxType::PaymentToOther).unwrap();
        let memo_info = |recipient_memo: &str| {
            let mut info = info(&[1000]);
            info.payment_id = memo("output memo");
            info.recipients[0].payment_id = memo(recipient_memo);
            info
        };
        let renderings = |recipient_memo: &str| {
            let deposit = PayloadSummary::from_multisig(PrepareDepositMultisigTransactionResult {
                version: get_latest_version(),
                tx_id: TxId::from(1u64),
                info: OneSidedMultisigTransactionInfo::new(memo_info(recipient_memo), vec![], 1),
                payload_signature: PayloadIntegritySignature {
                    signature: Default::default(),
                },
                payload_key_id: None,
                expires_at: unix_now().saturating_add(3600),
            });
            let withdraw = PayloadSummary::from_withdraw_multisig(PrepareWithdrawMultisigTransactionResult {
                version: get_latest_version(),
                tx_id: TxId::from(1u64),
                info: memo_info(recipient_memo),
                payload_signature: PayloadIntegritySignature {
                    signature: Default::default(),
                },
                payload_key_id: None,
                expires_at: unix_now().saturating_add(3600),
            });
            [deposit.to_string(), withdraw.to_string()]
        };

        for rendered in renderings("other memo") {
            let joined = rendered.replace(&format!("\n{MEMO_INDENT}"), "");
            assert!(
                joined.contains("  recipient [1]:type(PaymentToOther), data(output memo)"),
                "{rendered}"
            );
            assert!(!joined.contains("other memo"), "{rendered}");
            assert!(
                rendered.contains(
                    "WARNING : the payload's recipient memo differs from the memo that will be put on the output"
                ),
                "{rendered}"
            );
            assert!(
                rendered.contains("(WARNING: recipient memo differs, see above)"),
                "{rendered}"
            );
        }
        for rendered in renderings("output memo") {
            assert!(!rendered.contains("memo differs"), "{rendered}");
        }
    }

    #[test]
    fn multisig_summary_labels_required_signatures() {
        use tari_common_types::types::{CompressedPublicKey, PrivateKey, UncompressedPublicKey};

        let keys = (0..3)
            .map(|_| {
                CompressedPublicKey::new_from_pk(UncompressedPublicKey::from_secret_key(&PrivateKey::random(
                    &mut rand::rng(),
                )))
            })
            .collect();
        let request = PrepareDepositMultisigTransactionResult {
            version: get_latest_version(),
            tx_id: TxId::from(1u64),
            info: OneSidedMultisigTransactionInfo::new(info(&[1000]), keys, 2),
            payload_signature: PayloadIntegritySignature {
                signature: Default::default(),
            },
            payload_key_id: None,
            expires_at: unix_now().saturating_add(3600),
        };

        let summary = PayloadSummary::from_multisig(request);
        assert_eq!(summary.kind, PayloadKind::MultisigDeposit);
        let rendered = summary.to_string();
        assert!(rendered.contains("Payload        : multisig deposit"), "{rendered}");
        assert!(
            rendered.contains(&format!(
                "From           : {} (as stated by the payload, not checked)\n",
                summary.sender_address.to_base58()
            )),
            "{rendered}"
        );
        assert!(rendered.contains("Required signatures (m of n) : 2 of 3"), "{rendered}");
        assert!(rendered.contains("  Required signatures 2 of 3\n"), "{rendered}");
        assert!(!rendered.contains("Multisig party"), "{rendered}");
        let recap = &rendered[rendered.find("---- Recap ----").unwrap()..];
        for (i, key) in summary.multisig_public_keys.iter().enumerate() {
            assert!(recap.contains(&format!("    key [{}] {key}\n", i + 1)), "{rendered}");
        }
    }

    /// Reading from this input is a test failure.
    struct UnreadableInput;

    impl std::io::Read for UnreadableInput {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            panic!("input must not be read")
        }
    }

    impl BufRead for UnreadableInput {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            panic!("input must not be read")
        }

        fn consume(&mut self, _amt: usize) {
            panic!("input must not be read")
        }
    }

    #[test]
    fn confirmation_for_another_summary_is_refused() {
        let summary_a = PayloadSummary::from_one_sided(one_sided_payload(info(&[1000])));
        let summary_b = PayloadSummary::from_one_sided(one_sided_payload(info(&[1000])));
        let confirmation_a = present_payload_summary(
            &summary_a,
            ConfirmationMode::NonInteractive,
            &mut Vec::new(),
            &mut std::io::empty(),
        )
        .unwrap();
        assert!(confirmation_a.is_approved());
        assert_eq!(
            summary_b.confirm(confirmation_a).unwrap_err(),
            PayloadNotConfirmed::DifferentSummary
        );
    }

    #[test]
    fn declined_confirmation_is_refused() {
        for answer in ["n\n", "", "maybe\n"] {
            let summary = PayloadSummary::from_one_sided(one_sided_payload(info(&[1000])));
            let mut out = Vec::new();
            let confirmation = present_payload_summary(
                &summary,
                ConfirmationMode::Interactive,
                &mut out,
                &mut answer.as_bytes(),
            )
            .unwrap();
            assert!(!confirmation.is_approved(), "{answer:?}");
            assert_eq!(
                summary.confirm(confirmation).unwrap_err(),
                PayloadNotConfirmed::Declined,
                "{answer:?}"
            );
            assert!(
                String::from_utf8(out)
                    .unwrap()
                    .ends_with("Sign this transaction? [y/N]: ")
            );
        }
        for answer in ["y\n", "YES\n", " y \n"] {
            let summary = PayloadSummary::from_one_sided(one_sided_payload(info(&[1000])));
            let confirmation = present_payload_summary(
                &summary,
                ConfirmationMode::Interactive,
                &mut Vec::new(),
                &mut answer.as_bytes(),
            )
            .unwrap();
            assert!(confirmation.is_approved(), "{answer:?}");
            assert!(summary.confirm(confirmation).is_ok(), "{answer:?}");
        }
    }

    #[test]
    fn non_interactive_presentation_writes_summary() {
        let summary = PayloadSummary::from_one_sided(one_sided_payload(info(&[1000])));
        let mut out = Vec::new();
        let confirmation = present_payload_summary(
            &summary,
            ConfirmationMode::NonInteractive,
            &mut out,
            &mut UnreadableInput,
        )
        .unwrap();
        assert!(confirmation.is_approved());
        let written = String::from_utf8(out).unwrap();
        // Another test may install a logger at any moment, so either form is accepted
        let prefix = format!(
            "\nThe following transaction is about to be signed with your spend key:\n\n{summary}\nSigning without a \
             prompt (non-interactive mode or no terminal)."
        );
        let rest = written.strip_prefix(&prefix).unwrap_or_else(|| panic!("{written}"));
        assert!(
            rest == "\n" || rest == " A record (transaction id and payload hash) has been logged.\n",
            "{written}"
        );
    }
}
