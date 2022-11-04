// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

macro_rules! consensus_encoding_varint_impl {
    ($ty:ty) => {
        impl $crate::ConsensusEncoding for $ty {
            fn consensus_encode<W: std::io::Write>(&self, writer: &mut W) -> Result<(), std::io::Error> {
                use integer_encoding::VarIntWriter;
                writer.write_varint(*self)?;
                Ok(())
            }
        }

        impl $crate::ConsensusDecoding for $ty {
            fn consensus_decode<R: std::io::Read>(reader: &mut R) -> Result<Self, std::io::Error> {
                use integer_encoding::VarIntReader;
                let value = reader.read_varint()?;
                Ok(value)
            }
        }

        impl $crate::ConsensusEncodingSized for $ty {
            fn consensus_encode_exact_size(&self) -> usize {
                use integer_encoding::VarInt;
                self.required_space()
            }
        }
    };
}

consensus_encoding_varint_impl!(u16);
consensus_encoding_varint_impl!(u32);
consensus_encoding_varint_impl!(u64);

#[cfg(test)]
mod test {
    use crate::check_consensus_encoding_correctness;

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let subject = u16::MAX;
        check_consensus_encoding_correctness(subject).unwrap();

        let subject = u32::MAX;
        check_consensus_encoding_correctness(subject).unwrap();

        let subject = u64::MAX;
        check_consensus_encoding_correctness(subject).unwrap();
    }
}
