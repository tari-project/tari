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
//

use core::sync::atomic::AtomicBool;
use std::sync::Arc;
use tari_broadcast_channel::Subscriber;
use tari_core::{
    base_node::{states::BaseNodeState, LocalNodeCommsInterface},
    chain_storage::BlockchainBackend,
    consensus::ConsensusManager,
    mining::Miner,
};
use tari_service_framework::handles::ServiceHandles;

pub fn build_miner<B: BlockchainBackend + 'static, H: AsRef<ServiceHandles>>(
    handles: H,
    stop_flag: Arc<AtomicBool>,
    event_stream: Subscriber<BaseNodeState>,
    consensus_manager: ConsensusManager<B>,
    num_threads: usize,
) -> Miner<B>
{
    let handles = handles.as_ref();
    let node_local_interface = handles.get_handle::<LocalNodeCommsInterface>().unwrap();
    let mut miner = Miner::new(stop_flag, consensus_manager, &node_local_interface, num_threads);
    miner.subscribe_to_state_change(event_stream);
    miner
}
