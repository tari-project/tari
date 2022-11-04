// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    io,
    io::{Error, Read, Write},
};

use tari_crypto::{
    keys::{PublicKey as PublicKeyTrait, SecretKey},
    ristretto::{
        pedersen::PedersenCommitment,
        RistrettoComSig,
        RistrettoPublicKey,
        RistrettoSchnorr,
        RistrettoSecretKey,
    },
};
use tari_utilities::ByteArray;

use crate::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized};

//---------------------------------- PublicKey --------------------------------------------//

impl ConsensusEncoding for RistrettoPublicKey {
    fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        writer.write_all(self.as_bytes())?;
        Ok(())
    }
}

impl ConsensusEncodingSized for RistrettoPublicKey {
    fn consensus_encode_exact_size(&self) -> usize {
        RistrettoPublicKey::key_length()
    }
}

impl ConsensusDecoding for RistrettoPublicKey {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 32];
        reader.read_exact(&mut buf)?;
        let pk =
            RistrettoPublicKey::from_bytes(&buf[..]).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        Ok(pk)
    }
}

//---------------------------------- PrivateKey --------------------------------------------//

impl ConsensusEncoding for RistrettoSecretKey {
    fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        writer.write_all(self.as_bytes())?;
        Ok(())
    }
}

impl ConsensusEncodingSized for RistrettoSecretKey {
    fn consensus_encode_exact_size(&self) -> usize {
        RistrettoSecretKey::key_length()
    }
}

impl ConsensusDecoding for RistrettoSecretKey {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 32];
        reader.read_exact(&mut buf)?;
        let sk =
            RistrettoSecretKey::from_bytes(&buf[..]).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        Ok(sk)
    }
}

//---------------------------------- Commitment --------------------------------------------//

impl ConsensusEncoding for PedersenCommitment {
    fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        let buf = self.as_bytes();
        writer.write_all(buf)?;
        Ok(())
    }
}

impl ConsensusEncodingSized for PedersenCommitment {
    fn consensus_encode_exact_size(&self) -> usize {
        RistrettoPublicKey::key_length()
    }
}

impl ConsensusDecoding for PedersenCommitment {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 32];
        reader.read_exact(&mut buf)?;
        let commitment =
            PedersenCommitment::from_bytes(&buf[..]).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        Ok(commitment)
    }
}

//---------------------------------- Signature --------------------------------------------//

impl ConsensusEncoding for RistrettoSchnorr {
    fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        self.get_public_nonce().consensus_encode(writer)?;
        self.get_signature().consensus_encode(writer)?;
        Ok(())
    }
}

impl ConsensusEncodingSized for RistrettoSchnorr {
    fn consensus_encode_exact_size(&self) -> usize {
        self.get_signature().consensus_encode_exact_size() + self.get_public_nonce().consensus_encode_exact_size()
    }
}

impl ConsensusDecoding for RistrettoSchnorr {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let pub_nonce = RistrettoPublicKey::consensus_decode(reader)?;
        let sig = RistrettoSecretKey::consensus_decode(reader)?;
        Ok(RistrettoSchnorr::new(pub_nonce, sig))
    }
}

//---------------------------------- Commitment Signature --------------------------------------------//

impl ConsensusEncoding for RistrettoComSig {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.u().consensus_encode(writer)?;
        self.v().consensus_encode(writer)?;
        self.public_nonce().consensus_encode(writer)?;
        Ok(())
    }
}

impl ConsensusEncodingSized for RistrettoComSig {
    fn consensus_encode_exact_size(&self) -> usize {
        RistrettoSecretKey::key_length() * 2 + RistrettoPublicKey::key_length()
    }
}

impl ConsensusDecoding for RistrettoComSig {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let u = RistrettoSecretKey::consensus_decode(reader)?;
        let v = RistrettoSecretKey::consensus_decode(reader)?;
        let nonce = PedersenCommitment::consensus_decode(reader)?;
        Ok(RistrettoComSig::new(nonce, u, v))
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
