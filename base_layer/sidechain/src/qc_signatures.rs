// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{ValidatorBlockSignature, serde::hex_or_bytes};

/// The committee that signs a quorum certificate, in ascending order of public key with each key listed once. A
/// [`SignerBitmap`] addresses members by their index in this order.
pub trait SigningCommittee {
    /// The number of members in the committee.
    fn len(&self) -> usize;

    /// The encoded public key and voting power of the member at `index`, or `None` if `index` is not less than
    /// [`Self::len`].
    fn member(&self, index: usize) -> Option<(&[u8], u64)>;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The signatures of a quorum certificate, tagged by the scheme they are made under.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub enum QcSignatures {
    /// One Schnorr signature per signer, listed in ascending order of the signer's committee index.
    Schnorr {
        signers: SignerBitmap,
        signatures: Vec<ValidatorBlockSignature>,
    },
}

impl QcSignatures {
    pub fn signers(&self) -> &SignerBitmap {
        match self {
            Self::Schnorr { signers, .. } => signers,
        }
    }

    pub fn num_signatures(&self) -> usize {
        match self {
            Self::Schnorr { signatures, .. } => signatures.len(),
        }
    }

    /// Checks that the signer bitmap is the encoding for a committee of `committee_size` members and that there is
    /// exactly one signature per signer. Verifies no signature, so this bounds the work a certificate can cause before
    /// any is verified.
    pub fn check_well_formed(&self, committee_size: usize) -> Result<(), QcSignaturesError> {
        self.signers().check_committee_size(committee_size)?;
        let num_signers = self.signers().count();
        let num_signatures = self.num_signatures();
        if num_signers != num_signatures {
            return Err(QcSignaturesError::SignatureCountMismatch {
                num_signers,
                num_signatures,
            });
        }
        Ok(())
    }
}

/// The committee members that signed a quorum certificate. Bit `i % 8` of byte `i / 8`, counting from the least
/// significant bit, is set if the member at index `i` of the [`SigningCommittee`] signed.
///
/// For a committee of `n` members the bitmap is exactly `n.div_ceil(8)` bytes and every bit from index `n` onwards is
/// clear, so each set of signers has a single encoding and anything that hashes a certificate commits to its signers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
#[serde(transparent)]
pub struct SignerBitmap(#[serde(with = "hex_or_bytes")] Vec<u8>);

impl SignerBitmap {
    /// The bitmap of a committee of `committee_size` members in which the members at `indices` signed.
    pub fn from_indices<I: IntoIterator<Item = usize>>(
        committee_size: usize,
        indices: I,
    ) -> Result<Self, QcSignaturesError> {
        let mut bytes = vec![0u8; committee_size.div_ceil(8)];
        for index in indices {
            let byte = bytes
                .get_mut(index / 8)
                .filter(|_| index < committee_size)
                .ok_or(QcSignaturesError::SignerOutsideCommittee { index, committee_size })?;
            *byte |= bit_mask(index);
        }
        Ok(Self(bytes))
    }

    /// Wraps an encoded bitmap. [`Self::check_committee_size`] checks it against the committee it addresses.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    pub fn contains(&self, index: usize) -> bool {
        self.0.get(index / 8).is_some_and(|byte| byte & bit_mask(index) != 0)
    }

    /// The number of signers.
    pub fn count(&self) -> usize {
        self.0.iter().map(|byte| byte.count_ones() as usize).sum()
    }

    /// The committee indices of the signers, ascending.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().enumerate().flat_map(|(byte_index, &byte)| {
            (0..8)
                .filter(move |bit| byte & (1 << bit) != 0)
                .map(move |bit| byte_index.saturating_mul(8).saturating_add(bit))
        })
    }

    /// Checks that this is the encoding of a signer set of a committee of `committee_size` members.
    pub fn check_committee_size(&self, committee_size: usize) -> Result<(), QcSignaturesError> {
        let expected_len = committee_size.div_ceil(8);
        if self.0.len() != expected_len {
            return Err(QcSignaturesError::BitmapLengthMismatch {
                committee_size,
                expected_len,
                len: self.0.len(),
            });
        }
        if let Some(index) = self.iter().find(|&index| index >= committee_size) {
            return Err(QcSignaturesError::SignerOutsideCommittee { index, committee_size });
        }
        Ok(())
    }
}

