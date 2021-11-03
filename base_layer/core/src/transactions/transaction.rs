//  Copyright 2021, The Tari Project
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

use crate::{
    consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized, ConsensusEncodingWrapper},
    transactions::{
        aggregated_body::AggregateBody,
        crypto_factories::CryptoFactories,
        tari_amount::{uT, MicroTari},
        transaction_protocol::{build_challenge, RewindData, TransactionMetadata},
        weight::TransactionWeight,
    },
};
use blake2::Digest;
use integer_encoding::{VarInt, VarIntReader, VarIntWriter};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::{
    cmp::{max, min, Ordering},
    fmt,
    fmt::{Display, Formatter},
    hash::{Hash, Hasher},
    io,
    io::{Read, Write},
    ops::{Add, Shl},
};
use tari_common_types::types::{
    BlindingFactor,
    Challenge,
    ComSignature,
    Commitment,
    CommitmentFactory,
    HashDigest,
    MessageHash,
    PrivateKey,
    PublicKey,
    RangeProof,
    RangeProofService,
    Signature,
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey as PublicKeyTrait, SecretKey},
    range_proof::{
        FullRewindResult as CryptoFullRewindResult,
        RangeProofError,
        RangeProofService as RangeProofServiceTrait,
        RewindResult as CryptoRewindResult,
        REWIND_USER_MESSAGE_LENGTH,
    },
    ristretto::pedersen::PedersenCommitmentFactory,
    script::{ExecutionStack, ScriptError, StackItem, TariScript},
    signatures::CommitmentSignatureError,
    tari_utilities::{hex::Hex, message_format::MessageFormat, ByteArray, Hashable},
};
use thiserror::Error;

// Tx_weight(inputs(12,500), outputs(500), kernels(1)) = 126,510 still well enough below block weight of 127,795
pub const MAX_TRANSACTION_INPUTS: usize = 12_500;
pub const MAX_TRANSACTION_OUTPUTS: usize = 500;
pub const MAX_TRANSACTION_RECIPIENTS: usize = 15;

//--------------------------------------        Output features   --------------------------------------------------//

bitflags! {
    /// Options for a kernel's structure or use.
    /// TODO:  expand to accommodate Tari DAN transaction types, such as namespace and validator node registrations
    #[derive(Deserialize, Serialize)]
    pub struct KernelFeatures: u8 {
        /// Coinbase transaction
        const COINBASE_KERNEL = 1u8;
    }
}

impl KernelFeatures {
    pub fn create_coinbase() -> KernelFeatures {
        KernelFeatures::COINBASE_KERNEL
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct AssetOutputFeatures {
    pub public_key: PublicKey,
    pub template_ids_implemented: Vec<u32>,
}

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct MintNonFungibleFeatures {
    pub asset_public_key: PublicKey,
    pub asset_owner_commitment: Commitment,
    // pub proof_of_ownership: ComSignature
}

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct SideChainCheckpointFeatures {
    pub merkle_root: Vec<u8>,
    pub committee: Vec<PublicKey>,
}

/// Options for UTXO's
#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub struct OutputFeatures {
    /// Flags are the feature flags that differentiate between outputs, eg Coinbase all of which has different rules
    pub flags: OutputFlags,
    /// the maturity of the specific UTXO. This is the min lock height at which an UTXO can be spent. Coinbase UTXO
    /// require a min maturity of the Coinbase_lock_height, this should be checked on receiving new blocks.
    pub maturity: u64,
    pub metadata: Vec<u8>,
    pub unique_id: Option<Vec<u8>>,
    pub parent_public_key: Option<PublicKey>,
    pub asset: Option<AssetOutputFeatures>,
    pub mint_non_fungible: Option<MintNonFungibleFeatures>,
    pub sidechain_checkpoint: Option<SideChainCheckpointFeatures>,
}

impl OutputFeatures {
    /// The version number to use in consensus encoding. In future, this value could be dynamic.
    const CONSENSUS_ENCODING_VERSION: u8 = 0;

    /// Encodes output features using deprecated bincode encoding
    pub fn to_v1_bytes(&self) -> Vec<u8> {
        // unreachable panic: serialized_size is infallible because it uses DefaultOptions
        let encode_size = bincode::serialized_size(self).expect("unreachable");
        let mut buf = Vec::with_capacity(encode_size as usize);
        // unreachable panic: Vec's Write impl is infallible
        bincode::serialize_into(&mut buf, self).expect("unreachable");
        buf
    }

    /// Encodes output features using consensus encoding
    pub fn to_consensus_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.consensus_encode_exact_size());
        // unreachable panic: Vec's Write impl is infallible
        self.consensus_encode(&mut buf).expect("unreachable");
        buf
    }

    pub fn create_coinbase(maturity_height: u64) -> OutputFeatures {
        OutputFeatures {
            flags: OutputFlags::COINBASE_OUTPUT,
            maturity: maturity_height,
            ..Default::default()
        }
    }

    /// Create an `OutputFeatures` with the given maturity and all other values at their default setting
    pub fn with_maturity(maturity: u64) -> OutputFeatures {
        OutputFeatures {
            maturity,
            ..Default::default()
        }
    }

    pub fn custom(flags: OutputFlags, metadata: Vec<u8>) -> OutputFeatures {
        Self {
            flags,
            metadata,
            ..Default::default()
        }
    }

    pub fn for_asset_registration(
        metadata: Vec<u8>,
        public_key: PublicKey,
        template_ids_implemented: Vec<u32>,
    ) -> OutputFeatures {
        let unique_id = Some(public_key.as_bytes().to_vec());
        Self {
            flags: OutputFlags::ASSET_REGISTRATION,
            maturity: 0,
            metadata,
            asset: Some(AssetOutputFeatures {
                public_key,
                template_ids_implemented,
            }),
            unique_id,
            ..Default::default()
        }
    }

    pub fn for_minting(
        asset_public_key: PublicKey,
        asset_owner_commitment: Commitment,
        unique_id: Vec<u8>,
        other_features: Option<OutputFeatures>,
    ) -> OutputFeatures {
        Self {
            flags: OutputFlags::MINT_NON_FUNGIBLE |
                other_features
                    .as_ref()
                    .map(|of| of.flags)
                    .unwrap_or_else(OutputFlags::empty),
            mint_non_fungible: Some(MintNonFungibleFeatures {
                asset_public_key: asset_public_key.clone(),
                asset_owner_commitment,
            }),
            parent_public_key: Some(asset_public_key),
            unique_id: Some(unique_id),
            ..other_features.unwrap_or_default()
        }
    }

    pub fn for_checkpoint(
        parent_public_key: PublicKey,
        merkle_root: Vec<u8>,
        committee: Vec<PublicKey>,
    ) -> OutputFeatures {
        const CHECKPOINT_UNIQUE_ID: [u8; 32] = [3u8; 32];
        Self {
            flags: OutputFlags::SIDECHAIN_CHECKPOINT,
            sidechain_checkpoint: Some(SideChainCheckpointFeatures { merkle_root, committee }),
            parent_public_key: Some(parent_public_key),
            unique_id: Some(CHECKPOINT_UNIQUE_ID.to_vec()),
            ..Default::default()
        }
    }

    pub fn unique_asset_id(&self) -> Option<&[u8]> {
        self.unique_id.as_deref()
    }

    pub fn is_non_fungible_mint(&self) -> bool {
        self.flags.contains(OutputFlags::MINT_NON_FUNGIBLE)
    }

    pub fn is_non_fungible_burn(&self) -> bool {
        self.flags.contains(OutputFlags::BURN_NON_FUNGIBLE)
    }
}

impl ConsensusEncoding for OutputFeatures {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut written = writer.write_varint(Self::CONSENSUS_ENCODING_VERSION)?;
        written += writer.write_varint(self.maturity)?;
        written += self.flags.consensus_encode(writer)?;
        Ok(written)
    }
}
impl ConsensusEncodingSized for OutputFeatures {
    fn consensus_encode_exact_size(&self) -> usize {
        Self::CONSENSUS_ENCODING_VERSION.required_space() +
            self.flags.consensus_encode_exact_size() +
            self.maturity.required_space()
    }
}

impl ConsensusDecoding for OutputFeatures {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        // Changing the order of these operations is consensus breaking
        let version = reader.read_varint::<u8>()?;
        if version != Self::CONSENSUS_ENCODING_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Invalid version. Expected {} but got {}",
                    Self::CONSENSUS_ENCODING_VERSION,
                    version
                ),
            ));
        }
        // Decode safety: read_varint will stop reading the varint after 10 bytes
        let maturity = reader.read_varint()?;
        let flags = OutputFlags::consensus_decode(reader)?;
        Ok(Self {
            flags,
            maturity,
            ..Default::default()
        })
    }
}

