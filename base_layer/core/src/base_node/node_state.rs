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

/// The base node state represents the FSM of the base node synchronisation process.
///
/// ## Startup State
/// When the node starts up, it is in the `Startup` state. In this state, we need to obtain
/// i. The height of our chain tip,
/// ii. The height of the chain tip from the network.
///
/// Once these two values are obtained, we can move to the next state:
/// If we're behind the pruning horizon, i.e. our chain tip is not at the pruning horizon yet, then switch to
/// `LoadingHorizonState`.
/// If we're between the pruning horizon and the network chain tip, switch to `BlockSync`.
/// Otherwise switch to `Listening`
///
/// ## LoadingHorizonState
///
/// A. For each `n` from my chain tip height + 1 to the pruning horizon block (`h`):
///   1. Request block header `n`. Validate the header by:
///      - Checking that the header forms part of the chain.
///      - Confirming that the Proof of Work is correct (The nonce corresponds to the difficulty and the total
///        accumulated work has increased).
///     We could request batches of headers that are compressed; this would be more efficient, but an optimisation
///     for the future.
///   2. Request all the kernels for block `n`. Validate the kernel by
///     - Checking each kernel signature
///     - Summing the public excess and comparing it to the excess in the corresponding block header
///     - Calculating and comparing the MMR commitment in the corresponding block header
/// B. Request the MMR leaf node set and roaring bitmap for the UTXO set at block `h`.
///
/// When this information has been obtained and the horizon block has been validated (PoW, MMR roots, public excess
/// all match), then switch to `BlockSync`.
///
/// If errors occur, re-request the problematic header, kernel or Hash set. TODO: Give up after x failures
/// Full blocks received while in this state can be stored in the orphan pool until they are needed.
///
/// ## BlockSync
///
/// For each `n` from horizon block + 1 to the network chain tip, submit a request for block `n`. In this state, an
/// entire block is received, and the normal block validation and storage process is followed. The only difference
/// between `BlockSync` and `Listening` is that the former state is actively asking for blocks, while the latter is a
/// passive process.
///
/// After we have caught up on the chain, switch to `Listening`.
///
/// If errors occur, re-request the problematic block.
///
/// Give up after n failures and switch back to `Listening` (if a peer gave an erroneous chain tip and cannot provide
/// the blocks it says it has, we can switch back to `Listening` and try receive blocks passively.
///
/// Full blocks received while in this state can be stored in the orphan pool until they are needed.
///
/// ## Listening
///
/// Passively wait for new blocks to arrive, and process them accordingly.
///
/// Periodically poll peers to request the chain tip height. If we are more than one block behind the network chain
/// tip, switch to `BlockSync` mode.
///
/// ## Shutdown
///
/// Reject all new requests with a `Shutdown` message, complete current validations / tasks, flush all state if
/// required, and then shutdown.
#[derive(Clone, Debug, PartialEq)]
pub enum BaseNodeState {
    Startup,
    LoadingHorizonState,
    BlockSync,
    Listening,
    Shutdown,
}
