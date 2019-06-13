// Copyright 2019 The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE

#[cfg(test)]
mod test {
    use crate::support::{
        factories::{self, TestFactory},
        helpers::ConnectionMessageCounter,
    };
    use chrono;
    use std::{convert::TryFrom, sync::Arc, thread, time::Duration};
    use tari_comms::{
        connection::{Connection, ConnectionError, Direction, InprocAddress, NetAddress, ZmqContext},
        connection_manager::{ConnectionManager, PeerConnectionConfig},
        control_service::{ControlService, ControlServiceConfig},
        message::{FrameSet, MessageEnvelope, MessageFlags},
        outbound_message_service::{
            outbound_message_pool::OutboundMessagePoolConfig,
            outbound_message_service::OutboundMessageService,
            BroadcastStrategy,
            OutboundMessage,
            OutboundMessagePool,
        },
        peer_manager::{Peer, PeerManager},
        types::{CommsDataStore, CommsPublicKey},
    };

    fn make_peer_connection_config(consumer_address: InprocAddress) -> PeerConnectionConfig {
        PeerConnectionConfig {
            control_service_establish_timeout: Duration::from_millis(2000),
            peer_connection_establish_timeout: Duration::from_secs(5),
            max_message_size: 1024,
            host: "127.0.0.1".parse().unwrap(),
            max_connect_retries: 3,
            message_sink_address: consumer_address,
            socks_proxy_address: None,
        }
    }

    fn make_peer_manager(peers: Vec<Peer<CommsPublicKey>>) -> Arc<PeerManager<CommsPublicKey, CommsDataStore>> {
        Arc::new(factories::peer_manager::create().with_peers(peers).build().unwrap())
    }

    pub fn init() {
        let _ = simple_logger::init();
    }

    #[test]
    #[allow(non_snake_case)]
    fn test_outbound_message_pool() {
        init();
        let context = ZmqContext::new();
        let node_identity = Arc::new(factories::node_identity::create::<CommsPublicKey>().build().unwrap());

        //---------------------------------- Node B Setup --------------------------------------------//

        let node_B_consumer_address = InprocAddress::random();
        let node_B_control_port_address: NetAddress = "127.0.0.1:45899".parse().unwrap();

        let node_B_msg_counter = ConnectionMessageCounter::new(&context);
        node_B_msg_counter.start(node_B_consumer_address.clone());

        let node_B_peer = factories::peer::create()
            .with_net_addresses(vec![node_B_control_port_address.clone()])
            // Set node B's secret key to be the same as node A's so that we can generate the same shared secret
            .with_public_key(node_identity.identity.public_key.clone())
            .build()
            .unwrap();

        // Node B knows no peers
        let node_B_peer_manager = make_peer_manager(vec![]);
        let node_B_connection_manager = Arc::new(ConnectionManager::new(
            context.clone(),
            node_identity.clone(),
            node_B_peer_manager,
            make_peer_connection_config(node_B_consumer_address.clone()),
        ));

        // Start node B's control service
        let node_B_control_service =
            ControlService::new(context.clone(), node_identity.clone(), ControlServiceConfig {
                socks_proxy_address: None,
                listener_address: node_B_control_port_address,
                accept_message_type: "CUSTOM".to_string(),
            })
            .serve(node_B_connection_manager)
            .unwrap();

        //---------------------------------- Node A setup --------------------------------------------//

        let node_A_consumer_address = InprocAddress::random();

        // Add node B to node A's peer manager
        let node_A_peer_manager = make_peer_manager(vec![node_B_peer.clone()]);
        let node_A_connection_manager = Arc::new(
            factories::connection_manager::create()
                .with_peer_manager(node_A_peer_manager.clone())
                .with_peer_connection_config(make_peer_connection_config(node_A_consumer_address))
                .build()
                .unwrap(),
        );

        // Setup Node A OMP and OMS
        let omp_inbound_address = InprocAddress::random();
        let omp_config = OutboundMessagePoolConfig::default();
        let omp = OutboundMessagePool::new(
            omp_config.clone(),
            context.clone(),
            omp_inbound_address.clone(),
            omp_inbound_address.clone(),
            node_A_peer_manager.clone(),
            node_A_connection_manager.clone(),
        )
        .unwrap();

        let oms = OutboundMessageService::new(
            context.clone(),
            node_identity.clone(),
            omp_inbound_address.clone(),
            node_A_peer_manager.clone(),
        )
        .unwrap();
        let oms2 = OutboundMessageService::new(
            context.clone(),
            node_identity.clone(),
            omp_inbound_address,
            node_A_peer_manager.clone(),
        )
        .unwrap();

        let _omp = omp.start();
        let message_envelope_body = vec![0, 1, 2, 3];

        // Send 8 message alternating two different OMS's
        for _ in 0..4 {
            oms.send(
                BroadcastStrategy::Direct(node_B_peer.node_id.clone()),
                MessageFlags::ENCRYPTED,
                message_envelope_body.clone(),
            )
            .unwrap();
            oms2.send(
                BroadcastStrategy::Direct(node_B_peer.node_id.clone()),
                MessageFlags::ENCRYPTED,
                message_envelope_body.clone(),
            )
            .unwrap();
        }

        node_B_msg_counter.assert_count(8, 1000);
        node_B_control_service.shutdown().unwrap();
        node_B_control_service.handle.join().unwrap().unwrap();
    }