fn bit_mask(index: usize) -> u8 {
    1 << (index % 8)
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QcSignaturesError {
    #[error("The signer bitmap is {len} bytes, but a committee of {committee_size} members requires {expected_len}")]
    BitmapLengthMismatch {
        committee_size: usize,
        expected_len: usize,
        len: usize,
    },
    #[error("The signer bitmap marks member {index}, but the committee has {committee_size} members")]
    SignerOutsideCommittee { index: usize, committee_size: usize },
    #[error("The signer bitmap marks {num_signers} signers, but the certificate carries {num_signatures} signatures")]
    SignatureCountMismatch { num_signers: usize, num_signatures: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_round_trips_signer_indices() {
        let bitmap = SignerBitmap::from_indices(12, [0, 3, 8, 11]).unwrap();
        assert_eq!(bitmap.as_bytes(), &[0b0000_1001, 0b0000_1001]);
        assert_eq!(bitmap.iter().collect::<Vec<_>>(), vec![0, 3, 8, 11]);
        assert_eq!(bitmap.count(), 4);
        assert!(bitmap.contains(8));
        assert!(!bitmap.contains(9));
        assert!(!bitmap.contains(100));
        bitmap.check_committee_size(12).unwrap();
    }

    #[test]
    fn it_rejects_an_index_outside_the_committee() {
        assert_eq!(
            SignerBitmap::from_indices(12, [12]).unwrap_err(),
            QcSignaturesError::SignerOutsideCommittee {
                index: 12,
                committee_size: 12
            }
        );
        assert_eq!(
            SignerBitmap::from_indices(8, [8]).unwrap_err(),
            QcSignaturesError::SignerOutsideCommittee {
                index: 8,
                committee_size: 8
            }
        );
    }

    #[test]
    fn a_bitmap_has_one_encoding_per_committee_size() {
        let bitmap = SignerBitmap::from_indices(12, [0]).unwrap();
        assert_eq!(
            bitmap.check_committee_size(8).unwrap_err(),
            QcSignaturesError::BitmapLengthMismatch {
                committee_size: 8,
                expected_len: 1,
                len: 2
            }
        );
        assert_eq!(
            bitmap.check_committee_size(17).unwrap_err(),
            QcSignaturesError::BitmapLengthMismatch {
                committee_size: 17,
                expected_len: 3,
                len: 2
            }
        );
        SignerBitmap::from_indices(0, [])
            .unwrap()
            .check_committee_size(0)
            .unwrap();

        // A set padding bit marks a member the committee does not have.
        let padded = SignerBitmap::from_bytes(vec![0b0000_0001, 0b0001_0000]);
        assert_eq!(
            padded.check_committee_size(12).unwrap_err(),
            QcSignaturesError::SignerOutsideCommittee {
                index: 12,
                committee_size: 12
            }
        );
    }

    #[test]
    fn it_requires_one_signature_per_signer() {
        let signatures = QcSignatures::Schnorr {
            signers: SignerBitmap::from_indices(4, [1, 2]).unwrap(),
            signatures: vec![ValidatorBlockSignature::default()],
        };
        assert_eq!(
            signatures.check_well_formed(4).unwrap_err(),
            QcSignaturesError::SignatureCountMismatch {
                num_signers: 2,
                num_signatures: 1
            }
        );
    }

    #[test]
    fn the_bitmap_serializes_as_hex() {
        let bitmap = SignerBitmap::from_indices(12, [0, 11]).unwrap();
        let json = serde_json::to_value(&bitmap).unwrap();
        assert_eq!(json, serde_json::json!("0108"));
        assert_eq!(serde_json::from_value::<SignerBitmap>(json).unwrap(), bitmap);
        assert_eq!(borsh::to_vec(&bitmap).unwrap(), [2, 0, 0, 0, 1, 8]);
    }
}