impl Default for OutputFeatures {
    fn default() -> Self {
        OutputFeatures {
            flags: OutputFlags::empty(),
            maturity: 0,
            metadata: vec![],
            unique_id: None,
            parent_public_key: None,
            asset: None,
            mint_non_fungible: None,
            sidechain_checkpoint: None,
        }
    }
}

impl PartialOrd for OutputFeatures {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OutputFeatures {
    fn cmp(&self, other: &Self) -> Ordering {
        self.maturity.cmp(&other.maturity)
    }
}

impl Display for OutputFeatures {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OutputFeatures: Flags = {:?}, Maturity = {}",
            self.flags, self.maturity
        )
    }
}

bitflags! {
    #[derive(Deserialize, Serialize)]
    pub struct OutputFlags: u8 {
        /// Output is a coinbase output, must not be spent until maturity
        const COINBASE_OUTPUT = 0b0000_0001;
        const NON_FUNGIBLE = 0b0000_1000;
        // TODO: separate these flags
        const ASSET_REGISTRATION = 0b0000_0010 | Self::NON_FUNGIBLE.bits;
        const MINT_NON_FUNGIBLE = 0b0000_0100 | Self::NON_FUNGIBLE.bits;
        const BURN_NON_FUNGIBLE = 0b1000_0000 | Self::NON_FUNGIBLE.bits;
        const SIDECHAIN_CHECKPOINT = 0b0001_0000 | Self::NON_FUNGIBLE.bits;
    }
}

impl ConsensusEncoding for OutputFlags {
    fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        writer.write(&self.bits.to_le_bytes())
    }
}

impl ConsensusEncodingSized for OutputFlags {
    fn consensus_encode_exact_size(&self) -> usize {
        1
    }
}

impl ConsensusDecoding for OutputFlags {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        // SAFETY: we have 3 options here:
        // 1. error if unsupported flags are used, meaning that every new flag will be a hard fork
        // 2. truncate unsupported flags, means different hashes will be produced for the same block
        // 3. ignore unsupported flags, which could be set at any time and persisted to the blockchain.
        //   Once those flags are defined at some point in the future, depending on the functionality of the flag,
        //   a consensus rule may be needed that ignores flags prior to a given block height.
        // Option 3 is used here
        Ok(unsafe { OutputFlags::from_bits_unchecked(u8::from_le_bytes(buf)) })
    }
}

//----------------------------------------     TransactionError   ----------------------------------------------------//

