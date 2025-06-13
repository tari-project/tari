use std::fmt::Display;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TxSubmissionResponse {
    pub accepted: bool,
    pub rejection_reason: TxSubmissionRejectionReason,
    pub is_synced: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxSubmissionRejectionReason {
    None,
    AlreadyMined,
    DoubleSpend,
    Orphan,
    TimeLocked,
    ValidationFailed,
    FeeTooLow,
}

impl Display for TxSubmissionRejectionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TxSubmissionRejectionReason::None => write!(f, "None"),
            TxSubmissionRejectionReason::AlreadyMined => write!(f, "Already Mined"),
            TxSubmissionRejectionReason::DoubleSpend => write!(f, "Double Spend"),
            TxSubmissionRejectionReason::Orphan => write!(f, "Orphan"),
            TxSubmissionRejectionReason::TimeLocked => write!(f, "Time Locked"),
            TxSubmissionRejectionReason::ValidationFailed => write!(f, "Validation Failed"),
            TxSubmissionRejectionReason::FeeTooLow => write!(f, "Fee Too Low"),
        }
    }
}
