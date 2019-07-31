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

use crate::{
    connection::{zmq::ZmqEndpoint, Connection, ConnectionError, Direction, EstablishedConnection, ZmqContext},
    message::{DomainMessageContext, Frame, FrameSet, MessageEnvelope, MessageError},
    peer_manager::PeerNodeIdentity,
    types::CommsPublicKey,
};
use derive_error::Error;
use serde::{Deserialize, Serialize};
use std::{marker::PhantomData, time::Duration};
use tari_utilities::message_format::{MessageFormat, MessageFormatError};

#[derive(Debug, Error)]
pub enum ConnectorError {
    ConnectionError(ConnectionError),
    #[error(no_from)]
    ListenFailed(ConnectionError),
    DeserializeFailed(MessageFormatError),
    MessageError(MessageError),
    #[error(msg_embedded, non_std, no_from)]
    ExtractFrameSetError(String),
}

/// Information about the message received
#[derive(Debug)]
pub struct MessageInfo {
    pub peer_source: PeerNodeIdentity,
    pub origin_source: CommsPublicKey,
    pub message_envelope: MessageEnvelope,
}

/// # DomainConnector
///
/// Receives frames from a connection, extracts the frame at index 1, deserializes the
/// message (DomainMessageContext) and returns a ([MessageInfo], T) tuple
pub struct DomainConnector<'de> {
    connector: Connector<'de, DomainMessageContext>,
}

impl<'de> DomainConnector<'de> {
    /// Start listening for messages. The message is expected to be at index 1
    pub fn listen<A>(context: &ZmqContext, address: &A) -> Result<Self, ConnectorError>
    where A: ZmqEndpoint {
        Ok(Self {
            connector: Connector::listen(context, address, ByIndexFrameExtractor(1))?,
        })
    }

    /// Receive and deserialize a message.
    ///
    /// This method returns:
    /// `Ok((MessageInfo, T))` if a message is received within the timeout and can be deserialized
    /// `Ok(None)` if the timeout is reached before a message arrives
    /// `Err(DomainConnectorError)` if there is a connection error, or a message fails to deserialize
    pub fn receive_timeout<T>(&self, duration: Duration) -> Result<Option<(MessageInfo, T)>, ConnectorError>
    where T: MessageFormat {
        match self.connector.receive_timeout(duration)? {
            Some(domain_context) => Ok(Some((
                MessageInfo {
                    peer_source: domain_context.peer_source,
                    origin_source: domain_context.origin_source,
                    message_envelope: domain_context.message_envelope,
                },
                domain_context
                    .message
                    .deserialize_message()
                    .map_err(ConnectorError::MessageError)?,
            ))),
            None => Ok(None),
        }
    }
}

/// # Connector
///
/// The domain connector receives messages from an inbound address, and extracts and deserializes
/// two frames into what the caller specifies.
///
/// This should be used by services/protocol managers to receive messages from the [InboundMessageService].
///
/// ## Generics
///
/// T - The type into which the frame should be deserialized
pub struct Connector<'de, T> {
    connection: EstablishedConnection,
    frame_extractor: Box<dyn FrameExtractor<Error = ConnectorError> + Send + Sync>,
    _t: PhantomData<&'de T>,
}