#[derive(Clone, Debug, PartialEq, Error, Deserialize, Serialize)]
pub enum TransactionError {
    #[error("Error validating the transaction: {0}")]
    ValidationError(String),
    #[error("Signature is invalid: {0}")]
    InvalidSignatureError(String),
    #[error("Transaction kernel does not contain a signature")]
    NoSignatureError,
    #[error("A range proof construction or verification has produced an error: {0}")]
    RangeProofError(#[from] RangeProofError),
    #[error("An error occurred while performing a commitment signature: {0}")]
    SigningError(#[from] CommitmentSignatureError),
    #[error("Invalid kernel in body")]
    InvalidKernel,
    #[error("Invalid coinbase in body")]
    InvalidCoinbase,
    #[error("Invalid coinbase maturity in body")]
    InvalidCoinbaseMaturity,
    #[error("More than one coinbase in body")]
    MoreThanOneCoinbase,
    #[error("No coinbase in body")]
    NoCoinbase,
    #[error("Input maturity not reached")]
    InputMaturity,
    #[error("Tari script error : {0}")]
    ScriptError(#[from] ScriptError),
    #[error("Error performing conversion: {0}")]
    ConversionError(String),
    #[error("The script offset in body does not balance")]
    ScriptOffset,
    #[error("Error executing script: {0}")]
    ScriptExecutionError(String),
}

//-----------------------------------------     UnblindedOutput   ----------------------------------------------------//
#[derive(Debug, Clone)]
pub struct UnblindedOutputBuilder {
    pub value: MicroTari,
    spending_key: BlindingFactor,
    pub features: OutputFeatures,
    pub script: Option<TariScript>,
    input_data: Option<ExecutionStack>,
    script_private_key: Option<PrivateKey>,
    sender_offset_public_key: Option<PublicKey>,
    metadata_signature: Option<ComSignature>,
    metadata_signed_by_receiver: bool,
    metadata_signed_by_sender: bool,
}

impl UnblindedOutputBuilder {
    pub fn new(value: MicroTari, spending_key: BlindingFactor) -> Self {
        Self {
            value,
            spending_key,
            features: OutputFeatures::default(),
            script: None,
            input_data: None,
            script_private_key: None,
            sender_offset_public_key: None,
            metadata_signature: None,
            metadata_signed_by_receiver: false,
            metadata_signed_by_sender: false,
        }
    }

    pub fn sign_as_receiver(
        &mut self,
        sender_offset_public_key: PublicKey,
        public_nonce_commitment: PublicKey,
    ) -> Result<(), TransactionError> {
        self.sender_offset_public_key = Some(sender_offset_public_key.clone());

        let metadata_partial = TransactionOutput::create_partial_metadata_signature(
            &self.value,
            &self.spending_key,
            self.script
                .as_ref()
                .ok_or_else(|| TransactionError::ValidationError("script must be set".to_string()))?,
            &self.features,
            &sender_offset_public_key,
            &public_nonce_commitment,
        )?;
        self.metadata_signature = Some(metadata_partial);
        self.metadata_signed_by_receiver = true;
        Ok(())
    }

    pub fn sign_as_sender(&mut self, sender_offset_private_key: &PrivateKey) -> Result<(), TransactionError> {
        let metadata_sig = TransactionOutput::create_final_metadata_signature(
            &self.value,
            &self.spending_key,
            self.script
                .as_ref()
                .ok_or_else(|| TransactionError::ValidationError("script must be set".to_string()))?,
            &self.features,
            sender_offset_private_key,
        )?;
        self.metadata_signature = Some(metadata_sig);
        self.metadata_signed_by_sender = true;
        Ok(())
    }

    pub fn try_build(self) -> Result<UnblindedOutput, TransactionError> {
        if !self.metadata_signed_by_receiver {
            return Err(TransactionError::ValidationError(
                "Cannot build output because it has not been signed by the receiver".to_string(),
            ));
        }
        if !self.metadata_signed_by_sender {
            return Err(TransactionError::ValidationError(
                "Cannot build output because it has not been signed by the sender".to_string(),
            ));
        }
        let ub = UnblindedOutput {
            value: self.value,
            spending_key: self.spending_key,
            features: self.features,
            script: self
                .script
                .ok_or_else(|| TransactionError::ValidationError("script must be set".to_string()))?,
            input_data: self
                .input_data
                .ok_or_else(|| TransactionError::ValidationError("input_data must be set".to_string()))?,
            script_private_key: self
                .script_private_key
                .ok_or_else(|| TransactionError::ValidationError("script_private_key must be set".to_string()))?,
            sender_offset_public_key: self
                .sender_offset_public_key
                .ok_or_else(|| TransactionError::ValidationError("sender_offset_public_key must be set".to_string()))?,
            metadata_signature: self
                .metadata_signature
                .ok_or_else(|| TransactionError::ValidationError("metadata_signature must be set".to_string()))?,
        };
        Ok(ub)
    }

    pub fn with_features(mut self, features: OutputFeatures) -> Self {
        self.features = features;
        self
    }

    pub fn with_script(mut self, script: TariScript) -> Self {
        self.script = Some(script);
        self
    }

    pub fn with_input_data(mut self, input_data: ExecutionStack) -> Self {
        self.input_data = Some(input_data);
        self
    }

    pub fn with_script_private_key(mut self, script_private_key: PrivateKey) -> Self {
        self.script_private_key = Some(script_private_key);
        self
    }
}

/// An unblinded output is one where the value and spending key (blinding factor) are known. This can be used to
/// build both inputs and outputs (every input comes from an output)
// TODO: Try to get rid of 'Serialize' and 'Deserialize' traits here; see related comment at 'struct RawTransactionInfo'
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnblindedOutput {
    pub value: MicroTari,
    pub spending_key: BlindingFactor,
    pub features: OutputFeatures,
    pub script: TariScript,
    pub input_data: ExecutionStack,
    pub script_private_key: PrivateKey,
    pub sender_offset_public_key: PublicKey,
    pub metadata_signature: ComSignature,
}

impl UnblindedOutput {
    /// Creates a new un-blinded output
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        value: MicroTari,
        spending_key: BlindingFactor,
        features: OutputFeatures,
        script: TariScript,
        input_data: ExecutionStack,
        script_private_key: PrivateKey,
        sender_offset_public_key: PublicKey,
        metadata_signature: ComSignature,
    ) -> UnblindedOutput {
        UnblindedOutput {
            value,
            spending_key,
            features,
            script,
            input_data,
            script_private_key,
            sender_offset_public_key,
            metadata_signature,
        }
    }

    /// Commits an UnblindedOutput into a Transaction input
    pub fn as_transaction_input(&self, factory: &CommitmentFactory) -> Result<TransactionInput, TransactionError> {
        let commitment = factory.commit(&self.spending_key, &self.value.into());
        let script_nonce_a = PrivateKey::random(&mut OsRng);
        let script_nonce_b = PrivateKey::random(&mut OsRng);
        let nonce_commitment = factory.commit(&script_nonce_b, &script_nonce_a);

        let challenge = TransactionInput::build_script_challenge(
            &nonce_commitment,
            &self.script,
            &self.input_data,
            &PublicKey::from_secret_key(&self.script_private_key),
            &commitment,
        );
        let script_signature = ComSignature::sign(
            self.value.into(),
            &self.script_private_key + &self.spending_key,
            script_nonce_a,
            script_nonce_b,
            &challenge,
            factory,
        )
        .map_err(|_| TransactionError::InvalidSignatureError("Generating script signature".to_string()))?;

        Ok(TransactionInput {
            features: self.features.clone(),
            commitment,
            script: self.script.clone(),
            input_data: self.input_data.clone(),
            script_signature,
            sender_offset_public_key: self.sender_offset_public_key.clone(),
        })
    }

    pub fn as_transaction_output(&self, factories: &CryptoFactories) -> Result<TransactionOutput, TransactionError> {
        if factories.range_proof.range() < 64 && self.value >= MicroTari::from(1u64.shl(&factories.range_proof.range()))
        {
            return Err(TransactionError::ValidationError(
                "Value provided is outside the range allowed by the range proof".into(),
            ));
        }
        let commitment = factories.commitment.commit(&self.spending_key, &self.value.into());
        let output = TransactionOutput {
            features: self.features.clone(),
            commitment,
            proof: RangeProof::from_bytes(
                &factories
                    .range_proof
                    .construct_proof(&self.spending_key, self.value.into())?,
            )
            .map_err(|_| TransactionError::RangeProofError(RangeProofError::ProofConstructionError))?,
            script: self.script.clone(),
            sender_offset_public_key: self.sender_offset_public_key.clone(),
            metadata_signature: self.metadata_signature.clone(),
        };

        Ok(output)
    }

    pub fn as_rewindable_transaction_output(
        &self,
        factories: &CryptoFactories,
        rewind_data: &RewindData,
    ) -> Result<TransactionOutput, TransactionError> {
        if factories.range_proof.range() < 64 && self.value >= MicroTari::from(1u64.shl(&factories.range_proof.range()))
        {
            return Err(TransactionError::ValidationError(
                "Value provided is outside the range allowed by the range proof".into(),
            ));
        }
        let commitment = factories.commitment.commit(&self.spending_key, &self.value.into());

        let proof_bytes = factories.range_proof.construct_proof_with_rewind_key(
            &self.spending_key,
            self.value.into(),
            &rewind_data.rewind_key,
            &rewind_data.rewind_blinding_key,
            &rewind_data.proof_message,
        )?;

        let proof = RangeProof::from_bytes(&proof_bytes)
            .map_err(|_| TransactionError::RangeProofError(RangeProofError::ProofConstructionError))?;

        let output = TransactionOutput {
            features: self.features.clone(),
            commitment,
            proof,
            script: self.script.clone(),
            sender_offset_public_key: self.sender_offset_public_key.clone(),
            metadata_signature: self.metadata_signature.clone(),
        };

        Ok(output)
    }

    pub fn metadata_byte_size(&self) -> usize {
        self.features.consensus_encode_exact_size() +
            ConsensusEncodingWrapper::wrap(&self.script).consensus_encode_exact_size()
    }
}

// These implementations are used for order these outputs for UTXO selection which will be done by comparing the values
impl Eq for UnblindedOutput {}

impl PartialEq for UnblindedOutput {
    fn eq(&self, other: &UnblindedOutput) -> bool {
        self.value == other.value
    }
}

impl Hash for UnblindedOutput {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl PartialOrd<UnblindedOutput> for UnblindedOutput {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl Ord for UnblindedOutput {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

//----------------------------------------     TransactionInput   ----------------------------------------------------//

/// A transaction input.
///
/// Primarily a reference to an output being spent by the transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionInput {
    /// The features of the output being spent. We will check maturity for all outputs.
    pub features: OutputFeatures,
    /// The commitment referencing the output being spent.
    pub commitment: Commitment,
    /// The serialised script
    pub script: TariScript,
    /// The script input data, if any
    pub input_data: ExecutionStack,
    /// A signature with k_s, signing the script, input data, and mined height
    pub script_signature: ComSignature,
    /// The offset public key, K_O
    pub sender_offset_public_key: PublicKey,
}

/// An input for a transaction that spends an existing output
impl TransactionInput {
    /// Create a new Transaction Input
    pub fn new(
        features: OutputFeatures,
        commitment: Commitment,
        script: TariScript,
        input_data: ExecutionStack,
        script_signature: ComSignature,
        sender_offset_public_key: PublicKey,
    ) -> TransactionInput {
        TransactionInput {
            features,
            commitment,
            script,
            input_data,
            script_signature,
            sender_offset_public_key,
        }
    }

    pub fn build_script_challenge(
        nonce_commitment: &Commitment,
        script: &TariScript,
        input_data: &ExecutionStack,
        script_public_key: &PublicKey,
        commitment: &Commitment,
    ) -> Vec<u8> {
        Challenge::new()
            .chain(nonce_commitment.as_bytes())
            .chain(script.as_bytes().as_slice())
            .chain(input_data.as_bytes().as_slice())
            .chain(script_public_key.as_bytes())
            .chain(commitment.as_bytes())
            .finalize()
            .to_vec()
    }

    /// Accessor method for the commitment contained in an input
    pub fn commitment(&self) -> &Commitment {
        &self.commitment
    }

    /// Checks if the given un-blinded input instance corresponds to this blinded Transaction Input
    pub fn opened_by(&self, input: &UnblindedOutput, factory: &CommitmentFactory) -> bool {
        factory.open(&input.spending_key, &input.value.into(), &self.commitment)
    }

    /// This will check if the input and the output is the same transactional output by looking at the commitment and
    /// features and script. This will ignore all other output and input fields
    pub fn is_equal_to(&self, output: &TransactionOutput) -> bool {
        self.output_hash() == output.hash()
    }

    /// This will run the script contained in the TransactionInput, returning either a script error or the resulting
    /// public key.
    pub fn run_script(&self) -> Result<PublicKey, TransactionError> {
        match self.script.execute(&self.input_data)? {
            StackItem::PublicKey(pubkey) => Ok(pubkey),
            _ => Err(TransactionError::ScriptExecutionError(
                "The script executed successfully but it did not leave a public key on the stack".to_string(),
            )),
        }
    }

    pub fn validate_script_signature(
        &self,
        public_script_key: &PublicKey,
        factory: &CommitmentFactory,
    ) -> Result<(), TransactionError> {
        let challenge = TransactionInput::build_script_challenge(
            self.script_signature.public_nonce(),
            &self.script,
            &self.input_data,
            public_script_key,
            &self.commitment,
        );
        if self
            .script_signature
            .verify_challenge(&(&self.commitment + public_script_key), &challenge, factory)
        {
            Ok(())
        } else {
            Err(TransactionError::InvalidSignatureError(
                "Verifying script signature".to_string(),
            ))
        }
    }

    /// This will run the script and verify the script signature. If its valid, it will return the resulting public key
    /// from the script.
    pub fn run_and_verify_script(&self, factory: &CommitmentFactory) -> Result<PublicKey, TransactionError> {
        let key = self.run_script()?;
        self.validate_script_signature(&key, factory)?;
        Ok(key)
    }

    /// Returns true if this input is mature at the given height, otherwise false
    pub fn is_mature_at(&self, block_height: u64) -> bool {
        self.features.maturity <= block_height
    }

    /// Returns the hash of the output data contained in this input.
    /// This hash matches the hash of a transaction output that this input spends.
    pub fn output_hash(&self) -> Vec<u8> {
        HashDigest::new()
            .chain(self.features.to_v1_bytes())
            .chain(self.commitment.as_bytes())
            .chain(self.script.as_bytes())
            .finalize()
            .to_vec()
    }
}

/// Implement the canonical hashing function for TransactionInput for use in ordering
impl Hashable for TransactionInput {
    fn hash(&self) -> Vec<u8> {
        HashDigest::new()
            .chain(self.features.to_v1_bytes())
            .chain(self.commitment.as_bytes())
            .chain(self.script.as_bytes())
            .chain(self.sender_offset_public_key.as_bytes())
            .chain(self.script_signature.u().as_bytes())
            .chain(self.script_signature.v().as_bytes())
            .chain(self.script_signature.public_nonce().as_bytes())
            .chain(self.input_data.as_bytes())
            .finalize()
            .to_vec()
    }
}

impl Display for TransactionInput {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            fmt,
            "{} [{:?}], Script hash: ({}), Offset_Pubkey: ({})",
            self.commitment.to_hex(),
            self.features,
            self.script,
            self.sender_offset_public_key.to_hex()
        )
    }
}

impl PartialOrd for TransactionInput {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.commitment.partial_cmp(&other.commitment)
    }
}

impl Ord for TransactionInput {
    fn cmp(&self, other: &Self) -> Ordering {
        self.commitment.cmp(&other.commitment)
    }
}

//----------------------------------------   TransactionOutput    ----------------------------------------------------//

/// Output for a transaction, defining the new ownership of coins that are being transferred. The commitment is a
/// blinded value for the output while the range proof guarantees the commitment includes a positive value without
/// overflow and the ownership of the private key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionOutput {
    /// Options for an output's structure or use
    pub features: OutputFeatures,
    /// The homomorphic commitment representing the output amount
    pub commitment: Commitment,
    /// A proof that the commitment is in the right range
    pub proof: RangeProof,
    /// The script that will be executed when spending this output
    pub script: TariScript,
    /// Tari script offset pubkey, K_O
    pub sender_offset_public_key: PublicKey,
    /// UTXO signature with the script offset private key, k_O
    pub metadata_signature: ComSignature,
}

