// Copyright 2018 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use std::{
    convert::{TryFrom, TryInto},
    io,
    io::{ErrorKind, Read, Write},
};

use serde::{Deserialize, Serialize};

use crate::consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd)]
#[repr(u8)]
pub enum TransactionOutputVersion {
    V0 = 0,
    /// Currently only used in tests, this can be used as the next version
    V1 = 1,
}

impl TransactionOutputVersion {
    pub fn get_current_version() -> Self {
        Self::V0
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for TransactionOutputVersion {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TransactionOutputVersion::V0),
            1 => Ok(TransactionOutputVersion::V1),
            v => Err(format!("Unknown output version {}!", v)),
        }
    }
}

impl ConsensusEncoding for TransactionOutputVersion {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        writer.write_all(&[self.as_u8()])?;
        Ok(())
    }
}

impl ConsensusEncodingSized for TransactionOutputVersion {
    fn consensus_encode_exact_size(&self) -> usize {
        1
    }
}

impl ConsensusDecoding for TransactionOutputVersion {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        let version = buf[0]
            .try_into()
            .map_err(|_| io::Error::new(ErrorKind::InvalidInput, format!("Unknown output version {}", buf[0])))?;
        Ok(version)
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::consensus::check_consensus_encoding_correctness;

    #[test]
    fn test_try_from() {
        assert_eq!(TransactionOutputVersion::try_from(0), Ok(TransactionOutputVersion::V0));
        assert_eq!(TransactionOutputVersion::try_from(1), Ok(TransactionOutputVersion::V1));
        assert!(TransactionOutputVersion::try_from(3).is_err());
    }

    #[test]
    fn test_consensus_encoding() {
        check_consensus_encoding_correctness(TransactionOutputVersion::get_current_version()).unwrap();
    }
}
