// Copyright 2019. The Tari Project
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
use log::*;
use tari_comms::{
    multiaddr,
    peer_manager::{node_id::NodeIdError, NodeIdentityError},
};
use tari_comms_dht::store_forward::StoreAndForwardError;
use tari_crypto::{
    signatures::SchnorrSignatureError,
    tari_utilities::{hex::HexError, ByteArrayError},
};
use tari_wallet::{
    contacts_service::error::{ContactsServiceError, ContactsServiceStorageError},
    error::{WalletError, WalletStorageError},
    output_manager_service::error::{OutputManagerError, OutputManagerStorageError},
    transaction_service::error::{TransactionServiceError, TransactionStorageError},
};
use thiserror::Error;

const LOG_TARGET: &str = "wallet_ffi::error";

#[derive(Debug, Error, PartialEq)]
pub enum InterfaceError {
    #[error("An error has occurred due to one of the parameters being null: `{0}`")]
    NullError(String),
    #[error("An error has occurred when checking the length of the allocated object")]
    AllocationError,
    #[error("An error because the supplied position was out of range")]
    PositionInvalidError,
    #[error("An error has occurred when trying to create the tokio runtime: `{0}`")]
    TokioError(String),
    #[error("Emoji ID is invalid")]
    InvalidEmojiId,
    #[error("Comms Private Key is not present while Db appears to be encrypted which should not happen")]
    MissingCommsPrivateKey,
}

/// This struct is meant to hold an error for use by FFI client applications. The error has an integer code and string
/// message
#[derive(Debug, Clone)]
pub struct LibWalletError {
    pub code: i32,
    pub message: String,
}

impl From<InterfaceError> for LibWalletError {
    fn from(v: InterfaceError) -> Self {
        error!(target: LOG_TARGET, "{}", format!("{:?}", v));
        match v {
            InterfaceError::NullError(_) => Self {
                code: 1,
                message: format!("{:?}", v),
            },
            InterfaceError::AllocationError => Self {
                code: 2,
                message: format!("{:?}", v),
            },
            InterfaceError::PositionInvalidError => Self {
                code: 3,
                message: format!("{:?}", v),
            },
            InterfaceError::TokioError(_) => Self {
                code: 4,
                message: format!("{:?}", v),
            },
            InterfaceError::InvalidEmojiId => Self {
                code: 6,
                message: format!("{:?}", v),
            },
            InterfaceError::MissingCommsPrivateKey => Self {
                code: 7,
                message: format!("{:?}", v),
            },
        }
    }
}

