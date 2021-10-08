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

use super::{error::MergeMineError, fixed_array::FixedByteArray, merkle_tree::MerkleProof};
use crate::{
    blocks::BlockHeader,
    crypto::tari_utilities::hex::Hex,
    proof_of_work::monero_rx::{fixed_array, helpers::create_block_hashing_blob},
    tari_utilities::hex::to_hex,
};
use monero::cryptonote::hash::Hashable;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    fmt::{Display, Formatter},
};

/// This is a struct to deserialize the data from he pow field into data required for the randomX Monero merged mine
/// pow.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct MoneroPowData {
    /// Monero header fields
    pub header: monero::BlockHeader,
    /// RandomX vm key - the key length varies to a maximum length of 60. We'll allow a up to 63 bytes represented in
    /// fixed 64-byte struct (63 bytes + 1-byte length).
    pub randomx_key: FixedByteArray<{ fixed_array::MAX_ARR_SIZE }>,
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
        bincode::deserialize(&tari_header.pow.pow_data)
            .map_err(|e| MergeMineError::DeserializeError(format!("{:?}", e)))
    }

    /// Returns true if the coinbase merkle proof produces the `merkle_root` hash, otherwise false
    pub fn is_valid_merkle_root(&self) -> bool {
        let coinbase_hash = self.coinbase_tx.hash();
        let merkle_root = self.coinbase_merkle_proof.calculate_root(&coinbase_hash);
        self.merkle_root == merkle_root
    }

    pub fn to_blockhashing_blob(&self) -> Vec<u8> {
        create_block_hashing_blob(&self.header, &self.merkle_root, self.transaction_count as u64)
    }

    pub fn randomx_key(&self) -> &[u8] {
        self.randomx_key.as_slice()
    }
}

impl Display for MoneroPowData {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        writeln!(fmt, "MoneroBlockHeader: {} ", self.header)?;
        writeln!(fmt, "RandomX vm key: {}", self.randomx_key.to_hex())?;
        writeln!(fmt, "Monero tx count: {}", self.transaction_count.to_string())?;
        writeln!(fmt, "Monero tx root: {}", to_hex(self.merkle_root.as_bytes()))?;
        writeln!(fmt, "Monero coinbase tx: {}", self.coinbase_tx)
    }
}

#[cfg(test)]
mod test {
    // use crate::crypto::tari_utilities::hex::from_hex;
    // use crate::proof_of_work::monero_rx::MoneroPowData;

    // TODO: Update this test

    // const POW_DATA_BLOB: &str =
    // "0e0eff8a828606e62827cbb1c8f13eeddaae1d2c5dbb36c12a3d30d20d20b35a540bdba9d8e162604a0000202378cf4e85ef9a0629719e228c8c9807575469c3f45b3710c7960079a5dfdd661600b3cdc310a8f619ea2feadb178021ea0b853caa2f41749f7f039dcd4102d24f0504b4d72f22ca81245c538371a07331546cbd9935068637166d9cd627c521fb0e98d6161a7d971ee608b2b93719327d1cf5f95f9cc15beab7c6fb0894205c9218e4f9810873976eaf62d53ce631e8ad37bbaacc5da0267cd38342d66bdecce6541bb5c761b8ff66e7f6369cd3b0c2cb106a325c7342603516c77c9dcbb67388128a04000000000002fd873401ffc1873401c983eae58cd001026eb5be712030e2d49c9329f7f578325daa8ad7296a58985131544d8fe8a24c934d01ad27b94726423084ffc0f7eda31a8c9691836839c587664a036c3986b33f568f020861f4f1c2c37735680300916c27a920e462fbbfce5ac661ea9ef91fc78d620c61c43d5bb6a9644e3c17e000"
    // ;

    //#[test]
    // fn consensus_serialization() {
    //    let bytes = from_hex(POW_DATA_BLOB).unwrap();
    //    let data = bincode::deserialize::<MoneroPowData>(&bytes).expect("If this fails then consensus has changed");
    //    assert_eq!(data.transaction_count, 22);
    //    assert_eq!(data.coinbase_merkle_proof.branch().len(), 4);
    //    assert_eq!(bytes.len(), 374);
    //    let ser = bincode::serialize(&data).unwrap();
    //    assert_eq!(ser, bytes);
    //}

    mod fuzz {
        use monero::{consensus::deserialize, TxIn};

        #[test]
        fn simple_capacity_overflow_panic() {
            let data = &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f];
            let _ = deserialize::<Vec<TxIn>>(data).unwrap_err();
        }

        #[test]
        fn panic_alloc_capacity_overflow_moneroblock_deserialize() {
            let data = [
                0x0f, 0x9e, 0xa5, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04,
                0x00, 0x08, 0x9e, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x04, 0x9e, 0x9e, 0x9e, 0x9e, 0x9e, 0x9e, 0x9e, 0x9e,
                0x9e, 0xe7, 0xaa, 0xfd, 0x8b, 0x47, 0x06, 0x8d, 0xed, 0xe3, 0x00, 0xed, 0x44, 0xfc, 0x77, 0xd6, 0x58,
                0xf6, 0xf2, 0x69, 0x06, 0x8d, 0xed, 0xe3, 0x00, 0xed, 0x44, 0xfc, 0x77, 0xd6, 0x58, 0xf6, 0xf2, 0x69,
                0x62, 0x38, 0xdb, 0x5e, 0x4d, 0x6d, 0x9c, 0x94, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0f, 0x00,
                0x8f, 0x74, 0x3c, 0xb3, 0x1b, 0x6e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ];
            let _ = deserialize::<monero::Block>(&data).unwrap_err();
        }
    }
}
