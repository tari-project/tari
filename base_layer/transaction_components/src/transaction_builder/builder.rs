// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{fmt, fmt::Debug};

use log::*;
use tari_common::configuration::Network;
use tari_common_types::{
    tari_address::{TariAddress, TariAddressFeatures},
    types::{
        ComAndPubSignature,
        CompressedCommitment,
        CompressedPublicKey,
        CompressedSignature,
        FixedHash,
        PrivateKey,
        UncompressedPublicKey,
        UncompressedSignature,
    },
};
use tari_script::{ExecutionStack, Opcode, TariScript, push_pubkey_script, script};
use tari_utilities::{ByteArray, hex::Hex};

use crate::{
    MicroMinotari,
    consensus::ConsensusConstants,
    fee::{Fee, recipient_output_features_and_scripts_size},
    helpers::borsh::SerializedSize,
    key_manager::{TariKeyAndId, TariKeyId, TransactionKeyManagerInterface, TxoStage, error::KeyManagerError},
    multisig::script::derive_multisig_ephemeral_pubkeys,
    transaction_builder::{
        error::TransactionBuilderError,
        models::{
            FinalizedTransaction,
            OutputPair,
            RecipientDetails,
            RecipientKeys,
            RecipientMetadataSignature,
            RecipientScript,
            RecipientScriptKey,
            RecipientSpec,
        },
    },
    transaction_components::{
        CoreTransactionBuilder,
        KernelBuilder,
        KernelFeatures,
        MAX_TRANSACTION_INPUTS,
        MAX_TRANSACTION_OUTPUTS,
        OutputFeatures,
        TransactionKernel,
        TransactionKernelVersion,
        TransactionOutput,
        TransactionOutputVersion,
        WalletOutput,
        WalletOutputBuilder,
        covenants::Covenant,
        memo_field::{MemoField, TxType},
        one_sided::{public_key_to_output_encryption_key, public_key_to_output_spending_key},
    },
    tx_outputs_to_tx_id,
    weight::TransactionWeight,
};

pub const LOG_TARGET: &str = "c::tx::tx_builder";

/// Which half of the transaction is still open.
///
/// The phase exists because exactly one `get_script_offset` call may be made per transaction. That call folds in
/// every input script key and generates every sender offset key at once, so the set of inputs and the number of
/// outputs both have to be final before it happens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuilderPhase {
    /// Inputs and recipient specs may still be added.
    Collecting,
    /// The sender offset keys have been reserved; the pool and the change decision are fixed.
    Reserved,
}

/// An output that will be attached to the builder *after* the sender offset keys have been reserved.
///
/// The change decision made in [`TransactionBuilder::reserve_sender_offset_keys`] is binding, and so is the fee it
/// computes, so the reservation has to see the value and the weight of every output the transaction will carry.
/// Flows that cannot express an output as a [`RecipientSpec`] - because the published sender offset key is only one
/// share of an aggregate, or because the output arrived fully formed from an untrusted payload - declare it here
/// and attach it afterwards with [`TransactionBuilder::with_output`] or [`TransactionBuilder::add_recipient`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingOutput {
    value: MicroMinotari,
    features_and_scripts_size: usize,
    takes_reserved_key: bool,
}

impl PendingOutput {
    /// An output that will take one of the sender offset keys the reservation returns.
    pub fn keyed(value: MicroMinotari, features_and_scripts_size: usize) -> Self {
        Self {
            value,
            features_and_scripts_size,
            takes_reserved_key: true,
        }
    }

    /// An output that publishes a sender offset key the caller derived itself and registered with
    /// [`TransactionBuilder::with_host_derived_partial_script_offset`], so it takes no key from the pool.
    pub fn host_keyed(value: MicroMinotari, features_and_scripts_size: usize) -> Self {
        Self {
            value,
            features_and_scripts_size,
            takes_reserved_key: false,
        }
    }

    /// Declare an output that does not exist yet, measured from the shape it will have.
    ///
    /// `build` checks the declaration against what actually arrives, so measuring by hand is a trap; this is the
    /// same measurement the fee calculation uses. A same-shaped placeholder script is fine - what matters is that
    /// the declaration does not come out *smaller* than the finished output.
    pub fn measured(
        weighting: &TransactionWeight,
        value: MicroMinotari,
        features: &OutputFeatures,
        script: &TariScript,
        covenant: &Covenant,
        memo: &MemoField,
    ) -> Result<Self, TransactionBuilderError> {
        Ok(Self::keyed(
            value,
            recipient_output_features_and_scripts_size(weighting, features, script, covenant, memo)?,
        ))
    }

    pub fn value(&self) -> MicroMinotari {
        self.value
    }

    pub fn features_and_scripts_size(&self) -> usize {
        self.features_and_scripts_size
    }

    /// Declare an output that already exists.
    pub fn from_output(output: &WalletOutput) -> Result<Self, TransactionBuilderError> {
        Ok(Self::keyed(
            output.value(),
            output
                .features_and_scripts_byte_size()
                .map_err(|e| TransactionBuilderError::InvalidSerializedSize(e.to_string()))?,
        ))
    }
}

/// The fee and change the reservation committed the transaction to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FeeAndChange {
    fee: MicroMinotari,
    /// `Some` exactly when a change key was reserved, so `build` must emit a change output for it.
    change: Option<MicroMinotari>,
}

#[derive(Clone)]
pub struct TransactionBuilder<KM> {
    consensus_constants: ConsensusConstants,
    key_manager: KM,
    fee_per_gram: Option<MicroMinotari>,
    fee: MicroMinotari,
    recipient_specs: Vec<RecipientSpec>,
    recipient_outputs: Vec<RecipientDetails>,
    inputs: Vec<OutputPair>,
    custom_outputs: Vec<OutputPair>,
    prevent_fee_gt_amount: bool,
    tx_type: TxType,
    memo_field: Option<MemoField>,
    lock_height: u64,
    kernel_features: KernelFeatures,
    burn_commitment: Option<CompressedCommitment>,
    own_address: TariAddress,
    /// The running `sum(script keys) - sum(sender offset keys)` for the parts of the transaction that have already
    /// been accounted for. Partial offsets are additive, so a transaction can combine the one the key manager
    /// returned with any the host derived itself.
    partial_script_offset: PrivateKey,
    /// Script keys of the inputs, folded into `partial_script_offset` by the single reservation.
    pending_input_script_keys: Vec<TariKeyId>,
    phase: BuilderPhase,
    /// One reserved key per recipient spec, in spec order.
    spec_sender_offset_keys: Vec<TariKeyAndId>,
    /// The reserved change key. Its presence *is* the change decision.
    change_sender_offset_key: Option<TariKeyAndId>,
    /// The binding fee and change amount, fixed when the keys were reserved.
    fee_and_change: Option<FeeAndChange>,
    /// How many outputs of each kind were attached before the reservation, so that `build` can tell which ones
    /// arrived afterwards and check them against what was declared.
    custom_outputs_before_reserve: usize,
    recipient_outputs_before_reserve: usize,
    /// What the reservation was told would follow it, and therefore what the fee and the change decision it
    /// committed to were computed for.
    declared_pending_outputs: usize,
    declared_pending_value: MicroMinotari,
    declared_pending_weight: usize,
    /// The keys handed back to the caller by the reservation. Each one is subtracted from the script offset, so
    /// each one has to end up on a published output.
    caller_sender_offset_keys: Vec<TariKeyId>,
}

