// Copyright 2020. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

pub mod crossterm_events;
pub mod db;
pub mod events;
pub mod signal_handler;

use std::sync::OnceLock;

use tari_common::configuration::Network;
use tari_common_types::types::FixedHash;
use tari_core::blocks::genesis_block::get_genesis_block;

/// A header hash every base node of this network holds, pruned or not: `query_deleted_utxos` refuses a request whose
/// `must_include_header` is not a header in the node's chain.
pub fn genesis_header_hash() -> FixedHash {
    static HASH: OnceLock<FixedHash> = OnceLock::new();
    *HASH.get_or_init(|| *get_genesis_block(Network::get_current_or_user_setting_or_default()).hash())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn genesis_header_hash_is_this_networks_genesis_header() {
        let genesis = get_genesis_block(Network::get_current_or_user_setting_or_default());
        assert_eq!(genesis_header_hash(), *genesis.hash());
        assert_eq!(genesis_header_hash(), genesis.header().hash());
    }
}
