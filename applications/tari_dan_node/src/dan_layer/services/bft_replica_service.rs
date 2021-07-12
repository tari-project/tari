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

use crate::dan_layer::models::{ViewId, View};
use tari_comms::NodeIdentity;
use tari_comms::peer_manager::NodeId;

pub trait BftReplicaService {
    fn current_view(&self ) -> View;
}

pub struct ConcreteBftReplicaService {
    current_view: ViewId,
    node_identity: NodeIdentity,
    committee: Vec<NodeId>,
    position_in_committee: usize
}

impl ConcreteBftReplicaService {
    pub fn new(node_identity: NodeIdentity, committee: Vec<NodeId>) -> Self {
        let mut committee = committee;
        if !committee.contains(node_identity.node_id()) {
            committee.push(node_identity.node_id().clone());
        }

        committee.sort();
        let position_in_committee = committee.iter().position(|n| n == node_identity.node_id()).expect("NodeID should always be present since we add it");
        Self { current_view: ViewId(0), node_identity, committee, position_in_committee}
    }


}

impl BftReplicaService for ConcreteBftReplicaService{
    fn current_view(&self) -> View {
       View {
           view_id: self.current_view,
           is_leader: self.current_view.current_leader(self.committee.len()) == self.position_in_committee
       }
    }
}