impl<KM> TransactionBuilder<KM>
where KM: TransactionKeyManagerInterface
{
    pub fn new(
        consensus_constants: ConsensusConstants,
        key_manager: KM,
        network: Network,
    ) -> Result<Self, TransactionBuilderError> {
        let view_key = key_manager.get_view_key();
        let spend_key = key_manager.get_spend_key();
        let own_address = TariAddress::new_dual_address(
            view_key.pub_key.clone(),
            spend_key.pub_key.clone(),
            network,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )?;

        Ok(Self {
            consensus_constants,
            key_manager,
            fee_per_gram: None,
            fee: MicroMinotari::zero(),
            recipient_specs: Vec::new(),
            recipient_outputs: Vec::new(),
            inputs: Vec::new(),
            custom_outputs: Vec::new(),
            prevent_fee_gt_amount: true,
            tx_type: TxType::PaymentToOther,
            memo_field: None,
            lock_height: 0,
            kernel_features: KernelFeatures::empty(),
            burn_commitment: None,
            own_address,
            partial_script_offset: PrivateKey::default(),
            pending_input_script_keys: Vec::new(),
            phase: BuilderPhase::Collecting,
            spec_sender_offset_keys: Vec::new(),
            change_sender_offset_key: None,
            fee_and_change: None,
            custom_outputs_before_reserve: 0,
            recipient_outputs_before_reserve: 0,
            declared_pending_outputs: 0,
            declared_pending_value: MicroMinotari::zero(),
            declared_pending_weight: 0,
            caller_sender_offset_keys: Vec::new(),
        })
    }

    /// Set the fee per weight for the transaction. See (Fee::calculate)[Struct.Fee.html#calculate] for how the
    /// absolute fee is calculated from the fee-per-gram value. This will take precedence over the fee set by
    /// `with_fee`.
    pub fn with_fee_per_gram(&mut self, fee_per_gram: MicroMinotari) -> &mut Self {
        self.fee_per_gram = Some(fee_per_gram);
        self
    }

    pub fn fee(&self) -> MicroMinotari {
        self.fee
    }

    pub fn fee_per_gram(&self) -> Option<MicroMinotari> {
        self.fee_per_gram
    }

    pub fn with_lock_height(&mut self, lock_height: u64) -> &mut Self {
        self.lock_height = lock_height;
        self
    }

    /// Sets the fee of the transaction. Fee per gram takes precedence over this value.
    pub fn with_fee(&mut self, fee: MicroMinotari) -> &mut Self {
        self.fee = fee;
        self
    }

    /// Sets the transaction type, default is TxType::PaymentToOther
    pub fn with_tx_type(&mut self, tx_type: TxType) -> &mut Self {
        self.tx_type = tx_type;
        self
    }

    /// Sets the payment id for the transaction. This is used to identify the transaction and is included in the
    /// transaction metadata.
    pub fn with_memo(&mut self, memo: MemoField) -> &mut Self {
        self.memo_field = Some(memo);
        self
    }

    pub fn with_kernel_features(&mut self, kernel_features: KernelFeatures) -> &mut Self {
        self.kernel_features = kernel_features;
        self
    }

    /// Add a recipient whose output is already built.
    ///
    /// `sender_offset_key_id` must come from [`Self::reserve_sender_offset_keys`], or the caller must have registered
    /// the matching partial script offset with [`Self::with_host_derived_partial_script_offset`]; otherwise the key
    /// is never subtracted from the script offset and the transaction will not validate.
    pub fn add_recipient(
        &mut self,
        recipient_address: TariAddress,
        recipient_output: WalletOutput,
        sender_offset_key_id: TariKeyId,
        custom_recovery_key_id: Option<TariKeyId>,
    ) -> Result<&mut Self, TransactionBuilderError> {
        let kernel_nonce = self.key_manager.get_random_key(None, None)?;
        let recipient_output = OutputPair::new(
            recipient_output,
            kernel_nonce.key_id,
            sender_offset_key_id,
            custom_recovery_key_id,
        );
        let recipient_details = RecipientDetails {
            output: recipient_output,
            recipient_address,
        };
        self.recipient_outputs.push(recipient_details);
        Ok(self)
    }

    /// Declare a recipient output for the builder to construct in [`Self::build`].
    ///
    /// Specs belong to the collecting phase: the sender offset key an output publishes is only reserved once the
    /// builder knows how many outputs the transaction has, because a single `get_script_offset` call for the whole
    /// transaction is what keeps both sides of that sum blinded. Declaring a recipient after the reservation would
    /// mint a key against no input script keys at all, which is what this design exists to prevent.
    pub fn with_recipient_spec(&mut self, spec: RecipientSpec) -> Result<&mut Self, TransactionBuilderError> {
        if self.phase != BuilderPhase::Collecting {
            return Err(TransactionBuilderError::RecipientSpecAfterReserve);
        }
        self.recipient_specs.push(spec);
        Ok(self)
    }

    /// Declare an ordinary one sided payment to `destination`.
    ///
    /// A thin wrapper over [`Self::with_recipient_spec`], and therefore part of the collecting phase.
    pub fn add_stealth_recipient(
        &mut self,
        destination: TariAddress,
        amount: MicroMinotari,
        output_features: OutputFeatures,
        memo_field: MemoField,
    ) -> Result<(), TransactionBuilderError> {
        self.with_recipient_spec(RecipientSpec::stealth(destination, amount, output_features, memo_field))?;
        Ok(())
    }

    /// Reserve every sender offset key this transaction needs, in a single call to the key manager.
    ///
    /// This is the one point at which key material is generated for the transaction's outputs. It folds every input
    /// script key into the script offset and generates one sender offset key per recipient spec, per declared
    /// pending output that asked for one, and one more for the change output if there will be one. Splitting it
    /// across several calls is what the design forbids: a later call would see no input script keys left to fold in,
    /// and its reply would be a bare sender offset private key.
    ///
    /// The change decision and the fee are made here and are binding - `build` emits exactly what this decided.
    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
    pub fn reserve_sender_offset_keys(
        &mut self,
        pending_outputs: &[PendingOutput],
    ) -> Result<Vec<TariKeyAndId>, TransactionBuilderError> {
        if self.phase != BuilderPhase::Collecting {
            return Err(TransactionBuilderError::SenderOffsetKeysAlreadyReserved);
        }

        let fee_and_change = self.decide_fee_and_change(pending_outputs)?;
        let extra = pending_outputs.iter().filter(|o| o.takes_reserved_key).count();
        let count = self
            .recipient_specs
            .len()
            .saturating_add(usize::from(fee_and_change.change.is_some()))
            .saturating_add(extra);
        let script_keys = std::mem::take(&mut self.pending_input_script_keys);
        let (partial_offset, mut sender_offset_keys) = self
            .key_manager
            .get_script_offset(&script_keys, count)
            .map_err(surface_device_key_limit)?;
        if sender_offset_keys.len() != count {
            return Err(TransactionBuilderError::SenderOffsetKeyPoolExhausted);
        }
        self.partial_script_offset = &self.partial_script_offset + partial_offset;

        // The caller's keys come off the end, so the pool keeps the spec keys at the front in spec order.
        let caller_keys = sender_offset_keys.split_off(count.saturating_sub(extra));
        if fee_and_change.change.is_some() {
            self.change_sender_offset_key = sender_offset_keys.pop();
        }
        self.spec_sender_offset_keys = sender_offset_keys;

        self.fee_and_change = Some(fee_and_change);
        self.custom_outputs_before_reserve = self.custom_outputs.len();
        self.recipient_outputs_before_reserve = self.recipient_outputs.len();
        self.declared_pending_outputs = pending_outputs.len();
        self.declared_pending_value =
            pending_outputs
                .iter()
                .map(|o| o.value)
                .try_fold(MicroMinotari::zero(), |acc, x| {
                    acc.checked_add(x)
                        .ok_or(TransactionBuilderError::TransactionAmountOverflow)
                })?;
        self.declared_pending_weight = self.rounded_weight(pending_outputs.iter().map(|o| o.features_and_scripts_size));
        self.caller_sender_offset_keys = caller_keys.iter().map(|k| k.key_id.clone()).collect();
        self.phase = BuilderPhase::Reserved;
        Ok(caller_keys)
    }

    /// Register a partial script offset the host derived itself.
    ///
    /// The only legitimate use is an output whose sender offset key is reconstructible from the wallet seed and is
    /// never a device secret - an L2 bound burn's `r`, derived from the output's own commitment mask, so that the
    /// burn proof can be rebuilt from seed alone after recovery. A key that came off a ledger device must never be
    /// registered here: the whole point of the device generating it is that the host never learns it.
    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
    pub fn with_host_derived_partial_script_offset(&mut self, partial_script_offset: PrivateKey) -> &mut Self {
        self.partial_script_offset = &self.partial_script_offset + partial_script_offset;
        self
    }

    /// Add an input to the transaction.
    ///
    /// Inputs must all be added before the sender offset keys are reserved: reserving folds the script keys of the
    /// inputs added so far into the script offset, so a later input would never be accounted for.
    pub fn with_input(&mut self, input: WalletOutput) -> Result<&mut Self, TransactionBuilderError> {
        if self.phase != BuilderPhase::Collecting {
            return Err(TransactionBuilderError::InputsAfterOutputs);
        }
        let nonce = self.key_manager.get_random_key(None, None)?;
        self.pending_input_script_keys.push(input.script_key_id().clone());
        let pair = OutputPair::new(input, nonce.key_id, TariKeyId::Zero, None);
        self.inputs.push(pair);
        Ok(self)
    }

    /// This will allow the receipient to sign the burn commitment
    pub fn with_burn_commitment(&mut self, commitment: Option<CompressedCommitment>) -> &mut Self {
        self.burn_commitment = commitment;
        self
    }

    /// Add a custom output to the transaction.
    ///
    /// `sender_offset_key_id` must come from [`Self::reserve_sender_offset_keys`], or the caller must have registered
    /// the matching partial script offset with [`Self::with_host_derived_partial_script_offset`]; otherwise the key
    /// is never subtracted from the script offset and the transaction will not validate.
    pub fn with_output(
        &mut self,
        output: WalletOutput,
        sender_offset_key_id: TariKeyId,
        custom_recovery_key_id: Option<TariKeyId>,
    ) -> Result<&mut Self, TransactionBuilderError> {
        let nonce = self.key_manager.get_random_key(None, None)?;
        let pair = OutputPair::new(output, nonce.key_id, sender_offset_key_id, custom_recovery_key_id);
        self.custom_outputs.push(pair);
        Ok(self)
    }

    /// Enable or disable spending of an amount less than the fee
    pub fn with_prevent_fee_gt_amount(&mut self, prevent_fee_gt_amount: bool) -> &mut Self {
        self.prevent_fee_gt_amount = prevent_fee_gt_amount;
        self
    }

    /// The weight a recipient spec's output will contribute, measured from the spec rather than from the finished
    /// output.
    ///
    /// The reservation has to charge for outputs that do not exist yet, and the change decision it makes is binding,
    /// so `build` must not re-measure them against the outputs it goes on to construct - a byte of difference in
    /// either direction could flip the decision.
    pub fn spec_features_and_scripts_size(&self, spec: &RecipientSpec) -> Result<usize, TransactionBuilderError> {
        let script = match &spec.script {
            RecipientScript::Explicit(script) => (**script).clone(),
            // Every default script is a single `PushPubKey`, whatever key ends up in it.
            RecipientScript::Default => script!(PushPubKey(Box::default()))?,
            RecipientScript::Multisig {
                party_number,
                public_keys,
                message,
            } => TariScript::new(vec![
                Opcode::CheckMultiSigVerify(
                    *party_number,
                    u8::try_from(public_keys.len()).unwrap_or(u8::MAX),
                    vec![CompressedPublicKey::default(); public_keys.len()],
                    message.clone(),
                ),
                Opcode::PushPubKey(Box::default()),
            ])?,
        };
        recipient_output_features_and_scripts_size(
            self.consensus_constants.transaction_weight_params(),
            &spec.features,
            &script,
            &spec.covenant,
            &spec.memo,
        )
    }

    fn get_total_features_and_scripts_size_for_outputs(&self) -> Result<usize, TransactionBuilderError> {
        let fee_weighting = Fee::new(*self.consensus_constants.transaction_weight_params());
        let mut size = 0usize;
        for o in &self.custom_outputs {
            size = size.saturating_add(
                fee_weighting.weighting().round_up_features_and_scripts_size(
                    o.output
                        .features_and_scripts_byte_size()
                        .map_err(|e| TransactionBuilderError::InvalidSerializedSize(e.to_string()))?,
                ),
            );
        }
        for recipient in &self.recipient_outputs {
            size = size.saturating_add(
                fee_weighting.weighting().round_up_features_and_scripts_size(
                    recipient
                        .output
                        .output
                        .features_and_scripts_byte_size()
                        .map_err(|e| TransactionBuilderError::InvalidSerializedSize(e.to_string()))?,
                ),
            );
        }
        for spec in &self.recipient_specs {
            size = size.saturating_add(self.spec_features_and_scripts_size(spec)?);
        }
        Ok(size)
    }

    pub fn get_total_input_value(&self) -> Result<MicroMinotari, TransactionBuilderError> {
        self.inputs
            .iter()
            .map(|i| i.output.value())
            .try_fold(MicroMinotari::zero(), |acc, x| {
                acc.checked_add(x)
                    .ok_or(TransactionBuilderError::TransactionAmountOverflow)
            })
    }

    pub fn inputs(&self) -> &[OutputPair] {
        &self.inputs
    }

    pub fn recipient_outputs(&self) -> &[RecipientDetails] {
        &self.recipient_outputs
    }

    pub fn recipient_specs(&self) -> &[RecipientSpec] {
        &self.recipient_specs
    }

    pub fn custom_outputs(&self) -> &[OutputPair] {
        &self.custom_outputs
    }

    pub fn get_fee_estimate_without_change(&self) -> Result<MicroMinotari, TransactionBuilderError> {
        self.fee_estimate_without_change(&[])
    }

    /// The fee this transaction would carry with `pending` further outputs attached and no change output.
    ///
    /// Flows that spend an entire input to a single recipient need this before they can work out what that
    /// recipient's amount is, and the output does not exist yet at that point.
    pub fn get_fee_estimate_with(&self, pending: &[PendingOutput]) -> Result<MicroMinotari, TransactionBuilderError> {
        self.fee_estimate_without_change(pending)
    }

    fn fee_estimate_without_change(&self, pending: &[PendingOutput]) -> Result<MicroMinotari, TransactionBuilderError> {
        let num_outputs = self
            .custom_outputs
            .len()
            .saturating_add(self.recipient_outputs.len())
            .saturating_add(self.recipient_specs.len())
            .saturating_add(pending.len());
        let num_inputs = self.inputs.len();
        let fee_weighting = Fee::new(*self.consensus_constants.transaction_weight_params());
        Ok(match self.fee_per_gram {
            Some(fee_per_gram) => {
                let mut features_and_scripts_size = self.get_total_features_and_scripts_size_for_outputs()?;
                for output in pending {
                    features_and_scripts_size = features_and_scripts_size.saturating_add(
                        fee_weighting
                            .weighting()
                            .round_up_features_and_scripts_size(output.features_and_scripts_size),
                    );
                }
                fee_weighting.calculate(fee_per_gram, 1, num_inputs, num_outputs, features_and_scripts_size)
            },
            None => self.fee,
        })
    }

    /// Round each output's features and scripts size the way the fee calculation does, and total them. Rounding is
    /// idempotent, so a caller that measured with `recipient_output_features_and_scripts_size` and one that measured
    /// a finished output land on the same number whenever they describe the same output.
    fn rounded_weight(&self, sizes: impl Iterator<Item = usize>) -> usize {
        let weighting = Fee::new(*self.consensus_constants.transaction_weight_params());
        sizes.fold(0usize, |acc, size| {
            acc.saturating_add(weighting.weighting().round_up_features_and_scripts_size(size))
        })
    }

    /// Check the outputs that were attached after the reservation against what was declared to it.
    ///
    /// The reservation's fee and change decision are binding, and they were computed from the declaration, so a
    /// declaration that does not describe what actually arrived means the transaction carries a fee for outputs it
    /// does not have. The count alone is not enough: an output of the same count but a larger value or weight is
    /// exactly the case that under-pays the fee and over-states the change.
    ///
    /// Weight is checked in one direction only. An output that does not exist yet can only be measured from a
    /// same-shaped placeholder, and the encrypted memo it ends up carrying may be a little shorter than the memo it
    /// was measured from. Declaring more than arrives only over-pays the fee, which is the same safe direction as
    /// the under-reserved change case; declaring less is what has to be refused.
    fn check_pending_outputs_match_declaration(&self) -> Result<(), TransactionBuilderError> {
        let attached_custom = self
            .custom_outputs
            .get(self.custom_outputs_before_reserve..)
            .unwrap_or_default();
        let attached_recipients = self
            .recipient_outputs
            .get(self.recipient_outputs_before_reserve..)
            .unwrap_or_default();
        let attached = attached_custom
            .iter()
            .chain(attached_recipients.iter().map(|r| &r.output))
            .map(|pair| &pair.output)
            .collect::<Vec<_>>();

        if attached.len() != self.declared_pending_outputs {
            return Err(TransactionBuilderError::UndeclaredOutputAfterReserve {
                declared: self.declared_pending_outputs,
                added: attached.len(),
            });
        }

        let actual_value = attached
            .iter()
            .map(|output| output.value())
            .try_fold(MicroMinotari::zero(), |acc, x| {
                acc.checked_add(x)
                    .ok_or(TransactionBuilderError::TransactionAmountOverflow)
            })?;
        let mut sizes = Vec::with_capacity(attached.len());
        for output in &attached {
            sizes.push(
                output
                    .features_and_scripts_byte_size()
                    .map_err(|e| TransactionBuilderError::InvalidSerializedSize(e.to_string()))?,
            );
        }
        let actual_weight = self.rounded_weight(sizes.into_iter());

        if actual_value != self.declared_pending_value || actual_weight > self.declared_pending_weight {
            return Err(TransactionBuilderError::PendingOutputMismatch {
                declared_value: self.declared_pending_value,
                actual_value,
                declared_weight: self.declared_pending_weight,
                actual_weight,
            });
        }
        Ok(())
    }

    fn check_conditions(&self) -> Result<(), TransactionBuilderError> {
        if self.fee_per_gram.is_none() && self.fee == MicroMinotari::zero() {
            return Err(TransactionBuilderError::FeeNotSet);
        }
        if self.recipient_outputs.is_empty() && self.custom_outputs.is_empty() && self.recipient_specs.is_empty() {
            return Err(TransactionBuilderError::NoRecipients);
        }
        if self.inputs.is_empty() {
            return Err(TransactionBuilderError::NoInputs);
        }
        if self.inputs.len() > MAX_TRANSACTION_INPUTS {
            return Err(TransactionBuilderError::ExceedsMaxInputs(MAX_TRANSACTION_INPUTS));
        }
        if self
            .recipient_outputs
            .len()
            .saturating_add(self.custom_outputs.len())
            .saturating_add(self.recipient_specs.len()) >
            MAX_TRANSACTION_OUTPUTS
        {
            return Err(TransactionBuilderError::ExceedsMaxOutputs(MAX_TRANSACTION_OUTPUTS));
        }
        Ok(())
    }

    fn total_output_value(&self, pending: &[PendingOutput]) -> Result<MicroMinotari, TransactionBuilderError> {
        let add = |acc: MicroMinotari, x: MicroMinotari| {
            acc.checked_add(x)
                .ok_or(TransactionBuilderError::TransactionAmountOverflow)
        };
        let mut total = self
            .custom_outputs
            .iter()
            .map(|o| o.output.value())
            .try_fold(MicroMinotari::zero(), add)?;
        total = self
            .recipient_outputs
            .iter()
            .map(|o| o.output.output.value())
            .try_fold(total, add)?;
        total = self.recipient_specs.iter().map(|s| s.amount).try_fold(total, add)?;
        pending.iter().map(|o| o.value).try_fold(total, add)
    }

    /// The weight induced fee a change output would add.
    fn change_output_fee(&self) -> Result<MicroMinotari, TransactionBuilderError> {
        let fee_weighting = Fee::new(*self.consensus_constants.transaction_weight_params());
        Ok(match self.fee_per_gram {
            Some(fee_per_gram) => {
                fee_weighting.calculate(fee_per_gram, 0, 0, 1, self.change_features_and_scripts_size()?)
            },
            None => 0.into(),
        })
    }

    fn change_features_and_scripts_size(&self) -> Result<usize, TransactionBuilderError> {
        let fee_weighting = Fee::new(*self.consensus_constants.transaction_weight_params());
        let temp_script = script!(PushPubKey(Box::default()))?;
        let change_payment_id_size = self
            .create_change_memo(MicroMinotari(0))
            .map(|m| m.get_size())
            .unwrap_or(0);
        let size = OutputFeatures::default()
            .get_serialized_size()
            .map_err(|e| TransactionBuilderError::InvalidSerializedSize(e.to_string()))?
            .saturating_add(
                temp_script
                    .get_serialized_size()
                    .map_err(|e| TransactionBuilderError::InvalidSerializedSize(e.to_string()))?,
            )
            .saturating_add(change_payment_id_size);
        Ok(fee_weighting.weighting().round_up_features_and_scripts_size(size))
    }

    /// Decide, once and for all, what this transaction's fee is and whether it carries change.
    ///
    /// The decision is deliberately conservative: a change key is reserved only when the projected change exceeds
    /// the weight induced fee that same output would add. That is the same test the change amount itself has to
    /// pass, so the case where a key is reserved and there turns out to be nothing to put it on cannot arise from
    /// rounding. The opposite case is harmless - with no change output the remainder simply goes to the fee.
    fn decide_fee_and_change(&self, pending: &[PendingOutput]) -> Result<FeeAndChange, TransactionBuilderError> {
        let total_being_spent = self.get_total_input_value()?;
        let total_sent = self.total_output_value(pending)?;
        let fee_without_change = self.fee_estimate_without_change(pending)?;
        let combined_sent = total_sent
            .checked_add(fee_without_change)
            .ok_or(TransactionBuilderError::TransactionAmountOverflow)?;

        let remainder =
            total_being_spent
                .checked_sub(combined_sent)
                .ok_or(TransactionBuilderError::SpendingMoreThanAvailable {
                    available: total_being_spent,
                    sent: combined_sent,
                })?;

        let change_fee = self.change_output_fee()?;
        let (fee, change) = match remainder.checked_sub(change_fee) {
            // Not enough to cover a change output, so the remainder goes to the fee.
            None | Some(MicroMinotari(0)) => (add_fee(fee_without_change, remainder)?, None),
            Some(change) => (add_fee(fee_without_change, change_fee)?, Some(change)),
        };

        Ok(FeeAndChange { fee, change })
    }

    fn create_change_memo(&self, amount: MicroMinotari) -> Result<MemoField, TransactionBuilderError> {
        let mut memo = MemoField::new_transaction_info(
            TariAddress::default(),
            MicroMinotari::default(),
            amount,
            true,
            self.tx_type,
            Vec::new(),
            self.memo_field
                .as_ref()
                .map(|pay_id| pay_id.payment_id_as_bytes())
                .unwrap_or_default(),
        )
        .map_err(TransactionBuilderError::InvalidMemo)?;

        // we only set for the first output, otherwise the extra data gets too large
        let first_recipient = self
            .recipient_outputs
            .first()
            .map(|r| (r.output.output.value(), r.recipient_address.clone()))
            .or_else(|| self.recipient_specs.first().map(|s| (s.amount, s.destination.clone())));
        if let Some((value, address)) = first_recipient {
            memo.transaction_info_set_amount(value);
            match memo.get_type() {
                TxType::PaymentToOther => memo
                    .transaction_info_set_address(address)
                    .map_err(TransactionBuilderError::InvalidMemo)?,
                TxType::PaymentToSelf |
                TxType::CoinSplit |
                TxType::CoinJoin |
                TxType::ValidatorNodeRegistration |
                TxType::CodeTemplateRegistration |
                TxType::ClaimAtomicSwap |
                TxType::HtlcAtomicSwapRefund => memo
                    .transaction_info_set_address(self.own_address.clone())
                    .map_err(TransactionBuilderError::InvalidMemo)?,
                _ => {},
            }
        } else {
            memo.transaction_info_set_amount(amount);
            memo.transaction_info_set_address(self.own_address.clone())
                .map_err(TransactionBuilderError::InvalidMemo)?;
        }
        let mut sent_hashes = Vec::new();
        for recipient in &self.recipient_outputs {
            sent_hashes.push(recipient.output.output.output_hash());
        }
        // A spec's output does not exist yet, but its hash occupies the same space in the memo as any other. The
        // reservation charges for this memo and the change decision it makes is binding, so the memo it measures has
        // to be the same size as the one `build` writes - by which time every spec has become a recipient output.
        for _ in &self.recipient_specs {
            sent_hashes.push(FixedHash::zero());
        }
        // if its too much outputs, we dont track this
        if sent_hashes.len() <= 2 {
            memo.transaction_info_set_sent_output_hashes(sent_hashes)
                .map_err(TransactionBuilderError::InvalidMemo)?;
        }
        Ok(memo)
    }

    /// Resolve a spec's commitment mask, the key the recipient recovers the output with, and - for an output the
    /// wallet is sending to itself - the script key generated alongside the mask.
    fn spec_keys(
        &self,
        spec: &RecipientSpec,
        sender_offset: &TariKeyAndId,
    ) -> Result<(TariKeyId, Option<TariKeyId>, Option<TariKeyAndId>), TransactionBuilderError> {
        match &spec.keys {
            RecipientKeys::DiffieHellman => {
                let view_key = spec
                    .destination
                    .public_view_key()
                    .ok_or(TransactionBuilderError::InvalidAddressNoViewKey)?
                    .clone();
                Ok((
                    TariKeyId::DHCommitmentMask {
                        private_key: sender_offset.key_id.clone().into(),
                        public_key: view_key.clone(),
                    },
                    Some(TariKeyId::DHEncryptedData {
                        private_key: sender_offset.key_id.clone().into(),
                        public_key: view_key,
                    }),
                    None,
                ))
            },
            RecipientKeys::DiffieHellmanEncrypted => {
                let view_key = spec
                    .destination
                    .public_view_key()
                    .ok_or(TransactionBuilderError::InvalidAddressNoViewKey)?;
                let shared_secret = self
                    .key_manager
                    .get_diffie_hellman_shared_secret(&sender_offset.key_id, view_key)?;
                let commitment_mask = self.key_manager.create_encrypted_key(
                    public_key_to_output_spending_key(&shared_secret).map_err(TransactionBuilderError::from)?,
                    None,
                )?;
                let encryption = self.key_manager.create_encrypted_key(
                    public_key_to_output_encryption_key(&shared_secret).map_err(TransactionBuilderError::from)?,
                    None,
                )?;
                Ok((commitment_mask, Some(encryption), None))
            },
            RecipientKeys::Own => {
                let (commitment_mask, script_key) = self.key_manager.get_next_commitment_mask_and_script_key()?;
                Ok((commitment_mask.key_id, None, Some(script_key)))
            },
        }
    }

    /// Build the script a spec's output publishes.
    fn spec_script(
        &self,
        spec: &RecipientSpec,
        sender_offset: &TariKeyAndId,
        commitment_mask_key_id: &TariKeyId,
        own_script_key: Option<&TariKeyAndId>,
    ) -> Result<TariScript, TransactionBuilderError> {
        match &spec.script {
            RecipientScript::Explicit(script) => Ok((**script).clone()),
            RecipientScript::Default => match own_script_key {
                Some(script_key) => Ok(script!(PushPubKey(Box::new(script_key.pub_key.clone())))?),
                None => Ok(push_pubkey_script(
                    &self.key_manager.stealth_address_script_spending_key(
                        commitment_mask_key_id,
                        spec.destination.public_spend_key(),
                    )?,
                )),
            },
            RecipientScript::Multisig {
                party_number,
                public_keys,
                message,
            } => {
                let ephemeral_pubkeys =
                    derive_multisig_ephemeral_pubkeys(&self.key_manager, public_keys, &sender_offset.key_id)?;
                let script_pubkey = self
                    .key_manager
                    .stealth_address_script_spending_key(commitment_mask_key_id, spec.destination.public_spend_key())?;
                Ok(TariScript::new(vec![
                    Opcode::CheckMultiSigVerify(
                        *party_number,
                        u8::try_from(ephemeral_pubkeys.len())
                            .map_err(|e| TransactionBuilderError::Other(e.to_string()))?,
                        ephemeral_pubkeys,
                        message.clone(),
                    ),
                    Opcode::PushPubKey(Box::new(script_pubkey)),
                ])?)
            },
        }
    }

    /// Construct the output a recipient spec describes, around the sender offset key reserved for it.
    fn build_spec_output(
        &self,
        spec: &RecipientSpec,
        sender_offset: &TariKeyAndId,
        fee: MicroMinotari,
    ) -> Result<(WalletOutput, Option<TariKeyId>), TransactionBuilderError> {
        let mut memo = spec.memo.clone();
        memo.set_fee(fee);

        let (commitment_mask_key_id, recovery_key_id, own_script_key) = self.spec_keys(spec, sender_offset)?;
        let script = self.spec_script(spec, sender_offset, &commitment_mask_key_id, own_script_key.as_ref())?;
        let script_key_id = match &spec.script_key {
            RecipientScriptKey::Zero => TariKeyId::Zero,
            RecipientScriptKey::OwnSpendKey => self.key_manager.get_spend_key().key_id,
            RecipientScriptKey::Own => own_script_key.as_ref().map(|k| k.key_id.clone()).ok_or_else(|| {
                TransactionBuilderError::Other(
                    "RecipientScriptKey::Own needs RecipientKeys::Own to generate the key".to_string(),
                )
            })?,
        };

        let builder = WalletOutputBuilder::new(spec.amount, commitment_mask_key_id)
            .with_features(spec.features.clone())
            .with_script(script)
            .with_covenant(spec.covenant.clone())
            .encrypt_data_for_recovery(&self.key_manager, recovery_key_id.as_ref(), memo)?
            .with_input_data(ExecutionStack::default())
            .with_sender_offset_public_key(sender_offset.pub_key.clone())
            .with_script_key(script_key_id)
            .with_minimum_value_promise(spec.minimum_value_promise);

        // Spec built outputs are constructed after the fee is final, so each one is signed once and a ledger device
        // prompts once.
        let builder = match spec.metadata_signature {
            RecipientMetadataSignature::UserVerified => builder.sign_metadata_signature_user_verified(
                &self.key_manager,
                &sender_offset.key_id,
                &spec.destination,
            )?,
            RecipientMetadataSignature::Unverified => {
                builder.sign_metadata_signature(&self.key_manager, &sender_offset.key_id)?
            },
        };

        Ok((builder.try_build(&self.key_manager)?, recovery_key_id))
    }

    fn build_change(
        &mut self,
        amount: MicroMinotari,
        sender_offset: TariKeyAndId,
    ) -> Result<OutputPair, TransactionBuilderError> {
        let (change_commitment_mask_key, change_script_key) =
            self.key_manager.get_next_commitment_mask_and_script_key()?;
        let memo = self.create_change_memo(amount)?;
        let script = script!(PushPubKey(Box::new(change_script_key.pub_key.clone())))?;
        let input_data = ExecutionStack::default();

        let encrypted_data = self.key_manager.encrypt_data_for_recovery(
            &change_commitment_mask_key.key_id,
            None,
            amount.as_u64(),
            memo.clone(),
        )?;

        let minimum_value_promise = MicroMinotari::zero();

        let output_version = TransactionOutputVersion::get_current_version();

        let features = OutputFeatures::default();
        let covenant = Covenant::default();
        let metadata_message = TransactionOutput::metadata_signature_message_from_parts(
            output_version,
            &script,
            &features,
            &covenant,
            &encrypted_data,
            &minimum_value_promise,
        );

        let metadata_sig = self.key_manager.get_metadata_signature(
            &change_commitment_mask_key.key_id,
            &amount.into(),
            &sender_offset.key_id,
            output_version,
            &metadata_message,
            features.range_proof_type,
        )?;

        let change_wallet_output = WalletOutput::new_current_version(
            amount,
            change_commitment_mask_key.key_id.clone(),
            features,
            script,
            input_data,
            change_script_key.key_id,
            sender_offset.pub_key.clone(),
            metadata_sig,
            0,
            covenant,
            encrypted_data,
            minimum_value_promise,
            memo,
            &self.key_manager,
        )?;
        let nonce = self.key_manager.get_random_key(None, None)?;
        Ok(OutputPair::new(
            change_wallet_output,
            nonce.key_id,
            sender_offset.key_id,
            None,
        ))
    }

    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
    fn calculate_total_nonce_and_total_public_excess(
        &self,
        change: &Option<OutputPair>,
    ) -> Result<(CompressedPublicKey, CompressedPublicKey), TransactionBuilderError> {
        // lets calculate the total sender kernel signature nonce
        let mut public_nonce = UncompressedPublicKey::default();
        // lets calculate the total sender kernel exess
        let mut public_excess = UncompressedPublicKey::default();
        for input in &self.inputs {
            public_nonce = public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&input.kernel_nonce)?
                    .to_public_key()?;
            public_excess = public_excess -
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(
                        input.output.commitment_mask_key_id(),
                        &input.kernel_nonce,
                    )?
                    .to_public_key()?;
        }
        for output in &self.custom_outputs {
            public_nonce = public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&output.kernel_nonce)?
                    .to_public_key()?;
            public_excess = public_excess +
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(
                        output.output.commitment_mask_key_id(),
                        &output.kernel_nonce,
                    )?
                    .to_public_key()?;
        }

        for output in &self.recipient_outputs {
            public_nonce = public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&output.output.kernel_nonce)?
                    .to_public_key()?;
            public_excess = public_excess +
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(
                        output.output.output.commitment_mask_key_id(),
                        &output.output.kernel_nonce,
                    )?
                    .to_public_key()?;
        }

        if let Some(change) = change {
            public_nonce = public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&change.kernel_nonce)?
                    .to_public_key()?;
            public_excess = public_excess +
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(
                        change.output.commitment_mask_key_id(),
                        &change.kernel_nonce,
                    )?
                    .to_public_key()?;
        }
        Ok((
            CompressedPublicKey::new_from_pk(public_nonce),
            CompressedPublicKey::new_from_pk(public_excess),
        ))
    }

    // Helper function to change the memo field and encrypted data if the fee has changed due to a change output
    fn update_encrypted_data_and_metadata_sig(
        key_manager: &KM,
        output_pair: &mut OutputPair,
        final_fee: MicroMinotari,
        recipient_address: Option<&TariAddress>,
    ) -> Result<(), TransactionBuilderError> {
        let mut memo_field = output_pair.output.payment_id().clone();
        let mut need_update = false;
        if let Some(existing_fee) = memo_field.get_fee() {
            if existing_fee == final_fee {
                debug!(
                    target: LOG_TARGET,
                    "[Update fee] Fee ({}) was correct for output '{}'",
                    existing_fee, output_pair.output.commitment().to_hex()
                );
            } else {
                debug!(
                    target: LOG_TARGET,
                    "[Update fee] Changing fee changed from {} to {} for output '{}'",
                    existing_fee, final_fee, output_pair.output.commitment().to_hex()
                );
                need_update = true;
            }
        }
        if output_pair.output.metadata_signature() == &ComAndPubSignature::default() {
            debug!(
                target: LOG_TARGET,
                "[Update fee] Metadata signature is a placeholder for output '{}', updating encrypted data",
                output_pair.output.commitment().to_hex()
            );
            need_update = true;
        };
        info!(
            target: LOG_TARGET,
            "[Update fee] Final fee is {} for output '{}'",
            final_fee,
            output_pair.output.commitment().to_hex()
        );
        if need_update {
            memo_field.set_fee(final_fee);
            let encrypted_data = key_manager.encrypt_data_for_recovery(
                output_pair.output.commitment_mask_key_id(),
                output_pair.custom_recovery_key_id.as_ref(),
                output_pair.output.value().as_u64(),
                memo_field.clone(),
            )?;
            // This will change all the necessary fields in the wallet output
            if let Some(recipient) = recipient_address {
                output_pair.output.change_encrypted_data_with_verified_signature(
                    encrypted_data,
                    &output_pair.sender_offset_key_id,
                    memo_field,
                    recipient,
                    key_manager,
                )?;
            } else {
                output_pair.output.change_encrypted_data(
                    encrypted_data,
                    &output_pair.sender_offset_key_id,
                    memo_field,
                    key_manager,
                )?;
            }
        }

        Ok(())
    }

    /// Turn every recipient spec into an output, using the keys reserved for them.
    fn build_spec_outputs(&mut self, total_fee: MicroMinotari) -> Result<Vec<WalletOutput>, TransactionBuilderError> {
        let specs = std::mem::take(&mut self.recipient_specs);
        let keys = std::mem::take(&mut self.spec_sender_offset_keys);
        let mut keys = keys.into_iter();
        let mut spec_outputs = Vec::with_capacity(specs.len());
        for spec in &specs {
            let sender_offset = keys
                .next()
                .ok_or(TransactionBuilderError::SenderOffsetKeyPoolExhausted)?;
            let (output, recovery_key_id) = self.build_spec_output(spec, &sender_offset, total_fee)?;
            spec_outputs.push(output.clone());
            self.add_recipient(spec.destination.clone(), output, sender_offset.key_id, recovery_key_id)?;
        }
        let remaining = keys.count();
        if remaining > 0 {
            return Err(TransactionBuilderError::SenderOffsetKeyPoolNotDrained { remaining });
        }
        Ok(spec_outputs)
    }

    /// Build the transaction. This will return an error if the transaction is invalid.
    #[allow(clippy::too_many_lines)]
    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
    pub fn build(mut self) -> Result<FinalizedTransaction, TransactionBuilderError> {
        // The reservation is the only place the input script keys are folded into the script offset, so a builder
        // that never reserved would produce a transaction whose script offset does not balance.
        let FeeAndChange {
            fee: total_fee,
            change: change_amount,
        } = self
            .fee_and_change
            .ok_or(TransactionBuilderError::SenderOffsetKeysNotReserved)?;

        // Every key the reservation handed back is subtracted from the script offset, so one that never made it
        // onto a published output would leave the offset unbalanced and the transaction unspendable.
        let unplaced = self
            .caller_sender_offset_keys
            .iter()
            .filter(|key_id| {
                !self
                    .custom_outputs
                    .iter()
                    .chain(self.recipient_outputs.iter().map(|r| &r.output))
                    .any(|o| &&o.sender_offset_key_id == key_id)
            })
            .count();
        if unplaced > 0 {
            return Err(TransactionBuilderError::SenderOffsetKeyPoolNotDrained { remaining: unplaced });
        }

        // Everything the reservation charged for was either present then, declared then, or is a spec - and what
        // was declared has to be what actually arrived, not merely the same number of outputs.
        self.check_pending_outputs_match_declaration()?;

        self.check_conditions()?;

        let total_sent = self.total_output_value(&[])?;
        if total_fee > total_sent {
            warn!(
                target: LOG_TARGET,
                "Fee ({total_fee}) is greater than amount ({total_sent}) being sent for Transaction.",
            );
            if self.prevent_fee_gt_amount {
                return Err(TransactionBuilderError::FeeGreaterThanAmount {
                    fee: total_fee,
                    sent: total_sent,
                });
            }
        }

        // Construct the spec outputs first: the change memo names the first recipient, and the transaction id is
        // derived from the outputs.
        let spec_outputs = self.build_spec_outputs(total_fee)?;

        let mut change_output = match (change_amount, self.change_sender_offset_key.take()) {
            (Some(amount), Some(sender_offset)) => Some(self.build_change(amount, sender_offset)?),
            (None, None) => None,
            // The change decision was made when the keys were reserved and is binding in both directions. A key
            // with no output to carry it is an invariant violation, not a case to paper over: it would be
            // subtracted from the script offset and never published.
            (_, key) => {
                return Err(TransactionBuilderError::SenderOffsetKeyPoolNotDrained {
                    remaining: usize::from(key.is_some()),
                });
            },
        };

        let mut core_tx_builder = CoreTransactionBuilder::new();

        let (total_public_nonce, total_public_excess) =
            self.calculate_total_nonce_and_total_public_excess(&change_output)?;

        let mut offset = PrivateKey::default();
        let mut signature = UncompressedSignature::default();

        let kernel_version = TransactionKernelVersion::get_current_version();
        for input in &self.inputs {
            core_tx_builder.add_input(input.output.to_transaction_input(&self.key_manager)?.clone());
        }
        let mut sent_outputs = Vec::new();
        for recipient in &mut self.recipient_outputs {
            Self::update_encrypted_data_and_metadata_sig(
                &self.key_manager,
                &mut recipient.output,
                total_fee,
                Some(&recipient.recipient_address),
            )?;

            let output = recipient.output.output.to_transaction_output()?;
            sent_outputs.push(recipient.output.clone());
            if self.tx_type == TxType::Burn {
                // lets do some burn logic
                if output.is_burned() {
                    match self.burn_commitment {
                        Some(_burn_commitment) => {
                            // we can only have a single burn commitment here, so we error here
                            return Err(TransactionBuilderError::MultipleBurnCommitments);
                        },
                        None => {
                            self.burn_commitment = Some(output.commitment.clone());
                        },
                    }
                }
            }
            core_tx_builder.add_output(output);
        }

        let kernel_message = TransactionKernel::build_kernel_signature_message(
            TransactionKernelVersion::get_current_version(),
            total_fee,
            self.lock_height,
            &self.kernel_features,
            &self.burn_commitment,
        );

        for input in &self.inputs {
            signature = &signature +
                &(self
                    .key_manager
                    .get_partial_txo_kernel_signature(
                        input.output.commitment_mask_key_id(),
                        &input.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        kernel_version,
                        &kernel_message,
                        &self.kernel_features,
                        TxoStage::Input,
                    )?
                    .to_schnorr_signature()?);
            offset = offset -
                self.key_manager
                    .get_txo_private_kernel_offset(input.output.commitment_mask_key_id(), &input.kernel_nonce)?;
        }

        for output in &mut self.custom_outputs {
            Self::update_encrypted_data_and_metadata_sig(&self.key_manager, output, total_fee, None)?;
            core_tx_builder.add_output(output.output.to_transaction_output()?);
            signature = &signature +
                self.key_manager
                    .get_partial_txo_kernel_signature(
                        output.output.commitment_mask_key_id(),
                        &output.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        kernel_version,
                        &kernel_message,
                        &self.kernel_features,
                        TxoStage::Output,
                    )?
                    .to_schnorr_signature()?;
            offset = offset +
                &self
                    .key_manager
                    .get_txo_private_kernel_offset(output.output.commitment_mask_key_id(), &output.kernel_nonce)?;
        }

        for output in &self.recipient_outputs {
            signature = &signature +
                self.key_manager
                    .get_partial_txo_kernel_signature(
                        output.output.output.commitment_mask_key_id(),
                        &output.output.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        kernel_version,
                        &kernel_message,
                        &self.kernel_features,
                        TxoStage::Output,
                    )?
                    .to_schnorr_signature()?;
            offset = offset +
                &self.key_manager.get_txo_private_kernel_offset(
                    output.output.output.commitment_mask_key_id(),
                    &output.output.kernel_nonce,
                )?;
        }

        if let Some(change) = &mut change_output {
            Self::update_encrypted_data_and_metadata_sig(&self.key_manager, change, total_fee, None)?;
            core_tx_builder.add_output(change.output.to_transaction_output()?);
            signature = &signature +
                &self
                    .key_manager
                    .get_partial_txo_kernel_signature(
                        change.output.commitment_mask_key_id(),
                        &change.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        kernel_version,
                        &kernel_message,
                        &self.kernel_features,
                        TxoStage::Output,
                    )?
                    .to_schnorr_signature()?;
            offset = offset +
                &self
                    .key_manager
                    .get_txo_private_kernel_offset(change.output.commitment_mask_key_id(), &change.kernel_nonce)?;
        }

        let script_offset = self.partial_script_offset.clone();

        core_tx_builder.add_offset(offset);
        core_tx_builder.add_script_offset(script_offset);
        let excess = CompressedCommitment::from_compressed_key(total_public_excess);

        let kernel = KernelBuilder::new()
            .with_fee(total_fee)
            .with_features(self.kernel_features)
            .with_lock_height(self.lock_height)
            .with_burn_commitment(self.burn_commitment.clone())
            .with_excess(&excess)
            .with_signature(CompressedSignature::new_from_schnorr(signature))
            .build()?;
        core_tx_builder.with_kernel(kernel);
        let tx = core_tx_builder.build()?;

        let view_key = self.key_manager.get_view_key().pub_key;
        let tx_id = if let Some(wallet_output) = &change_output {
            wallet_output.output.calculate_tx_id(view_key.as_bytes())
        } else {
            tx_outputs_to_tx_id(view_key.as_bytes(), tx.body.outputs())
        };

        let destination_addresses = self
            .recipient_outputs
            .iter()
            .map(|r| r.recipient_address.clone())
            .collect::<Vec<TariAddress>>();

        let mut amount = self
            .recipient_outputs
            .iter()
            .map(|r| r.output.output.value())
            .try_fold(MicroMinotari::zero(), |acc, x| {
                acc.checked_add(x)
                    .ok_or(TransactionBuilderError::TransactionAmountOverflow)
            })?;
        amount += self
            .custom_outputs
            .iter()
            .map(|o| o.output.value())
            .try_fold(MicroMinotari::zero(), |acc, x| {
                acc.checked_add(x)
                    .ok_or(TransactionBuilderError::TransactionAmountOverflow)
            })?;
        let mut sent_hashes = Vec::new();
        for recipient in &self.recipient_outputs {
            sent_hashes.push(recipient.output.output.output_hash());
        }
        let mut received_hashes = Vec::new();
        for output in &self.custom_outputs {
            received_hashes.push(output.output.output_hash());
        }
        let change_output_hash = match &change_output {
            Some(o) => vec![o.output.output_hash()],
            None => vec![],
        };

        let payment_id = if let Some(mut memo_field) = self.memo_field {
            if let Some(fee) = memo_field.get_fee() {
                if fee == total_fee {
                    debug!(target: LOG_TARGET, "[Update fee] Fee ({}) was correct for entire transaction", total_fee);
                } else {
                    debug!(target: LOG_TARGET,
                        "[Update fee] Fee changed from {} to {} for entire transaction",
                        fee, total_fee
                    );
                }

                memo_field.set_fee(total_fee);
                memo_field
            } else {
                memo_field
            }
        } else {
            MemoField::default()
        };

        // The spec built outputs went through `update_encrypted_data_and_metadata_sig` above like every other
        // recipient, so return the versions that are actually in the body.
        let spec_outputs = spec_outputs
            .iter()
            .map(|built| {
                sent_outputs
                    .iter()
                    .find(|o| o.output.commitment() == built.commitment())
                    .map(|o| o.output.clone())
                    .unwrap_or_else(|| built.clone())
            })
            .collect();

        Ok(FinalizedTransaction {
            tx_id,
            source_address: self.own_address,
            destination_addresses,
            amount,
            fee: total_fee,
            transaction: tx,
            payment_id,
            change: change_output.map(|o| o.output),
            spec_outputs,
            custom_outputs: self.custom_outputs.into_iter().map(|o| o.output).collect(),
            sent_outputs,
            // Hashes of outputs being sent to others (excluding change)
            sent_output_hashes: sent_hashes,
            // Hashes of outputs received from others (excluding change)
            received_output_hashes: received_hashes,
            // Hashes of change outputs (for reference)
            change_output_hashes: change_output_hash,
        })
    }
}
// For some reason clippy picks up the debug impl as not used, and key_manager is a trait without debug, so we need to
// manually implement Debug for TransactionBuilder
#[allow(dead_code)]
impl<KM> Debug for TransactionBuilder<KM> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        #[derive(Debug)]
        pub struct TransactionBuilder<'a> {
            consensus_constants: &'a ConsensusConstants,
            fee_per_gram: &'a Option<MicroMinotari>,
            fee: &'a MicroMinotari,
            recipient_specs: usize,
            recipient_outputs: &'a Vec<RecipientDetails>,
            inputs: &'a Vec<OutputPair>,
            custom_outputs: &'a Vec<OutputPair>,
            prevent_fee_gt_amount: &'a bool,
            tx_type: &'a TxType,
            memo_field: &'a Option<MemoField>,
            lock_height: &'a u64,
            kernel_features: &'a KernelFeatures,
            burn_commitment: &'a Option<CompressedCommitment>,
            own_address: &'a TariAddress,
        }

        let Self {
            consensus_constants,
            key_manager: _,
            fee_per_gram,
            fee,
            recipient_specs,
            recipient_outputs,
            inputs,
            custom_outputs,
            prevent_fee_gt_amount,
            tx_type,
            memo_field,
            lock_height,
            kernel_features,
            burn_commitment,
            own_address,
            partial_script_offset: _,
            pending_input_script_keys: _,
            phase: _,
            spec_sender_offset_keys: _,
            change_sender_offset_key: _,
            fee_and_change: _,
            custom_outputs_before_reserve: _,
            recipient_outputs_before_reserve: _,
            declared_pending_outputs: _,
            declared_pending_value: _,
            declared_pending_weight: _,
            caller_sender_offset_keys: _,
        } = self;

        fmt::Debug::fmt(
            &TransactionBuilder {
                consensus_constants,
                fee_per_gram,
                fee,
                recipient_specs: recipient_specs.len(),
                recipient_outputs,
                inputs,
                custom_outputs,
                prevent_fee_gt_amount,
                tx_type,
                memo_field,
                lock_height,
                kernel_features,
                burn_commitment,
                own_address,
            },
            f,
        )
    }
}

