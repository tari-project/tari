// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    convert::TryFrom,
    io,
    io::{Error, Read, Write},
    ops::Deref,
};

use integer_encoding::{VarInt, VarIntReader, VarIntWriter};

use crate::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized};

impl ConsensusEncoding for Vec<u8> {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        let len = self.len();
        writer.write_varint(len)?;
        writer.write_all(self)?;
        Ok(())
    }
}

impl ConsensusEncodingSized for Vec<u8> {
    fn consensus_encode_exact_size(&self) -> usize {
        let len = self.len();
        len.required_space() + len
    }
}

pub struct MaxSizeBytes<const MAX: usize> {
    inner: Vec<u8>,
}

impl<const MAX: usize> From<MaxSizeBytes<MAX>> for Vec<u8> {
    fn from(value: MaxSizeBytes<MAX>) -> Self {
        value.inner
    }
}

impl<const MAX: usize> TryFrom<Vec<u8>> for MaxSizeBytes<MAX> {
    type Error = Vec<u8>;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        if value.len() > MAX {
            return Err(value);
        }
        Ok(MaxSizeBytes { inner: value })
    }
}

impl<const MAX: usize> ConsensusDecoding for MaxSizeBytes<MAX> {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let len = reader.read_varint()?;
        if len > MAX {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Vec size ({}) exceeded maximum ({})", len, MAX),
            ));
        }
        let mut bytes = vec![0u8; len];
        reader.read_exact(&mut bytes)?;
        Ok(Self { inner: bytes })
    }
}

impl<const MAX: usize> AsRef<[u8]> for MaxSizeBytes<MAX> {
    fn as_ref(&self) -> &[u8] {
        &self.inner
    }
}

impl<const MAX: usize> Deref for MaxSizeBytes<MAX> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl ConsensusEncoding for &[u8] {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        let len = self.len();
        writer.write_varint(len)?;
        writer.write_all(self)?;
        Ok(())
    }
}

impl ConsensusEncodingSized for &[u8] {
    fn consensus_encode_exact_size(&self) -> usize {
        self.len()
    }
}

impl<const N: usize> ConsensusEncoding for [u8; N] {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        // For fixed length types we dont need a length byte
        writer.write_all(&self[..])?;
        Ok(())
    }
}

impl<const N: usize> ConsensusEncodingSized for [u8; N] {
    fn consensus_encode_exact_size(&self) -> usize {
        N
    }
}

impl<const N: usize> ConsensusDecoding for [u8; N] {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; N];
        reader.read_exact(&mut buf)?;
        Ok(buf)
    }
}

#[cfg(test)]
mod test {
    use rand::{rngs::OsRng, RngCore};

    use super::*;
    use crate::consensus::{check_consensus_encoding_correctness, ToConsensusBytes};

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let mut subject = [0u8; 1024];
        OsRng.fill_bytes(&mut subject);
        check_consensus_encoding_correctness(subject).unwrap();

        // Get vec encoding with length byte
        let encoded = subject.to_vec().to_consensus_bytes();
        let decoded = MaxSizeBytes::<1024>::consensus_decode(&mut encoded.as_slice()).unwrap();
        assert_eq!(*decoded, *subject.as_slice());
    }
}
