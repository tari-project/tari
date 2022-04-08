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
            _ => Err("Unknown version!".to_string()),
        }
    }
}

impl ConsensusEncoding for TransactionInputVersion {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        writer.write_all(&[self.as_u8()])?;
        Ok(1)
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
            .map_err(|_| io::Error::new(ErrorKind::InvalidInput, format!("Unknown version {}", buf[0])))?;
        Ok(version)
    }
}

#[cfg(test)]
mod test {
    use std::io::Cursor;

    use super::*;

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
        let mut buffer = Cursor::new(vec![
            0;
            TransactionInputVersion::get_current_version()
                .consensus_encode_exact_size()
        ]);
        assert_eq!(
            TransactionInputVersion::get_current_version()
                .consensus_encode(&mut buffer)
                .unwrap(),
            TransactionInputVersion::get_current_version().consensus_encode_exact_size()
        );
        // Reset the buffer to original position, we are going to read.
        buffer.set_position(0);
        assert_eq!(
            TransactionInputVersion::consensus_decode(&mut buffer).unwrap(),
            TransactionInputVersion::get_current_version()
        );
    }
}
