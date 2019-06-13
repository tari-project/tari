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
    message::{Frame, FrameSet},
};
use derive_error::Error;
use serde::{Deserialize, Serialize};
use std::{cmp, marker::PhantomData, time::Duration};
use tari_utilities::message_format::{MessageFormat, MessageFormatError};

#[derive(Debug, Error)]
pub enum DomainConnectorError {
    ConnectionError(ConnectionError),
    #[error(no_from)]
    ListenFailed(ConnectionError),
    DeserializeFailed(MessageFormatError),
    #[error(msg_embedded, non_std, no_from)]
    ExtractFrameSetError(String),
}

/// # DomainConnector
///
/// The domain connector receives messages from an inbound address, and extracts and deserializes
/// two frames into what the caller specifies.
///
/// This should be used by services/protocol managers to receive messages from the [InboundMessageService].
pub struct DomainConnector<'de, H, T> {
    connection: EstablishedConnection,
    frame_extractor: Box<FrameExtractor<Error = DomainConnectorError>>,
    _t: PhantomData<&'de (H, T)>,
}

impl<'de, H, T> DomainConnector<'de, H, T>
where
    T: Serialize + Deserialize<'de>,
    H: Serialize + Deserialize<'de>,
{
    /// Start listening for messages. Frames are extracted by the ByIndexFrameExtractor, which
    /// expects the header frame to be at index 1 and the message frame to be at index 2.
    pub fn listen<A>(context: &ZmqContext, address: &A) -> Result<Self, DomainConnectorError>
    where A: ZmqEndpoint {
        DomainConnector::listen_with_frame_extractor(context, address, ByIndexFrameExtractor {
            header_index: 1,
            body_index: 2,
        })
    }

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
    pub fn listen_with_frame_extractor<FE, A>(
        context: &ZmqContext,
        address: &A,
        frame_extractor: FE,
    ) -> Result<Self, DomainConnectorError>
    where
        A: ZmqEndpoint,
        FE: FrameExtractor<Error = DomainConnectorError>,
        FE: 'static,
    {
        let connection = Connection::new(context, Direction::Inbound)
            .establish(&address)
            .map_err(DomainConnectorError::ListenFailed)?;

        Ok(Self {
            connection,
            frame_extractor: Box::new(frame_extractor),
            _t: PhantomData,
        })
    }

    pub fn receive_timeout(&self, duration: Duration) -> Result<Option<(H, T)>, DomainConnectorError> {
        match connection_try!(self.connection.receive(duration.as_millis() as u32)) {
            Some(frames) => self.deserialize_from(frames).map(Some),
            None => Ok(None),
        }
    }

    fn deserialize_from(&self, mut frames: FrameSet) -> Result<(H, T), DomainConnectorError> {
        let (header_frame, body_frame) = self.frame_extractor.extract(&mut frames)?;
        Ok((
            H::from_binary(&header_frame).map_err(DomainConnectorError::DeserializeFailed)?,
            T::from_binary(&body_frame).map_err(DomainConnectorError::DeserializeFailed)?,
        ))
    }
}

pub trait FrameExtractor {
    type Error;

    fn extract(&self, frames: &mut FrameSet) -> Result<(Frame, Frame), Self::Error>;
}

impl<F, E> FrameExtractor for F
where F: Fn(&mut FrameSet) -> Result<(Frame, Frame), E>
{
    type Error = E;

    fn extract(&self, frames: &mut FrameSet) -> Result<(Frame, Frame), E> {
        (self)(frames)
    }
}

pub struct ByIndexFrameExtractor {
    header_index: usize,
    body_index: usize,
}

impl FrameExtractor for ByIndexFrameExtractor {
    type Error = DomainConnectorError;

