// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    io,
    io::{Read, Write},
};

use integer_encoding::VarIntWriter;

use crate::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized};

impl<T: ConsensusEncoding> ConsensusEncoding for Option<T> {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        match self {
            Some(t) => {
                writer.write_all(&[1u8])?;
                t.consensus_encode(writer)?;
            },
            None => {
                writer.write_all(&[0u8])?;
            },
        }

        Ok(())
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

//---------------------------------- Box<T> --------------------------------------------//

impl<T: ConsensusEncoding> ConsensusEncoding for Box<T> {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        self.as_ref().consensus_encode(writer)?;
        Ok(())
    }
}

impl<T: ConsensusEncodingSized> ConsensusEncodingSized for Box<T> {}

impl<T: ConsensusDecoding> ConsensusDecoding for Box<T> {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let t = T::consensus_decode(reader)?;
        Ok(Box::new(t))
    }
}

//---------------------------------- Vec<T> --------------------------------------------//
impl<T: ConsensusEncoding> ConsensusEncoding for Vec<T> {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        writer.write_varint(self.len())?;
        for elem in self {
            elem.consensus_encode(writer)?;
        }
        Ok(())
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
