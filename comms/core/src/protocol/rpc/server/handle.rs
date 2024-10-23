//  Copyright 2021, The Tari Project
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

use tokio::sync::{mpsc, oneshot};

use super::RpcServerError;
use crate::peer_manager::NodeId;

#[derive(Debug)]
pub enum RpcServerRequest {
    GetNumActiveSessions(oneshot::Sender<usize>),
    GetNumActiveSessionsForPeer(NodeId, oneshot::Sender<usize>),
    CloseAllSessionsForPeer(NodeId, oneshot::Sender<usize>),
}

#[derive(Debug, Clone)]
pub struct RpcServerHandle {
    sender: mpsc::Sender<RpcServerRequest>,
}

impl RpcServerHandle {
    pub(super) fn new(sender: mpsc::Sender<RpcServerRequest>) -> Self {
        Self { sender }
    }

    pub async fn get_num_active_sessions(&mut self) -> Result<usize, RpcServerError> {
        let (req, resp) = oneshot::channel();
        self.sender
            .send(RpcServerRequest::GetNumActiveSessions(req))
            .await
            .map_err(|_| RpcServerError::RequestCanceled)?;
        resp.await.map_err(Into::into)
    }

    pub async fn get_num_active_sessions_for(&mut self, peer: NodeId) -> Result<usize, RpcServerError> {
        let (req, resp) = oneshot::channel();
        self.sender
            .send(RpcServerRequest::GetNumActiveSessionsForPeer(peer, req))
            .await
            .map_err(|_| RpcServerError::RequestCanceled)?;
        resp.await.map_err(Into::into)
    }

    pub async fn close_all_sessions_for(&mut self, peer: NodeId) -> Result<usize, RpcServerError> {
        let (req, resp) = oneshot::channel();
        self.sender
            .send(RpcServerRequest::CloseAllSessionsForPeer(peer, req))
            .await
            .map_err(|_| RpcServerError::RequestCanceled)?;
        resp.await.map_err(Into::into)
    }
}