/// An output for a transaction, includes a range proof and Tari script metadata
impl TransactionOutput {
    /// Create new Transaction Output
    pub fn new(
        features: OutputFeatures,
        commitment: Commitment,
        proof: RangeProof,
        script: TariScript,
        sender_offset_public_key: PublicKey,
        metadata_signature: ComSignature,
    ) -> TransactionOutput {
        TransactionOutput {
            features,
            commitment,
            proof,
            script,
            sender_offset_public_key,
            metadata_signature,
        }
    }

    /// Accessor method for the commitment contained in an output
    pub fn commitment(&self) -> &Commitment {
        &self.commitment
    }

    /// Accessor method for the range proof contained in an output
    pub fn proof(&self) -> &RangeProof {
        &self.proof
    }

    /// Verify that range proof is valid
    pub fn verify_range_proof(&self, prover: &RangeProofService) -> Result<bool, TransactionError> {
        Ok(prover.verify(&self.proof.0, &self.commitment))
    }

    /// Verify that the metadata signature is valid
    pub fn verify_metadata_signature(&self) -> Result<(), TransactionError> {
        let challenge = TransactionOutput::build_metadata_signature_challenge(
            &self.script,
            &self.features,
            &self.sender_offset_public_key,
            self.metadata_signature.public_nonce(),
            &self.commitment,
        );
        if !self.metadata_signature.verify_challenge(
            &(&self.commitment + &self.sender_offset_public_key),
            &challenge,
            &PedersenCommitmentFactory::default(),
        ) {
            return Err(TransactionError::InvalidSignatureError(
                "Metadata signature not valid!".to_string(),
            ));
        }
        Ok(())
    }

    /// Attempt to rewind the range proof to reveal the proof message and committed value
    pub fn rewind_range_proof_value_only(
        &self,
        prover: &RangeProofService,
        rewind_public_key: &PublicKey,
        rewind_blinding_public_key: &PublicKey,
    ) -> Result<RewindResult, TransactionError> {
        Ok(prover
            .rewind_proof_value_only(
                &self.proof.0,
                &self.commitment,
                rewind_public_key,
                rewind_blinding_public_key,
            )?
            .into())
    }

    /// Attempt to fully rewind the range proof to reveal the proof message, committed value and blinding factor
    pub fn full_rewind_range_proof(
        &self,
        prover: &RangeProofService,
        rewind_key: &PrivateKey,
        rewind_blinding_key: &PrivateKey,
    ) -> Result<FullRewindResult, TransactionError> {
        Ok(prover
            .rewind_proof_commitment_data(&self.proof.0, &self.commitment, rewind_key, rewind_blinding_key)?
            .into())
    }

    /// This will check if the input and the output is the same commitment by looking at the commitment and features.
    /// This will ignore the output range proof
    #[inline]
    pub fn is_equal_to(&self, output: &TransactionInput) -> bool {
        self.commitment == output.commitment && self.features == output.features
    }

    /// Returns true if the output is a coinbase, otherwise false
    pub fn is_coinbase(&self) -> bool {
        self.features.flags.contains(OutputFlags::COINBASE_OUTPUT)
    }

    /// Convenience function that returns the challenge for the metadata commitment signature
    pub fn get_metadata_signature_challenge(&self, partial_commitment_nonce: Option<&PublicKey>) -> MessageHash {
        let nonce_commitment = match partial_commitment_nonce {
            None => self.metadata_signature.public_nonce().clone(),
            Some(partial_nonce) => self.metadata_signature.public_nonce() + partial_nonce,
        };
        TransactionOutput::build_metadata_signature_challenge(
            &self.script,
            &self.features,
            &self.sender_offset_public_key,
            &nonce_commitment,
            &self.commitment,
        )
    }

    /// Convenience function that calculates the challenge for the metadata commitment signature
    pub fn build_metadata_signature_challenge(
        script: &TariScript,
        features: &OutputFeatures,
        sender_offset_public_key: &PublicKey,
        public_commitment_nonce: &Commitment,
        commitment: &Commitment,
    ) -> MessageHash {
        Challenge::new()
            .chain(public_commitment_nonce.as_bytes())
            .chain(script.as_bytes())
            // TODO: Use consensus encoded bytes #testnet reset
            .chain(features.to_v1_bytes())
            .chain(sender_offset_public_key.as_bytes())
            .chain(commitment.as_bytes())
            .finalize()
            .to_vec()
    }

    // Create commitment signature for the metadata
    fn create_metadata_signature(
        value: &MicroTari,
        spending_key: &BlindingFactor,
        script: &TariScript,
        output_features: &OutputFeatures,
        sender_offset_public_key: &PublicKey,
        partial_commitment_nonce: Option<&PublicKey>,
        sender_offset_private_key: Option<&PrivateKey>,
    ) -> Result<ComSignature, TransactionError> {
        let nonce_a = PrivateKey::random(&mut OsRng);
        let nonce_b = PrivateKey::random(&mut OsRng);
        let nonce_commitment = PedersenCommitmentFactory::default().commit(&nonce_b, &nonce_a);
        let nonce_commitment = match partial_commitment_nonce {
            None => nonce_commitment,
            Some(partial_nonce) => &nonce_commitment + partial_nonce,
        };
        let value = PrivateKey::from(value.as_u64());
        let commitment = PedersenCommitmentFactory::default().commit(spending_key, &value);
        let e = TransactionOutput::build_metadata_signature_challenge(
            script,
            output_features,
            sender_offset_public_key,
            &nonce_commitment,
            &commitment,
        );
        let secret_x = match sender_offset_private_key {
            None => spending_key.clone(),
            Some(key) => &spending_key.clone() + key,
        };
        Ok(ComSignature::sign(
            value,
            secret_x,
            nonce_a,
            nonce_b,
            &e,
            &PedersenCommitmentFactory::default(),
        )?)
    }

    /// Create partial commitment signature for the metadata, usually done by the receiver
    pub fn create_partial_metadata_signature(
        value: &MicroTari,
        spending_key: &BlindingFactor,
        script: &TariScript,
        output_features: &OutputFeatures,
        sender_offset_public_key: &PublicKey,
        partial_commitment_nonce: &PublicKey,
    ) -> Result<ComSignature, TransactionError> {
        TransactionOutput::create_metadata_signature(
            value,
            spending_key,
            script,
            output_features,
            sender_offset_public_key,
            Some(partial_commitment_nonce),
            None,
        )
    }