/// Translate the key manager's device key limit into the output limit it really is.
///
/// A ledger device derives every sender offset key of a transaction in one exchange, so it caps how many it will
/// make. A caller that hits that cap needs to be told its transaction has too many outputs, not handed a key manager
/// error - let alone a status word - to reverse engineer. Software key managers have no such cap, so nothing else
/// is rewritten.
fn surface_device_key_limit(e: KeyManagerError) -> TransactionBuilderError {
    match e {
        KeyManagerError::TooManySenderOffsetKeys { requested, max } => {
            TransactionBuilderError::TooManyOutputsForDevice { requested, max }
        },
        other => other.into(),
    }
}

/// Adds two fee amounts, returning an overflow error rather than wrapping.
fn add_fee(fee: MicroMinotari, extra: MicroMinotari) -> Result<MicroMinotari, TransactionBuilderError> {
    fee.checked_add(extra)
        .ok_or(TransactionBuilderError::TransactionAmountOverflow)
}

#[cfg(test)]
#[allow(clippy::too_many_lines)]
mod test {
    use minotari_ledger_wallet_common::script_offset::MAX_SENDER_OFFSET_KEYS;
    use tari_common_types::seeds::{cipher_seed::CipherSeed, mnemonic::Mnemonic, seed_words::SeedWords};
    use tari_script::Opcode;
    use tari_utilities::Hidden;