/// This implementation maps the internal WalletError to a set of LibWalletErrors. The mapping is explicitly managed
/// here and error code 999 is a catch-all code for any errors that are not explicitly mapped
impl From<WalletError> for LibWalletError {
    fn from(w: WalletError) -> Self {
        error!(target: LOG_TARGET, "{}", format!("{:?}", w));
        match w {
            // Output Manager Service Errors
            WalletError::OutputManagerError(OutputManagerError::NotEnoughFunds) => Self {
                code: 101,
                message: format!("{:?}", w),
            },
            WalletError::OutputManagerError(OutputManagerError::FundsPending) => Self {
                code: 115,
                message: format!("{:?}", w),
            },
            WalletError::OutputManagerError(OutputManagerError::IncompleteTransaction(_)) => Self {
                code: 102,
                message: format!("{:?}", w),
            },
            WalletError::OutputManagerError(OutputManagerError::DuplicateOutput) => Self {
                code: 103,
                message: format!("{:?}", w),
            },
            WalletError::TransactionServiceError(TransactionServiceError::TransactionStorageError(
                TransactionStorageError::DuplicateOutput,
            )) => Self {
                code: 103,
                message: format!("{:?}", w),
            },
            WalletError::OutputManagerError(OutputManagerError::OutputManagerStorageError(
                OutputManagerStorageError::ValuesNotFound,
            )) => Self {
                code: 104,
                message: format!("{:?}", w),
            },
            WalletError::OutputManagerError(OutputManagerError::OutputManagerStorageError(
                OutputManagerStorageError::OutputAlreadySpent,
            )) => Self {
                code: 105,
                message: format!("{:?}", w),
            },
            WalletError::OutputManagerError(OutputManagerError::OutputManagerStorageError(
                OutputManagerStorageError::PendingTransactionNotFound,
            )) => Self {
                code: 106,
                message: format!("{:?}", w),
            },
            WalletError::OutputManagerError(OutputManagerError::OutputManagerStorageError(
                OutputManagerStorageError::ValueNotFound(_),
            )) => Self {
                code: 108,
                message: format!("{:?}", w),
            },
            WalletError::OutputManagerError(OutputManagerError::NoBaseNodeKeysProvided) => Self {
                code: 109,
                message: format!("{:?}", w),
            },
            WalletError::ContactsServiceError(ContactsServiceError::ContactsServiceStorageError(
                ContactsServiceStorageError::ValuesNotFound,
            )) => Self {
                code: 110,
                message: format!("{:?}", w),
            },
            WalletError::TransactionServiceError(TransactionServiceError::TransactionStorageError(
                TransactionStorageError::ValueNotFound(_),
            )) => Self {
                code: 111,
                message: format!("{:?}", w),
            },
            WalletError::OutputManagerError(OutputManagerError::OutputManagerStorageError(
                OutputManagerStorageError::DuplicateOutput,
            )) => Self {
                code: 112,
                message: format!("{:?}", w),
            },
            WalletError::TransactionServiceError(TransactionServiceError::OutputManagerError(
                OutputManagerError::NotEnoughFunds,
            )) => Self {
                code: 113,
                message: format!("{:?}", w),
            },
            WalletError::OutputManagerError(_) => Self {
                code: 114,
                message: format!("{:?}", w),
            },
            // Transaction Service Errors
            WalletError::TransactionServiceError(TransactionServiceError::InvalidStateError) => Self {
                code: 201,
                message: format!("{:?}", w),
            },
            WalletError::TransactionServiceError(TransactionServiceError::TransactionProtocolError(_)) => Self {
                code: 202,
                message: format!("{:?}", w),
            },
            WalletError::TransactionServiceError(TransactionServiceError::RepeatedMessageError) => Self {
                code: 203,
                message: format!("{:?}", w),
            },
            WalletError::TransactionServiceError(TransactionServiceError::TransactionDoesNotExistError) => Self {
                code: 204,
                message: format!("{:?}", w),
            },
            WalletError::TransactionServiceError(TransactionServiceError::OutputManagerError(_)) => Self {
                code: 206,
                message: format!("{:?}", w),
            },
            WalletError::TransactionServiceError(TransactionServiceError::TransactionError(_)) => Self {
                code: 207,
                message: format!("{:?}", w),
            },
            WalletError::TransactionServiceError(TransactionServiceError::OutboundSendDiscoveryInProgress(_)) => Self {
                code: 210,
                message: format!("{:?}", w),
            },
            WalletError::TransactionServiceError(_) => Self {
                code: 211,
                message: format!("{:?}", w),
            },
            // Comms Stack errors
            WalletError::MultiaddrError(_) => Self {
                code: 301,
                message: format!("{:?}", w),
            },
            WalletError::StoreAndForwardError(_) => Self {
                code: 302,
                message: format!("{:?}", w),
            },
            WalletError::ContactsServiceError(ContactsServiceError::ContactNotFound) => Self {
                code: 401,
                message: format!("{:?}", w),
            },
            WalletError::ContactsServiceError(ContactsServiceError::ContactsServiceStorageError(
                ContactsServiceStorageError::OperationNotSupported,
            )) => Self {
                code: 403,
                message: format!("{:?}", w),
            },
            WalletError::ContactsServiceError(ContactsServiceError::ContactsServiceStorageError(
                ContactsServiceStorageError::ConversionError,
            )) => Self {
                code: 404,
                message: format!("{:?}", w),
            },
            // Wallet Encryption Errors
            WalletError::WalletStorageError(WalletStorageError::InvalidEncryptionCipher) => Self {
                code: 420,
                message: format!("{:?}", w),
            },
            WalletError::WalletStorageError(WalletStorageError::MissingNonce) => Self {
                code: 421,
                message: format!("{:?}", w),
            },
            WalletError::WalletStorageError(WalletStorageError::AlreadyEncrypted) => Self {
                code: 422,
                message: format!("{:?}", w),
            },
            WalletError::WalletStorageError(WalletStorageError::AeadError(_)) => Self {
                code: 423,
                message: format!("{:?}", w),
            },
            WalletError::WalletStorageError(WalletStorageError::ValuesNotFound) => Self {
                code: 424,
                message: format!("{:?}", w),
            },
            WalletError::WalletStorageError(WalletStorageError::CannotAcquireFileLock) => Self {
                code: 425,
                message: format!("{:?}", w),
            },
            WalletError::WalletStorageError(WalletStorageError::NoPasswordError) => Self {
                code: 426,
                message: format!("{:?}", w),
            },
            // This is the catch all error code. Any error that is not explicitly mapped above will be given this code
            _ => Self {
                code: 999,
                message: format!("{:?}", w),
            },
        }
    }
}