    fn extract(&self, frames: &mut FrameSet) -> Result<(Frame, Frame), DomainConnectorError> {
        let max_index = cmp::max(self.header_index, self.body_index);
        match frames.len() {
            len if len <= max_index => Err(DomainConnectorError::ExtractFrameSetError(format!(
                "Not enough frames to extract. (len={}, required len={})",
                frames.len(),
                max_index
            ))),
            _ => {
                let header = frames.remove(self.header_index);
                let mut body_index = self.body_index;
                if self.header_index < self.body_index {
                    // The header removal has changed the size of the vec
                    // and effected any index greater than it.
                    body_index = body_index - 1;
                }
                Ok((header, frames.remove(body_index)))
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::connection::InprocAddress;

    #[derive(Eq, PartialEq, Debug, Serialize, Deserialize)]
    struct TestHeader {
        from: String,
        to: String,
    }

    #[derive(Eq, PartialEq, Debug, Serialize, Deserialize)]
    struct TestMessage {
        poem: String,
    }

    #[test]
    fn index_extractor() {
        let mut frames = vec![
            "body".as_bytes().to_vec(),
            "no".as_bytes().to_vec(),
            "header".as_bytes().to_vec(),
        ];
        let (header, body) = ByIndexFrameExtractor {
            header_index: 2,
            body_index: 0,
        }
        .extract(&mut frames)
        .unwrap();
        assert_eq!(String::from_utf8_lossy(header.as_slice()), "header");
        assert_eq!(String::from_utf8_lossy(body.as_slice()), "body");
        assert_eq!(String::from_utf8_lossy(&frames[0]), "no");
    }

    #[test]
    fn index_extractor_not_enough_frames() {
        let mut frames = vec![
            "some".as_bytes().to_vec(),
            "random".as_bytes().to_vec(),
            "frames".as_bytes().to_vec(),
        ];

        let result = ByIndexFrameExtractor {
            header_index: 8,
            body_index: 0,
        }
        .extract(&mut frames);

        match result {
            Ok(_) => panic!("Unexpected Ok result"),
            Err(DomainConnectorError::ExtractFrameSetError(_)) => {},
            Err(err) => panic!("Unexpected error {:?}", err),
        }
    }

    #[test]
    fn connect() {
        let context = ZmqContext::new();
        let addr = InprocAddress::random();
        let connector = DomainConnector::<TestHeader, TestMessage>::listen(&context, &addr).unwrap();

        assert!(connector.connection.get_connected_address().is_none());
        assert_eq!(
            connector.connection.get_socket().get_last_endpoint().unwrap().unwrap(),
            addr.to_zmq_endpoint()
        );
    }

    #[test]
    fn receive_default_extractor() {
        let context = ZmqContext::new();
        let addr = InprocAddress::random();

        let connector = DomainConnector::listen(&context, &addr).unwrap();

        let source = Connection::new(&context, Direction::Outbound).establish(&addr).unwrap();

        let expected_header = TestHeader {
            from: "Cryptokitties".to_string(),
            to: "Mike".to_string(),
        };
        let expected_message = TestMessage {
            poem: "meow meow".to_string(),
        };

        assert!(connector.receive_timeout(Duration::from_millis(1)).unwrap().is_none());

        source
            .send(&[
                expected_header.to_binary().unwrap(),
                expected_message.to_binary().unwrap(),
            ])
            .unwrap();

        match connector.receive_timeout(Duration::from_millis(2000)).unwrap() {
            Some(resp) => {
                let (header, msg): (TestHeader, TestMessage) = resp;
                assert_eq!(header, expected_header);
                assert_eq!(msg, expected_message);
            },
            None => panic!("DomainConnector Timed out"),
        }
    }

    #[test]
    fn receive_fail_deserialize() {
        let context = ZmqContext::new();
        let addr = InprocAddress::random();

        let connector = DomainConnector::<TestHeader, TestMessage>::listen(&context, &addr).unwrap();

        let source = Connection::new(&context, Direction::Outbound).establish(&addr).unwrap();

        source.send(&["broken", "message"]).unwrap();

        match connector.receive_timeout(Duration::from_millis(2000)) {
            Ok(_) => panic!("Unexpected success with bad serialization data"),
            Err(DomainConnectorError::DeserializeFailed(_)) => {},
            Err(err) => panic!("Unexpected error {:?}", err),
        }
    }
}
