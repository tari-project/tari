use std::{
    convert::TryFrom,
    fmt::{Display, Error, Formatter},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use crate::tx_id::TxId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionStatus {
    /// This transaction has been completed between the parties but has not been broadcast to the base layer network.
    Completed,
    /// This transaction has been broadcast to the base layer network and is currently in one or more base node
    /// mempools.
    Broadcast,
    /// This transaction has been mined and included in a block.
    MinedUnconfirmed,
    /// This transaction was generated as part of importing a spendable unblinded UTXO
    Imported,
    /// This transaction is still being negotiated by the parties
    Pending,
    /// This is a created Coinbase Transaction
    Coinbase,
    /// This transaction is mined and confirmed at the current base node's height
    MinedConfirmed,
    /// This transaction was Rejected by the mempool
    Rejected,
    /// This is faux transaction mainly for one-sided transaction outputs or wallet recovery outputs have been found
    FauxUnconfirmed,
    /// All Imported and FauxUnconfirmed transactions will end up with this status when the outputs have been confirmed
    FauxConfirmed,
}

impl TransactionStatus {
    pub fn is_faux(&self) -> bool {
        matches!(
            self,
            TransactionStatus::Imported | TransactionStatus::FauxUnconfirmed | TransactionStatus::FauxConfirmed
        )
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
            8 => Ok(TransactionStatus::FauxUnconfirmed),
            9 => Ok(TransactionStatus::FauxConfirmed),
            code => Err(TransactionConversionError { code }),
        }
    }
}

impl Default for TransactionStatus {
    fn default() -> Self {
        TransactionStatus::Pending
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
            TransactionStatus::FauxUnconfirmed => write!(f, "FauxUnconfirmed"),
            TransactionStatus::FauxConfirmed => write!(f, "FauxConfirmed"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ImportStatus {
    /// This transaction import status is used when importing a spendable UTXO
    Imported,
    /// This transaction import status is used when a one-sided transaction has been scanned but is unconfirmed
    FauxUnconfirmed,
    /// This transaction import status is used when a one-sided transaction has been scanned and confirmed
    FauxConfirmed,
}

impl TryFrom<ImportStatus> for TransactionStatus {
    type Error = TransactionConversionError;

    fn try_from(value: ImportStatus) -> Result<Self, Self::Error> {
        match value {
            ImportStatus::Imported => Ok(TransactionStatus::Imported),
            ImportStatus::FauxUnconfirmed => Ok(TransactionStatus::FauxUnconfirmed),
            ImportStatus::FauxConfirmed => Ok(TransactionStatus::FauxConfirmed),
        }
    }
}

impl TryFrom<TransactionStatus> for ImportStatus {
    type Error = TransactionConversionError;

    fn try_from(value: TransactionStatus) -> Result<Self, Self::Error> {
        match value {
            TransactionStatus::Imported => Ok(ImportStatus::Imported),
            TransactionStatus::FauxUnconfirmed => Ok(ImportStatus::FauxUnconfirmed),
            TransactionStatus::FauxConfirmed => Ok(ImportStatus::FauxConfirmed),
            _ => Err(TransactionConversionError { code: i32::MAX }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
