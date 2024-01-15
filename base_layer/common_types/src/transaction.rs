// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    convert::TryFrom,
    fmt,
    fmt::{Display, Error, Formatter},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use crate::tx_id::TxId;

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionStatus {
    /// This transaction has been completed between the parties but has not been broadcast to the base layer network.
    Completed = 0,
    /// This transaction has been broadcast to the base layer network and is currently in one or more base node
    /// mempools.
    Broadcast = 1,
    /// This transaction has been mined and included in a block.
    MinedUnconfirmed = 2,
    /// This transaction was generated as part of importing a spendable unblinded UTXO
    Imported = 3,
    /// This transaction is still being negotiated by the parties
    #[default]
    Pending = 4,
    /// This is a created Coinbase Transaction
    Coinbase = 5,
    /// This transaction is mined and confirmed at the current base node's height
    MinedConfirmed = 6,
    /// This transaction was Rejected by the mempool
    Rejected = 7,
    /// This transaction import status is used when a one-sided transaction has been scanned but is unconfirmed
    OneSidedUnconfirmed = 8,
    /// This transaction import status is used when a one-sided transaction has been scanned and confirmed
    OneSidedConfirmed = 9,
    /// This transaction is still being queued for initial sending
    Queued = 10,
    /// This transaction import status is used when a coinbase transaction has been scanned but is unconfirmed
    CoinbaseUnconfirmed = 11,
    /// This transaction import status is used when a coinbase transaction has been scanned and confirmed
    CoinbaseConfirmed = 12,
    /// This transaction import status is used when a coinbase transaction has been scanned but the outputs are not
    /// currently confirmed on the blockchain via the output manager
    CoinbaseNotInBlockChain = 13,
}

impl TransactionStatus {
    pub fn is_imported_from_chain(&self) -> bool {
        matches!(
            self,
            TransactionStatus::Imported | TransactionStatus::OneSidedUnconfirmed | TransactionStatus::OneSidedConfirmed
        )
    }

    pub fn is_coinbase(&self) -> bool {
        matches!(
            self,
            TransactionStatus::CoinbaseUnconfirmed |
                TransactionStatus::CoinbaseConfirmed |
                TransactionStatus::CoinbaseNotInBlockChain
        )
    }

    pub fn is_confirmed(&self) -> bool {
        matches!(
            self,
            TransactionStatus::OneSidedConfirmed |
                TransactionStatus::CoinbaseConfirmed |
                TransactionStatus::MinedConfirmed
        )
    }

    pub fn mined_confirm(&self) -> Self {
        match self {
            TransactionStatus::Completed |
            TransactionStatus::Broadcast |
            TransactionStatus::Pending |
            TransactionStatus::Coinbase |
            TransactionStatus::Rejected |
            TransactionStatus::Queued |
            TransactionStatus::MinedUnconfirmed |
            TransactionStatus::MinedConfirmed => TransactionStatus::MinedConfirmed,
            TransactionStatus::Imported |
            TransactionStatus::OneSidedUnconfirmed |
            TransactionStatus::OneSidedConfirmed => TransactionStatus::OneSidedConfirmed,
            TransactionStatus::CoinbaseNotInBlockChain |
            TransactionStatus::CoinbaseConfirmed |
            TransactionStatus::CoinbaseUnconfirmed => TransactionStatus::CoinbaseConfirmed,
        }
    }

    pub fn mined_unconfirm(&self) -> Self {
        match self {
            TransactionStatus::Completed |
            TransactionStatus::Broadcast |
            TransactionStatus::Pending |
            TransactionStatus::Coinbase |
            TransactionStatus::Rejected |
            TransactionStatus::Queued |
            TransactionStatus::MinedUnconfirmed |
            TransactionStatus::MinedConfirmed => TransactionStatus::MinedUnconfirmed,
            TransactionStatus::Imported |
            TransactionStatus::OneSidedUnconfirmed |
            TransactionStatus::OneSidedConfirmed => TransactionStatus::OneSidedUnconfirmed,
            TransactionStatus::CoinbaseConfirmed |
            TransactionStatus::CoinbaseUnconfirmed |
            TransactionStatus::CoinbaseNotInBlockChain => TransactionStatus::CoinbaseUnconfirmed,
        }
    }
}

#[derive(Debug, Error)]
#[error("Invalid TransactionStatus: {code}")]
pub struct TransactionConversionError {
    pub code: i32,
}

impl TryFrom<i32> for TransactionStatus {
    type Error = TransactionConversionError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TransactionStatus::Completed),
            1 => Ok(TransactionStatus::Broadcast),
            2 => Ok(TransactionStatus::MinedUnconfirmed),
            3 => Ok(TransactionStatus::Imported),
            4 => Ok(TransactionStatus::Pending),
            5 => Ok(TransactionStatus::Coinbase),
            6 => Ok(TransactionStatus::MinedConfirmed),
            7 => Ok(TransactionStatus::Rejected),
            8 => Ok(TransactionStatus::OneSidedUnconfirmed),
            9 => Ok(TransactionStatus::OneSidedConfirmed),
            10 => Ok(TransactionStatus::Queued),
            11 => Ok(TransactionStatus::CoinbaseUnconfirmed),
            12 => Ok(TransactionStatus::CoinbaseConfirmed),
            13 => Ok(TransactionStatus::CoinbaseNotInBlockChain),
            code => Err(TransactionConversionError { code }),
        }
    }
}

