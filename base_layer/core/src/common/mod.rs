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

use tari_max_size::MaxSizeVec;

pub(crate) mod inbound_backpressure;
pub mod rolling_avg;
pub mod rolling_vec;
pub(crate) mod waiting_requests;

/// The maximum number of merge mined aux chain hashes, the bound of [`AuxChainHashes`].
pub const MAX_AUX_CHAIN_HASHES: usize = 128;

/// AuxChainHashes is a vector of limited size (at most [`MAX_AUX_CHAIN_HASHES`] merge mined aux chain hashes).
///
/// It is only used when *building* Monero merge mining pow data (the merge mining proxy); a node never decodes an
/// `AuxChainHashes`, so this bound is not itself a decode-time consensus rule. What a node decodes is the aux chain
/// merkle proof built from it, whose branch length has its own (larger) decode bound. The bound must stay small
/// enough that a proof over this many aux chains still decodes; `aux_chain_hashes_bound_fits_the_merkle_proof_bound`
/// checks that.
pub type AuxChainHashes = MaxSizeVec<monero::Hash, MAX_AUX_CHAIN_HASHES>;

pub type RequestKey = u64;
