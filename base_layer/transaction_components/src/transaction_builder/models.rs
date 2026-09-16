// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use serde::{Deserialize, Serialize};
use tari_common_types::{
    tari_address::TariAddress,
    transaction::TxId,
    types::{CompressedPublicKey, FixedHash},
};
use tari_script::TariScript;

use crate::{
    MicroMinotari,
    key_manager::TariKeyId,
    transaction_components::{MemoField, OutputFeatures, Transaction, WalletOutput, covenants::Covenant},
};

#[derive(Clone, Debug, PartialEq)]
pub struct RecipientDetails {
    pub output: OutputPair,
    pub recipient_address: TariAddress,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputPair {
    pub output: WalletOutput,
    pub kernel_nonce: TariKeyId,
    /// The sender offset key this output publishes. It is always known: it comes either from
    /// [`crate::TransactionBuilder::reserve_sender_offset_keys`] or from a caller that registered the matching
    /// partial script offset with the builder.
    pub(crate) sender_offset_key_id: TariKeyId,
    pub custom_recovery_key_id: Option<TariKeyId>,
}

impl OutputPair {
    pub fn new(
        output: WalletOutput,
        kernel_nonce: TariKeyId,
        sender_offset_key_id: TariKeyId,
        custom_recovery_key_id: Option<TariKeyId>,
    ) -> Self {
        Self {
            output,
            kernel_nonce,
            sender_offset_key_id,
            custom_recovery_key_id,
        }
    }
}

/// Where a spec built output's commitment mask and recovery key come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecipientKeys {
    /// Diffie-Hellman between the reserved sender offset key and the destination's public view key, recorded as
    /// `DHCommitmentMask` and `DHEncryptedData` key ids so that no key material is stored.
    DiffieHellman,
    /// The same Diffie-Hellman secrets, resolved once and stored as `Encrypted` key ids.
    DiffieHellmanEncrypted,
    /// A fresh commitment mask and script key the wallet generates for itself. Recovery uses its own view key.
    Own,
}

/// The script a spec built output publishes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecipientScript {
    /// The natural script for the key scheme: the stealth `PushPubKey` for the destination under either
    /// Diffie-Hellman scheme, and `PushPubKey(own script key)` for [`RecipientKeys::Own`].
    Default,
    /// A script the caller supplies verbatim.
    Explicit(Box<TariScript>),
    /// `CheckMultiSigVerify` over ephemeral keys derived from the reserved sender offset key, in front of the
    /// stealth `PushPubKey`. The ephemeral keys cannot be derived before the key is reserved, which is why this is a
    /// variant rather than an `Explicit` script the caller builds.
    Multisig {
        party_number: u8,
        public_keys: Vec<CompressedPublicKey>,
        message: Box<[u8; 32]>,
    },
}

/// The script key a spec built output records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecipientScriptKey {
    /// The recipient owns the script key, so the sender records nothing.
    Zero,
    /// The wallet's own spend key, for a script only this wallet can execute.
    OwnSpendKey,
    /// The script key generated alongside [`RecipientKeys::Own`]'s commitment mask.
    Own,
}

/// Which metadata signature a spec built output carries.
///
/// There is no placeholder variant: spec built outputs are constructed inside `build`, after the fee is final, so
/// they are signed exactly once and a ledger device prompts exactly once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecipientMetadataSignature {
    /// Signed with the destination address shown to the user on a ledger device.
    UserVerified,
    /// Signed without user verification, for outputs the wallet sends to itself or burns.
    Unverified,
}

/// A recipient output that [`crate::TransactionBuilder::build`] constructs.
///
/// The output cannot be built when the caller declares it, because the sender offset key it publishes is only
/// reserved once the builder knows how many outputs the transaction has: one `get_script_offset` call for the whole
/// transaction is what keeps both sides of that sum blinded by a term the host cannot compute. A spec therefore
/// records what the flow varies and nothing else.
#[derive(Clone, Debug)]
pub struct RecipientSpec {
    pub destination: TariAddress,
    pub amount: MicroMinotari,
    pub features: OutputFeatures,
    /// The memo the output stores. Its fee is overwritten with the transaction's final fee at build time.
    pub memo: MemoField,
    pub covenant: Covenant,
    pub minimum_value_promise: MicroMinotari,
    pub keys: RecipientKeys,
    pub script: RecipientScript,
    pub script_key: RecipientScriptKey,
    pub metadata_signature: RecipientMetadataSignature,
}

