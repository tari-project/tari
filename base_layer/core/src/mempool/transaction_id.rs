// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::fmt::Display;

use tari_common_types::types::Signature;
use tari_utilities::ByteArray;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct MempoolTransactionId([u8; MempoolTransactionId::byte_len()]);

impl MempoolTransactionId {
    pub const fn byte_len() -> usize {
        32
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&[u8]> for MempoolTransactionId {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != Self::byte_len() {
            return Err(());
        }
        let mut id = [0u8; Self::byte_len()];
        id.copy_from_slice(value);
        Ok(Self(id))
    }
}

impl From<&Signature> for MempoolTransactionId {
    fn from(sig: &Signature) -> Self {
        Self::try_from(sig.get_signature().as_bytes())
            .expect("From<Signature> for MempoolTransactionId: Signature bytes expected to be 32")
    }
}

impl Display for MempoolTransactionId {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        for byte in self.as_bytes() {
            write!(fmt, "{:02x}", byte)?;
        }
        Ok(())
    }
}