    /// Create final commitment signature for the metadata, signing with both keys
    pub fn create_final_metadata_signature(
        value: &MicroTari,
        spending_key: &BlindingFactor,
        script: &TariScript,
        output_features: &OutputFeatures,
        sender_offset_private_key: &PrivateKey,
    ) -> Result<ComSignature, TransactionError> {
        let sender_offset_public_key = PublicKey::from_secret_key(sender_offset_private_key);
        TransactionOutput::create_metadata_signature(
            value,
            spending_key,
            script,
            output_features,
            &sender_offset_public_key,
            None,
            Some(sender_offset_private_key),
        )
    }

    pub fn witness_hash(&self) -> Vec<u8> {
        HashDigest::new()
            .chain(self.proof.as_bytes())
            .chain(self.metadata_signature.u().as_bytes())
            .chain(self.metadata_signature.v().as_bytes())
            .chain(self.metadata_signature.public_nonce().as_bytes())
            .finalize()
            .to_vec()
    }
}

/// Implement the canonical hashing function for TransactionOutput for use in ordering.
///
/// We can exclude the range proof from this hash. The rationale for this is:
/// a) It is a significant performance boost, since the RP is the biggest part of an output
/// b) Range proofs are committed to elsewhere and so we'd be hashing them twice (and as mentioned, this is slow)
/// c) TransactionInputs will now have the same hash as UTXOs, which makes locating STXOs easier when doing reorgs
impl Hashable for TransactionOutput {
    fn hash(&self) -> Vec<u8> {
        HashDigest::new()
            // TODO: use consensus encoding #testnetreset
            .chain(self.features.to_v1_bytes())
            .chain(self.commitment.as_bytes())
            // .chain(range proof) // See docs as to why we exclude this
            .chain(self.script.as_bytes())
            .finalize()
            .to_vec()
    }
}

// impl Default for TransactionOutput {
//     fn default() -> Self {
//         TransactionOutput::new(
//             OutputFeatures::default(),
//             CommitmentFactory::default().zero(),
//             RangeProof::default(),
//             TariScript::default(),
//             PublicKey::default(),
//         )
//     }
// }

impl Display for TransactionOutput {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let proof = self.proof.to_hex();
        let proof = if proof.len() > 32 {
            format!(
                "{}..{}",
                proof[0..16].to_string(),
                proof[proof.len() - 16..proof.len()].to_string()
            )
        } else {
            proof
        };
        write!(
            fmt,
            "{} [{:?}], Script: ({}), Offset Pubkey: ({}), Metadata Signature: ({}, {}, {}), Proof: {}",
            self.commitment.to_hex(),
            self.features,
            self.script,
            self.sender_offset_public_key.to_hex(),
            self.metadata_signature.u().to_hex(),
            self.metadata_signature.v().to_hex(),
            self.metadata_signature.public_nonce().to_hex(),
            proof
        )
    }
}

impl PartialOrd for TransactionOutput {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.commitment.partial_cmp(&other.commitment)
    }
}

impl Ord for TransactionOutput {
    fn cmp(&self, other: &Self) -> Ordering {
        self.commitment.cmp(&other.commitment)
    }
}

/// A wrapper struct to hold the result of a successful range proof rewinding to reveal the committed value and proof
/// message
#[derive(Debug, PartialEq)]
pub struct RewindResult {
    pub committed_value: MicroTari,
    pub proof_message: [u8; REWIND_USER_MESSAGE_LENGTH],
}

impl RewindResult {
    pub fn new(committed_value: MicroTari, proof_message: [u8; REWIND_USER_MESSAGE_LENGTH]) -> Self {
        Self {
            committed_value,
            proof_message,
        }
    }
}

impl From<CryptoRewindResult> for RewindResult {
    fn from(crr: CryptoRewindResult) -> Self {
        Self {
            committed_value: crr.committed_value.into(),
            proof_message: crr.proof_message,
        }
    }
}

/// A wrapper struct to hold the result of a successful range proof full rewinding to reveal the committed value, proof
/// message and blinding factor
#[derive(Debug, PartialEq)]
pub struct FullRewindResult {
    pub committed_value: MicroTari,
    pub proof_message: [u8; REWIND_USER_MESSAGE_LENGTH],
    pub blinding_factor: BlindingFactor,
}

impl FullRewindResult {
    pub fn new(
        committed_value: MicroTari,
        proof_message: [u8; REWIND_USER_MESSAGE_LENGTH],
        blinding_factor: BlindingFactor,
    ) -> Self {
        Self {
            committed_value,
            proof_message,
            blinding_factor,
        }
    }
}

impl From<CryptoFullRewindResult<BlindingFactor>> for FullRewindResult {
    fn from(crr: CryptoFullRewindResult<BlindingFactor>) -> Self {
        Self {
            committed_value: crr.committed_value.into(),
            proof_message: crr.proof_message,
            blinding_factor: crr.blinding_factor,
        }
    }
}

//----------------------------------------   Transaction Kernel   ----------------------------------------------------//

/// The transaction kernel tracks the excess for a given transaction. For an explanation of what the excess is, and
/// why it is necessary, refer to the
/// [Mimblewimble TLU post](https://tlu.tarilabs.com/protocols/mimblewimble-1/sources/PITCHME.link.html?highlight=mimblewimble#mimblewimble).
/// The kernel also tracks other transaction metadata, such as the lock height for the transaction (i.e. the earliest
/// this transaction can be mined) and the transaction fee, in cleartext.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionKernel {
    /// Options for a kernel's structure or use
    pub features: KernelFeatures,
    /// Fee originally included in the transaction this proof is for.
    pub fee: MicroTari,
    /// This kernel is not valid earlier than lock_height blocks
    /// The max lock_height of all *inputs* to this transaction
    pub lock_height: u64,
    /// Remainder of the sum of all transaction commitments (minus an offset). If the transaction is well-formed,
    /// amounts plus fee will sum to zero, and the excess is hence a valid public key.
    pub excess: Commitment,
    /// An aggregated signature of the metadata in this kernel, signed by the individual excess values and the offset
    /// excess of the sender.
    pub excess_sig: Signature,
}

impl TransactionKernel {
    pub fn is_coinbase(&self) -> bool {
        self.features.contains(KernelFeatures::COINBASE_KERNEL)
    }

    pub fn verify_signature(&self) -> Result<(), TransactionError> {
        let excess = self.excess.as_public_key();
        let r = self.excess_sig.get_public_nonce();
        let m = TransactionMetadata {
            lock_height: self.lock_height,
            fee: self.fee,
        };
        let c = build_challenge(r, &m);
        if self.excess_sig.verify_challenge(excess, &c) {
            Ok(())
        } else {
            Err(TransactionError::InvalidSignatureError(
                "Verifying kernel signature".to_string(),
            ))
        }
    }

    /// This method was used to sort kernels. It has been replaced, and will be removed in future
    pub fn deprecated_cmp(&self, other: &Self) -> Ordering {
        self.features
            .cmp(&other.features)
            .then(self.fee.cmp(&other.fee))
            .then(self.lock_height.cmp(&other.lock_height))
            .then(self.excess.cmp(&other.excess))
            .then(self.excess_sig.cmp(&other.excess_sig))
    }
}

impl Hashable for TransactionKernel {
    /// Produce a canonical hash for a transaction kernel. The hash is given by
    /// $$ H(feature_bits | fee | lock_height | P_excess | R_sum | s_sum)
    fn hash(&self) -> Vec<u8> {
        HashDigest::new()
            .chain(&[self.features.bits])
            .chain(u64::from(self.fee).to_le_bytes())
            .chain(self.lock_height.to_le_bytes())
            .chain(self.excess.as_bytes())
            .chain(self.excess_sig.get_public_nonce().as_bytes())
            .chain(self.excess_sig.get_signature().as_bytes())
            .finalize()
            .to_vec()
    }
}

impl Display for TransactionKernel {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            fmt,
            "Fee: {}\nLock height: {}\nFeatures: {:?}\nExcess: {}\nExcess signature: {}\n",
            self.fee,
            self.lock_height,
            self.features,
            self.excess.to_hex(),
            self.excess_sig
                .to_json()
                .unwrap_or_else(|_| "Failed to serialize signature".into()),
        )
    }
}

impl PartialOrd for TransactionKernel {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.excess_sig.partial_cmp(&other.excess_sig)
    }
}

impl Ord for TransactionKernel {
    fn cmp(&self, other: &Self) -> Ordering {
        self.excess_sig.cmp(&other.excess_sig)
    }
}

/// A version of Transaction kernel with optional fields. This struct is only used in constructing transaction kernels
pub struct KernelBuilder {
    features: KernelFeatures,
    fee: MicroTari,
    lock_height: u64,
    excess: Option<Commitment>,
    excess_sig: Option<Signature>,
}

