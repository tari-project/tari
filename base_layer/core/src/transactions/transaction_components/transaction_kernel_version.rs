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
pub enum TransactionKernelVersion {
    V0 = 0,
}

impl TransactionKernelVersion {
    pub fn get_current_version() -> Self {
        Self::V0
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }
}
impl TryFrom<u8> for TransactionKernelVersion {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TransactionKernelVersion::V0),
            _ => Err("Unknown version!".to_string()),
        }
    }
}

impl ConsensusEncoding for TransactionKernelVersion {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        writer.write_all(&[self.as_u8()])?;
        Ok(1)
    }
}

impl ConsensusEncodingSized for TransactionKernelVersion {
    fn consensus_encode_exact_size(&self) -> usize {
        1
    }
}

impl ConsensusDecoding for TransactionKernelVersion {
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
    fn test_try_from() {
        assert_eq!(TransactionKernelVersion::try_from(0), Ok(TransactionKernelVersion::V0));
        assert!(TransactionKernelVersion::try_from(1).is_err());
    }

    #[test]
    fn test_consensus() {
        let mut buffer = Cursor::new(vec![
            0;
            TransactionKernelVersion::get_current_version()
                .consensus_encode_exact_size()
        ]);
        assert_eq!(
            TransactionKernelVersion::get_current_version()
                .consensus_encode(&mut buffer)
                .unwrap(),
            TransactionKernelVersion::get_current_version().consensus_encode_exact_size()
        );
        // Reset the buffer to original position, we are going to read.
        buffer.set_position(0);
        assert_eq!(
            TransactionKernelVersion::consensus_decode(&mut buffer).unwrap(),
            TransactionKernelVersion::get_current_version()
        );
    }
}
