//  Copyright 2019 The Tari Project
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

use std::{
    sync::mpsc::{sync_channel, RecvTimeoutError, SyncSender},
    thread,
    time::Duration,
};

use crate::connection::{
    connection::Connection,
    message::Frame,
    peer_connection::{control::ControlMessage, PeerConnectionContext, PeerConnectionError},
    types::Direction,
    ConnectionError,
};

macro_rules! try_recv {
    ($inner: expr) => {
        match $inner {
            Ok(f) => Some(f),
            Err(e) => match e {
                ConnectionError::Timeout => None,
                _ => break Err(e),
            },
        }
    };
}

/// Take the given PeerConnectionContext and use it to start a worker thread which:
/// - Establishes a connection to peer
/// - Establishes a connection to the message consumer
/// - Receives and handles ControlMessages
///
/// # Arguments
///
/// `context` - The PeerConnectionContext which will be owned by the worker thread
pub(super) fn start_thread(context: PeerConnectionContext) -> SyncSender<ControlMessage> {
    let (sender, receiver) = sync_channel(5);

    let mut identity: Option<Frame> = None;

    thread::spawn(move || {
        let peer_conn = Connection::new(&context.context, context.direction.clone())
            .set_curve_encryption(context.curve_encryption)
            .set_receive_hwm(10)
            .set_send_hwm(10)
            .set_socks_proxy_addr(context.socks_address)
            .set_max_message_size(Some(context.max_msg_size))
            .establish(&context.peer_address)?;

        let consumer = Connection::new(&context.context, Direction::Outbound).establish(&context.consumer_address)?;

        loop {
            match receiver.recv_timeout(Duration::from_millis(5)) {
                Ok(msg) => match msg {
                    ControlMessage::Shutdown => break Ok(()),
                    ControlMessage::SendMsg(frames) => {
                        match context.direction {
                            Direction::Outbound => peer_conn.send(frames)?,

                            // Add identity frame to the front of the payload for ROUTER socket
                            Direction::Inbound => match identity {
                                Some(ref id) => {
                                    let mut payload = vec![id.clone()];
                                    payload.extend(frames);
                                    peer_conn.send(payload)?;
                                },
                                None => break Err(PeerConnectionError::IdentityNotEstablished.into()),
                            },
                        }
                    },
                },
                Err(e) => match e {
                    RecvTimeoutError::Disconnected => {
                        break Err(PeerConnectionError::ControlPortDisconnected.into());
                    },
                    RecvTimeoutError::Timeout => {},
                },
            }

            if let Some(frames) = try_recv!(peer_conn.receive(10)) {
                match context.direction {
                    // For a ROUTER socket, the first frame is the identity
                    Direction::Inbound => {
                        match identity {
                            Some(ref ident) => {
                                if frames[0] != *ident {
                                    break Err(PeerConnectionError::UnexpectedIdentity.into());
                                }
                            },
                            None => {
                                identity = Some(frames[0].clone());
                            },
                        }

                        consumer.send(&frames[1..])?;
                    },
                    Direction::Outbound => consumer.send(frames)?,
                }
            }
        }
    });

    sender
}
