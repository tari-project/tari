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

use std::{io, io::Read};

use tari_common_types::types::{Commitment, PrivateKey, PublicKey, Signature};
use tari_crypto::keys::{PublicKey as PublicKeyTrait, SecretKey};
use tari_utilities::ByteArray;

use crate::consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized};

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
        let mut buf = [0u8; 32];
        reader.read_exact(&mut buf)?;
        let pub_nonce =
            PublicKey::from_bytes(&buf[..]).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        let mut buf = [0u8; 32];
        reader.read_exact(&mut buf)?;
        let sig = PrivateKey::from_bytes(&buf[..]).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        Ok(Signature::new(pub_nonce, sig))
    }
}