impl Display for TransactionStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        // No struct or tuple variants
        match self {
            TransactionStatus::Completed => write!(f, "Completed"),
            TransactionStatus::Broadcast => write!(f, "Broadcast"),
            TransactionStatus::MinedUnconfirmed => write!(f, "Mined Unconfirmed"),
            TransactionStatus::MinedConfirmed => write!(f, "Mined Confirmed"),
            TransactionStatus::Imported => write!(f, "Imported"),
            TransactionStatus::Pending => write!(f, "Pending"),
            TransactionStatus::Coinbase => write!(f, "Coinbase"),
            TransactionStatus::Rejected => write!(f, "Rejected"),
            TransactionStatus::OneSidedUnconfirmed => write!(f, "One-Sided Unconfirmed"),
            TransactionStatus::OneSidedConfirmed => write!(f, "One-Sided Confirmed"),
            TransactionStatus::CoinbaseUnconfirmed => write!(f, "Coinbase Unconfirmed"),
            TransactionStatus::CoinbaseConfirmed => write!(f, "Coinbase Confirmed"),
            TransactionStatus::CoinbaseNotInBlockChain => write!(f, "Coinbase not mined"),
            TransactionStatus::Queued => write!(f, "Queued"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportStatus {
    /// This transaction import status is used when importing a spendable UTXO
    Imported,
    /// This transaction import status is used when a one-sided transaction has been scanned but is unconfirmed
    OneSidedUnconfirmed,
    /// This transaction import status is used when a one-sided transaction has been scanned and confirmed
    OneSidedConfirmed,
    /// This transaction import status is used when a coinbasetransaction has been scanned but is unconfirmed
    CoinbaseUnconfirmed,
    /// This transaction import status is used when a coinbase transaction has been scanned and confirmed
    CoinbaseConfirmed,
}

impl TryFrom<ImportStatus> for TransactionStatus {
    type Error = TransactionConversionError;

    fn try_from(value: ImportStatus) -> Result<Self, Self::Error> {
        match value {
            ImportStatus::Imported => Ok(TransactionStatus::Imported),
            ImportStatus::OneSidedUnconfirmed => Ok(TransactionStatus::OneSidedUnconfirmed),
            ImportStatus::OneSidedConfirmed => Ok(TransactionStatus::OneSidedConfirmed),
            ImportStatus::CoinbaseUnconfirmed => Ok(TransactionStatus::CoinbaseUnconfirmed),
            ImportStatus::CoinbaseConfirmed => Ok(TransactionStatus::CoinbaseConfirmed),
        }
    }
}

impl TryFrom<TransactionStatus> for ImportStatus {
    type Error = TransactionConversionError;

    fn try_from(value: TransactionStatus) -> Result<Self, Self::Error> {
        match value {
            TransactionStatus::Imported => Ok(ImportStatus::Imported),
            TransactionStatus::OneSidedUnconfirmed => Ok(ImportStatus::OneSidedUnconfirmed),
            TransactionStatus::OneSidedConfirmed => Ok(ImportStatus::OneSidedConfirmed),
            TransactionStatus::CoinbaseUnconfirmed => Ok(ImportStatus::CoinbaseUnconfirmed),
            TransactionStatus::CoinbaseConfirmed => Ok(ImportStatus::CoinbaseConfirmed),
            _ => Err(TransactionConversionError { code: i32::MAX }),
        }
    }
}

impl fmt::Display for ImportStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            ImportStatus::Imported => write!(f, "Imported"),
            ImportStatus::OneSidedUnconfirmed => write!(f, "OneSidedUnconfirmed"),
            ImportStatus::OneSidedConfirmed => write!(f, "OneSidedConfirmed"),
            ImportStatus::CoinbaseUnconfirmed => write!(f, "CoinbaseUnconfirmed"),
            ImportStatus::CoinbaseConfirmed => write!(f, "CoinbaseConfirmed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransactionDirection {
    Inbound,
    Outbound,
    Unknown,
}

#[derive(Debug, Error)]
#[error("Invalid TransactionDirection: {code}")]
pub struct TransactionDirectionError {
    pub code: i32,
}

impl TryFrom<i32> for TransactionDirection {
    type Error = TransactionDirectionError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TransactionDirection::Inbound),
            1 => Ok(TransactionDirection::Outbound),
            2 => Ok(TransactionDirection::Unknown),
            code => Err(TransactionDirectionError { code }),
        }
    }
}

impl Display for TransactionDirection {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        // No struct or tuple variants
        match self {
            TransactionDirection::Inbound => write!(f, "Inbound"),
            TransactionDirection::Outbound => write!(f, "Outbound"),
            TransactionDirection::Unknown => write!(f, "Unknown"),
        }
    }
}