    use crate::{
        key_manager::SecretTransactionKeyManagerInterface,
        transaction_components::{
            EncryptedData,
            one_sided::{public_key_to_output_encryption_key, public_key_to_output_spending_key},
        },
    };
    fn create_view_key_manager(view_wallet: ViewWallet) -> Result<KeyManager, KeyManagerError> {
        let wallet = WalletType::ViewWallet(view_wallet);
        KeyManager::new(wallet)
    }
    use tari_crypto::keys::SecretKey;
    use tari_script::{TariScript, script};

    use super::*;
    use crate::{
        crypto_factories::CryptoFactories,
        key_manager::{
            KeyManager,
            error::KeyManagerError,
            wallet_types::{SeedWordsWallet, ViewWallet, WalletType},
        },
        tari_amount::{MicroMinotari, uT},
        test_helpers::{
            TestParams,
            UtxoTestParams,
            add_outputs_with_reserved_sender_offset_keys,
            create_consensus_constants,
            create_consensus_manager,
            create_test_input,
            create_wallet_output_with_data,
        },
        transaction_builder::TransactionBuilder,
        transaction_components::{MemoField, OutputFeatures},
        validation::transaction::TransactionInternalConsistencyValidator,
    };

    /// A wallet address to send to that is not this wallet's own.
    fn random_address() -> TariAddress {
        TariAddress::new_dual_address(
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap()
    }

    /// Assert that a transaction is internally consistent, which includes checking that
    /// `sum(input script keys) - sum(output sender offset keys)` equals the published script offset.
    fn assert_validates(tx: &crate::transaction_components::Transaction) {
        let validator =
            TransactionInternalConsistencyValidator::new(false, create_consensus_manager(), CryptoFactories::default());
        validator
            .validate(tx, None, None, u64::MAX)
            .expect("the transaction, and therefore its script offset, must be valid");
    }

    /// Hit the edge case where our change isn't enough to cover the cost of an extra output
    #[test]
    #[allow(clippy::identity_op)]
    fn change_edge_case() {
        // Create some inputs
        let key_manager = KeyManager::new_random().unwrap();
        let p = TestParams::new(&key_manager);
        let constants = create_consensus_constants(0);
        let weighting = constants.transaction_weight_params();
        let tx_fee = Fee::new(*weighting).calculate(1.into(), 1, 1, 1, 0);
        let fee_for_change_output = weighting.params().output_weight * uT;
        // fee == 340, output = 80
        // outputs weight: 1060, kernel weight: 10, input weight: 9, output weight: 53,

        // Pay out so that I should get change, but not enough to pay for the output
        let input = create_test_input(
            // one under the amount required to pay the fee for a change output
            2000 * uT + tx_fee + fee_for_change_output - 1 * uT,
            0,
            &key_manager,
            vec![],
            None,
        );
        let output = p
            .create_output(
                UtxoTestParams {
                    value: 2000 * uT,
                    ..Default::default()
                },
                &key_manager,
            )
            .unwrap();
        // Start the builder
        let mut builder = TransactionBuilder::new(constants, key_manager.clone(), Network::LocalNet).unwrap();
        builder
            .with_lock_height(0)
            .with_input(input)
            .unwrap()
            .with_fee_per_gram(MicroMinotari(1))
            .with_prevent_fee_gt_amount(false);
        add_outputs_with_reserved_sender_offset_keys(&mut builder, vec![output]).unwrap();
        let result = builder.build().unwrap();
        assert_eq!(
            result.transaction.body.kernels().first().unwrap().lock_height,
            0,
            "Lock height"
        );
        assert_eq!(result.fee, tx_fee + fee_for_change_output - 1 * uT, "Fee");
        assert_eq!(
            result.transaction.body.kernels().first().unwrap().fee,
            tx_fee + fee_for_change_output - 1 * uT,
            "Fee"
        );
        assert_eq!(result.transaction.body.outputs().len(), 1, "There should be 1 output");
        assert_eq!(result.transaction.body.inputs().len(), 1, "There should be 1 input");
    }

    #[test]
    fn too_many_inputs() {
        // Create some inputs
        let key_manager = KeyManager::new_random().unwrap();
        let p = TestParams::new(&key_manager);

        let output = create_wallet_output_with_data(
            script!(Nop).unwrap(),
            OutputFeatures::default(),
            &p,
            MicroMinotari(500),
            &key_manager,
        )
        .unwrap();
        let constants = create_consensus_constants(0);
        // Start the builder
        let mut builder = TransactionBuilder::new(constants, key_manager.clone(), Network::LocalNet).unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(2))
            .with_prevent_fee_gt_amount(false);
        let input_base = create_test_input(MicroMinotari(50), 0, &key_manager, vec![], None);
        for _ in 0..=MAX_TRANSACTION_INPUTS {
            builder.with_input(input_base.clone()).unwrap();
        }
        add_outputs_with_reserved_sender_offset_keys(&mut builder, vec![output]).unwrap();
        let err = builder.build().unwrap_err();
        assert!(
            matches!(err, TransactionBuilderError::ExceedsMaxInputs(_)),
            "unexpected error: {err:?}"
        );
    }

