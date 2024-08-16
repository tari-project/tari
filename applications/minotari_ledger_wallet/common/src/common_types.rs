// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::string::String;

use crate::utils;

/// Ledger application status words.
#[repr(u16)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum AppSW {
    Deny = 0xB001,
    WrongP1P2 = 0xB002,
    InsNotSupported = 0xB003,
    ScriptSignatureFail = 0xB004,
    RawSchnorrSignatureFail = 0xB005,
    SchnorrSignatureFail = 0xB006,
    ScriptOffsetNotUnique = 0xB007,
    KeyDeriveFail = 0xB008,
    KeyDeriveFromCanonical = 0xB009,
    KeyDeriveFromUniform = 0xB00A,
    RandomNonceFail = 0xB00B,
    BadBranchKey = 0xB00C,
    MetadataSignatureFail = 0xB00D,
    WrongApduLength = 0x6e03, // See ledger-device-rust-sdk/ledger_device_sdk/src/io.rs:16
    UserCancelled = 0x6e04,   // See ledger-device-rust-sdk/ledger_device_sdk/src/io.rs:16
}

impl TryFrom<u16> for AppSW {
    type Error = String;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0xB001 => Ok(AppSW::Deny),
            0xB002 => Ok(AppSW::WrongP1P2),
            0xB003 => Ok(AppSW::InsNotSupported),
            0xB004 => Ok(AppSW::ScriptSignatureFail),
            0xB005 => Ok(AppSW::RawSchnorrSignatureFail),
            0xB006 => Ok(AppSW::SchnorrSignatureFail),
            0xB007 => Ok(AppSW::ScriptOffsetNotUnique),
            0xB008 => Ok(AppSW::KeyDeriveFail),
            0xB009 => Ok(AppSW::KeyDeriveFromCanonical),
            0xB00A => Ok(AppSW::KeyDeriveFromUniform),
            0xB00B => Ok(AppSW::RandomNonceFail),
            0xB00C => Ok(AppSW::BadBranchKey),
            0xB00D => Ok(AppSW::MetadataSignatureFail),
            0x6e03 => Ok(AppSW::WrongApduLength),
            0x6e04 => Ok(AppSW::UserCancelled),
            _ => Err(String::from("Invalid value for AppSW (") + utils::u16_to_string(value).as_str() + ")"),
        }
    }
}

/// Ledger application instructions.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Instruction {
    GetVersion = 0x01,
    GetAppName = 0x02,
    GetPublicSpendKey = 0x03,
    GetPublicKey = 0x04,
    GetScriptSignatureDerived = 0x05,
    GetScriptOffset = 0x06,
    GetViewKey = 0x07,
    GetDHSharedSecret = 0x08,
    GetRawSchnorrSignature = 0x09,
    GetScriptSchnorrSignature = 0x10,
    GetOneSidedMetadataSignature = 0x11,
    GetScriptSignatureManaged = 0x12,
}

impl Instruction {
    pub fn as_byte(self) -> u8 {
        self as u8
    }

    pub fn from_byte(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Instruction::GetVersion),
            0x02 => Some(Instruction::GetAppName),
            0x03 => Some(Instruction::GetPublicSpendKey),
            0x04 => Some(Instruction::GetPublicKey),
            0x05 => Some(Instruction::GetScriptSignatureDerived),
            0x06 => Some(Instruction::GetScriptOffset),
            0x07 => Some(Instruction::GetViewKey),
            0x08 => Some(Instruction::GetDHSharedSecret),
            0x09 => Some(Instruction::GetRawSchnorrSignature),
            0x10 => Some(Instruction::GetScriptSchnorrSignature),
            0x11 => Some(Instruction::GetOneSidedMetadataSignature),
            0x12 => Some(Instruction::GetScriptSignatureManaged),
            _ => None,
        }
    }
}

/// Key manager branches shared by the Ledger application and the wallet.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Branch {
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
}

impl Branch {
    pub fn as_byte(self) -> u8 {
        self as u8
    }

    pub fn from_byte(value: u8) -> Option<Self> {
        match value {
            0x00 => Some(Branch::DataEncryption),
            0x01 => Some(Branch::MetadataEphemeralNonce),
            0x02 => Some(Branch::CommitmentMask),
            0x03 => Some(Branch::Nonce),
            0x04 => Some(Branch::KernelNonce),
            0x05 => Some(Branch::SenderOffset),
            0x06 => Some(Branch::OneSidedSenderOffset),
            0x07 => Some(Branch::Spend),
            0x08 => Some(Branch::RandomKey),
            0x09 => Some(Branch::PreMine),
            _ => None,
        }
    }
}