impl RecipientSpec {
    /// An ordinary one sided payment: stealth script, Diffie-Hellman keys, the recipient owns the script key, and
    /// the destination address is shown to the user on a ledger device.
    pub fn stealth(destination: TariAddress, amount: MicroMinotari, features: OutputFeatures, memo: MemoField) -> Self {
        Self {
            destination,
            amount,
            features,
            memo,
            covenant: Covenant::default(),
            minimum_value_promise: MicroMinotari::zero(),
            keys: RecipientKeys::DiffieHellman,
            script: RecipientScript::Default,
            script_key: RecipientScriptKey::Zero,
            metadata_signature: RecipientMetadataSignature::UserVerified,
        }
    }

    /// An output the wallet sends to itself: its own commitment mask and script key, and no user verification
    /// because there is no counterparty address to verify.
    pub fn to_self(amount: MicroMinotari, features: OutputFeatures, memo: MemoField) -> Self {
        Self {
            destination: TariAddress::default(),
            amount,
            features,
            memo,
            covenant: Covenant::default(),
            minimum_value_promise: MicroMinotari::zero(),
            keys: RecipientKeys::Own,
            script: RecipientScript::Default,
            script_key: RecipientScriptKey::Own,
            metadata_signature: RecipientMetadataSignature::Unverified,
        }
    }

    pub fn with_covenant(mut self, covenant: Covenant) -> Self {
        self.covenant = covenant;
        self
    }

    pub fn with_minimum_value_promise(mut self, minimum_value_promise: MicroMinotari) -> Self {
        self.minimum_value_promise = minimum_value_promise;
        self
    }

    pub fn with_keys(mut self, keys: RecipientKeys) -> Self {
        self.keys = keys;
        self
    }

    pub fn with_script(mut self, script: TariScript) -> Self {
        self.script = RecipientScript::Explicit(Box::new(script));
        self
    }

    pub fn with_multisig_script(
        mut self,
        party_number: u8,
        public_keys: Vec<CompressedPublicKey>,
        message: Box<[u8; 32]>,
    ) -> Self {
        self.script = RecipientScript::Multisig {
            party_number,
            public_keys,
            message,
        };
        self
    }

    pub fn with_script_key(mut self, script_key: RecipientScriptKey) -> Self {
        self.script_key = script_key;
        self
    }

    pub fn with_metadata_signature(mut self, metadata_signature: RecipientMetadataSignature) -> Self {
        self.metadata_signature = metadata_signature;
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FinalizedTransaction {
    pub tx_id: TxId,
    pub source_address: TariAddress,
    pub destination_addresses: Vec<TariAddress>,
    pub amount: MicroMinotari,
    pub fee: MicroMinotari,
    pub transaction: Transaction,
    pub payment_id: MemoField,
    pub change: Option<WalletOutput>,
    pub sent_outputs: Vec<OutputPair>,
    /// The outputs the builder constructed from the specs supplied to
    /// [`crate::TransactionBuilder::with_recipient_spec`], in the order the specs were added.
    ///
    /// A spec built output does not exist until `build` runs, so this is the only place a caller can get at it -
    /// to record it in a wallet database, or to hand the finished `TransactionOutput` to a counterparty.
    pub spec_outputs: Vec<WalletOutput>,
    /// The outputs supplied via [`crate::TransactionBuilder::with_output`], as they appear in `transaction`.
    ///
    /// Building can rewrite these — the encrypted data carries the final fee, and the metadata signature is remade
    /// when it does — so a caller that stores its own pre-build copy will record a hash that does not match the UTXO
    /// on chain. Store these instead.
    pub custom_outputs: Vec<WalletOutput>,
    /// Hashes of outputs being sent to others (excluding change)
    pub sent_output_hashes: Vec<FixedHash>,
    /// Hashes of outputs received from others (excluding change)
    pub received_output_hashes: Vec<FixedHash>,
    /// Hashes of change outputs (for reference)
    pub change_output_hashes: Vec<FixedHash>,
}