/// This implementation maps the internal HexError to a set of LibWalletErrors.
/// The mapping is explicitly managed here.
impl From<HexError> for LibWalletError {
    fn from(h: HexError) -> Self {
        error!(target: LOG_TARGET, "{}", format!("{:?}", h));
        match h {
            HexError::HexConversionError => Self {
                code: 404,
                message: format!("{:?}", h),
            },
            HexError::LengthError => Self {
                code: 501,
                message: format!("{:?}", h),
            },
            HexError::InvalidCharacter(_) => Self {
                code: 503,
                message: format!("{:?}", h),
            },
        }
    }
}

/// This implementation maps the internal ByteArrayError to a set of LibWalletErrors.
/// The mapping is explicitly managed here.
impl From<ByteArrayError> for LibWalletError {
    fn from(b: ByteArrayError) -> Self {
        error!(target: LOG_TARGET, "{}", format!("{:?}", b));
        match b {
            ByteArrayError::ConversionError(_) => Self {
                code: 404,
                message: format!("{:?}", b),
            },
            ByteArrayError::IncorrectLength => Self {
                code: 601,
                message: format!("{:?}", b),
            },
        }
    }
}

impl From<NodeIdentityError> for LibWalletError {
    fn from(n: NodeIdentityError) -> Self {
        error!(target: LOG_TARGET, "{}", format!("{:?}", n));
        match n {
            NodeIdentityError::NodeIdError(NodeIdError::IncorrectByteCount) => Self {
                code: 701,
                message: format!("{:?}", n),
            },
            // No longer applicable:
            // 702 NodeIdentityError::OutOfBounds
            // 703 NodeIdentityError::AddressLockPoisoned
            NodeIdentityError::NodeIdError(NodeIdError::InvalidDigestOutputSize) => Self {
                code: 704,
                message: format!("{:?}", n),
            },
        }
    }
}

impl From<multiaddr::Error> for LibWalletError {
    fn from(err: multiaddr::Error) -> Self {
        error!(target: LOG_TARGET, "{}", format!("{:?}", err));
        match err {
            multiaddr::Error::ParsingError(_) => Self {
                code: 801,
                message: format!("{:?}", err),
            },
            multiaddr::Error::InvalidMultiaddr => Self {
                code: 802,
                message: format!("{:?}", err),
            },
            multiaddr::Error::DataLessThanLen => Self {
                code: 803,
                message: format!("{:?}", err),
            },
            multiaddr::Error::InvalidProtocolString => Self {
                code: 804,
                message: format!("{:?}", err),
            },
            multiaddr::Error::UnknownProtocolString(_) => Self {
                code: 805,
                message: format!("{:?}", err),
            },
            multiaddr::Error::InvalidUvar(_) => Self {
                code: 806,
                message: format!("{:?}", err),
            },
            err => Self {
                code: 810,
                message: format!("Multiaddr error: {:?}", err),
            },
        }
    }
}

impl From<SchnorrSignatureError> for LibWalletError {
    fn from(err: SchnorrSignatureError) -> Self {
        error!(target: LOG_TARGET, "{}", format!("{:?}", err));
        match err {
            SchnorrSignatureError::InvalidChallenge => Self {
                code: 901,
                message: format!("{:?}", err),
            },
        }
    }
}

impl From<StoreAndForwardError> for LibWalletError {
    fn from(err: StoreAndForwardError) -> Self {
        error!(target: LOG_TARGET, "{}", format!("{:?}", err));
        Self {
            code: 902,
            message: format!("{:?}", err),
        }
    }
}
#[derive(Debug, Error, PartialEq)]
pub enum TransactionError {
    #[error("The transaction has an incorrect status: `{0}`")]
    StatusError(String),
    #[error("The transaction has the wrong number of kernels: `{0}`")]
    KernelError(String),
}

/// This implementation maps the internal TransactionError to a set of LibWalletErrors.
/// The mapping is explicitly managed here.
impl From<TransactionError> for LibWalletError {
    fn from(v: TransactionError) -> Self {
        error!(target: LOG_TARGET, "{}", v);
        match v {
            TransactionError::StatusError(_) => Self {
                code: 640,
                message: v.to_string(),
            },
            TransactionError::KernelError(_) => Self {
                code: 650,
                message: format!("{:?}", v),
            },
        }
    }
}
