// Copyright 2019. The Tari Project
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

#[cfg(feature = "base_node")]
mod accumulated_data;

#[cfg(feature = "base_node")]
pub use accumulated_data::{
    BlockAccumulatedData,
    BlockHeaderAccumulatedData,
    ChainBlock,
    ChainHeader,
    CompleteDeletedBitmap,
    DeletedBitmap,
    UpdateBlockAccumulatedData,
};
use chrono::{DateTime, FixedOffset};
use tari_crypto::hash_domain;

mod error;
pub use error::BlockError;

mod block;
pub use block::{Block, BlockBuilder, BlockValidationError, NewBlock};

#[cfg(any(feature = "base_node", feature = "base_node_proto"))]
mod block_header;
#[cfg(any(feature = "base_node", feature = "base_node_proto"))]
pub use block_header::{BlockHeader, BlockHeaderValidationError};

#[cfg(feature = "base_node")]
pub mod genesis_block;

#[cfg(feature = "base_node")]
mod historical_block;
#[cfg(feature = "base_node")]
pub use historical_block::HistoricalBlock;

#[cfg(feature = "base_node")]
mod new_block_template;
#[cfg(feature = "base_node")]
pub use new_block_template::NewBlockTemplate;

#[cfg(feature = "base_node")]
mod new_blockheader_template;
#[cfg(feature = "base_node")]
pub use new_blockheader_template::NewBlockHeaderTemplate;
use tari_common::configuration::Network;

hash_domain!(BlocksHashDomain, "com.tari.base_layer.core.blocks", 0);

/// Returns the network birth date for the selected network.
pub fn network_birth_date(network: Network) -> DateTime<FixedOffset> {
    const IGOR_BIRTH_DATE: &str = "08 Aug 2022 10:00:00 +0200";
    const ESMERALDA_BIRTH_DATE: &str = "24 Aug 2022 10:00:00 +0200";
    const LOCAL_NET_BIRTH_DATE: &str = ESMERALDA_BIRTH_DATE;

    use tari_common::configuration::Network::{
        Dibbler,
        Esmeralda,
        Igor,
        LocalNet,
        MainNet,
        Ridcully,
        Stibbons,
        Weatherwax,
    };
    let birth_date = match network {
        MainNet => unimplemented!(),
        Igor => IGOR_BIRTH_DATE,
        Esmeralda => ESMERALDA_BIRTH_DATE,
        LocalNet => LOCAL_NET_BIRTH_DATE,
        Dibbler => unimplemented!("Dibbler is longer supported"),
        Ridcully => unimplemented!("Ridcully is longer supported"),
        Stibbons => unimplemented!("Stibbons is longer supported"),
        Weatherwax => unimplemented!("Weatherwax longer supported"),
    };
    DateTime::parse_from_rfc2822(birth_date).expect("'<NETWORK>_BIRTH_DATE' string error! This may not fail.")
}
