//  Copyright 2022, The Tari Project
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

use std::{
    io,
    io::{Read, Write},
};

use integer_encoding::VarIntWriter;

use crate::consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized};

impl<T: ConsensusEncoding> ConsensusEncoding for Option<T> {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut written = 0;
        match self {
            Some(t) => {
                writer.write_all(&[1u8])?;
                written += 1;
                written += t.consensus_encode(writer)?;
            },
            None => {
                writer.write_all(&[0u8])?;
                written += 1;
            },
        }

        Ok(written)
    }
}

impl<T: ConsensusEncodingSized> ConsensusEncodingSized for Option<T> {}

impl<T: ConsensusDecoding> ConsensusDecoding for Option<T> {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        match buf[0] {
            0 => Ok(None),
            1 => {
                let t = T::consensus_decode(reader)?;
                Ok(Some(t))
            },
            b => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("consensus decode: Invalid Option byte {}", b),
            )),
        }
    }
}

//---------------------------------- Vec<T> --------------------------------------------//
impl<T: ConsensusEncoding> ConsensusEncoding for Vec<T> {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut written = writer.write_varint(self.len())?;
        for elem in self {
            written += elem.consensus_encode(writer)?;
        }
        Ok(written)
    }
}

impl<T: ConsensusEncodingSized> ConsensusEncodingSized for Vec<T> {}

// Important: No ConsensusDecode impl for Vec<T> because the implementer needs to manually ensure there is a maximum
// number of elements that can be decoded to prevent unbounded allocation.

#[cfg(test)]
mod test {
    use super::*;
    use crate::consensus::{check_consensus_encoding_correctness, MaxSizeVec};

    mod option {
        use super::*;

        #[test]
        fn it_encodes_and_decodes_correctly() {
            let subject = Option::<u32>::None;
            check_consensus_encoding_correctness(subject).unwrap();
            let subject = Some(123u32);
            check_consensus_encoding_correctness(subject).unwrap();
        }
    }

    mod vec {
        use super::*;

        #[test]
        fn it_encodes_and_decodes_correctly() {
            let subject = vec![vec![1u32, 2, 3], vec![1u32, 3, 2]];
            let mut buf = Vec::new();
            subject.consensus_encode(&mut buf).unwrap();
            assert_eq!(buf.len(), subject.consensus_encode_exact_size());
            let v = MaxSizeVec::<MaxSizeVec<u32, 10>, 100>::consensus_decode(&mut buf.as_slice()).unwrap();
            assert_eq!(v.into_vec(), subject);
        }
    }
}
