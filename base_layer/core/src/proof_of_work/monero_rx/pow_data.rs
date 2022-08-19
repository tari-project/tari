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

use std::{
    fmt,
    fmt::{Display, Formatter},
    io,
    io::{Read, Write},
};

use monero::{
    consensus::{Decodable, Encodable},
    cryptonote::hash::Hashable,
};
use tari_utilities::hex::{to_hex, Hex};

use super::{error::MergeMineError, fixed_array::FixedByteArray, merkle_tree::MerkleProof};
use crate::{
    blocks::BlockHeader,
    consensus::{ConsensusDecoding, ConsensusEncoding, FromConsensusBytes},
    proof_of_work::monero_rx::helpers::create_block_hashing_blob,
};

/// This is a struct to deserialize the data from he pow field into data required for the randomX Monero merged mine
/// pow.
#[derive(Clone, Debug)]
pub struct MoneroPowData {
    /// Monero header fields
    pub header: monero::BlockHeader,
    /// RandomX vm key - the key length varies to a maximum length of 60. We'll allow a up to 63 bytes represented in
    /// fixed 64-byte struct (63 bytes + 1-byte length).
    pub randomx_key: FixedByteArray,
    /// The number of transactions included in this Monero block. This is used to produce the blockhashing_blob
    pub transaction_count: u16,
    /// Transaction root
    pub merkle_root: monero::Hash,
    /// Coinbase merkle proof hashes
    pub coinbase_merkle_proof: MerkleProof,
    /// Coinbase tx from Monero
    pub coinbase_tx: monero::Transaction,
}

impl MoneroPowData {
    pub fn from_header(tari_header: &BlockHeader) -> Result<MoneroPowData, MergeMineError> {
        MoneroPowData::from_consensus_bytes(tari_header.pow.pow_data.as_slice())
            .map_err(|e| MergeMineError::DeserializeError(format!("{:?}", e)))
    }

    /// Returns true if the coinbase merkle proof produces the `merkle_root` hash, otherwise false
    pub fn is_valid_merkle_root(&self) -> bool {
        let coinbase_hash = self.coinbase_tx.hash();
        let merkle_root = self.coinbase_merkle_proof.calculate_root(&coinbase_hash);
        self.merkle_root == merkle_root
    }

    pub fn to_blockhashing_blob(&self) -> Vec<u8> {
        create_block_hashing_blob(&self.header, &self.merkle_root, u64::from(self.transaction_count))
    }

    pub fn randomx_key(&self) -> &[u8] {
        self.randomx_key.as_slice()
    }
}

impl Display for MoneroPowData {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        writeln!(fmt, "MoneroBlockHeader: {} ", self.header)?;
        writeln!(fmt, "RandomX vm key: {}", self.randomx_key.to_hex())?;
        writeln!(fmt, "Monero tx count: {}", self.transaction_count)?;
        writeln!(fmt, "Monero tx root: {}", to_hex(self.merkle_root.as_bytes()))?;
        writeln!(fmt, "Monero coinbase tx: {}", self.coinbase_tx)
    }
}

impl ConsensusDecoding for MoneroPowData {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        Ok(Self {
            header: Decodable::consensus_decode(reader).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Could not decode Monero header {}", e),
                )
            })?,
            randomx_key: ConsensusDecoding::consensus_decode(reader)?,
            transaction_count: ConsensusDecoding::consensus_decode(reader)?,
            merkle_root: Decodable::consensus_decode(reader).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Could not decode Monero merkle header {}", e),
                )
            })?,
            coinbase_merkle_proof: ConsensusDecoding::consensus_decode(reader)?,
            coinbase_tx: Decodable::consensus_decode(reader).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Could not decode Monero coinbase transaction {}", e),
                )
            })?,
        })
    }
}

impl ConsensusEncoding for MoneroPowData {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        let _ = self.header.consensus_encode(writer)?;
        self.randomx_key.consensus_encode(writer)?;
        ConsensusEncoding::consensus_encode(&self.transaction_count, writer)?;
        let _ = self.merkle_root.consensus_encode(writer)?;
        self.coinbase_merkle_proof.consensus_encode(writer)?;
        let _ = self.coinbase_tx.consensus_encode(writer)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use tari_utilities::hex::from_hex;

    use super::*;

    const POW_DATA_BLOB: &str = "1010989af89706d7fc36490967c52552f5f970b3e71857145426d55f19a0f291aad87fe3949ca7ab2b03002098a7ff37940ab2a8199192b6468d7704b1a46b37aa533298c8b020c2945f36485088afcd6c40c6d6b5fba15ffc256d7bdfdc7879e98287803d9602752df500e35b066d1cf333fcce964b72063915f082d730c708859a0e9288241bfdd9c3c6b471a432a8434282ada7df2675826e086c85b0085bef38b88f2984790553d4925e74f445cc42a810ed9ae296f7e105e5da77e8c58c51fe3e6f1b122c94ae2e27ecffff8511d9dc3554b49d41c9acdaccab04452126e4e2d897d09d49a794e192cd51b76b52628bed70ddb8a3f755035e4e6f23eda8e01e5af885f07c5e5ec742307c88f4446cf32225f52bf019ef198fa2f3957937b6ba96366c731ee47212be92ac5e06000292a9a40101ffd6a8a40101b9f998fcd81103502bb7087b807c5f4fec15891983ac05d05412e5900ca47e6bdf31d7e2c55082574d01ffb9bb5f384f2725a21e36b44fb100791f7259066d7982d616950981e9ce77010208e74c7cee8930e6800300020c4db762c76a89966cebe345f55f725a59c6cbba8630cc0b6bae388718dd1f00";

    #[test]
    fn consensus_serialization() {
        let bytes = from_hex(POW_DATA_BLOB).unwrap();
        let data =
            MoneroPowData::from_consensus_bytes(bytes.as_slice()).expect("If this fails then consensus has changed");
        assert_eq!(data.transaction_count, 80);
        assert_eq!(data.coinbase_merkle_proof.branch().len(), 6);
        assert_eq!(bytes.len(), 435);
        let mut ser = Vec::new();
        data.consensus_encode(&mut ser).unwrap();
        assert_eq!(ser, bytes);
    }

    #[test]
    fn consensus_deserialize_reject_extra_bytes() {
        let mut bytes = from_hex(POW_DATA_BLOB).unwrap();
        bytes.extend(&[0u8; 10]);

        let _err = MoneroPowData::from_consensus_bytes(bytes.as_slice()).unwrap_err();

        let mut bytes = from_hex(POW_DATA_BLOB).unwrap();
        bytes.push(1);
        let _err = MoneroPowData::from_consensus_bytes(bytes.as_slice()).unwrap_err();
    }
}