impl<'de, T> Connector<'de, T>
where T: Serialize + Deserialize<'de>
{
    /// Start listening for messages. Frames are extracted using the given impl of FrameExtractor.
    ///
    /// ```edition2018,no_compile
    /// DomainConnector::listen_with_frame_extractor(&context, &address, |frames: &mut Frames| {
    ///    // This extractor removes the first two frames as the header and body (or panics)
    ///    Ok((
    ///       frames.remove(0),
    ///       frames.remove(0),
    ///    ))
    /// });
    pub fn listen<FE, A>(context: &ZmqContext, address: &A, frame_extractor: FE) -> Result<Self, ConnectorError>
    where
        A: ZmqEndpoint,
        FE: FrameExtractor<Error = ConnectorError>,
        FE: Send + Sync,
        FE: 'static,
    {
        let connection = Connection::new(context, Direction::Inbound)
            .establish(&address)
            .map_err(ConnectorError::ListenFailed)?;

        Ok(Self {
            connection,
            frame_extractor: Box::new(frame_extractor),
            _t: PhantomData,
        })
    }

    /// Receive and deserialize a message.
    ///
    /// This method returns:
    /// `Ok(T)` if a message is received within the timeout and can be deserialized
    /// `Ok(None)` if the timeout is reached before a message arrives
    /// `Err(DomainConnectorError)` if there is a connection error, or a message fails to deserialize
    pub fn receive_timeout(&self, duration: Duration) -> Result<Option<T>, ConnectorError>
    where T: MessageFormat {
        match connection_try!(self.connection.receive(duration.as_millis() as u32)) {
            Some(frames) => self.deserialize_from(frames).map(Some),
            None => Ok(None),
        }
    }

    fn deserialize_from(&self, mut frames: FrameSet) -> Result<T, ConnectorError>
    where T: MessageFormat {
        let frame = self.frame_extractor.extract(&mut frames)?;
        Ok(T::from_binary(&frame).map_err(ConnectorError::DeserializeFailed)?)
    }
}

/// Generic trait for which provides a generalization of zero copy [Frame] extraction from a [FrameSet]
pub trait FrameExtractor {
    type Error;

    fn extract(&self, frames: &mut FrameSet) -> Result<Frame, Self::Error>;
}

/// Impl for functions bearing the same signature as `FrameExtractor::extract`
impl<F, E> FrameExtractor for F
where F: Fn(&mut FrameSet) -> Result<Frame, E>
{
    type Error = E;

    fn extract(&self, frames: &mut FrameSet) -> Result<Frame, E> {
        (self)(frames)
    }
}

/// Specialized [FrameExtractor] which extracts the frame at a given index
struct ByIndexFrameExtractor(usize);

impl ByIndexFrameExtractor {
    /// Fetch the index
    fn index(&self) -> usize {
        self.0
    }
}

impl FrameExtractor for ByIndexFrameExtractor {
    type Error = ConnectorError;