#[cfg(test)]
mod test {
    use crate::common_types::{AppSW, Instruction};

    #[test]
    fn test_app_sw_conversion() {
        let mappings = [
            (0xB001, AppSW::Deny),
            (0xB002, AppSW::WrongP1P2),
            (0xB003, AppSW::InsNotSupported),
            (0xB004, AppSW::ScriptSignatureFail),
            (0xB005, AppSW::RawSchnorrSignatureFail),
            (0xB006, AppSW::SchnorrSignatureFail),
            (0xB007, AppSW::ScriptOffsetNotUnique),
            (0xB008, AppSW::KeyDeriveFail),
            (0xB009, AppSW::KeyDeriveFromCanonical),
            (0xB00A, AppSW::KeyDeriveFromUniform),
            (0xB00B, AppSW::RandomNonceFail),
            (0xB00C, AppSW::BadBranchKey),
            (0xB00D, AppSW::MetadataSignatureFail),
            (0x6e03, AppSW::WrongApduLength),
            (0x6e04, AppSW::UserCancelled),
        ];

        for (value, expected_app_sw) in &mappings {
            match expected_app_sw {
                AppSW::Deny => {
                    assert_eq!(AppSW::try_from(*value).unwrap(), *expected_app_sw);
                },
                AppSW::WrongP1P2 => {
                    assert_eq!(AppSW::try_from(*value).unwrap(), *expected_app_sw);
                },
                AppSW::InsNotSupported => {
                    assert_eq!(AppSW::try_from(*value).unwrap(), *expected_app_sw);
                },
                AppSW::ScriptSignatureFail => {
                    assert_eq!(AppSW::try_from(*value).unwrap(), *expected_app_sw);
                },
                AppSW::RawSchnorrSignatureFail => {
                    assert_eq!(AppSW::try_from(*value).unwrap(), *expected_app_sw);
                },
                AppSW::SchnorrSignatureFail => {
                    assert_eq!(AppSW::try_from(*value).unwrap(), *expected_app_sw);
                },
                AppSW::ScriptOffsetNotUnique => {
                    assert_eq!(AppSW::try_from(*value).unwrap(), *expected_app_sw);
                },
                AppSW::KeyDeriveFail => {
                    assert_eq!(AppSW::try_from(*value).unwrap(), *expected_app_sw);
                },
                AppSW::KeyDeriveFromCanonical => {
                    assert_eq!(AppSW::try_from(*value).unwrap(), *expected_app_sw);
                },
                AppSW::KeyDeriveFromUniform => {
                    assert_eq!(AppSW::try_from(*value).unwrap(), *expected_app_sw);
                },
                AppSW::RandomNonceFail => {
                    assert_eq!(AppSW::try_from(*value).unwrap(), *expected_app_sw);
                },
                AppSW::BadBranchKey => {
                    assert_eq!(AppSW::try_from(*value).unwrap(), *expected_app_sw);
                },
                AppSW::MetadataSignatureFail => {
                    assert_eq!(AppSW::try_from(*value).unwrap(), *expected_app_sw);
                },
                AppSW::WrongApduLength => {
                    assert_eq!(AppSW::try_from(*value).unwrap(), *expected_app_sw);
                },
                AppSW::UserCancelled => {
                    assert_eq!(AppSW::try_from(*value).unwrap(), *expected_app_sw);
                },
            }
        }
    }

