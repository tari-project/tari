// Copyright 2025. The Tari Project
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

use std::fmt::Display;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TxSubmissionResponse {
    pub accepted: bool,
    pub rejection_reason: TxSubmissionRejectionReason,
    pub is_synced: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TxSubmissionResponseV1 {
    pub accepted: bool,
    pub rejection_reason: TxSubmissionRejectionReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection_reason_details: Option<String>,
    pub is_synced: bool,
}

impl From<TxSubmissionResponseV1> for TxSubmissionResponse {
    fn from(value: TxSubmissionResponseV1) -> Self {
        Self {
            accepted: value.accepted,
            rejection_reason: value.rejection_reason,
            is_synced: value.is_synced,
        }
    }
}

impl From<TxSubmissionResponse> for TxSubmissionResponseV1 {
    fn from(value: TxSubmissionResponse) -> Self {
        Self {
            accepted: value.accepted,
            rejection_reason: value.rejection_reason,
            rejection_reason_details: None,
            is_synced: value.is_synced,
        }
    }
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn deserializes_submission_response_v1_without_details() {
        let response = serde_json::json!({
            "accepted": false,
            "rejection_reason": "TimeLocked",
            "is_synced": true
        });

        let response = serde_json::from_value::<TxSubmissionResponseV1>(response).unwrap();

        assert!(!response.accepted);
        assert_eq!(response.rejection_reason, TxSubmissionRejectionReason::TimeLocked);
        assert!(response.rejection_reason_details.is_none());
        assert!(response.is_synced);
    }

    #[test]
    fn serializes_submission_response_v1_details_when_present() {
        let response = TxSubmissionResponseV1 {
            accepted: false,
            rejection_reason: TxSubmissionRejectionReason::DoubleSpend,
            rejection_reason_details: Some("Transaction spends an output that is already spent.".to_string()),
            is_synced: true,
        };

        let value = serde_json::to_value(response).unwrap();

        assert_eq!(
            value["rejection_reason_details"],
            "Transaction spends an output that is already spent."
        );
    }

    #[test]
    fn serializes_submission_response_without_v1_details() {
        let response = TxSubmissionResponse {
            accepted: false,
            rejection_reason: TxSubmissionRejectionReason::DoubleSpend,
            is_synced: true,
        };

        let value = serde_json::to_value(response).unwrap();

        assert!(value.get("rejection_reason_details").is_none());
    }

    #[test]
    fn converts_submission_response_v1_to_legacy_response() {
        let response = TxSubmissionResponseV1 {
            accepted: false,
            rejection_reason: TxSubmissionRejectionReason::DoubleSpend,
            rejection_reason_details: Some("already spent".to_string()),
            is_synced: true,
        };

        let legacy = TxSubmissionResponse::from(response);

        assert!(!legacy.accepted);
        assert_eq!(legacy.rejection_reason, TxSubmissionRejectionReason::DoubleSpend);
        assert!(legacy.is_synced);
    }
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
