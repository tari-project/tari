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

use crate::{
    base_node::comms_interface::{
        CommsInterfaceError,
        InboundNodeCommsInterface,
        NodeCommsRequest,
        NodeCommsResponse,
        OutboundNodeCommsInterface,
    },
    chain_storage::{BlockchainDatabase, ChainMetadata, MemoryDatabase},
    proof_of_work::Difficulty,
    types::HashDigest,
};
use futures::{executor::block_on, StreamExt};
use std::sync::Arc;
use tari_service_framework::{reply_channel, reply_channel::Receiver};

async fn test_request_responder(
    receiver: &mut Receiver<NodeCommsRequest, Result<Vec<NodeCommsResponse>, CommsInterfaceError>>,
    response: Vec<NodeCommsResponse>,
)
{
    let req_context = receiver.next().await.unwrap();
    req_context.reply(Ok(response)).unwrap()
}

#[test]
fn outbound_get_metadata() {
    let (sender, mut receiver) = reply_channel::unbounded();
    let mut outbound_nci = OutboundNodeCommsInterface::new(sender);

    block_on(async {
        let metadata1 = ChainMetadata::new(5, vec![0u8], 2.into(), 3);
        let metadata2 = ChainMetadata::new(6, vec![1u8], 3.into(), 4);
        let metadata_response: Vec<NodeCommsResponse> = vec![
            NodeCommsResponse::ChainMetadata(metadata1.clone()),
            NodeCommsResponse::ChainMetadata(metadata2.clone()),
        ];
        let (received_metadata, _) = futures::join!(
            outbound_nci.get_metadata(),
            test_request_responder(&mut receiver, metadata_response)
        );
        let received_metadata = received_metadata.unwrap();
        assert_eq!(received_metadata.len(), 2);
        assert!(received_metadata.contains(&metadata1));
        assert!(received_metadata.contains(&metadata2));
    });
}

#[test]
fn inbound_get_metadata() {
    let store = Arc::new(BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap());
    let inbound_nci = InboundNodeCommsInterface::new(store);

    block_on(async {
        if let Ok(NodeCommsResponse::ChainMetadata(received_metadata)) =
            inbound_nci.handle_request(&NodeCommsRequest::GetChainMetadata).await
        {
            assert_eq!(received_metadata.height_of_longest_chain, None);
            assert_eq!(received_metadata.best_block, None);
            assert_eq!(received_metadata.total_accumulated_difficulty, Difficulty::from(0));
            assert_eq!(received_metadata.pruning_horizon, 0);
        } else {
            assert!(false);
        }
    });
}