    fn extract(&self, frames: &mut FrameSet) -> Result<Frame, ConnectorError> {
        match frames.len() {
            len if len <= self.index() => Err(ConnectorError::ExtractFrameSetError(format!(
                "Not enough frames to extract. (Frame count={}, Required count={})",
                frames.len(),
                self.index() + 1
            ))),
            _ => Ok(frames.remove(self.index())),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        connection::InprocAddress,
        message::{Message, MessageFlags, MessageHeader, NodeDestination},
        peer_manager::NodeIdentity,
    };
    use std::sync::Arc;

    #[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
    struct TestMessage {
        from: String,
        to: String,
        poem: String,
    }

    #[test]
    fn index_extractor() {
        let mut frames = vec![
            "body".as_bytes().to_vec(),
            "fizz".as_bytes().to_vec(),
            "buzz".as_bytes().to_vec(),
        ];
        let body = ByIndexFrameExtractor(0).extract(&mut frames).unwrap();
        assert_eq!(String::from_utf8_lossy(body.as_slice()), "body");
        assert_eq!(String::from_utf8_lossy(&frames[0]), "fizz");
        assert_eq!(String::from_utf8_lossy(&frames[1]), "buzz");
    }

    #[test]
    fn index_extractor_not_enough_frames() {
        let mut frames = vec![
            "some".as_bytes().to_vec(),
            "random".as_bytes().to_vec(),
            "frames".as_bytes().to_vec(),
        ];

        let result = ByIndexFrameExtractor(4).extract(&mut frames);

        match result {
            Ok(_) => panic!("Unexpected Ok result"),
            Err(ConnectorError::ExtractFrameSetError(_)) => {},
            Err(err) => panic!("Unexpected error {:?}", err),
        }
    }

    fn second_frame_extractor(frames: &mut FrameSet) -> Result<Frame, ConnectorError> {
        Ok(frames.remove(1))
    }

    #[test]
    fn connector_connect() {
        let context = ZmqContext::new();
        let addr = InprocAddress::random();
        let connector = Connector::<TestMessage>::listen(&context, &addr, second_frame_extractor).unwrap();

        assert!(connector.connection.get_connected_address().is_none());
        assert_eq!(
            connector.connection.get_socket().get_last_endpoint().unwrap().unwrap(),
            addr.to_zmq_endpoint()
        );
    }

    #[test]
    fn connector_receive() {
        let context = ZmqContext::new();
        let addr = InprocAddress::random();

        let connector = Connector::listen(&context, &addr, second_frame_extractor).unwrap();

        let source = Connection::new(&context, Direction::Outbound).establish(&addr).unwrap();

        let expected_message = TestMessage {
            from: "Cryptokitties".to_string(),
            to: "Mike".to_string(),
            poem: "meow meow".to_string(),
        };

        assert!(connector.receive_timeout(Duration::from_millis(1)).unwrap().is_none());

        source.send(&[expected_message.to_binary().unwrap()]).unwrap();

        match connector.receive_timeout(Duration::from_millis(2000)).unwrap() {
            Some(resp) => {
                let msg: TestMessage = resp;
                assert_eq!(msg, expected_message);
            },
            None => panic!("DomainConnector Timed out"),
        }
    }

    #[test]
    fn connector_receive_fail_deserialize() {
        let context = ZmqContext::new();
        let addr = InprocAddress::random();

        let connector = Connector::<TestMessage>::listen(&context, &addr, second_frame_extractor).unwrap();

        let source = Connection::new(&context, Direction::Outbound).establish(&addr).unwrap();

        source.send(&["broken"]).unwrap();

        match connector.receive_timeout(Duration::from_millis(2000)) {
            Ok(_) => panic!("Unexpected success with bad serialization data"),
            Err(ConnectorError::DeserializeFailed(_)) => {},
            Err(err) => panic!("Unexpected error {:?}", err),
        }
    }

    #[test]
    fn domain_connector_receive() {
        let context = ZmqContext::new();
        let addr = InprocAddress::random();

        let connector = DomainConnector::listen(&context, &addr).unwrap();
        let source = Connection::new(&context, Direction::Outbound).establish(&addr).unwrap();

        let expected_message = TestMessage {
            from: "Cryptokitties".to_string(),
            to: "Mike".to_string(),
            poem: "meow meow".to_string(),
        };

        let node_identity = Arc::new(NodeIdentity::random_for_test(None));
        let dest_node_identity = Arc::new(NodeIdentity::random_for_test(None));
        let header = MessageHeader::new(123).unwrap();
        let message = Message::from_message_format(header, expected_message.clone()).unwrap();
        let message_envelope = MessageEnvelope::construct(
            &node_identity,
            dest_node_identity.identity.public_key.clone(),
            NodeDestination::Unknown,
            message.to_binary().unwrap(),
            MessageFlags::NONE,
        )
        .unwrap();

        let domain_message_context = DomainMessageContext {
            peer_source: node_identity.identity.clone(),
            origin_source: node_identity.identity.public_key.clone(),
            message,
            message_envelope,
        };

        source.send(&[domain_message_context.to_binary().unwrap()]).unwrap();

        match connector.receive_timeout(Duration::from_millis(2000)).unwrap() {
            Some((info, resp)) => {
                let msg: TestMessage = resp;
                assert_eq!(msg, expected_message);
                assert_eq!(info.peer_source.public_key, node_identity.identity.public_key);
            },
            None => panic!("DomainConnector Timed out"),
        }
    }
}
