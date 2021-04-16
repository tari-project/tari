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

use crate::mempool::MempoolError;
use futures::io;
use tari_comms::peer_manager::NodeId;
use thiserror::Error;

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub enum MempoolProtocolError {
    #[error("Transaction from peer `{0}` did not contain a kernel excess signature")]
    ExcessSignatureMissing(NodeId),
    #[error("Peer `{0}` unexpectedly closed the substream")]
    SubstreamClosed(NodeId),
    #[error("Mempool database error: {0}")]
    MempoolError(#[from] MempoolError),
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    #[error("Failed to decode message from peer `{peer}`: {source}")]
    DecodeFailed { peer: NodeId, source: prost::DecodeError },
    #[error("Wire message from `{peer}` failed to convert to local type: {message}")]
    MessageConversionFailed { peer: NodeId, message: String },
}
