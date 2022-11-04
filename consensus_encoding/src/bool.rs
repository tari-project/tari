// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::io;

use crate::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized};

impl ConsensusEncoding for bool {
    fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        writer.write_all(&[u8::from(*self)])?;
        Ok(())
    }
}

impl ConsensusEncodingSized for bool {
    fn consensus_encode_exact_size(&self) -> usize {
        1
    }
}

impl ConsensusDecoding for bool {
    fn consensus_decode<R: io::Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        match buf[0] {
            0 => Ok(false),
            1 => Ok(true),
            b => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("valid bool values are 0 or 1, got '{}'", b),
            )),
        }
    }
}

#[cfg(test)]
mod test {
    use rand::{rngs::OsRng, RngCore};

    use super::*;
    use crate::consensus::check_consensus_encoding_correctness;

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let subject = true;
        check_consensus_encoding_correctness(subject).unwrap();

        let subject = false;
        check_consensus_encoding_correctness(subject).unwrap();
    }

    #[test]
    fn it_fails_decoding_for_invalid_values() {
        let mut buf = [0u8];
        while buf[0] == 0 || buf[0] == 1 {
            OsRng.fill_bytes(&mut buf);
        }
        bool::consensus_decode(&mut buf.as_slice()).unwrap_err();
    }
}