/// Implementation of the transaction kernel
impl KernelBuilder {
    /// Creates an empty transaction kernel
    pub fn new() -> KernelBuilder {
        KernelBuilder::default()
    }

    /// Build a transaction kernel with the provided features
    pub fn with_features(mut self, features: KernelFeatures) -> KernelBuilder {
        self.features = features;
        self
    }

    /// Build a transaction kernel with the provided fee
    pub fn with_fee(mut self, fee: MicroTari) -> KernelBuilder {
        self.fee = fee;
        self
    }

    /// Build a transaction kernel with the provided lock height
    pub fn with_lock_height(mut self, lock_height: u64) -> KernelBuilder {
        self.lock_height = lock_height;
        self
    }

    /// Add the excess (sum of public spend keys minus the offset)
    pub fn with_excess(mut self, excess: &Commitment) -> KernelBuilder {
        self.excess = Some(excess.clone());
        self
    }

    /// Add the excess signature
    pub fn with_signature(mut self, signature: &Signature) -> KernelBuilder {
        self.excess_sig = Some(signature.clone());
        self
    }

    pub fn build(self) -> Result<TransactionKernel, TransactionError> {
        if self.excess.is_none() || self.excess_sig.is_none() {
            return Err(TransactionError::NoSignatureError);
        }
        Ok(TransactionKernel {
            features: self.features,
            fee: self.fee,
            lock_height: self.lock_height,
            excess: self.excess.unwrap(),
            excess_sig: self.excess_sig.unwrap(),
        })
    }
}

impl Default for KernelBuilder {
    fn default() -> Self {
        KernelBuilder {
            features: KernelFeatures::empty(),
            fee: MicroTari::from(0),
            lock_height: 0,
            excess: None,
            excess_sig: None,
        }
    }
}

/// This struct holds the result of calculating the sum of the kernels in a Transaction
/// and returns the summed commitments and the total fees
#[derive(Default)]
pub struct KernelSum {
    pub sum: Commitment,
    pub fees: MicroTari,
}

//----------------------------------------      Transaction       ----------------------------------------------------//

/// A transaction which consists of a kernel offset and an aggregate body made up of inputs, outputs and kernels.
/// This struct is used to describe single transactions only. The common part between transactions and Tari blocks is
/// accessible via the `body` field, but single transactions also need to carry the public offset around with them so
/// that these can be aggregated into block offsets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    /// This kernel offset will be accumulated when transactions are aggregated to prevent the "subset" problem where
    /// kernels can be linked to inputs and outputs by testing a series of subsets and see which produce valid
    /// transactions.
    pub offset: BlindingFactor,
    /// The constituents of a transaction which has the same structure as the body of a block.
    pub body: AggregateBody,
    /// A scalar offset that links outputs and inputs to prevent cut-through, enforcing the correct application of
    /// the output script.
    pub script_offset: BlindingFactor,
}

impl Transaction {
    /// Create a new transaction from the provided inputs, outputs, kernels and offset
    pub fn new(
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        kernels: Vec<TransactionKernel>,
        offset: BlindingFactor,
        script_offset: BlindingFactor,
    ) -> Self {
        Self {
            offset,
            body: AggregateBody::new(inputs, outputs, kernels),
            script_offset,
        }
    }

    /// Validate this transaction by checking the following:
    /// 1. The sum of inputs, outputs and fees equal the (public excess value + offset)
    /// 1. The signature signs the canonical message with the private excess
    /// 1. Range proofs of the outputs are valid
    ///
    /// This function does NOT check that inputs come from the UTXO set
    #[allow(clippy::erasing_op)] // This is for 0 * uT
    pub fn validate_internal_consistency(
        &self,
        bypass_range_proof_verification: bool,
        factories: &CryptoFactories,
        reward: Option<MicroTari>,
    ) -> Result<(), TransactionError> {
        let reward = reward.unwrap_or_else(|| 0 * uT);
        self.body.validate_internal_consistency(
            &self.offset,
            &self.script_offset,
            bypass_range_proof_verification,
            reward,
            factories,
        )
    }

    pub fn body(&self) -> &AggregateBody {
        &self.body
    }

    /// Returns the byte size or weight of a transaction
    pub fn calculate_weight(&self, transaction_weight: &TransactionWeight) -> u64 {
        self.body.calculate_weight(transaction_weight)
    }

    /// Returns the minimum maturity of the input UTXOs
    pub fn min_input_maturity(&self) -> u64 {
        self.body.inputs().iter().fold(u64::MAX, |min_maturity, input| {
            min(min_maturity, input.features.maturity)
        })
    }

    /// Returns the maximum maturity of the input UTXOs
    pub fn max_input_maturity(&self) -> u64 {
        self.body
            .inputs()
            .iter()
            .fold(0, |max_maturity, input| max(max_maturity, input.features.maturity))
    }

    /// Returns the maximum time lock of the kernels inside of the transaction
    pub fn max_kernel_timelock(&self) -> u64 {
        self.body.max_kernel_timelock()
    }

    /// Returns the height of the minimum height where the transaction is spendable. This is calculated from the
    /// transaction kernel lock_heights and the maturity of the input UTXOs.
    pub fn min_spendable_height(&self) -> u64 {
        max(self.max_kernel_timelock(), self.max_input_maturity())
    }

    /// This function adds two transactions together. It does not do cut-through. Calling Tx1 + Tx2 will result in
    /// vut-through being applied.
    pub fn add_no_cut_through(mut self, other: Self) -> Self {
        self.offset = self.offset + other.offset;
        self.script_offset = self.script_offset + other.script_offset;
        let (mut inputs, mut outputs, mut kernels) = other.body.dissolve();
        self.body.add_inputs(&mut inputs);
        self.body.add_outputs(&mut outputs);
        self.body.add_kernels(&mut kernels);
        self
    }

    pub fn first_kernel_excess_sig(&self) -> Option<&Signature> {
        Some(&self.body.kernels().first()?.excess_sig)
    }
}

impl Add for Transaction {
    type Output = Self;

    fn add(mut self, other: Self) -> Self {
        self = self.add_no_cut_through(other);
        self
    }
}

impl Display for Transaction {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        fmt.write_str("-------------- Transaction --------------\n")?;
        fmt.write_str("--- Offset ---\n")?;
        fmt.write_str(&format!("{}\n", self.offset.to_hex()))?;
        fmt.write_str("--- Script Offset ---\n")?;
        fmt.write_str(&format!("{}\n", self.script_offset.to_hex()))?;
        fmt.write_str("---  Body  ---\n")?;
        fmt.write_str(&format!("{}\n", self.body))
    }
}

//----------------------------------------  Transaction Builder   ----------------------------------------------------//
pub struct TransactionBuilder {
    body: AggregateBody,
    offset: Option<BlindingFactor>,
    script_offset: Option<BlindingFactor>,
    reward: Option<MicroTari>,
}

impl TransactionBuilder {
    /// Create an new empty TransactionBuilder
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the offset of an existing transaction
    pub fn add_offset(&mut self, offset: BlindingFactor) -> &mut Self {
        self.offset = Some(offset);
        self
    }

    /// Update the script offset of an existing transaction
    pub fn add_script_offset(&mut self, script_offset: BlindingFactor) -> &mut Self {
        self.script_offset = Some(script_offset);
        self
    }

    /// Add an input to an existing transaction
    pub fn add_input(&mut self, input: TransactionInput) -> &mut Self {
        self.body.add_input(input);
        self
    }

    /// Add an output to an existing transaction
    pub fn add_output(&mut self, output: TransactionOutput) -> &mut Self {
        self.body.add_output(output);
        self
    }

    /// Moves a series of inputs to an existing transaction, leaving `inputs` empty
    pub fn add_inputs(&mut self, inputs: &mut Vec<TransactionInput>) -> &mut Self {
        self.body.add_inputs(inputs);
        self
    }

    /// Moves a series of outputs to an existing transaction, leaving `outputs` empty
    pub fn add_outputs(&mut self, outputs: &mut Vec<TransactionOutput>) -> &mut Self {
        self.body.add_outputs(outputs);
        self
    }

    /// Set the kernel of a transaction. Currently only one kernel is allowed per transaction
    pub fn with_kernel(&mut self, kernel: TransactionKernel) -> &mut Self {
        self.body.set_kernel(kernel);
        self
    }

    pub fn with_reward(&mut self, reward: MicroTari) -> &mut Self {
        self.reward = Some(reward);
        self
    }

