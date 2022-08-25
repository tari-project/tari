// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    convert::{TryFrom, TryInto},
    io,
    io::{ErrorKind, Read, Write},
};

use serde::{Deserialize, Serialize};

use crate::consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd)]
#[repr(u8)]
pub enum TransactionInputVersion {
    V0 = 0,
    /// Currently only used in tests, this can be used as the next version
    V1 = 1,
}

impl TransactionInputVersion {
    pub fn get_current_version() -> Self {
        Self::V0
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for TransactionInputVersion {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TransactionInputVersion::V0),
            1 => Ok(TransactionInputVersion::V1),
            v => Err(format!("Unknown input version {}!", v)),
        }
    }
}

impl ConsensusEncoding for TransactionInputVersion {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        writer.write_all(&[self.as_u8()])?;
        Ok(())
    }
}

impl ConsensusEncodingSized for TransactionInputVersion {
    fn consensus_encode_exact_size(&self) -> usize {
        1
    }
}

impl ConsensusDecoding for TransactionInputVersion {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        let version = buf[0]
            .try_into()
            .map_err(|_| io::Error::new(ErrorKind::InvalidInput, format!("Unknown input version {}", buf[0])))?;
        Ok(version)
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::consensus::check_consensus_encoding_correctness;

    #[test]
    fn test_as_u8() {
        assert_eq!(TransactionInputVersion::V0.as_u8(), 0);
        assert_eq!(TransactionInputVersion::V1.as_u8(), 1);
    }

    #[test]
    fn test_try_from() {
        assert_eq!(TransactionInputVersion::try_from(0), Ok(TransactionInputVersion::V0));
        assert_eq!(TransactionInputVersion::try_from(1), Ok(TransactionInputVersion::V1));
        assert!(TransactionInputVersion::try_from(2).is_err());
    }

    #[test]
    fn test_encode_exact_size() {
        assert_eq!(TransactionInputVersion::V0.consensus_encode_exact_size(), 1);
        assert_eq!(TransactionInputVersion::V1.consensus_encode_exact_size(), 1);
    }

    #[test]
    fn test_decode_encode() {
        check_consensus_encoding_correctness(TransactionInputVersion::get_current_version()).unwrap();
    }
}
