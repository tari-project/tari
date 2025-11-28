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

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LegacyTransactionStatus {
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

impl LegacyTransactionStatus {
    pub fn is_imported_from_chain(&self) -> bool {
        matches!(
            self,
            LegacyTransactionStatus::Imported |
                LegacyTransactionStatus::OneSidedUnconfirmed |
                LegacyTransactionStatus::OneSidedConfirmed
        )
    }

    pub fn is_coinbase(&self) -> bool {
        matches!(
            self,
            LegacyTransactionStatus::CoinbaseUnconfirmed |
                LegacyTransactionStatus::CoinbaseConfirmed |
                LegacyTransactionStatus::CoinbaseNotInBlockChain
        )
    }

    pub fn is_broadcast(&self) -> bool {
        matches!(self, LegacyTransactionStatus::Broadcast)
    }

    pub fn is_confirmed(&self) -> bool {
        matches!(
            self,
            LegacyTransactionStatus::OneSidedConfirmed |
                LegacyTransactionStatus::CoinbaseConfirmed |
                LegacyTransactionStatus::MinedConfirmed
        )
    }

    pub fn is_mined(&self) -> bool {
        matches!(
            self,
            LegacyTransactionStatus::MinedUnconfirmed |
                LegacyTransactionStatus::MinedConfirmed |
                LegacyTransactionStatus::CoinbaseConfirmed |
                LegacyTransactionStatus::CoinbaseUnconfirmed |
                LegacyTransactionStatus::OneSidedConfirmed |
                LegacyTransactionStatus::OneSidedUnconfirmed
        )
    }

    pub fn mined_confirm(&self) -> Self {
        match self {
            LegacyTransactionStatus::Completed |
            LegacyTransactionStatus::Broadcast |
            LegacyTransactionStatus::Pending |
            LegacyTransactionStatus::Coinbase |
            LegacyTransactionStatus::Rejected |
            LegacyTransactionStatus::Queued |
            LegacyTransactionStatus::MinedUnconfirmed |
            LegacyTransactionStatus::MinedConfirmed => LegacyTransactionStatus::MinedConfirmed,
            LegacyTransactionStatus::Imported |
            LegacyTransactionStatus::OneSidedUnconfirmed |
            LegacyTransactionStatus::OneSidedConfirmed => LegacyTransactionStatus::OneSidedConfirmed,
            LegacyTransactionStatus::CoinbaseNotInBlockChain |
            LegacyTransactionStatus::CoinbaseConfirmed |
            LegacyTransactionStatus::CoinbaseUnconfirmed => LegacyTransactionStatus::CoinbaseConfirmed,
        }
    }

    pub fn mined_unconfirm(&self) -> Self {
        match self {
            LegacyTransactionStatus::Completed |
            LegacyTransactionStatus::Broadcast |
            LegacyTransactionStatus::Pending |
            LegacyTransactionStatus::Coinbase |
            LegacyTransactionStatus::Rejected |
            LegacyTransactionStatus::Queued |
            LegacyTransactionStatus::MinedUnconfirmed |
            LegacyTransactionStatus::MinedConfirmed => LegacyTransactionStatus::MinedUnconfirmed,
            LegacyTransactionStatus::Imported |
            LegacyTransactionStatus::OneSidedUnconfirmed |
            LegacyTransactionStatus::OneSidedConfirmed => LegacyTransactionStatus::OneSidedUnconfirmed,
            LegacyTransactionStatus::CoinbaseConfirmed |
            LegacyTransactionStatus::CoinbaseUnconfirmed |
            LegacyTransactionStatus::CoinbaseNotInBlockChain => LegacyTransactionStatus::CoinbaseUnconfirmed,
        }
    }
}

#[derive(Debug, Error)]
#[error("Invalid TransactionStatus: {code}")]
pub struct TransactionConversionError {
    pub code: i32,
}

impl TryFrom<i32> for LegacyTransactionStatus {
    type Error = TransactionConversionError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(LegacyTransactionStatus::Completed),
            1 => Ok(LegacyTransactionStatus::Broadcast),
            2 => Ok(LegacyTransactionStatus::MinedUnconfirmed),
            3 => Ok(LegacyTransactionStatus::Imported),
            4 => Ok(LegacyTransactionStatus::Pending),
            5 => Ok(LegacyTransactionStatus::Coinbase),
            6 => Ok(LegacyTransactionStatus::MinedConfirmed),
            7 => Ok(LegacyTransactionStatus::Rejected),
            8 => Ok(LegacyTransactionStatus::OneSidedUnconfirmed),
            9 => Ok(LegacyTransactionStatus::OneSidedConfirmed),
            10 => Ok(LegacyTransactionStatus::Queued),
            11 => Ok(LegacyTransactionStatus::CoinbaseUnconfirmed),
            12 => Ok(LegacyTransactionStatus::CoinbaseConfirmed),
            13 => Ok(LegacyTransactionStatus::CoinbaseNotInBlockChain),
            code => Err(TransactionConversionError { code }),
        }
    }
}

