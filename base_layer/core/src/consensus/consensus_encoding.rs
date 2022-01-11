//  Copyright 2021, The Tari Project
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

use std::io;

/// Abstracts the ability of a type to canonically encode itself for the purposes of consensus
pub trait ConsensusEncoding {
    /// Encode to the given writer returning the number of bytes written.
    /// If writing to this Writer is infallible, this implementation MUST always succeed.
    fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error>;
}

pub trait ConsensusEncodingSized: ConsensusEncoding {
    /// The return value MUST be the exact byte size of the implementing type
    /// and SHOULD be implemented without allocations.
    fn consensus_encode_exact_size(&self) -> usize;
}

/// Abstracts the ability of a type to be decoded from canonical consensus bytes
pub trait ConsensusDecoding: Sized {
    /// Attempt to decode this type from the given reader
    fn consensus_decode<R: io::Read>(reader: &mut R) -> Result<Self, io::Error>;
}

pub trait ToConsensusBytes {
    fn to_consensus_bytes(&self) -> Vec<u8>;
}

impl<T: ConsensusEncoding + ConsensusEncodingSized + ?Sized> ToConsensusBytes for T {
    fn to_consensus_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.consensus_encode_exact_size());
        // Vec's write impl is infallible, as per the ConsensusEncoding contract, consensus_encode is infallible
        self.consensus_encode(&mut buf).expect("unreachable panic");
        buf
    }
}

impl ConsensusEncoding for Vec<u8> {
    fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        writer.write(self)
    }
}

impl ConsensusEncodingSized for Vec<u8> {
    fn consensus_encode_exact_size(&self) -> usize {
        self.len()
    }
}

macro_rules! consensus_encoding_varint_impl {
    ($ty:ty) => {
        impl ConsensusEncoding for $ty {
            fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
                use integer_encoding::VarIntWriter;
                let bytes_written = writer.write_varint(*self)?;
                Ok(bytes_written)
            }
        }

        impl ConsensusDecoding for $ty {
            fn consensus_decode<R: io::Read>(reader: &mut R) -> Result<Self, io::Error> {
                use integer_encoding::VarIntReader;
                let value = reader.read_varint()?;
                Ok(value)
            }
        }

        impl ConsensusEncodingSized for $ty {
            fn consensus_encode_exact_size(&self) -> usize {
                use integer_encoding::VarInt;
                self.required_space()
            }
        }
    };
}

consensus_encoding_varint_impl!(u8);
consensus_encoding_varint_impl!(u64);

// Keep separate the dependencies of the impls that may in future be implemented in tari crypto
mod impls {
    use std::io::Read;

    use tari_common_types::types::{Commitment, PrivateKey, PublicKey, Signature};
    use tari_crypto::{
        keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait},
        script::{ExecutionStack, TariScript},
    };
    use tari_utilities::ByteArray;
    use tari_utilities::ByteArray;

    use super::*;
    use crate::common::byte_counter::ByteCounter;

    //---------------------------------- TariScript --------------------------------------------//

    impl ConsensusEncoding for TariScript {
        fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
            self.as_bytes().consensus_encode(writer)
        }
    }

    impl ConsensusEncodingSized for TariScript {
        fn consensus_encode_exact_size(&self) -> usize {
            let mut counter = ByteCounter::new();
            // TODO: consensus_encode_exact_size must be cheap to run
            // unreachable panic: ByteCounter is infallible
            self.consensus_encode(&mut counter).expect("unreachable");
            counter.get()
        }
    }

    impl ConsensusEncoding for ExecutionStack {
        fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
            self.as_bytes().consensus_encode(writer)
        }
    }

    impl ConsensusEncodingSized for ExecutionStack {
        fn consensus_encode_exact_size(&self) -> usize {
            let mut counter = ByteCounter::new();
            // TODO: consensus_encode_exact_size must be cheap to run
            // unreachable panic: ByteCounter is infallible
            self.consensus_encode(&mut counter).expect("unreachable");
            counter.get()
        }
    }

    //---------------------------------- PublicKey --------------------------------------------//

    impl ConsensusEncoding for PublicKey {
        fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
            writer.write(self.as_bytes())
        }
    }

    impl ConsensusEncodingSized for PublicKey {
        fn consensus_encode_exact_size(&self) -> usize {
            PublicKey::key_length()
        }
    }

    impl ConsensusDecoding for PublicKey {
        fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
            let mut buf = [0u8; 32];
            reader.read_exact(&mut buf)?;
            let pk = PublicKey::from_bytes(&buf[..]).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
            Ok(pk)
        }
    }

    //---------------------------------- PrivateKey --------------------------------------------//

    impl ConsensusEncoding for PrivateKey {
        fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
            writer.write(self.as_bytes())
        }
    }

    impl ConsensusEncodingSized for PrivateKey {
        fn consensus_encode_exact_size(&self) -> usize {
            PrivateKey::key_length()
        }
    }

    impl ConsensusDecoding for PrivateKey {
        fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
            let mut buf = [0u8; 32];
            reader.read_exact(&mut buf)?;
            let sk =
                PrivateKey::from_bytes(&buf[..]).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
            Ok(sk)
        }
    }

    //---------------------------------- Commitment --------------------------------------------//

    impl ConsensusEncoding for Commitment {
        fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
            let buf = self.as_bytes();
            let len = buf.len();
            writer.write_all(buf)?;
            Ok(len)
        }
    }

    impl ConsensusEncodingSized for Commitment {
        fn consensus_encode_exact_size(&self) -> usize {
            32
        }
    }

    impl ConsensusDecoding for Commitment {
        fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
            let mut buf = [0u8; 32];
            reader.read_exact(&mut buf)?;
            let commitment =
                Commitment::from_bytes(&buf[..]).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
            Ok(commitment)
        }
    }

    //---------------------------------- Signature --------------------------------------------//

    impl ConsensusEncoding for Signature {
        fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
            let pub_nonce = self.get_public_nonce().as_bytes();
            let mut written = pub_nonce.len();
            writer.write_all(pub_nonce)?;
            let sig = self.get_signature().as_bytes();
            written += sig.len();
            writer.write_all(sig)?;
            Ok(written)
        }
    }

    impl ConsensusEncodingSized for Signature {
        fn consensus_encode_exact_size(&self) -> usize {
            self.get_signature().consensus_encode_exact_size() + self.get_public_nonce().consensus_encode_exact_size()
        }
    }

    impl ConsensusDecoding for Signature {
        fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
            let mut buf = [0u8; PublicKey::key_length()];
            reader.read_exact(&mut buf)?;
            let pub_nonce =
                PublicKey::from_bytes(&buf[..]).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
            let mut buf = [0u8; PrivateKey::key_length()];
            reader.read_exact(&mut buf)?;
            let sig =
                PrivateKey::from_bytes(&buf[..]).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
            Ok(Signature::new(pub_nonce, sig))
        }
    }
}