    /// Build the transaction.
    pub fn build(self, factories: &CryptoFactories) -> Result<Transaction, TransactionError> {
        if let (Some(script_offset), Some(offset)) = (self.script_offset, self.offset) {
            let (i, o, k) = self.body.dissolve();
            let tx = Transaction::new(i, o, k, offset, script_offset);
            tx.validate_internal_consistency(true, factories, self.reward)?;
            Ok(tx)
        } else {
            Err(TransactionError::ValidationError(
                "Transaction validation failed".into(),
            ))
        }
    }
}

impl Default for TransactionBuilder {
    fn default() -> Self {
        Self {
            offset: None,
            body: AggregateBody::empty(),
            reward: None,
            script_offset: None,
        }
    }
}

//-----------------------------------------       Tests           ----------------------------------------------------//

#[cfg(test)]
mod test {
    use crate::{
        transactions::{
            tari_amount::T,
            test_helpers,
            test_helpers::{TestParams, UtxoTestParams},
            transaction::OutputFeatures,
        },
        txn_schema,
    };
    use rand::{self, rngs::OsRng};
    use tari_common_types::types::{BlindingFactor, PrivateKey, PublicKey};
    use tari_crypto::{
        keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait},
        ristretto::pedersen::PedersenCommitmentFactory,
        script,
        script::ExecutionStack,
    };

    use super::*;

    #[test]
    fn input_and_output_hash_match() {
        let test_params = TestParams::new();
        let factory = PedersenCommitmentFactory::default();

        let i = test_params.create_unblinded_output(Default::default());
        let output = i.as_transaction_output(&CryptoFactories::default()).unwrap();
        let input = i.as_transaction_input(&factory).unwrap();
        assert_eq!(output.hash(), input.output_hash());
    }

    #[test]
    fn unblinded_input() {
        let test_params = TestParams::new();
        let factory = PedersenCommitmentFactory::default();

        let i = test_params.create_unblinded_output(Default::default());
        let input = i
            .as_transaction_input(&factory)
            .expect("Should be able to create transaction input");
        assert_eq!(input.features, OutputFeatures::default());
        assert!(input.opened_by(&i, &factory));
    }

    #[test]
    fn with_maturity() {
        let features = OutputFeatures::with_maturity(42);
        assert_eq!(features.maturity, 42);
        assert_eq!(features.flags, OutputFlags::empty());
    }

    #[test]
    fn range_proof_verification() {
        let factories = CryptoFactories::new(32);
        // Directly test the tx_output verification
        let test_params_1 = TestParams::new();
        let test_params_2 = TestParams::new();
        let output_features = OutputFeatures::default();

        // For testing the max range has been limited to 2^32 so this value is too large.
        let unblinded_output1 = test_params_1.create_unblinded_output(UtxoTestParams {
            value: (2u64.pow(32) - 1u64).into(),
            ..Default::default()
        });
        let script = unblinded_output1.script.clone();
        let tx_output1 = unblinded_output1.as_transaction_output(&factories).unwrap();
        assert!(tx_output1.verify_range_proof(&factories.range_proof).unwrap());

        let unblinded_output2 = test_params_2.create_unblinded_output(UtxoTestParams {
            value: (2u64.pow(32) + 1u64).into(),
            ..Default::default()
        });
        let tx_output2 = unblinded_output2.as_transaction_output(&factories);
        match tx_output2 {
            Ok(_) => panic!("Range proof should have failed to verify"),
            Err(e) => assert_eq!(
                e,
                TransactionError::ValidationError(
                    "Value provided is outside the range allowed by the range proof".to_string()
                )
            ),
        }

        let value = 2u64.pow(32) + 1;
        let v = PrivateKey::from(value);
        let c = factories.commitment.commit(&test_params_2.spend_key, &v);
        let proof = factories
            .range_proof
            .construct_proof(&test_params_2.spend_key, 2u64.pow(32) + 1)
            .unwrap();

        let tx_output3 = TransactionOutput::new(
            output_features.clone(),
            c,
            RangeProof::from_bytes(&proof).unwrap(),
            script.clone(),
            test_params_2.sender_offset_public_key,
            TransactionOutput::create_final_metadata_signature(
                &value.into(),
                &test_params_2.spend_key,
                &script,
                &output_features,
                &test_params_2.sender_offset_private_key,
            )
            .unwrap(),
        );
        assert!(!tx_output3.verify_range_proof(&factories.range_proof).unwrap());
    }

    #[test]
    fn sender_signature_verification() {
        let test_params = TestParams::new();
        let factories = CryptoFactories::new(32);
        let unblinded_output = test_params.create_unblinded_output(Default::default());

        let mut tx_output = unblinded_output.as_transaction_output(&factories).unwrap();
        assert!(tx_output.verify_metadata_signature().is_ok());
        tx_output.script = TariScript::default();
        assert!(tx_output.verify_metadata_signature().is_err());

        tx_output = unblinded_output.as_transaction_output(&factories).unwrap();
        assert!(tx_output.verify_metadata_signature().is_ok());
        tx_output.features = OutputFeatures::create_coinbase(0);
        assert!(tx_output.verify_metadata_signature().is_err());

        tx_output = unblinded_output.as_transaction_output(&factories).unwrap();
        assert!(tx_output.verify_metadata_signature().is_ok());
        tx_output.sender_offset_public_key = PublicKey::default();
        assert!(tx_output.verify_metadata_signature().is_err());
    }

    #[test]
    fn kernel_hash() {
        let s = PrivateKey::from_hex("6c6eebc5a9c02e1f3c16a69ba4331f9f63d0718401dea10adc4f9d3b879a2c09").unwrap();
        let r = PublicKey::from_hex("28e8efe4e5576aac931d358d0f6ace43c55fa9d4186d1d259d1436caa876d43b").unwrap();
        let sig = Signature::new(r, s);
        let excess = Commitment::from_hex("9017be5092b85856ce71061cadeb20c2d1fabdf664c4b3f082bf44cf5065e650").unwrap();
        let k = KernelBuilder::new()
            .with_signature(&sig)
            .with_fee(100.into())
            .with_excess(&excess)
            .with_lock_height(500)
            .build()
            .unwrap();
        assert_eq!(
            &k.hash().to_hex(),
            "fe25e4e961d5efec889c489d43e40a1334bf9b4408be4c2e8035a523f231a732"
        );
    }

    #[test]
    fn kernel_metadata() {
        let s = PrivateKey::from_hex("df9a004360b1cf6488d8ff7fb625bc5877f4b013f9b2b20d84932172e605b207").unwrap();
        let r = PublicKey::from_hex("5c6bfaceaa1c83fa4482a816b5f82ca3975cb9b61b6e8be4ee8f01c5f1bee561").unwrap();
        let sig = Signature::new(r, s);
        let excess = Commitment::from_hex("e0bd3f743b566272277c357075b0584fc840d79efac49e9b3b6dbaa8a351bc0c").unwrap();
        let k = KernelBuilder::new()
            .with_signature(&sig)
            .with_fee(100.into())
            .with_excess(&excess)
            .with_lock_height(500)
            .build()
            .unwrap();
        assert_eq!(
            &k.hash().to_hex(),
            "f1e7348b0952d8afbec6bfaa07a1cbc9c45e51e022242d3faeb0f190e2a9dd07"
        )
    }

    #[test]
    fn check_timelocks() {
        let factories = CryptoFactories::new(32);
        let k = BlindingFactor::random(&mut OsRng);
        let v = PrivateKey::from(2u64.pow(32) + 1);
        let c = factories.commitment.commit(&k, &v);

        let script = TariScript::default();
        let input_data = ExecutionStack::default();
        let script_signature = ComSignature::default();
        let offset_pub_key = PublicKey::default();
        let mut input = TransactionInput::new(
            OutputFeatures::default(),
            c,
            script,
            input_data,
            script_signature,
            offset_pub_key,
        );

        let mut kernel = test_helpers::create_test_kernel(0.into(), 0);
        let mut tx = Transaction::new(Vec::new(), Vec::new(), Vec::new(), 0.into(), 0.into());

        // lets add time locks
        input.features.maturity = 5;
        kernel.lock_height = 2;
        tx.body.add_input(input.clone());
        tx.body.add_kernel(kernel.clone());
        assert_eq!(tx.body.check_stxo_rules(1), Err(TransactionError::InputMaturity));
        assert_eq!(tx.body.check_stxo_rules(5), Ok(()));

        assert_eq!(tx.max_input_maturity(), 5);
        assert_eq!(tx.max_kernel_timelock(), 2);
        assert_eq!(tx.min_spendable_height(), 5);

        input.features.maturity = 4;
        kernel.lock_height = 3;
        tx.body.add_input(input.clone());
        tx.body.add_kernel(kernel.clone());

        assert_eq!(tx.max_input_maturity(), 5);
        assert_eq!(tx.max_kernel_timelock(), 3);
        assert_eq!(tx.min_spendable_height(), 5);

        input.features.maturity = 2;
        kernel.lock_height = 10;
        tx.body.add_input(input);
        tx.body.add_kernel(kernel);

        assert_eq!(tx.max_input_maturity(), 5);
        assert_eq!(tx.max_kernel_timelock(), 10);
        assert_eq!(tx.min_spendable_height(), 10);
    }

    #[test]
    fn test_validate_internal_consistency() {
        let (tx, _, _) = test_helpers::create_tx(5000.into(), 3.into(), 1, 2, 1, 4);

        let factories = CryptoFactories::default();
        assert!(tx.validate_internal_consistency(false, &factories, None).is_ok());
    }

    #[test]
    #[allow(clippy::identity_op)]
    fn check_cut_through() {
        let (tx, _, outputs) = test_helpers::create_tx(50000000.into(), 3.into(), 1, 2, 1, 2);

        assert_eq!(tx.body.inputs().len(), 2);
        assert_eq!(tx.body.outputs().len(), 2);
        assert_eq!(tx.body.kernels().len(), 1);

        let factories = CryptoFactories::default();
        assert!(tx.validate_internal_consistency(false, &factories, None).is_ok());

        let schema = txn_schema!(from: vec![outputs[1].clone()], to: vec![1 * T, 2 * T]);
        let (tx2, _outputs, _) = test_helpers::spend_utxos(schema);

        assert_eq!(tx2.body.inputs().len(), 1);
        assert_eq!(tx2.body.outputs().len(), 3);
        assert_eq!(tx2.body.kernels().len(), 1);

        let tx3 = tx + tx2;
        let mut tx3_cut_through = tx3.clone();
        // check that all inputs are as we expect them to be
        assert_eq!(tx3.body.inputs().len(), 3);
        assert_eq!(tx3.body.outputs().len(), 5);
        assert_eq!(tx3.body.kernels().len(), 2);

        // Do manual cut-through on tx3
        let double_inputs: Vec<TransactionInput> = tx3_cut_through
            .body
            .inputs()
            .clone()
            .iter()
            .filter(|input| tx3_cut_through.body.outputs_mut().iter().any(|o| o.is_equal_to(input)))
            .cloned()
            .collect();
        for input in double_inputs {
            tx3_cut_through.body.outputs_mut().retain(|x| !input.is_equal_to(x));
            tx3_cut_through.body.inputs_mut().retain(|x| *x != input);
        }

        // Validate basis transaction where cut-through has not been applied.
        assert!(tx3.validate_internal_consistency(false, &factories, None).is_ok());

        // tx3_cut_through has manual cut-through, it should not be possible so this should fail
        assert!(tx3_cut_through
            .validate_internal_consistency(false, &factories, None)
            .is_err());
    }

    #[test]
    fn check_duplicate_inputs_outputs() {
        let (tx, _, _outputs) = test_helpers::create_tx(50000000.into(), 3.into(), 1, 2, 1, 2);
        assert!(!tx.body.contains_duplicated_outputs());
        assert!(!tx.body.contains_duplicated_inputs());

        let input = tx.body.inputs()[0].clone();
        let output = tx.body.outputs()[0].clone();

        let mut broken_tx_1 = tx.clone();
        let mut broken_tx_2 = tx;

        broken_tx_1.body.add_input(input);
        broken_tx_2.body.add_output(output);

        assert!(broken_tx_1.body.contains_duplicated_inputs());
        assert!(broken_tx_2.body.contains_duplicated_outputs());
    }

    #[test]
    fn inputs_not_malleable() {
        let (mut inputs, outputs) = test_helpers::create_unblinded_txos(5000.into(), 1, 1, 2, 15.into());
        let mut stack = inputs[0].input_data.clone();
        inputs[0].script = script!(Drop Nop);
        inputs[0].input_data.push(StackItem::Hash([0; 32])).unwrap();
        let mut tx = test_helpers::create_transaction_with(1, 15.into(), inputs, outputs);

        stack
            .push(StackItem::Hash(*b"Pls put this on tha tari network"))
            .unwrap();

        tx.body.inputs_mut()[0].input_data = stack;

        let factories = CryptoFactories::default();
        let err = tx.validate_internal_consistency(false, &factories, None).unwrap_err();
        assert!(matches!(err, TransactionError::InvalidSignatureError(_)));
    }

    #[test]
    fn test_output_rewinding() {
        let test_params = TestParams::new();
        let factories = CryptoFactories::new(32);
        let v = MicroTari::from(42);
        let rewind_key = PrivateKey::random(&mut OsRng);
        let rewind_blinding_key = PrivateKey::random(&mut OsRng);
        let random_key = PrivateKey::random(&mut OsRng);
        let rewind_public_key = PublicKey::from_secret_key(&rewind_key);
        let rewind_blinding_public_key = PublicKey::from_secret_key(&rewind_blinding_key);
        let public_random_key = PublicKey::from_secret_key(&random_key);
        let proof_message = b"testing12345678910111";

        let rewind_data = RewindData {
            rewind_key: rewind_key.clone(),
            rewind_blinding_key: rewind_blinding_key.clone(),
            proof_message: proof_message.to_owned(),
        };

        let unblinded_output = test_params.create_unblinded_output(UtxoTestParams {
            value: v,
            ..Default::default()
        });
        let output = unblinded_output
            .as_rewindable_transaction_output(&factories, &rewind_data)
            .unwrap();

        assert_eq!(
            output.rewind_range_proof_value_only(
                &factories.range_proof,
                &public_random_key,
                &rewind_blinding_public_key
            ),
            Err(TransactionError::RangeProofError(RangeProofError::InvalidRewind))
        );
        assert_eq!(
            output.rewind_range_proof_value_only(&factories.range_proof, &rewind_public_key, &public_random_key),
            Err(TransactionError::RangeProofError(RangeProofError::InvalidRewind))
        );

        let rewind_result = output
            .rewind_range_proof_value_only(&factories.range_proof, &rewind_public_key, &rewind_blinding_public_key)
            .unwrap();

        assert_eq!(rewind_result.committed_value, v);
        assert_eq!(&rewind_result.proof_message, proof_message);

        assert_eq!(
            output.full_rewind_range_proof(&factories.range_proof, &random_key, &rewind_blinding_key),
            Err(TransactionError::RangeProofError(RangeProofError::InvalidRewind))
        );
        assert_eq!(
            output.full_rewind_range_proof(&factories.range_proof, &rewind_key, &random_key),
            Err(TransactionError::RangeProofError(RangeProofError::InvalidRewind))
        );

        let full_rewind_result = output
            .full_rewind_range_proof(&factories.range_proof, &rewind_key, &rewind_blinding_key)
            .unwrap();
        assert_eq!(full_rewind_result.committed_value, v);
        assert_eq!(&full_rewind_result.proof_message, proof_message);
        assert_eq!(full_rewind_result.blinding_factor, test_params.spend_key);
    }
    mod output_features {
        use super::*;

        #[test]
        fn consensus_encode_minimal() {
            let features = OutputFeatures::with_maturity(0);
            let mut buf = Vec::new();
            let written = features.consensus_encode(&mut buf).unwrap();
            assert_eq!(buf.len(), 3);
            assert_eq!(written, 3);
        }

        #[test]
        fn consensus_encode_decode() {
            let features = OutputFeatures::create_coinbase(u64::MAX);
            let known_size = features.consensus_encode_exact_size();
            let mut buf = Vec::with_capacity(known_size);
            assert_eq!(known_size, 12);
            let written = features.consensus_encode(&mut buf).unwrap();
            assert_eq!(buf.len(), 12);
            assert_eq!(written, 12);
            let decoded_features = OutputFeatures::consensus_decode(&mut &buf[..]).unwrap();
            assert_eq!(features, decoded_features);
        }

        #[test]
        fn consensus_decode_bad_flags() {
            let data = [0x00u8, 0x00, 0x02];
            let features = OutputFeatures::consensus_decode(&mut &data[..]).unwrap();
            // Assert the flag data is preserved
            assert_eq!(features.flags.bits & 0x02, 0x02);
        }

        #[test]
        fn consensus_decode_bad_maturity() {
            let data = [0x00u8, 0xFF];
            let err = OutputFeatures::consensus_decode(&mut &data[..]).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        }

        #[test]
        fn consensus_decode_attempt_maturity_overflow() {
            let data = [0x00u8, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
            let err = OutputFeatures::consensus_decode(&mut &data[..]).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        }
    }
}
