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

use blake2::Blake2b;
use digest::consts::U64;
use tari_hashing::ConfidentialOutputHashDomain;

use crate::consensus::DomainSeparatedConsensusHasher;

pub mod borsh;
pub mod byte_counter;
pub mod limited_reader;
pub mod one_sided;

#[cfg(feature = "base_node")]
pub mod rolling_avg;
#[cfg(feature = "base_node")]
pub mod rolling_vec;
pub(crate) mod waiting_requests;

/// Hasher used in the DAN to derive masks and encrypted value keys
pub type ConfidentialOutputHasher = DomainSeparatedConsensusHasher<ConfidentialOutputHashDomain, Blake2b<U64>>;

/// The reason for a peer being banned
#[derive(Clone, Debug)]
pub struct BanReason {
    /// The reason for the ban
    pub reason: String,
    /// The duration of the ban
    pub ban_duration: BanPeriod,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BanPeriod {
    Short,
    Long,
}

impl BanReason {
    /// Create a new ban reason
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// The duration of the ban
    pub fn ban_duration(&self) -> BanPeriod {
        self.ban_duration
    }
}