    /// A custom output can be rewritten during the build — the encrypted data records the final fee, and the metadata
    /// signature is remade to match — so the transaction body carries a different output to the one handed in.
    /// `FinalizedTransaction::custom_outputs` must expose the published version, because a caller that stores its own
    /// pre-build copy would record a hash that is not on chain and lose track of the output.
    #[test]
    fn it_returns_custom_outputs_as_they_were_published() {
        let key_manager = KeyManager::new_random().unwrap();
        let p = TestParams::new(&key_manager);
        let input = create_test_input(MicroMinotari(50000), 0, &key_manager, vec![], None);

        // A memo whose recorded fee is wrong, which is what makes the builder rewrite the output
        let mut payment_id = MemoField::new_address_and_data(
            TariAddress::default(),
            MicroMinotari(999999),
            true,
            TxType::PaymentToSelf,
            Vec::new(),
        )
        .unwrap();
        payment_id.set_fee(MicroMinotari(999999));

        let output = p
            .create_output(
                UtxoTestParams {
                    value: MicroMinotari(10000),
                    payment_id,
                    ..Default::default()
                },
                &key_manager,
            )
            .unwrap();
        let handed_in = output.clone();

        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(5))
            .with_input(input)
            .unwrap();
        add_outputs_with_reserved_sender_offset_keys(&mut builder, vec![output]).unwrap();

        let finalized = builder.build().unwrap();
        let published = finalized.custom_outputs.first().expect("the custom output is returned");

