use serde::{Deserialize, Serialize};
use std::{
    convert::TryFrom,
    fmt::{Display, Error, Formatter},
};
use thiserror::Error;

pub type TxId = u64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionStatus {
    /// This transaction has been completed between the parties but has not been broadcast to the base layer network.
    Completed,
    /// This transaction has been broadcast to the base layer network and is currently in one or more base node
    /// mempools.
    Broadcast,
    /// This transaction has been mined and included in a block.
    MinedUnconfirmed,
    /// This transaction was generated as part of importing a spendable UTXO
    Imported,
    /// This transaction is still being negotiated by the parties
    Pending,
    /// This is a created Coinbase Transaction
    Coinbase,
    /// This transaction is mined and confirmed at the current base node's height
    MinedConfirmed,
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
        }
    }
}
