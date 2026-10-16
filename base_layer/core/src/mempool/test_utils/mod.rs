//  Copyright 2020, The Tari Project
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

pub mod mock;

/// A valid one-input, one-output transaction whose output covenant field holds `covenant`, behind its borsh (varint)
/// length prefix
#[cfg(test)]
pub(crate) fn transaction_proto_with_covenant(covenant: &[u8]) -> crate::proto::types::Transaction {
    use tari_transaction_components::{MicroMinotari, key_manager::KeyManager, tx};

    let key_manager = KeyManager::new_random().unwrap();
    let tx = tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
        .expect("Failed to get tx")
        .0;
    let mut tx = crate::proto::types::Transaction::try_from(tx).unwrap();
    let mut field = Vec::new();
    prost::encoding::encode_varint(covenant.len() as u64, &mut field);
    field.extend_from_slice(covenant);
    tx.body.as_mut().unwrap().outputs.first_mut().unwrap().covenant = field;
    tx
}

/// A covenant of 129 tokens: 63 `identity()` tokens and a nested covenant (0x05, length 65) of 65 more, one over the
/// total token limit
#[cfg(test)]
pub(crate) fn covenant_over_token_limit() -> Vec<u8> {
    [vec![0x20; 63], vec![0x05, 65], vec![0x20; 65]].concat()
}