        // The rewrite happened, so the copy the caller handed in is already out of date
        assert_ne!(
            published.output_hash(),
            handed_in.output_hash(),
            "this test is only meaningful when the builder rewrites the output"
        );
        // ...and what is returned is what went into the transaction
        assert!(
            finalized
                .transaction
                .body
                .outputs()
                .iter()
                .any(|o| o.hash() == published.output_hash()),
            "the returned output must be the one in the body"
        );
        assert_eq!(
            published.commitment(),
            handed_in.commitment(),
            "the commitment is unchanged, so callers can match the two up"
        );
    }

    #[test]
    fn not_enough_funds() {
        // Create some inputs
        let key_manager = KeyManager::new_random().unwrap();
        let p = TestParams::new(&key_manager);
        let input = create_test_input(MicroMinotari(400), 0, &key_manager, vec![], None);
        let script = script!(Nop).unwrap();
        let output = create_wallet_output_with_data(
            script.clone(),
            OutputFeatures::default(),
            &p,
            MicroMinotari(400),
            &key_manager,
        )
        .unwrap();
        // Start the builder
        let constants = create_consensus_constants(0);
        let mut builder = TransactionBuilder::new(constants, key_manager.clone(), Network::LocalNet).unwrap();
        builder
            .with_lock_height(0)
            .with_input(input)
            .unwrap()
            .with_fee_per_gram(MicroMinotari(1));
        // The reservation is where the fee and the change are decided, so this is where spending more than the
        // inputs cover is caught.
        let err = add_outputs_with_reserved_sender_offset_keys(&mut builder, vec![output]).unwrap_err();
        assert!(
            matches!(err, TransactionBuilderError::SpendingMoreThanAvailable { .. }),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn inputs_cannot_be_added_after_the_sender_offset_keys_are_reserved() {
        let key_manager = KeyManager::new_random().unwrap();
        let input = create_test_input(MicroMinotari(1200), 0, &key_manager, vec![], None);
        let late_input = create_test_input(MicroMinotari(1200), 0, &key_manager, vec![], None);
        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
        builder.with_fee_per_gram(MicroMinotari(2)).with_input(input).unwrap();

        // Reserving folds in the script keys of the inputs added so far, so a later input would never be accounted
        // for and the script offset would be wrong.
        builder.reserve_sender_offset_keys(&[]).unwrap();

        let err = builder.with_input(late_input).unwrap_err();
        assert!(
            matches!(err, TransactionBuilderError::InputsAfterOutputs),
            "unexpected error: {err:?}"
        );
    }

    /// The reservation is the only place the input script keys are folded into the script offset, so a builder that
    /// never reserved would publish an offset that does not balance.
    #[test]
    fn build_refuses_a_transaction_that_never_reserved_its_keys() {
        let key_manager = KeyManager::new_random().unwrap();
        let p = TestParams::new(&key_manager);
        let input = create_test_input(MicroMinotari(2000), 0, &key_manager, vec![], None);
        let output = p
            .create_output(
                UtxoTestParams {
                    value: MicroMinotari(1900),
                    ..Default::default()
                },
                &key_manager,
            )
            .unwrap();
        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
        builder
            .with_fee(MicroMinotari(100))
            .with_prevent_fee_gt_amount(false)
            .with_input(input)
            .unwrap()
            // The sender offset key came from `TestParams`, not from the builder, so nothing ever folded the input
            // script keys in.
            .with_output(output, p.sender_offset_key_id.clone(), None)
            .unwrap();

        let err = builder.build().unwrap_err();
        assert!(
            matches!(err, TransactionBuilderError::SenderOffsetKeysNotReserved),
            "unexpected error: {err:?}"
        );
    }

    /// One `get_script_offset` call per transaction is the whole design: a second call would see no input script
    /// keys left to fold in, and its reply would be a bare sender offset private key.
    #[test]
    fn the_sender_offset_keys_can_only_be_reserved_once() {
        let key_manager = KeyManager::new_random().unwrap();
        let input = create_test_input(MicroMinotari(20000), 0, &key_manager, vec![], None);
        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
        builder.with_fee_per_gram(MicroMinotari(2)).with_input(input).unwrap();

        builder.reserve_sender_offset_keys(&[]).unwrap();
        let err = builder.reserve_sender_offset_keys(&[]).unwrap_err();
        assert!(
            matches!(err, TransactionBuilderError::SenderOffsetKeysAlreadyReserved),
            "unexpected error: {err:?}"
        );
    }

    /// The semantic change in the two phase builder: declaring a recipient after the reservation used to silently
    /// mint a sender offset key against no input script keys at all, which is exactly the unblinded offset the
    /// design exists to prevent. Now it is refused.
    #[test]
    fn a_recipient_spec_cannot_be_declared_after_the_reservation() {
        let key_manager = KeyManager::new_random().unwrap();
        let input = create_test_input(MicroMinotari(20000), 0, &key_manager, vec![], None);
        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
        builder.with_fee_per_gram(MicroMinotari(2)).with_input(input).unwrap();
        builder.reserve_sender_offset_keys(&[]).unwrap();

        let err = builder
            .with_recipient_spec(RecipientSpec::stealth(
                random_address(),
                MicroMinotari(1000),
                OutputFeatures::default(),
                MemoField::new_empty(),
            ))
            .unwrap_err();
        assert!(
            matches!(err, TransactionBuilderError::RecipientSpecAfterReserve),
            "unexpected error: {err:?}"
        );

        // `add_stealth_recipient` is a thin wrapper over the spec, so it inherits the phase rule.
        let err = builder
            .add_stealth_recipient(
                random_address(),
                MicroMinotari(1000),
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .unwrap_err();
        assert!(
            matches!(err, TransactionBuilderError::RecipientSpecAfterReserve),
            "unexpected error: {err:?}"
        );
    }

    /// Every key the reservation hands back is subtracted from the script offset, so one that is never published
    /// would leave the transaction unspendable. That has to be caught at build time, not on chain.
    #[test]
    fn build_refuses_a_reserved_key_that_was_never_placed_on_an_output() {
        let key_manager = KeyManager::new_random().unwrap();
        let p = TestParams::new(&key_manager);
        let input = create_test_input(MicroMinotari(50000), 0, &key_manager, vec![], None);
        let output = p
            .create_output(
                UtxoTestParams {
                    value: MicroMinotari(10000),
                    ..Default::default()
                },
                &key_manager,
            )
            .unwrap();

        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
        builder.with_fee_per_gram(MicroMinotari(5)).with_input(input).unwrap();
        // Two keys are asked for, but only one output is ever attached.
        let pending = vec![
            PendingOutput::from_output(&output).unwrap(),
            PendingOutput::from_output(&output).unwrap(),
        ];
        let mut keys = builder.reserve_sender_offset_keys(&pending).unwrap();
        let first = keys.remove(0);
        let mut output = output;
        output.set_sender_offset_public_key(first.pub_key.clone());
        output.set_metadata_signature(Default::default());
        builder.with_output(output, first.key_id, None).unwrap();

        let err = builder.build().unwrap_err();
        assert!(
            matches!(err, TransactionBuilderError::SenderOffsetKeyPoolNotDrained {
                remaining: 1
            }),
            "unexpected error: {err:?}"
        );
    }

    /// The fee and the change decision are made when the keys are reserved, so an output attached afterwards that
    /// was never declared was never charged for.
    #[test]
    fn build_refuses_an_output_that_was_never_declared() {
        let key_manager = KeyManager::new_random().unwrap();
        let p = TestParams::new(&key_manager);
        let input = create_test_input(MicroMinotari(50000), 0, &key_manager, vec![], None);
        let output = p
            .create_output(
                UtxoTestParams {
                    value: MicroMinotari(10000),
                    ..Default::default()
                },
                &key_manager,
            )
            .unwrap();

        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
        builder.with_fee_per_gram(MicroMinotari(5)).with_input(input).unwrap();
        builder.reserve_sender_offset_keys(&[]).unwrap();
        builder
            .with_output(output, p.sender_offset_key_id.clone(), None)
            .unwrap();

        let err = builder.build().unwrap_err();
        assert!(
            matches!(err, TransactionBuilderError::UndeclaredOutputAfterReserve {
                declared: 0,
                added: 1
            }),
            "unexpected error: {err:?}"
        );
    }

    /// The reservation's fee and change decision are binding *and* were computed from the declaration, so the
    /// declaration has to describe what actually arrives. The count alone is not enough: an output of the same
    /// count but a larger value is exactly the case that over-states the change and leaves the transaction
    /// unbalanced.
    #[test]
    fn build_refuses_an_output_worth_more_than_was_declared() {
        let key_manager = KeyManager::new_random().unwrap();
        let p = TestParams::new(&key_manager);
        let input = create_test_input(MicroMinotari(50000), 0, &key_manager, vec![], None);
        let output = p
            .create_output(
                UtxoTestParams {
                    value: MicroMinotari(10000),
                    ..Default::default()
                },
                &key_manager,
            )
            .unwrap();

        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
        builder.with_fee_per_gram(MicroMinotari(5)).with_input(input).unwrap();
        // Declared as worth a tenth of what is actually attached.
        let mut declared = PendingOutput::from_output(&output).unwrap();
        declared = PendingOutput::keyed(MicroMinotari(1000), declared.features_and_scripts_size());
        let sender_offset = builder.reserve_sender_offset_keys(&[declared]).unwrap().pop().unwrap();

        let mut output = output;
        output.set_sender_offset_public_key(sender_offset.pub_key.clone());
        output.set_metadata_signature(Default::default());
        builder.with_output(output, sender_offset.key_id, None).unwrap();

        let err = builder.build().unwrap_err();
        match err {
            TransactionBuilderError::PendingOutputMismatch {
                declared_value,
                actual_value,
                ..
            } => {
                assert_eq!(declared_value, MicroMinotari(1000));
                assert_eq!(actual_value, MicroMinotari(10000));
            },
            other => panic!("expected PendingOutputMismatch, got {other:?}"),
        }
    }

    /// The same for weight, which is what the fee was computed from. Only the unsafe direction is refused: an
    /// output that does not exist yet can only be measured from a same-shaped placeholder, so declaring a little
    /// more than arrives has to stay legal - it just over-pays the fee.
    #[test]
    fn build_refuses_an_output_heavier_than_was_declared_but_allows_a_lighter_one() {
        let key_manager = KeyManager::new_random().unwrap();
        let constants = create_consensus_constants(0);

        let attach = |declared_size: usize| {
            let p = TestParams::new(&key_manager);
            let input = create_test_input(MicroMinotari(50000), 0, &key_manager, vec![], None);
            let output = p
                .create_output(
                    UtxoTestParams {
                        value: MicroMinotari(10000),
                        script: script!(PushPubKey(Box::default())).unwrap(),
                        ..Default::default()
                    },
                    &key_manager,
                )
                .unwrap();
            let mut builder =
                TransactionBuilder::new(constants.clone(), key_manager.clone(), Network::LocalNet).unwrap();
            builder.with_fee_per_gram(MicroMinotari(5)).with_input(input).unwrap();
            let sender_offset = builder
                .reserve_sender_offset_keys(&[PendingOutput::keyed(MicroMinotari(10000), declared_size)])
                .unwrap()
                .pop()
                .unwrap();
            let mut output = output;
            output.set_sender_offset_public_key(sender_offset.pub_key.clone());
            output.set_metadata_signature(Default::default());
            builder.with_output(output, sender_offset.key_id, None).unwrap();
            builder.build()
        };

        // Declaring nothing at all under-pays the fee for a real script and memo.
        let err = attach(0).unwrap_err();
        match err {
            TransactionBuilderError::PendingOutputMismatch {
                declared_weight,
                actual_weight,
                ..
            } => {
                assert_eq!(declared_weight, 0);
                assert!(actual_weight > 0, "the attached output has weight");
            },
            other => panic!("expected PendingOutputMismatch, got {other:?}"),
        }

        // Declaring generously is the safe direction and stays legal.
        assert!(attach(4096).is_ok());
    }

    /// ...and the disciplined path - declaring straight from the output that will be attached - must keep working,
    /// so the check cannot regress into refusing legitimate callers.
    #[test]
    fn a_declaration_taken_from_the_output_itself_is_accepted() {
        let key_manager = KeyManager::new_random().unwrap();
        let p = TestParams::new(&key_manager);
        let input = create_test_input(MicroMinotari(50000), 0, &key_manager, vec![], None);
        let output = p
            .create_output(
                UtxoTestParams {
                    value: MicroMinotari(10000),
                    ..Default::default()
                },
                &key_manager,
            )
            .unwrap();

        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
        builder.with_fee_per_gram(MicroMinotari(5)).with_input(input).unwrap();
        let sender_offset = builder
            .reserve_sender_offset_keys(&[PendingOutput::from_output(&output).unwrap()])
            .unwrap()
            .pop()
            .unwrap();

        let mut output = output;
        output.set_sender_offset_public_key(sender_offset.pub_key.clone());
        output.set_metadata_signature(Default::default());
        builder.with_output(output, sender_offset.key_id, None).unwrap();

        let finalized = builder.build().unwrap();
        assert_validates(&finalized.transaction);
    }

    /// A ledger device derives every sender offset key of a transaction in one exchange, so it caps how many it
    /// will make. That has to reach the caller as an output limit, not as a key manager error - let alone a status
    /// word - because the actionable fact is "this transaction has too many outputs for a ledger".
    ///
    /// The key manager side of this is covered by
    /// `key_manager::manager::tests::get_script_offset_refuses_more_sender_offset_keys_than_the_device_will_derive`;
    /// a ledger key manager cannot be constructed at all without the `ledger` feature, so the translation is
    /// exercised on its own here.
    #[test]
    fn the_device_key_limit_surfaces_as_an_output_limit() {
        let max = usize::try_from(MAX_SENDER_OFFSET_KEYS).unwrap();
        let err = surface_device_key_limit(KeyManagerError::TooManySenderOffsetKeys {
            requested: max + 1,
            max,
        });
        match err {
            TransactionBuilderError::TooManyOutputsForDevice {
                requested,
                max: reported,
            } => {
                assert_eq!(requested, max + 1);
                assert_eq!(reported, max);
            },
            other => panic!("expected TooManyOutputsForDevice, got {other:?}"),
        }
        assert!(
            err.to_string().contains("outputs"),
            "the message must name the limit the caller can act on: {err}"
        );

        // Everything else the key manager can say is passed through unchanged.
        let other = surface_device_key_limit(KeyManagerError::UnblindedScriptOffset {
            script_keys: 0,
            sender_offset_keys: 1,
        });
        assert!(
            matches!(other, TransactionBuilderError::KeyManagerError(_)),
            "unexpected error: {other:?}"
        );
    }

    /// A software key manager has no such cap - it is a guard on device work, not a protocol rule - so a large
    /// multi-recipient transaction still builds.
    #[test]
    fn a_software_key_manager_has_no_output_limit() {
        let key_manager = KeyManager::new_random().unwrap();
        let max = usize::try_from(MAX_SENDER_OFFSET_KEYS).unwrap();
        let input = create_test_input(MicroMinotari(10_000_000), 0, &key_manager, vec![], None);
        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
        builder.with_fee_per_gram(MicroMinotari(1));
        builder.with_input(input).unwrap();
        for _ in 0..=max {
            builder
                .add_stealth_recipient(
                    random_address(),
                    MicroMinotari(1000),
                    OutputFeatures::default(),
                    MemoField::new_empty(),
                )
                .unwrap();
        }
        assert!(builder.reserve_sender_offset_keys(&[]).is_ok());
        let finalized = builder.build().unwrap();
        assert_eq!(finalized.transaction.body.outputs().len(), max + 2);
        assert_validates(&finalized.transaction);
    }

    /// A transaction that will carry change reserves exactly one more key than it has outputs, and one that will
    /// not reserves none. The count is what the device is asked for, so it has to be exact.
    #[test]
    fn the_change_decision_fixes_the_number_of_keys_reserved() {
        let key_manager = KeyManager::new_random().unwrap();
        let constants = create_consensus_constants(0);

        // With change: a large input and a small payment.
        let mut with_change =
            TransactionBuilder::new(constants.clone(), key_manager.clone(), Network::LocalNet).unwrap();
        with_change
            .with_fee_per_gram(MicroMinotari(2))
            .with_input(create_test_input(MicroMinotari(100_000), 0, &key_manager, vec![], None))
            .unwrap();
        with_change
            .add_stealth_recipient(
                random_address(),
                MicroMinotari(1000),
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .unwrap();
        // The caller gets no keys back, but a change key was reserved behind the scenes, and `build` must produce
        // an output for it.
        assert!(with_change.reserve_sender_offset_keys(&[]).unwrap().is_empty());
        let finalized = with_change.build().unwrap();
        assert!(finalized.change.is_some(), "there should be a change output");
        assert_eq!(finalized.transaction.body.outputs().len(), 2);
        assert_validates(&finalized.transaction);
    }

    /// The change decision is deliberately conservative: a change key is reserved only when the projected change is
    /// worth more than the fee that output would itself add. Either side of that threshold has to work, because the
    /// decision is binding - `build` emits exactly what the reservation decided.
    #[test]
    #[allow(clippy::identity_op)]
    fn the_change_threshold_is_the_cost_of_the_change_output() {
        let key_manager = KeyManager::new_random().unwrap();
        let constants = create_consensus_constants(0);
        let weighting = constants.transaction_weight_params();
        let tx_fee = Fee::new(*weighting).calculate(1.into(), 1, 1, 1, 0);
        let fee_for_change_output = weighting.params().output_weight * uT;

        // The change output pays for itself only if the remainder exceeds its own weight induced fee. Sit one
        // micro-Minotari either side of that line and check the transaction that comes out.
        for (label, extra, expect_change) in [
            ("just below the threshold", MicroMinotari(0), false),
            ("just above the threshold", MicroMinotari(2000), true),
        ] {
            let p = TestParams::new(&key_manager);
            let input = create_test_input(
                2000 * uT + tx_fee + fee_for_change_output - 1 * uT + extra,
                0,
                &key_manager,
                vec![],
                None,
            );
            let output = p
                .create_output(
                    UtxoTestParams {
                        value: 2000 * uT,
                        ..Default::default()
                    },
                    &key_manager,
                )
                .unwrap();
            let mut builder =
                TransactionBuilder::new(constants.clone(), key_manager.clone(), Network::LocalNet).unwrap();
            builder
                .with_fee_per_gram(MicroMinotari(1))
                .with_prevent_fee_gt_amount(false)
                .with_input(input)
                .unwrap();
            add_outputs_with_reserved_sender_offset_keys(&mut builder, vec![output]).unwrap();
            let finalized = builder.build().unwrap();

            assert_eq!(
                finalized.change.is_some(),
                expect_change,
                "{label}: unexpected change decision"
            );
            assert_eq!(
                finalized.transaction.body.outputs().len(),
                if expect_change { 2 } else { 1 },
                "{label}: unexpected output count"
            );
            if !expect_change {
                // Below the threshold the excess is not lost, it goes to the fee.
                assert!(
                    finalized.fee > tx_fee,
                    "{label}: the remainder should have been added to the fee"
                );
            }
            assert_validates(&finalized.transaction);
        }
    }

    /// The offset the builder publishes has to be `sum(input script keys) - sum(output sender offset keys)` for
    /// every shape of transaction, otherwise the outputs are unspendable.
    #[test]
    fn the_script_offset_balances_for_every_shape_of_transaction() {
        let key_manager = KeyManager::new_random().unwrap();
        let constants = create_consensus_constants(0);

        for (label, input_values, recipients, expect_change) in [
            ("one input, one output, no change", vec![MicroMinotari(3000)], 1, false),
            (
                "one input, one output, with change",
                vec![MicroMinotari(100_000)],
                1,
                true,
            ),
            (
                "several inputs and outputs, with change",
                vec![MicroMinotari(50_000), MicroMinotari(60_000), MicroMinotari(70_000)],
                3,
                true,
            ),
        ] {
            let mut builder =
                TransactionBuilder::new(constants.clone(), key_manager.clone(), Network::LocalNet).unwrap();
            builder
                .with_fee_per_gram(MicroMinotari(1))
                .with_prevent_fee_gt_amount(false);
            for value in &input_values {
                builder
                    .with_input(create_test_input(*value, 0, &key_manager, vec![], None))
                    .unwrap();
            }
            let per_recipient = if expect_change {
                MicroMinotari(1000)
            } else {
                // Send everything the fee leaves behind, so the remainder cannot pay for a change output.
                let sizing_spec = RecipientSpec::stealth(
                    random_address(),
                    MicroMinotari::zero(),
                    OutputFeatures::default(),
                    MemoField::new_empty(),
                );
                let size = builder.spec_features_and_scripts_size(&sizing_spec).unwrap();
                let pending = vec![PendingOutput::keyed(MicroMinotari::zero(), size); recipients];
                let fee = builder.get_fee_estimate_with(&pending).unwrap();
                let total: u64 = input_values.iter().map(|v| v.as_u64()).sum();
                MicroMinotari(total.saturating_sub(fee.as_u64()) / recipients as u64)
            };
            for _ in 0..recipients {
                builder
                    .add_stealth_recipient(
                        random_address(),
                        per_recipient,
                        OutputFeatures::default(),
                        MemoField::new_empty(),
                    )
                    .unwrap();
            }
            builder.reserve_sender_offset_keys(&[]).unwrap();
            let finalized = builder.build().unwrap();

            assert_eq!(finalized.change.is_some(), expect_change, "{label}");
            assert_eq!(finalized.transaction.body.inputs().len(), input_values.len(), "{label}");
            assert_eq!(
                finalized.transaction.body.outputs().len(),
                recipients + usize::from(expect_change),
                "{label}"
            );
            assert_validates(&finalized.transaction);
        }
    }

    /// The flows that cannot express their output as a spec - because the published sender offset key is one share
    /// of an aggregate, or because the output arrived fully formed - declare it and take a key from the same single
    /// reservation. The offset still has to balance.
    #[test]
    fn keys_handed_back_to_the_caller_still_balance_the_offset() {
        let key_manager = KeyManager::new_random().unwrap();
        let p1 = TestParams::new(&key_manager);
        let p2 = TestParams::new(&key_manager);
        let input = create_test_input(MicroMinotari(100_000), 0, &key_manager, vec![], None);
        let outputs = vec![
            create_wallet_output_with_data(
                TariScript::default(),
                OutputFeatures::default(),
                &p1,
                MicroMinotari(5000),
                &key_manager,
            )
            .unwrap(),
            create_wallet_output_with_data(
                TariScript::default(),
                OutputFeatures::default(),
                &p2,
                MicroMinotari(4000),
                &key_manager,
            )
            .unwrap(),
        ];

        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
        builder.with_fee_per_gram(MicroMinotari(2)).with_input(input).unwrap();
        add_outputs_with_reserved_sender_offset_keys(&mut builder, outputs).unwrap();
        let finalized = builder.build().unwrap();
        assert_eq!(finalized.transaction.body.outputs().len(), 3, "two outputs plus change");
        assert_validates(&finalized.transaction);
    }

    #[test]
    fn zero_recipient_outputs() {
        let key_manager = KeyManager::new_random().unwrap();
        let p1 = TestParams::new(&key_manager);
        let p2 = TestParams::new(&key_manager);
        let input = create_test_input(MicroMinotari(1200), 0, &key_manager, vec![], None);
        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
        let script = TariScript::default();
        let output_features = OutputFeatures::default();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(2))
            .with_input(input)
            .unwrap();
        add_outputs_with_reserved_sender_offset_keys(&mut builder, vec![
            create_wallet_output_with_data(
                script.clone(),
                output_features.clone(),
                &p1,
                MicroMinotari(500),
                &key_manager,
            )
            .unwrap(),
            create_wallet_output_with_data(script, output_features, &p2, MicroMinotari(400), &key_manager).unwrap(),
        ])
        .unwrap();
        let finalized = builder.build().unwrap();
        assert_validates(&finalized.transaction);
    }

    #[test]
    fn single_recipient_no_change() {
        let key_manager = KeyManager::new_random().unwrap();
        let input = create_test_input(MicroMinotari(1200), 0, &key_manager, vec![], None);
        let utxo = input.to_transaction_input(&key_manager).unwrap();
        let consensus_constants = create_consensus_constants(0);
        let mut builder =
            TransactionBuilder::new(consensus_constants.clone(), key_manager.clone(), Network::LocalNet).unwrap();
        let fee_per_gram = MicroMinotari(4);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_prevent_fee_gt_amount(false)
            .with_input(input)
            .unwrap();
        // Send everything the fee leaves behind, so there is nothing for a change output.
        let spec = RecipientSpec::stealth(
            random_address(),
            MicroMinotari::zero(),
            OutputFeatures::default(),
            MemoField::new_empty(),
        );
        let size = builder.spec_features_and_scripts_size(&spec).unwrap();
        let fee = builder
            .get_fee_estimate_with(&[PendingOutput::keyed(MicroMinotari::zero(), size)])
            .unwrap();
        builder
            .with_recipient_spec(RecipientSpec {
                amount: MicroMinotari(1200) - fee,
                ..spec
            })
            .unwrap();
        builder.reserve_sender_offset_keys(&[]).unwrap();

        let finalized = builder.build().unwrap();

        let tx = finalized.transaction;
        assert_eq!(tx.body.inputs().len(), 1);
        assert_eq!(tx.body.inputs().first().unwrap().commitment(), utxo.commitment());
        assert_eq!(tx.body.outputs().len(), 1);
        assert!(tx.body.outputs().first().unwrap().verify_metadata_signature().is_ok());
        assert_validates(&tx);
    }

    #[test]
    fn single_recipient_with_change() {
        let key_manager = KeyManager::new_random().unwrap();
        let input = create_test_input(MicroMinotari(25000), 0, &key_manager, vec![], None);
        let consensus_constants = create_consensus_constants(0);
        let mut builder =
            TransactionBuilder::new(consensus_constants.clone(), key_manager.clone(), Network::LocalNet).unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .unwrap();
        builder
            .add_stealth_recipient(
                random_address(),
                MicroMinotari(5000),
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .unwrap();
        builder.reserve_sender_offset_keys(&[]).unwrap();
        let finalized = builder.build().unwrap();
        let tx = finalized.transaction;
        assert_eq!(tx.body.inputs().len(), 1);
        assert_eq!(tx.body.outputs().len(), 2);
        assert_validates(&tx);
    }

    #[test]
    fn single_recipient_multiple_inputs_with_change() {
        let key_manager = KeyManager::new_random().unwrap();
        let input = create_test_input(MicroMinotari(10000), 0, &key_manager, vec![], None);
        let input2 = create_test_input(MicroMinotari(2000), 0, &key_manager, vec![], None);
        let input3 = create_test_input(MicroMinotari(15000), 0, &key_manager, vec![], None);
        let consensus_constants = create_consensus_constants(0);
        let mut builder =
            TransactionBuilder::new(consensus_constants.clone(), key_manager.clone(), Network::LocalNet).unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .unwrap()
            .with_input(input2)
            .unwrap()
            .with_input(input3)
            .unwrap();
        builder
            .add_stealth_recipient(
                random_address(),
                MicroMinotari(5000),
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .unwrap();
        builder.reserve_sender_offset_keys(&[]).unwrap();
        let finalized = builder.build().unwrap();

        let tx = finalized.transaction;
        assert_eq!(tx.body.inputs().len(), 3);
        assert_eq!(tx.body.outputs().len(), 2);
        assert_validates(&tx);
    }

    #[test]
    fn add_stealth_recipient() {
        let key_manager = KeyManager::new_random().unwrap();
        let input = create_test_input(MicroMinotari(10000), 0, &key_manager, vec![], None);
        let input2 = create_test_input(MicroMinotari(2000), 0, &key_manager, vec![], None);
        let input3 = create_test_input(MicroMinotari(15000), 0, &key_manager, vec![], None);
        let consensus_constants = create_consensus_constants(0);
        let mut builder =
            TransactionBuilder::new(consensus_constants.clone(), key_manager.clone(), Network::LocalNet).unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .unwrap()
            .with_input(input2)
            .unwrap()
            .with_input(input3)
            .unwrap();
        let bob_address = random_address();

        builder
            .add_stealth_recipient(
                bob_address.clone(),
                MicroMinotari(5000),
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .unwrap();
        builder.reserve_sender_offset_keys(&[]).unwrap();
        let finalized = builder.build().unwrap();

        let bob_output = finalized.spec_outputs.first().unwrap().clone();
        let bob_sender_offset = finalized.sent_outputs.first().unwrap().sender_offset_key_id.clone();
        let shared_secret = key_manager
            .get_diffie_hellman_shared_secret(&bob_sender_offset, bob_address.public_view_key().unwrap())
            .unwrap();
        let commitment_mask_private_key = public_key_to_output_spending_key(&shared_secret).unwrap();
        let commitment_mask_pvt = key_manager
            .get_private_key(bob_output.commitment_mask_key_id())
            .unwrap();
        assert_eq!(commitment_mask_private_key, commitment_mask_pvt);

        let script_spending_key = key_manager
            .stealth_address_script_spending_key(bob_output.commitment_mask_key_id(), bob_address.public_spend_key())
            .unwrap();
        let script = push_pubkey_script(&script_spending_key);

        assert_eq!(*bob_output.script(), script);

        let encryption_private_key = public_key_to_output_encryption_key(&shared_secret).unwrap();
        let bob_tx_output = bob_output.to_transaction_output().unwrap();
        assert!(
            key_manager
                .is_this_output_ours(
                    bob_tx_output.commitment(),
                    bob_output.encrypted_data(),
                    Some(encryption_private_key)
                )
                .unwrap()
        );

        let tx = finalized.transaction;
        assert_eq!(tx.body.inputs().len(), 3);
        assert_eq!(tx.body.outputs().len(), 2);
        assert_validates(&tx);
    }

    #[test]
    fn disallow_fee_larger_than_amount() {
        let key_manager = KeyManager::new_random().unwrap();
        let (utxo_amount, fee_per_gram, amount) = (MicroMinotari(2500), MicroMinotari(10), MicroMinotari(500));
        let input = create_test_input(utxo_amount, 0, &key_manager, vec![], None);
        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .unwrap();
        builder
            .add_stealth_recipient(
                random_address(),
                amount,
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .unwrap();

        builder.reserve_sender_offset_keys(&[]).unwrap();
        let err = builder.build().unwrap_err();
        assert!(
            matches!(err, TransactionBuilderError::FeeGreaterThanAmount { .. }),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn allow_fee_larger_than_amount() {
        let key_manager = KeyManager::new_random().unwrap();
        let (utxo_amount, fee_per_gram, amount) = (MicroMinotari(2500), MicroMinotari(10), MicroMinotari(500));
        let input = create_test_input(utxo_amount, 0, &key_manager, vec![], None);
        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .unwrap()
            .with_prevent_fee_gt_amount(false);
        builder
            .add_stealth_recipient(
                random_address(),
                amount,
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .unwrap();
        builder.reserve_sender_offset_keys(&[]).unwrap();
        match builder.build() {
            Ok(_) => {},
            Err(e) => panic!("Unexpected error: {e:?}"),
        };
    }

    #[test]
    fn create_multi_recipients_transaction() {
        let alice_key_manager = KeyManager::new_random().unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();
        let carol_key_manager = KeyManager::new_random().unwrap();

        let spend_key = bob_key_manager.get_spend_key().pub_key;
        let view_key = bob_key_manager.get_view_key().pub_key;
        let bob_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let spend_key = carol_key_manager.get_spend_key().pub_key;
        let view_key = carol_key_manager.get_view_key().pub_key;
        let carol_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let input = create_test_input(MicroMinotari(5000), 0, &alice_key_manager, vec![], None);
        let consensus_constants = create_consensus_constants(0);
        let mut builder = TransactionBuilder::new(
            consensus_constants.clone(),
            alice_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        let fee_per_gram = MicroMinotari(4);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .unwrap();
        builder
            .add_stealth_recipient(
                bob_address,
                MicroMinotari(1000),
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .unwrap();
        builder
            .add_stealth_recipient(
                carol_address,
                MicroMinotari(1000),
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .unwrap();
        builder.reserve_sender_offset_keys(&[]).unwrap();
        let finalized = builder.build().unwrap();
        let tx = finalized.transaction;
        assert_eq!(tx.body.inputs().len(), 1);
        assert_eq!(tx.body.outputs().len(), 3);
        assert_validates(&tx);
    }

    /// this test will test recovery of a pregenerated transaction alice sent bob, they both need to recover one output
    /// each
    #[test]
    fn recover_multi_recipients_transaction() {
        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_keys = ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        );
        let alice_view_key_manager = create_view_key_manager(alice_keys).unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();
        let bob_keys = ViewWallet::new(
            bob_key_manager.get_spend_key().pub_key,
            bob_key_manager.get_private_view_key(),
            None,
        );
        let bob_view_key_manager = create_view_key_manager(bob_keys).unwrap();
        let carol_key_manager = KeyManager::new_random().unwrap();
        let carol_keys = ViewWallet::new(
            carol_key_manager.get_spend_key().pub_key,
            carol_key_manager.get_private_view_key(),
            None,
        );
        let carol_view_key_manager = create_view_key_manager(carol_keys).unwrap();

        let spend_key = bob_key_manager.get_spend_key().pub_key;
        let view_key = bob_key_manager.get_view_key().pub_key;
        let bob_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let spend_key = carol_key_manager.get_spend_key().pub_key;
        let view_key = carol_key_manager.get_view_key().pub_key;
        let carol_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let input = create_test_input(MicroMinotari(5000), 0, &alice_key_manager, vec![], None);
        let consensus_constants = create_consensus_constants(0);
        let mut builder = TransactionBuilder::new(
            consensus_constants.clone(),
            alice_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        let fee_per_gram = MicroMinotari(4);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .unwrap();
        builder
            .add_stealth_recipient(
                bob_address,
                MicroMinotari(1000),
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .unwrap();
        builder
            .add_stealth_recipient(
                carol_address,
                MicroMinotari(1000),
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .unwrap();
        builder.reserve_sender_offset_keys(&[]).unwrap();
        let finalized = builder.build().unwrap();
        let tx = finalized.transaction;
        let mut alice_count = 0;
        let mut bob_count = 0;
        let mut carol_count = 0;
        let mut wrong = 0;
        for output in tx.body.outputs() {
            // alice change output
            if alice_key_manager
                .is_this_output_ours(&output.commitment, &output.encrypted_data, None)
                .unwrap()
            {
                alice_count += 1;
            }
            // let assume a stealth key for alice
            let alice_shared_secret = alice_key_manager
                .get_diffie_hellman_shared_secret(
                    &alice_key_manager.get_view_key().key_id,
                    &output.sender_offset_public_key,
                )
                .unwrap();
            let alice_encryption_private_key = public_key_to_output_encryption_key(&alice_shared_secret).unwrap();
            if alice_key_manager
                .is_this_output_ours(
                    &output.commitment,
                    &output.encrypted_data,
                    Some(alice_encryption_private_key),
                )
                .unwrap()
            {
                wrong += 1;
            }

            // bob change output
            if bob_key_manager
                .is_this_output_ours(&output.commitment, &output.encrypted_data, None)
                .unwrap()
            {
                wrong += 1;
            }
            // let assume a stealth key for bob
            let bob_shared_secret = bob_key_manager
                .get_diffie_hellman_shared_secret(
                    &bob_key_manager.get_view_key().key_id,
                    &output.sender_offset_public_key,
                )
                .unwrap();
            let bob_encryption_private_key = public_key_to_output_encryption_key(&bob_shared_secret).unwrap();
            if bob_key_manager
                .is_this_output_ours(
                    &output.commitment,
                    &output.encrypted_data,
                    Some(bob_encryption_private_key),
                )
                .unwrap()
            {
                bob_count += 1;
            }

            // carol change output
            if carol_key_manager
                .is_this_output_ours(&output.commitment, &output.encrypted_data, None)
                .unwrap()
            {
                wrong += 1;
            }
            // let assume a stealth key for bob
            let carol_shared_secret = carol_key_manager
                .get_diffie_hellman_shared_secret(
                    &carol_key_manager.get_view_key().key_id,
                    &output.sender_offset_public_key,
                )
                .unwrap();
            let carol_encryption_private_key = public_key_to_output_encryption_key(&carol_shared_secret).unwrap();
            if carol_key_manager
                .is_this_output_ours(
                    &output.commitment,
                    &output.encrypted_data,
                    Some(carol_encryption_private_key),
                )
                .unwrap()
            {
                carol_count += 1;
            }
        }
        assert_eq!(alice_count, 1); // alice change output
        assert_eq!(bob_count, 1); // bob recipient output
        assert_eq!(carol_count, 1); // carol recipient output
        assert_eq!(wrong, 0);

        // lets do view only
        let mut alice_count = 0;
        let mut bob_count = 0;
        let mut carol_count = 0;
        let mut wrong = 0;
        for output in tx.body.outputs() {
            // alice change output
            if alice_view_key_manager
                .is_this_output_ours(&output.commitment, &output.encrypted_data, None)
                .unwrap()
            {
                alice_count += 1;
            }
            // let assume a stealth key for alice
            let alice_shared_secret = alice_view_key_manager
                .get_diffie_hellman_shared_secret(
                    &alice_view_key_manager.get_view_key().key_id,
                    &output.sender_offset_public_key,
                )
                .unwrap();
            let alice_encryption_private_key = public_key_to_output_encryption_key(&alice_shared_secret).unwrap();
            if alice_view_key_manager
                .is_this_output_ours(
                    &output.commitment,
                    &output.encrypted_data,
                    Some(alice_encryption_private_key),
                )
                .unwrap()
            {
                wrong += 1;
            }

            // bob change output
            if bob_view_key_manager
                .is_this_output_ours(&output.commitment, &output.encrypted_data, None)
                .unwrap()
            {
                wrong += 1;
            }
            // let assume a stealth key for bob
            let bob_shared_secret = bob_view_key_manager
                .get_diffie_hellman_shared_secret(
                    &bob_view_key_manager.get_view_key().key_id,
                    &output.sender_offset_public_key,
                )
                .unwrap();
            let bob_encryption_private_key = public_key_to_output_encryption_key(&bob_shared_secret).unwrap();
            if bob_view_key_manager
                .is_this_output_ours(
                    &output.commitment,
                    &output.encrypted_data,
                    Some(bob_encryption_private_key),
                )
                .unwrap()
            {
                bob_count += 1;
            }

            // carol change output
            if carol_view_key_manager
                .is_this_output_ours(&output.commitment, &output.encrypted_data, None)
                .unwrap()
            {
                wrong += 1;
            }
            // let assume a stealth key for bob
            let carol_shared_secret = carol_view_key_manager
                .get_diffie_hellman_shared_secret(
                    &carol_view_key_manager.get_view_key().key_id,
                    &output.sender_offset_public_key,
                )
                .unwrap();
            let carol_encryption_private_key = public_key_to_output_encryption_key(&carol_shared_secret).unwrap();
            if carol_view_key_manager
                .is_this_output_ours(
                    &output.commitment,
                    &output.encrypted_data,
                    Some(carol_encryption_private_key),
                )
                .unwrap()
            {
                carol_count += 1;
            }
        }
        assert_eq!(alice_count, 1); // alice change output
        assert_eq!(bob_count, 1); // bob recipient output
        assert_eq!(carol_count, 1); // carol recipient output
        assert_eq!(wrong, 0);
    }

    #[test]
    fn create_very_large_multi_recipients_transaction() {
        let alice_key_manager = KeyManager::new_random().unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();

        let spend_key = bob_key_manager.get_spend_key().pub_key;
        let view_key = bob_key_manager.get_view_key().pub_key;
        let bob_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let input = create_test_input(MicroMinotari(500000), 0, &alice_key_manager, vec![], None);
        let consensus_constants = create_consensus_constants(0);
        let mut builder = TransactionBuilder::new(
            consensus_constants.clone(),
            alice_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        let fee_per_gram = MicroMinotari(4);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .unwrap();
        for _ in 0..100 {
            builder
                .add_stealth_recipient(
                    bob_address.clone(),
                    MicroMinotari(1000),
                    OutputFeatures::default(),
                    MemoField::new_empty(),
                )
                .unwrap();
        }
        builder.reserve_sender_offset_keys(&[]).unwrap();

        let finalized = builder.build().unwrap();
        let tx = finalized.transaction;
        assert_eq!(tx.body.inputs().len(), 1);
        assert_eq!(tx.body.outputs().len(), 101);
        assert_validates(&tx);
    }

    /// this test will test recovery of a pregenerated transaction alice sent bob, they both need to recover one output
    /// each
    #[test]
    fn recover_historic_transaction_data() {
        let alice_wallet_seeds = "leopard tilt extend file rescue purity day blind office laptop task today stairs \
                                  now stairs conduct fruit pigeon make urban grace gasp suit drill"
            .to_string();
        let alice_seeds = SeedWords::new(
            alice_wallet_seeds
                .split(' ')
                .map(|s| Hidden::hide(s.to_string()))
                .collect::<Vec<Hidden<String>>>(),
        );

        let alice_cipher = CipherSeed::from_mnemonic(&alice_seeds, None).unwrap();
        let alice_wallet = WalletType::SeedWords(SeedWordsWallet::construct_new(alice_cipher).unwrap());
        let alice_key_manager = KeyManager::new(alice_wallet).unwrap();
        let bob_wallet_seeds = "park bright young fitness twin globe fresh dose gesture inmate already word minimum \
                                waste shop chair chef quick stairs subway paper brave case vessel"
            .to_string();
        let seeds = SeedWords::new(
            bob_wallet_seeds
                .split(' ')
                .map(|s| Hidden::hide(s.to_string()))
                .collect::<Vec<Hidden<String>>>(),
        );

        let cipher = CipherSeed::from_mnemonic(&seeds, None).unwrap();

        let bob_wallet = WalletType::SeedWords(SeedWordsWallet::construct_new(cipher).unwrap());
        let bob_key_manager = KeyManager::new(bob_wallet).unwrap();

        struct RecoverData {
            pub commitment: CompressedCommitment,
            pub encrypted_data: EncryptedData,
            pub sender_offset_public_key: CompressedPublicKey,
            pub script: TariScript,
        }
        let outputs = [
            RecoverData{
                commitment: CompressedCommitment::from_hex("006399307893ae875ac7677b564ba068a9bc18eb903f5245a39a78aeebecc87b").unwrap(),
                encrypted_data: EncryptedData::from_hex("0b52d2d3fc3ee4b1d660effbb925e14adbecbb411ef6a15263977c98a60e28cf0391f7fa462d25550b9dde46f1a918135de15bc2416e07bcc2979daf2913c489c1d7392c893abcda8702b855980beed01b90161b8c6be81e631a5b137c04822edbefc7e2ad176ea6c0a55f12d70878b82575adc3d0cee368fe752d76e2916ec142eecf22bd4d73f4c5c8898ffc62860f06ed89346dec6c8d7e0b464c25e517b7c39d576c4d49eacfd063f09fa576a2e9ba4e73887f7f31a4a1bdcf5f872ffdfade6bddd844bbcedea9ac818112ab7a2266d2").unwrap(),
                sender_offset_public_key: CompressedPublicKey::from_hex("cefb54097d450a875959dd50d6c6e2e17d0f2d285ac58005bcda7ff1e805c048").unwrap(),
                script: TariScript::from_hex("7e7cf0a1cefaca6355741d47564a8162b57a735f8c4fd1d3afde0bc9ac44df6f09").unwrap(),
            },
            RecoverData{
                commitment: CompressedCommitment::from_hex("e6b0f7ca79ae1d12fb44ca678f96296b25fefd6da25cde01d4f566f963c9d96e").unwrap(),
                encrypted_data: EncryptedData::from_hex("5355a9fa27de31511619e22810d41a73b9d0eed8dcfaa1cdd129b7dc4afa9905081e3c631f43b1946d5aef12413fe5f2e64c739d926d8d72883daa2577771776a45e5e6b1da727e0130f6b15c1fcf3ec").unwrap(),
                sender_offset_public_key: CompressedPublicKey::from_hex("b6de6ce6ca11e5fcbfddadc6277a97ce480ab0ca9ba09714e3cca1a7e8ebe34d").unwrap(),
                script: TariScript::from_hex("7e722481cc06bb13b6732edd6d19bdea213549f0b51df201a2dc8f04f9ddf7a039").unwrap(),
            },
        ];

        let mut alice_count = 0;
        let mut bob_count = 0;
        for output in outputs {
            // alice change output
            if let Ok(Some((commitment_key, _, _))) = alice_key_manager.try_output_key_recovery(
                &output.commitment,
                &output.encrypted_data,
                &output.sender_offset_public_key,
            ) {
                alice_count += 1;
                let script_spending_key = alice_key_manager
                    .stealth_address_script_spending_key(&commitment_key, &alice_key_manager.get_spend_key().pub_key)
                    .unwrap();
                if let [Opcode::PushPubKey(scanned_pk)] = output.script.as_slice() {
                    if script_spending_key != **scanned_pk {
                        panic!("should have found this");
                    }
                } else {
                    panic!("unexpected script");
                }
            }
            if let Ok(Some((commitment_key, _, _))) = bob_key_manager.try_output_key_recovery(
                &output.commitment,
                &output.encrypted_data,
                &output.sender_offset_public_key,
            ) {
                bob_count += 1;
                let script_spending_key = bob_key_manager
                    .stealth_address_script_spending_key(&commitment_key, &bob_key_manager.get_spend_key().pub_key)
                    .unwrap();
                if let [Opcode::PushPubKey(scanned_pk)] = output.script.as_slice() {
                    if script_spending_key != **scanned_pk {
                        panic!("should have found this");
                    }
                } else {
                    panic!("unexpected script");
                }
            }
        }
        assert_eq!(alice_count, 1); // alice change output
        assert_eq!(bob_count, 1); // bob recipient output
    }

    #[test]
    fn transaction_details_correct() {
        let alice_key_manager = KeyManager::new_random().unwrap();
        let spend_key = alice_key_manager.get_spend_key().pub_key;
        let view_key = alice_key_manager.get_view_key().pub_key;
        let alice_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();

        let spend_key = bob_key_manager.get_spend_key().pub_key;
        let view_key = bob_key_manager.get_view_key().pub_key;
        let bob_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let input = create_test_input(MicroMinotari(5000), 0, &alice_key_manager, vec![], None);
        let consensus_constants = create_consensus_constants(0);
        let mut builder = TransactionBuilder::new(
            consensus_constants.clone(),
            alice_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        let fee_per_gram = MicroMinotari(4);
        let payment_id =
            MemoField::new_address_and_data(alice_address, 1.into(), true, TxType::PaymentToOther, vec![]).unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .unwrap();
        builder
            .add_stealth_recipient(
                bob_address,
                MicroMinotari(1000),
                OutputFeatures::default(),
                payment_id.clone(),
            )
            .unwrap();
        builder.with_memo(payment_id);
        builder.reserve_sender_offset_keys(&[]).unwrap();
        let finalized = builder.build().unwrap();
        let tx = finalized.transaction.clone();
        let mut alice_memo = None;
        let mut bob_memo = None;
        for output in tx.body.outputs() {
            // alice output
            if let Some(output) = alice_key_manager
                .try_output_key_recovery(
                    &output.commitment,
                    &output.encrypted_data,
                    &output.sender_offset_public_key,
                )
                .unwrap()
            {
                alice_memo = Some(output.2);
            }

            // bob output
            if let Some(output) = bob_key_manager
                .try_output_key_recovery(
                    &output.commitment,
                    &output.encrypted_data,
                    &output.sender_offset_public_key,
                )
                .unwrap()
            {
                bob_memo = Some(output.2);
            }
        }
        let alice_memo = alice_memo.unwrap();
        let bob_memo = bob_memo.unwrap();
        assert_eq!(alice_memo.get_tx_type(), Some(TxType::PaymentToOther));
        assert_eq!(bob_memo.get_tx_type(), Some(TxType::PaymentToOther));
        assert_eq!(alice_memo.get_fee().unwrap(), tx.body.get_total_fee().unwrap());
        assert_eq!(bob_memo.get_fee().unwrap(), tx.body.get_total_fee().unwrap());
        assert_eq!(tx.body.get_total_fee().unwrap(), finalized.fee);
        assert_eq!(
            tx.body.get_total_fee().unwrap(),
            finalized.payment_id.get_fee().unwrap()
        );
    }
}
