//  Copyright 2022. The Tari Project
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

use minotari_ledger_wallet_common::common_types::LedgerKeyBranch;
use strum_macros::EnumIter;

use crate::WALLET_COMMS_AND_SPEND_KEY_BRANCH;

#[repr(u8)]
#[derive(Clone, Copy, EnumIter, Eq, PartialEq, Debug)]
// These byte reps must stay in sync with the ledger representations at:
// applications/minotari_ledger_wallet/wallet/src/main.rs
pub enum TransactionKeyManagerBranch {
    DataEncryption = 0x00,
    MetadataEphemeralNonce = 0x01,
    CommitmentMask = 0x02,
    Nonce = 0x03,
    KernelNonce = 0x04,
    SenderOffset = 0x05,
    OneSidedSenderOffset = 0x06,
    Spend = 0x07,
    RandomKey = 0x08,
    PreMine = 0x09,
    CodeTemplateAuthor = 0x0a,
}

pub const DATA_ENCRYPTION: &str = "data encryption";
pub const METADATA_EPHEMERAL_NONCE: &str = "metadata ephemeral nonce";
pub const COMMITMENT_MASK: &str = "commitment mask";
pub const NONCE: &str = "nonce";
pub const KERNEL_NONCE: &str = "kernel nonce";
pub const SENDER_OFFSET: &str = "sender offset";
pub const ONE_SIDED_SENDER_OFFSET: &str = "one sided sender offset";
pub const RANDOM_KEY: &str = "random key";
pub const PRE_MINE: &str = "pre-mine";
pub const CODE_TEMPLATE_AUTHOR: &str = "code-template-author";

impl TransactionKeyManagerBranch {
    /// Warning: Changing these strings will affect the backwards compatibility of the wallet with older databases or
    /// recovery.
    pub fn get_branch_key(self) -> String {
        match self {
            TransactionKeyManagerBranch::DataEncryption => DATA_ENCRYPTION.to_string(),
            TransactionKeyManagerBranch::MetadataEphemeralNonce => METADATA_EPHEMERAL_NONCE.to_string(),
            TransactionKeyManagerBranch::CommitmentMask => COMMITMENT_MASK.to_string(),
            TransactionKeyManagerBranch::Nonce => NONCE.to_string(),
            TransactionKeyManagerBranch::KernelNonce => KERNEL_NONCE.to_string(),
            TransactionKeyManagerBranch::SenderOffset => SENDER_OFFSET.to_string(),
            TransactionKeyManagerBranch::OneSidedSenderOffset => ONE_SIDED_SENDER_OFFSET.to_string(),
            TransactionKeyManagerBranch::RandomKey => RANDOM_KEY.to_string(),
            TransactionKeyManagerBranch::Spend => WALLET_COMMS_AND_SPEND_KEY_BRANCH.to_string(),
            TransactionKeyManagerBranch::PreMine => PRE_MINE.to_string(),
            TransactionKeyManagerBranch::CodeTemplateAuthor => CODE_TEMPLATE_AUTHOR.to_string(),
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key {
            DATA_ENCRYPTION => TransactionKeyManagerBranch::DataEncryption,
            METADATA_EPHEMERAL_NONCE => TransactionKeyManagerBranch::MetadataEphemeralNonce,
            COMMITMENT_MASK => TransactionKeyManagerBranch::CommitmentMask,
            NONCE => TransactionKeyManagerBranch::Nonce,
            KERNEL_NONCE => TransactionKeyManagerBranch::KernelNonce,
            SENDER_OFFSET => TransactionKeyManagerBranch::SenderOffset,
            ONE_SIDED_SENDER_OFFSET => TransactionKeyManagerBranch::OneSidedSenderOffset,
            RANDOM_KEY => TransactionKeyManagerBranch::RandomKey,
            WALLET_COMMS_AND_SPEND_KEY_BRANCH => TransactionKeyManagerBranch::Spend,
            PRE_MINE => TransactionKeyManagerBranch::PreMine,
            CODE_TEMPLATE_AUTHOR => TransactionKeyManagerBranch::CodeTemplateAuthor,
            _ => TransactionKeyManagerBranch::Nonce,
        }
    }

    pub fn into_ledger(self) -> Option<LedgerKeyBranch> {
        match self {
            TransactionKeyManagerBranch::DataEncryption => None,
            TransactionKeyManagerBranch::MetadataEphemeralNonce => Some(LedgerKeyBranch::MetadataEphemeralNonce),
            TransactionKeyManagerBranch::CommitmentMask => None,
            TransactionKeyManagerBranch::Nonce => None,
            TransactionKeyManagerBranch::KernelNonce => None,
            TransactionKeyManagerBranch::SenderOffset => None,
            TransactionKeyManagerBranch::OneSidedSenderOffset => Some(LedgerKeyBranch::OneSidedSenderOffset),
            TransactionKeyManagerBranch::RandomKey => Some(LedgerKeyBranch::Random),
            TransactionKeyManagerBranch::Spend => Some(LedgerKeyBranch::Spend),
            TransactionKeyManagerBranch::PreMine => Some(LedgerKeyBranch::PreMine),
            TransactionKeyManagerBranch::CodeTemplateAuthor => None,
        }
    }

    pub fn from_byte(value: u8) -> Option<Self> {
        match value {
            0x00 => Some(TransactionKeyManagerBranch::DataEncryption),
            0x01 => Some(TransactionKeyManagerBranch::MetadataEphemeralNonce),
            0x02 => Some(TransactionKeyManagerBranch::CommitmentMask),
            0x03 => Some(TransactionKeyManagerBranch::Nonce),
            0x04 => Some(TransactionKeyManagerBranch::KernelNonce),
            0x05 => Some(TransactionKeyManagerBranch::SenderOffset),
            0x06 => Some(TransactionKeyManagerBranch::OneSidedSenderOffset),
            0x07 => Some(TransactionKeyManagerBranch::Spend),
            0x08 => Some(TransactionKeyManagerBranch::RandomKey),
            0x09 => Some(TransactionKeyManagerBranch::PreMine),
            0x0a => Some(TransactionKeyManagerBranch::CodeTemplateAuthor),
            _ => None,
        }
    }

    pub fn is_ledger_branch(value: &str) -> bool {
        let branch = TransactionKeyManagerBranch::from_key(value);
        matches!(
            branch,
            TransactionKeyManagerBranch::OneSidedSenderOffset |
                TransactionKeyManagerBranch::Spend |
                TransactionKeyManagerBranch::RandomKey |
                TransactionKeyManagerBranch::PreMine |
                TransactionKeyManagerBranch::MetadataEphemeralNonce
        )
    }
}

#[cfg(test)]
mod test {
    use minotari_ledger_wallet_common::common_types::Branch;

