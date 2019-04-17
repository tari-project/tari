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

use std::convert::TryFrom;

use derive_error::Error;

#[derive(Error, Debug)]
pub enum MessageError {
    /// Multipart message is malformed
    MalformedMultipart,
    /// Failed to deserialize message
    DeserializeFailed,
}

const FRAMES_PER_MESSAGE: usize = 4;

/// Represents a single message frame.
pub type Frame = Vec<u8>;
/// Represents a collection of frames which make up a multipart message.
pub type FrameSet = Vec<Frame>;

/// Represents a message which is about to go on or has just come off the wire.
pub struct RawDataMessage {
    frames: FrameSet,
}

impl RawDataMessage {
    /// Create a new RawDataMessage from four frames
    pub fn new(identity: Frame, version: Frame, header: Frame, body: Frame) -> Self {
        RawDataMessage {
            frames: vec![identity, version, header, body],
        }
    }

    /// Returns the frame that is expected to be identity frame
    pub fn identity(&self) -> &Frame {
        &self.frames[0]
    }

    /// Returns the frame that is expected to be version frame
    pub fn version(&self) -> &Frame {
        &self.frames[1]
    }

    /// Returns the frame that is expected to be header frame
    pub fn header(&self) -> &Frame {
        &self.frames[2]
    }

    /// Returns the frame that is expected to be body frame
    pub fn body(&self) -> &Frame {
        &self.frames[3]
    }
}

impl TryFrom<FrameSet> for RawDataMessage {
    type Error = MessageError;

    /// Returns a RawDataMessage from a FrameSet
    fn try_from(frames: FrameSet) -> Result<Self, Self::Error> {
        if frames.len() != FRAMES_PER_MESSAGE {
            return Err(MessageError::MalformedMultipart);
        }

        Ok(RawDataMessage { frames })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::convert::TryInto;

    #[test]
    fn try_from_valid() {
        let example = vec![vec![0u8], vec![1u8], vec![2u8], vec![3u8]];

        let raw_message: Result<RawDataMessage, MessageError> = example.try_into();

        assert!(raw_message.is_ok());
        let raw_message = raw_message.unwrap();
        assert_eq!(raw_message.identity(), &[0u8]);
        assert_eq!(raw_message.version(), &[1u8]);
        assert_eq!(raw_message.header(), &[2u8]);
        assert_eq!(raw_message.body(), &[3u8]);
    }

    #[test]
    fn try_from_invalid() {
        let example = vec![vec![0u8], vec![1u8], vec![2u8]];

        let raw_message: Result<RawDataMessage, MessageError> = example.try_into();

        assert!(raw_message.is_err());
        let error = raw_message.err().unwrap();
        match error {
            MessageError::MalformedMultipart => {},
            _ => panic!("Unexpected MessageError {:?}", error),
        }
    }
}
