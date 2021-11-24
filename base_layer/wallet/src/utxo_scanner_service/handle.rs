// Copyright 2021. The Tari Project
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

use std::time::Duration;
use tari_comms::peer_manager::NodeId;
use tari_core::transactions::tari_amount::MicroTari;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub enum UtxoScannerEvent {
    ConnectingToBaseNode(NodeId),
    ConnectedToBaseNode(NodeId, Duration),
    ConnectionFailedToBaseNode {
        peer: NodeId,
        num_retries: usize,
        retry_limit: usize,
        error: String,
    },
    ScanningRoundFailed {
        num_retries: usize,
        retry_limit: usize,
        error: String,
    },
    /// Progress of the recovery process (current_block, current_chain_height)
    Progress {
        current_index: u64,
        total_index: u64,
    },
    /// Completed Recovery (Number scanned, Num of Recovered outputs, Value of recovered outputs, Time taken)
    Completed {
        number_scanned: u64,
        number_received: u64,
        value_received: MicroTari,
        time_taken: Duration,
    },
    /// Scanning process has failed and scanning process has exited
    ScanningFailed,
}

#[derive(Clone)]
pub struct UtxoScannerHandle {
    event_sender: broadcast::Sender<UtxoScannerEvent>,
}

impl UtxoScannerHandle {
    pub fn new(event_sender: broadcast::Sender<UtxoScannerEvent>) -> Self {
        UtxoScannerHandle { event_sender }
    }

    pub fn get_event_receiver(&mut self) -> broadcast::Receiver<UtxoScannerEvent> {
        self.event_sender.subscribe()
    }
}