    use crate::{
        key_branches::{
            TransactionKeyManagerBranch,
            COMMITMENT_MASK,
            DATA_ENCRYPTION,
            KERNEL_NONCE,
            METADATA_EPHEMERAL_NONCE,
            NONCE,
            ONE_SIDED_SENDER_OFFSET,
            PRE_MINE,
            RANDOM_KEY,
            SENDER_OFFSET,
        },
        WALLET_COMMS_AND_SPEND_KEY_BRANCH,
    };

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_branch_conversion() {
        let mappings = [
            (
                Branch::DataEncryption as u8,
                TransactionKeyManagerBranch::DataEncryption,
                DATA_ENCRYPTION,
            ),
            (
                Branch::MetadataEphemeralNonce as u8,
                TransactionKeyManagerBranch::MetadataEphemeralNonce,
                METADATA_EPHEMERAL_NONCE,
            ),
            (
                Branch::CommitmentMask as u8,
                TransactionKeyManagerBranch::CommitmentMask,
                COMMITMENT_MASK,
            ),
            (Branch::Nonce as u8, TransactionKeyManagerBranch::Nonce, NONCE),
            (
                Branch::KernelNonce as u8,
                TransactionKeyManagerBranch::KernelNonce,
                KERNEL_NONCE,
            ),
            (
                Branch::SenderOffset as u8,
                TransactionKeyManagerBranch::SenderOffset,
                SENDER_OFFSET,
            ),
            (
                Branch::OneSidedSenderOffset as u8,
                TransactionKeyManagerBranch::OneSidedSenderOffset,
                ONE_SIDED_SENDER_OFFSET,
            ),
            (
                Branch::Spend as u8,
                TransactionKeyManagerBranch::Spend,
                WALLET_COMMS_AND_SPEND_KEY_BRANCH,
            ),
            (
                Branch::RandomKey as u8,
                TransactionKeyManagerBranch::RandomKey,
                RANDOM_KEY,
            ),
            (Branch::PreMine as u8, TransactionKeyManagerBranch::PreMine, PRE_MINE),
        ];

        for (expected_byte, branch, key) in &mappings {
            assert_eq!(branch.as_byte(), *expected_byte);
            assert_eq!(TransactionKeyManagerBranch::from_byte(*expected_byte), Some(*branch));
            assert_eq!(&branch.get_branch_key(), *key);
            assert_eq!(TransactionKeyManagerBranch::from_key(key), *branch);
        }
    }
}
