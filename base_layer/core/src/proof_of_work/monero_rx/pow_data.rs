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
    io::Write,
};

use borsh::{BorshDeserialize, BorshSerialize};
use monero::{
    consensus::{Decodable, Encodable},
    cryptonote::hash::Hashable,
};
use tari_utilities::hex::{to_hex, Hex};

use super::{error::MergeMineError, fixed_array::FixedByteArray, merkle_tree::MerkleProof};
use crate::{blocks::BlockHeader, proof_of_work::monero_rx::helpers::create_block_hashing_blob};

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

impl BorshSerialize for MoneroPowData {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        self.header.consensus_encode(writer)?;
        BorshSerialize::serialize(&self.randomx_key, writer)?;
        BorshSerialize::serialize(&self.transaction_count, writer)?;
        self.merkle_root.consensus_encode(writer)?;
        BorshSerialize::serialize(&self.coinbase_merkle_proof, writer)?;
        self.coinbase_tx.consensus_encode(writer)?;
        Ok(())
    }
}

impl BorshDeserialize for MoneroPowData {
    fn deserialize_reader<R>(reader: &mut R) -> Result<Self, io::Error>
    where R: io::Read {
        let header = monero::BlockHeader::consensus_decode(reader)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let randomx_key = BorshDeserialize::deserialize_reader(reader)?;
        let transaction_count = BorshDeserialize::deserialize_reader(reader)?;
        let merkle_root = monero::Hash::consensus_decode(reader)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let coinbase_merkle_proof = BorshDeserialize::deserialize_reader(reader)?;
        let coinbase_tx = monero::Transaction::consensus_decode(reader)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        Ok(Self {
            header,
            randomx_key,
            transaction_count,
            merkle_root,
            coinbase_merkle_proof,
            coinbase_tx,
        })
    }
}

impl MoneroPowData {
    /// Create a new MoneroPowData struct from the given header
    pub fn from_header(tari_header: &BlockHeader) -> Result<MoneroPowData, MergeMineError> {
        let mut v = tari_header.pow.pow_data.as_slice();
        let pow_data =
            BorshDeserialize::deserialize(&mut v).map_err(|e| MergeMineError::DeserializeError(format!("{:?}", e)))?;
        if !v.is_empty() {
            return Err(MergeMineError::DeserializeError(format!(
                "{} bytes leftover after deserialize",
                v.len()
            )));
        }
        Ok(pow_data)
    }

    /// Returns true if the coinbase merkle proof produces the `merkle_root` hash, otherwise false
    pub fn is_valid_merkle_root(&self) -> bool {
        let coinbase_hash = self.coinbase_tx.hash();
        // is the coinbase in the tx merkle root and is in position 0
        let merkle_root = self.coinbase_merkle_proof.calculate_root(&coinbase_hash);
        self.merkle_root == merkle_root
    }

    /// Returns the blockhashing_blob for the Monero block
    pub fn to_blockhashing_blob(&self) -> Vec<u8> {
        create_block_hashing_blob(&self.header, &self.merkle_root, u64::from(self.transaction_count))
    }

    /// Returns the RandomX vm key
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

#[cfg(test)]
mod test {
    use borsh::{BorshDeserialize, BorshSerialize};
    use monero::{BlockHeader, Hash, Transaction, VarInt};
    use tari_utilities::ByteArray;

    use super::MoneroPowData;
    use crate::proof_of_work::monero_rx::{merkle_tree::MerkleProof, FixedByteArray};

    #[test]
    fn test_borsh_de_serialization() {
        let monero_pow_data = MoneroPowData {
            header: BlockHeader {
                major_version: VarInt(1),
                minor_version: VarInt(2),
                timestamp: VarInt(3),
                prev_id: Hash::new([4; 32]),
                nonce: 5,
            },
            randomx_key: FixedByteArray::from_bytes(&[6, 7, 8]).unwrap(),
            transaction_count: 9,
            merkle_root: Hash::new([10; 32]),
            coinbase_merkle_proof: MerkleProof::default(),
            coinbase_tx: Transaction::default(),
        };
        let mut buf = Vec::new();
        monero_pow_data.serialize(&mut buf).unwrap();
        buf.extend_from_slice(&[1, 2, 3]);
        let buf = &mut buf.as_slice();
        MoneroPowData::deserialize(buf).unwrap();
        assert_eq!(buf, &[1, 2, 3]);
    }
}
