use serde::{Deserialize, Serialize};
use tari_common_types::types::CompressedCommitment;

use crate::transaction_components::{KernelFeatures, MicroMinotari};

/// Transaction metadata, this includes all the fields that needs to be signed on the kernel
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct TransactionMetadata {
    /// The absolute fee for the transaction
    pub fee: MicroMinotari,
    /// The earliest block this transaction can be mined
    pub lock_height: u64,
    /// The kernel features
    pub kernel_features: KernelFeatures,
    /// optional burn commitment if present
    pub burn_commitment: Option<CompressedCommitment>,
}

impl TransactionMetadata {
    pub fn new(fee: MicroMinotari, lock_height: u64) -> Self {
        Self {
            fee,
            lock_height,
            kernel_features: KernelFeatures::default(),
            burn_commitment: None,
        }
    }

    pub fn new_with_features(fee: MicroMinotari, lock_height: u64, kernel_features: KernelFeatures) -> Self {
        Self {
            fee,
            lock_height,
            kernel_features,
            burn_commitment: None,
        }
    }
}