    #[test]
    fn test_instruction_conversion() {
        let mappings = [
            (0x01, Instruction::GetVersion),
            (0x02, Instruction::GetAppName),
            (0x03, Instruction::GetPublicSpendKey),
            (0x04, Instruction::GetPublicKey),
            (0x05, Instruction::GetScriptSignatureDerived),
            (0x06, Instruction::GetScriptOffset),
            (0x07, Instruction::GetViewKey),
            (0x08, Instruction::GetDHSharedSecret),
            (0x09, Instruction::GetRawSchnorrSignature),
            (0x10, Instruction::GetScriptSchnorrSignature),
            (0x11, Instruction::GetOneSidedMetadataSignature),
            (0x12, Instruction::GetScriptSignatureManaged),
        ];

        for (expected_byte, instruction) in &mappings {
            match instruction {
                Instruction::GetVersion => {
                    assert_eq!(instruction.as_byte(), *expected_byte);
                    assert_eq!(Instruction::from_byte(*expected_byte), Some(*instruction));
                },
                Instruction::GetAppName => {
                    assert_eq!(instruction.as_byte(), *expected_byte);
                    assert_eq!(Instruction::from_byte(*expected_byte), Some(*instruction));
                },
                Instruction::GetPublicSpendKey => {
                    assert_eq!(instruction.as_byte(), *expected_byte);
                    assert_eq!(Instruction::from_byte(*expected_byte), Some(*instruction));
                },
                Instruction::GetPublicKey => {
                    assert_eq!(instruction.as_byte(), *expected_byte);
                    assert_eq!(Instruction::from_byte(*expected_byte), Some(*instruction));
                },
                Instruction::GetScriptSignatureDerived => {
                    assert_eq!(instruction.as_byte(), *expected_byte);
                    assert_eq!(Instruction::from_byte(*expected_byte), Some(*instruction));
                },
                Instruction::GetScriptOffset => {
                    assert_eq!(instruction.as_byte(), *expected_byte);
                    assert_eq!(Instruction::from_byte(*expected_byte), Some(*instruction));
                },
                Instruction::GetViewKey => {
                    assert_eq!(instruction.as_byte(), *expected_byte);
                    assert_eq!(Instruction::from_byte(*expected_byte), Some(*instruction));
                },
                Instruction::GetDHSharedSecret => {
                    assert_eq!(instruction.as_byte(), *expected_byte);
                    assert_eq!(Instruction::from_byte(*expected_byte), Some(*instruction));
                },
                Instruction::GetRawSchnorrSignature => {
                    assert_eq!(instruction.as_byte(), *expected_byte);
                    assert_eq!(Instruction::from_byte(*expected_byte), Some(*instruction));
                },
                Instruction::GetScriptSchnorrSignature => {
                    assert_eq!(instruction.as_byte(), *expected_byte);
                    assert_eq!(Instruction::from_byte(*expected_byte), Some(*instruction));
                },
                Instruction::GetOneSidedMetadataSignature => {
                    assert_eq!(instruction.as_byte(), *expected_byte);
                    assert_eq!(Instruction::from_byte(*expected_byte), Some(*instruction));
                },
                Instruction::GetScriptSignatureManaged => {
                    assert_eq!(instruction.as_byte(), *expected_byte);
                    assert_eq!(Instruction::from_byte(*expected_byte), Some(*instruction));
                },
            }
        }
    }

    #[test]
    fn test_branch_conversion() {
        use crate::common_types::Branch;

        let mappings = [
            (0x00, Branch::DataEncryption),
            (0x01, Branch::MetadataEphemeralNonce),
            (0x02, Branch::CommitmentMask),
            (0x03, Branch::Nonce),
            (0x04, Branch::KernelNonce),
            (0x05, Branch::SenderOffset),
            (0x06, Branch::OneSidedSenderOffset),
            (0x07, Branch::Spend),
            (0x08, Branch::RandomKey),
            (0x09, Branch::PreMine),
        ];

        for (expected_byte, branch) in &mappings {
            match branch {
                Branch::DataEncryption => {
                    assert_eq!(branch.as_byte(), *expected_byte);
                    assert_eq!(Branch::from_byte(*expected_byte), Some(*branch));
                },
                Branch::MetadataEphemeralNonce => {
                    assert_eq!(branch.as_byte(), *expected_byte);
                    assert_eq!(Branch::from_byte(*expected_byte), Some(*branch));
                },
                Branch::CommitmentMask => {
                    assert_eq!(branch.as_byte(), *expected_byte);
                    assert_eq!(Branch::from_byte(*expected_byte), Some(*branch));
                },
                Branch::Nonce => {
                    assert_eq!(branch.as_byte(), *expected_byte);
                    assert_eq!(Branch::from_byte(*expected_byte), Some(*branch));
                },
                Branch::KernelNonce => {
                    assert_eq!(branch.as_byte(), *expected_byte);
                    assert_eq!(Branch::from_byte(*expected_byte), Some(*branch));
                },
                Branch::SenderOffset => {
                    assert_eq!(branch.as_byte(), *expected_byte);
                    assert_eq!(Branch::from_byte(*expected_byte), Some(*branch));
                },
                Branch::OneSidedSenderOffset => {
                    assert_eq!(branch.as_byte(), *expected_byte);
                    assert_eq!(Branch::from_byte(*expected_byte), Some(*branch));
                },
                Branch::Spend => {
                    assert_eq!(branch.as_byte(), *expected_byte);
                    assert_eq!(Branch::from_byte(*expected_byte), Some(*branch));
                },
                Branch::RandomKey => {
                    assert_eq!(branch.as_byte(), *expected_byte);
                    assert_eq!(Branch::from_byte(*expected_byte), Some(*branch));
                },
                Branch::PreMine => {
                    assert_eq!(branch.as_byte(), *expected_byte);
                    assert_eq!(Branch::from_byte(*expected_byte), Some(*branch));
                },
            }
        }
    }
}