    #[test]
    #[allow(non_snake_case)]
    fn test_outbound_message_pool_requeuing() {
        init();
        let context = ZmqContext::new();

        let node_identity = Arc::new(factories::node_identity::create::<CommsPublicKey>().build().unwrap());

        //---------------------------------- Node B Setup --------------------------------------------//
        let node_B_control_port_address: NetAddress = "127.0.0.1:45845".parse().unwrap();
        let node_B_peer = factories::peer::create()
            .with_net_addresses(vec![node_B_control_port_address.clone()])
            // Set node B's secret key to be the same as node A's so that we can generate the same shared secret
            .with_public_key(node_identity.identity.public_key.clone())
            .build()
            .unwrap();

        //---------------------------------- Node A setup --------------------------------------------//

        let node_A_consumer_address = InprocAddress::random();

        // Add node B to node A's peer manager
        let node_A_peer_manager = Arc::new(
            factories::peer_manager::create()
                .with_peers(vec![node_B_peer.clone()])
                .build()
                .unwrap(),
        );
        let node_A_connection_manager = Arc::new(ConnectionManager::new(
            context.clone(),
            node_identity.clone(),
            node_A_peer_manager.clone(),
            make_peer_connection_config(node_A_consumer_address),
        ));

        // Setup Node A OMP and OMS
        let omp_inbound_address = InprocAddress::random();
        let omp_requeue_address = InprocAddress::random();

        let omp_config = OutboundMessagePoolConfig {
            max_num_of_retries: 3,
            retry_wait_time: chrono::Duration::milliseconds(100),
            worker_timeout_in_ms: 100,
        };
        let omp = OutboundMessagePool::new(
            omp_config.clone(),
            context.clone(),
            omp_inbound_address.clone(),
            omp_requeue_address.clone(),
            node_A_peer_manager.clone(),
            node_A_connection_manager.clone(),
        )
        .unwrap();

        let oms = OutboundMessageService::new(
            context.clone(),
            node_identity.clone(),
            omp_inbound_address.clone(),
            node_A_peer_manager.clone(),
        )
        .unwrap();

        let _omp = omp.start();
        let message_envelope_body: Vec<u8> = vec![0, 1, 2, 3];

        // Now check that the requeuing happens
        let requeue_connection = Connection::new(&context.clone(), Direction::Inbound)
            .establish(&omp_requeue_address)
            .unwrap();
        let omp_inbound_connection = Connection::new(&context.clone(), Direction::Outbound)
            .establish(&omp_inbound_address)
            .unwrap();

        oms.send(
            BroadcastStrategy::Direct(node_B_peer.node_id.clone()),
            MessageFlags::ENCRYPTED,
            message_envelope_body,
        )
        .unwrap();

        // Receive first requeue
        let mut frame_set = requeue_connection.receive(1000).unwrap();
        let data: FrameSet = frame_set.drain(1..).collect();

        let msg = OutboundMessage::<MessageEnvelope>::try_from(data).unwrap();
        assert_eq!(
            node_A_peer_manager
                .find_with_node_id(&msg.destination_node_id.clone())
                .unwrap()
                .addresses
                .addresses[0]
                .connection_attempts,
            0
        );
        assert_eq!(msg.number_of_retries(), 1);
        omp_inbound_connection.send(vec![msg.to_frame().unwrap()]).unwrap();

        // Receive second requeue that happened before retry wait time elapsed
        let mut frame_set = requeue_connection.receive(1000).unwrap();
        let data: FrameSet = frame_set.drain(1..).collect();

        let msg = OutboundMessage::<MessageEnvelope>::try_from(data).unwrap();
        assert_eq!(msg.number_of_retries(), 1);
        thread::sleep(Duration::from_millis(200));
        omp_inbound_connection.send(vec![msg.to_frame().unwrap()]).unwrap();

        // Receive third requeue that happened after retry wait time elapsed
        let mut frame_set = requeue_connection.receive(1000).unwrap();
        let data: FrameSet = frame_set.drain(1..).collect();

        let msg = OutboundMessage::<MessageEnvelope>::try_from(data).unwrap();
        assert_eq!(msg.number_of_retries(), 2);
        thread::sleep(Duration::from_millis(200));
        omp_inbound_connection.send(vec![msg.to_frame().unwrap()]).unwrap();

        // Receive fourth requeue that happened after retry wait time elapsed
        let mut frame_set = requeue_connection.receive(100).unwrap();
        let data: FrameSet = frame_set.drain(1..).collect();
        let msg = OutboundMessage::<MessageEnvelope>::try_from(data).unwrap();
        assert_eq!(msg.number_of_retries(), 3);
        thread::sleep(Duration::from_millis(200));
        omp_inbound_connection.send(vec![msg.to_frame().unwrap()]).unwrap();

        // This time the requeue should not occur so this read should timeout
        assert_eq!(requeue_connection.receive(100), Err(ConnectionError::Timeout));
    }
}
