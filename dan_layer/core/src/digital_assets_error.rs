//  Copyright 2021. The Tari Project
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

use thiserror::Error;

use crate::{models::ModelError, services::ValidatorNodeClientError, storage::StorageError, workers::StateSyncError};

#[derive(Debug, Error)]
pub enum DigitalAssetError {
    #[error("Unknown method: {method_name}")]
    _UnknownMethod { method_name: String },
    #[error("Missing argument at position {position} (name: {argument_name}")]
    _MissingArgument { argument_name: String, position: usize },
    #[error("Invalid sig, TODO: fill in deets")]
    InvalidSignature,
    #[error("Peer sent an invalid message: {0}")]
    InvalidPeerMessage(String),
    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),
    #[error("Metadata was malformed: {0}")]
    MalformedMetadata(String),
    #[error("Could not convert between types:{0}")]
    ConversionError(String),
    #[error("Branched to an unexpected logic path, this is most likely due to a bug:{reason}")]
    InvalidLogicPath { reason: String },
    #[error("Could not decode protobuf message for {message_type}:{source}")]
    ProtoBufDecodeError {
        source: prost::DecodeError,
        message_type: String,
    },
    #[error("Could not encode protobuf message for {message_type}:{source}")]
    ProtoBufEncodeError {
        source: prost::EncodeError,
        message_type: String,
    },
    #[error("Arithmetic overflow")]
    Overflow,
    #[error("Not enough funds")]
    NotEnoughFunds,
    #[error("Entity {entity}:{id} was not found")]
    NotFound { entity: &'static str, id: String },
    #[error("Not authorised: {0}")]
    NotAuthorised(String),
    #[error("Database is missing or has not be created")]
    MissingDatabase,
    #[error("There was no committee for the asset")]
    NoCommitteeForAsset,
    #[error("None of the committee responded")]
    NoResponsesFromCommittee,
    #[error("Fatal error: {0}")]
    FatalError(String),
    #[error(transparent)]
    ModelError(#[from] ModelError),
    #[error("UTXO missing checkpoint data")]
    UtxoNoCheckpointData,
    #[error("Failed to synchronize state: {0}")]
    StateSyncError(#[from] StateSyncError),
    #[error("Validator node client error: {0}")]
    ValidatorNodeClientError(#[from] ValidatorNodeClientError),
    #[error("Peer did not send a quorum certificate in prepare phase")]
    PreparePhaseNoQuorumCertificate,
    #[error("Quorum certificate does not extend node")]
    PreparePhaseCertificateDoesNotExtendNode,
    #[error("Node not safe")]
    PreparePhaseNodeNotSafe,
    #[error("Unsupported template method {name}")]
    TemplateUnsupportedMethod { name: String },
}

impl From<lmdb_zero::Error> for DigitalAssetError {
    fn from(err: lmdb_zero::Error) -> Self {
        Self::StorageError(err.into())
    }
}
