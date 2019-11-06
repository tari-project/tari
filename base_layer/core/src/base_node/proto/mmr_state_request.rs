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

use crate::base_node::{
    comms_interface::MmrStateRequest,
    proto::base_node::{MmrStateRequest as ProtoMmrStateRequest, MmrTree as ProtoMmrTree},
};
use std::convert::{TryFrom, TryInto};

impl TryFrom<ProtoMmrStateRequest> for MmrStateRequest {
    type Error = String;

    fn try_from(request: ProtoMmrStateRequest) -> Result<Self, Self::Error> {
        let tree = ProtoMmrTree::from_i32(request.tree).ok_or("Invalid or unrecognised `MmrTree` enum".to_string())?;
        Ok(Self {
            tree: tree.try_into()?,
            index: request.index,
            count: request.count,
        })
    }
}

impl From<MmrStateRequest> for ProtoMmrStateRequest {
    fn from(request: MmrStateRequest) -> Self {
        let tree: ProtoMmrTree = request.tree.into();
        Self {
            tree: tree as i32,
            index: request.index,
            count: request.count,
        }
    }
}
