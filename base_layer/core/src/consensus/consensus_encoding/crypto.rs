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
    io::{Error, Read, Write},
};

use tari_common_types::types::{ComSignature, Commitment, PrivateKey, PublicKey, RangeProof, Signature};
use tari_crypto::keys::{PublicKey as PublicKeyTrait, SecretKey};
use tari_utilities::ByteArray;

use crate::consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized, MaxSizeBytes};

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
        let sk = PrivateKey::from_bytes(&buf[..]).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
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
        PublicKey::key_length()
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
        let mut written = self.get_public_nonce().consensus_encode(writer)?;
        written += self.get_signature().consensus_encode(writer)?;
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
        let pub_nonce = PublicKey::consensus_decode(reader)?;
        let sig = PrivateKey::consensus_decode(reader)?;
        Ok(Signature::new(pub_nonce, sig))
    }
}

//---------------------------------- RangeProof --------------------------------------------//

impl ConsensusEncoding for RangeProof {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, Error> {
        self.0.consensus_encode(writer)
    }
}

impl ConsensusEncodingSized for RangeProof {
    fn consensus_encode_exact_size(&self) -> usize {
        self.0.consensus_encode_exact_size()
    }
}

impl ConsensusDecoding for RangeProof {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        const MAX_RANGEPROOF_SIZE: usize = 1024;
        let bytes = MaxSizeBytes::<MAX_RANGEPROOF_SIZE>::consensus_decode(reader)?;
        Ok(Self(bytes.into()))
    }
}

//---------------------------------- Commitment Signature --------------------------------------------//

impl ConsensusEncoding for ComSignature {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, Error> {
        let mut written = self.u().consensus_encode(writer)?;
        written += self.v().consensus_encode(writer)?;
        written += self.public_nonce().consensus_encode(writer)?;
        Ok(written)
    }
}

impl ConsensusEncodingSized for ComSignature {
    fn consensus_encode_exact_size(&self) -> usize {
        PrivateKey::key_length() * 2 + PublicKey::key_length()
    }
}

impl ConsensusDecoding for ComSignature {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let u = PrivateKey::consensus_decode(reader)?;
        let v = PrivateKey::consensus_decode(reader)?;
        let nonce = Commitment::consensus_decode(reader)?;
        Ok(ComSignature::new(nonce, u, v))
    }
}

#[cfg(test)]
mod test {
    use rand::{rngs::OsRng, RngCore};
    use tari_crypto::range_proof::RangeProofService;

    use super::*;
    use crate::{consensus::check_consensus_encoding_correctness, transactions::CryptoFactories};

    mod keys {
        use super::*;

        #[test]
        fn it_encodes_and_decodes_correctly() {
            let (_, subject) = PublicKey::random_keypair(&mut OsRng);
            check_consensus_encoding_correctness(subject).unwrap();
            let (subject, _) = PublicKey::random_keypair(&mut OsRng);
            check_consensus_encoding_correctness(subject).unwrap();
        }
    }

    mod commitment {
        use super::*;

        #[test]
        fn it_encodes_and_decodes_correctly() {
            let (_, p) = PublicKey::random_keypair(&mut OsRng);
            let subject = Commitment::from_public_key(&p);
            check_consensus_encoding_correctness(subject).unwrap();
        }
    }

    mod signature {
        use super::*;

        #[test]
        fn it_encodes_and_decodes_correctly() {
            let (k, p) = PublicKey::random_keypair(&mut OsRng);
            let subject = Signature::new(p, k);
            check_consensus_encoding_correctness(subject).unwrap();
        }
    }

    mod range_proof {
        use super::*;

        #[test]
        fn it_encodes_and_decodes_correctly() {
            let k = PrivateKey::random(&mut OsRng);
            let subject = RangeProof::from_bytes(
                &CryptoFactories::default()
                    .range_proof
                    .construct_proof(&k, OsRng.next_u64())
                    .unwrap(),
            )
            .unwrap();
            check_consensus_encoding_correctness(subject).unwrap();
        }
    }
}
