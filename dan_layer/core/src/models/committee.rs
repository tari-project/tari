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

use crate::{models::ViewId, services::infrastructure_services::NodeAddressable};

#[derive(Clone)]
pub struct Committee<TAddr: NodeAddressable> {
    // TODO: encapsulate
    pub members: Vec<TAddr>,
}

impl<TAddr: NodeAddressable> Committee<TAddr> {
    pub fn new(members: Vec<TAddr>) -> Self {
        Self { members }
    }

    pub fn leader_for_view(&self, view_id: ViewId) -> &TAddr {
        let pos = view_id.current_leader(self.members.len());
        &self.members[pos]
    }

    pub fn consensus_threshold(&self) -> usize {
        let len = self.members.len();
        let max_failures = (len - 1) / 3;
        len - max_failures
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn contains(&self, member: &TAddr) -> bool {
        self.members.contains(member)
    }
}

impl<TAddr: NodeAddressable> IntoIterator for Committee<TAddr> {
    type IntoIter = std::vec::IntoIter<Self::Item>;
    type Item = TAddr;

    fn into_iter(self) -> Self::IntoIter {
        self.members.into_iter()
    }
}