impl Display for LegacyTransactionStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        // No struct or tuple variants
        match self {
            LegacyTransactionStatus::Completed => write!(f, "Completed"),
            LegacyTransactionStatus::Broadcast => write!(f, "Broadcast"),
            LegacyTransactionStatus::MinedUnconfirmed => write!(f, "Mined Unconfirmed"),
            LegacyTransactionStatus::MinedConfirmed => write!(f, "Mined Confirmed"),
            LegacyTransactionStatus::Imported => write!(f, "Imported"),
            LegacyTransactionStatus::Pending => write!(f, "Pending"),
            LegacyTransactionStatus::Coinbase => write!(f, "Coinbase"),
            LegacyTransactionStatus::Rejected => write!(f, "Rejected"),
            LegacyTransactionStatus::OneSidedUnconfirmed => write!(f, "One-Sided Unconfirmed"),
            LegacyTransactionStatus::OneSidedConfirmed => write!(f, "One-Sided Confirmed"),
            LegacyTransactionStatus::CoinbaseUnconfirmed => write!(f, "Coinbase Unconfirmed"),
            LegacyTransactionStatus::CoinbaseConfirmed => write!(f, "Coinbase Confirmed"),
            LegacyTransactionStatus::CoinbaseNotInBlockChain => write!(f, "Coinbase not mined"),
            LegacyTransactionStatus::Queued => write!(f, "Queued"),
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionStatus {
    /// This transaction has been completed between the parties but has not been broadcast to the base layer network.
    #[default]
    Completed = 0,
    /// This transaction has been broadcast to the base layer network and is currently in one or more base node
    /// mempools.
    Broadcast = 1,
    /// This transaction has been mined and included in a block.
    MinedUnconfirmed = 2,
    /// This transaction is mined and confirmed at the current base node's height
    MinedConfirmed = 3,
    /// This transaction was Rejected by the mempool
    Rejected = 4,
}

impl TransactionStatus {
    pub fn is_confirmed(&self) -> bool {
        matches!(self, TransactionStatus::MinedConfirmed)
    }

    pub fn is_mined(&self) -> bool {
        matches!(
            self,
            TransactionStatus::MinedUnconfirmed | TransactionStatus::MinedConfirmed
        )
    }
}

impl TryFrom<i32> for TransactionStatus {
    type Error = TransactionConversionError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TransactionStatus::Completed),
            1 => Ok(TransactionStatus::Broadcast),
            2 => Ok(TransactionStatus::MinedUnconfirmed),
            3 => Ok(TransactionStatus::MinedConfirmed),
            4 => Ok(TransactionStatus::Rejected),
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
            TransactionStatus::Rejected => write!(f, "Rejected"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LegacyImportStatus {
    /// Special case where we import a tx received from broadcast
    Broadcast,
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

impl TryFrom<LegacyImportStatus> for LegacyTransactionStatus {
    type Error = TransactionConversionError;

    fn try_from(value: LegacyImportStatus) -> Result<Self, Self::Error> {
        match value {
            LegacyImportStatus::Broadcast => Ok(LegacyTransactionStatus::Broadcast),
            LegacyImportStatus::Imported => Ok(LegacyTransactionStatus::Imported),
            LegacyImportStatus::OneSidedUnconfirmed => Ok(LegacyTransactionStatus::OneSidedUnconfirmed),
            LegacyImportStatus::OneSidedConfirmed => Ok(LegacyTransactionStatus::OneSidedConfirmed),
            LegacyImportStatus::CoinbaseUnconfirmed => Ok(LegacyTransactionStatus::CoinbaseUnconfirmed),
            LegacyImportStatus::CoinbaseConfirmed => Ok(LegacyTransactionStatus::CoinbaseConfirmed),
        }
    }
}

impl TryFrom<LegacyTransactionStatus> for LegacyImportStatus {
    type Error = TransactionConversionError;

    fn try_from(value: LegacyTransactionStatus) -> Result<Self, Self::Error> {
        match value {
            LegacyTransactionStatus::Broadcast => Ok(LegacyImportStatus::Broadcast),
            LegacyTransactionStatus::Imported => Ok(LegacyImportStatus::Imported),
            LegacyTransactionStatus::OneSidedUnconfirmed => Ok(LegacyImportStatus::OneSidedUnconfirmed),
            LegacyTransactionStatus::OneSidedConfirmed => Ok(LegacyImportStatus::OneSidedConfirmed),
            LegacyTransactionStatus::CoinbaseUnconfirmed => Ok(LegacyImportStatus::CoinbaseUnconfirmed),
            LegacyTransactionStatus::CoinbaseConfirmed => Ok(LegacyImportStatus::CoinbaseConfirmed),
            _ => Err(TransactionConversionError { code: i32::MAX }),
        }
    }
}

impl fmt::Display for LegacyImportStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            LegacyImportStatus::Broadcast => write!(f, "Broadcast"),
            LegacyImportStatus::Imported => write!(f, "Imported"),
            LegacyImportStatus::OneSidedUnconfirmed => write!(f, "OneSidedUnconfirmed"),
            LegacyImportStatus::OneSidedConfirmed => write!(f, "OneSidedConfirmed"),
            LegacyImportStatus::CoinbaseUnconfirmed => write!(f, "CoinbaseUnconfirmed"),
            LegacyImportStatus::CoinbaseConfirmed => write!(f, "CoinbaseConfirmed"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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
