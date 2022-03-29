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
