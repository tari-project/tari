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

use crate::{message::FrameSet, peer_manager::node_id::NodeId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use time::Duration as TimeDuration;

/// The OutboundMessage has a copy of the MessageEnvelope and tracks the number of send attempts, the creation
/// timestamp and the retry timestamp. The OutboundMessageService will create the OutboundMessage and forward it to
/// the outbound message pool. The OutboundMessages can then be retrieved from the pool by the ConnectionManager so they
/// can be sent to the peer destinations.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct OutboundMessage {
    destination_node_id: NodeId,
    num_attempts: u32,
    scheduled_timestamp: DateTime<Utc>,
    message_frames: FrameSet,
}

// Maximum number of seconds allowed by `time::Duration`. Unfortunately, not exposed publicly
const DURATION_MAX_SECS: i64 = std::i64::MAX / 1000;

fn exponential_backoff_offset(num_attempts: u32) -> TimeDuration {
    let secs = 0.5 * (f32::powf(2.0, num_attempts as f32) - 1.0);
    if secs > DURATION_MAX_SECS as f32 {
        TimeDuration::seconds(DURATION_MAX_SECS)
    } else {
        TimeDuration::seconds(secs.ceil() as i64)
    }
}

impl OutboundMessage {
    /// Create a new OutboundMessage from the destination_node_id and message_frames
    pub fn new(destination: NodeId, message_frames: FrameSet) -> OutboundMessage {
        OutboundMessage {
            destination_node_id: destination,
            num_attempts: 0,
            scheduled_timestamp: Utc::now(),
            message_frames,
        }
    }

    /// Increment the retry count and set scheduled_timestamp to the future using an exponential backoff formula
    /// based on number of attempts
    pub fn mark_failed_attempt(&mut self) {
        self.num_attempts += 1;
        self.scheduled_timestamp = Utc::now() + exponential_backoff_offset(self.num_attempts);
    }

    pub fn num_attempts(&self) -> u32 {
        self.num_attempts
    }

    pub fn is_scheduled(&self) -> bool {
        let now = Utc::now();
        let diff = self.scheduled_timestamp.signed_duration_since(now);
        diff.num_seconds() <= 0
    }

    pub fn destination_node_id(&self) -> &NodeId {
        &self.destination_node_id
    }

    pub fn message_frames(&self) -> &FrameSet {
        &self.message_frames
    }

    pub fn scheduled_duration(&self) -> TimeDuration {
        let now = Utc::now();
        self.scheduled_timestamp.signed_duration_since(now).into()
    }
}

impl PartialOrd<OutboundMessage> for OutboundMessage {
    /// Orders OutboundMessage from least to most time remaining from being scheduled
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.scheduled_duration().partial_cmp(&other.scheduled_duration())
    }
}

impl Ord for OutboundMessage {
    /// Orders OutboundMessage from least to most time remaining from being scheduled
    fn cmp(&self, other: &Self) -> Ordering {
        self.scheduled_duration().cmp(&other.scheduled_duration())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::iter::repeat_with;

    #[test]
    fn new() {
        let node_id = NodeId::new();
        let subject = OutboundMessage::new(node_id.clone(), vec![vec![1]]);
        assert_eq!(subject.message_frames[0].len(), 1);
        assert_eq!(subject.destination_node_id, node_id);
        assert_eq!(subject.num_attempts, 0);
    }

    #[test]
    fn exponential_backoff_offset() {
        use super::exponential_backoff_offset as subject;
        assert_eq!(subject(1).num_seconds(), 1);
        assert_eq!(subject(2).num_seconds(), 2);
        assert_eq!(subject(3).num_seconds(), 4);
        assert_eq!(subject(4).num_seconds(), 8);
        assert_eq!(subject(5).num_seconds(), 16);
        assert_eq!(subject(6).num_seconds(), 32);
        assert_eq!(subject(7).num_seconds(), 64);
        assert_eq!(subject(8).num_seconds(), 128);
        assert_eq!(subject(9).num_seconds(), 256);
        assert_eq!(subject(10).num_seconds(), 512);

        // Edge cases
        assert_eq!(subject(0).num_seconds(), 0);
        assert_eq!(subject(std::u32::MAX).num_seconds(), DURATION_MAX_SECS);
    }

    #[test]
    fn mark_failed_attempt() {
        let node_id = NodeId::new();
        let mut subject = OutboundMessage::new(node_id.clone(), vec![vec![1]]);
        for i in 0..5 {
            let old_scheduled_timestamp = subject.scheduled_timestamp.clone();
            subject.mark_failed_attempt();
            assert_eq!(subject.num_attempts, i + 1);
            assert!(subject.scheduled_timestamp > old_scheduled_timestamp);
        }
    }

    #[test]
    fn is_scheduled() {
        let node_id = NodeId::new();
        let mut subject = OutboundMessage::new(node_id, vec![vec![1]]);

        // Now
        subject.scheduled_timestamp = Utc::now();
        assert!(subject.is_scheduled());

        // In future
        subject.scheduled_timestamp = Utc::now() + TimeDuration::seconds(100);
        assert!(!subject.is_scheduled());

        // In past
        subject.scheduled_timestamp = Utc::now() - TimeDuration::seconds(100);
        assert!(subject.is_scheduled());
    }

    #[test]
    fn misc() {
        let node_id = NodeId::new();
        let frames = vec![vec![1]];
        let subject = OutboundMessage::new(node_id.clone(), frames.clone());

        assert_eq!(subject.destination_node_id(), &node_id);
        assert_eq!(subject.num_attempts(), 0);
        assert_eq!(subject.message_frames(), &frames);
        assert!(subject.scheduled_duration().num_seconds() <= 0);
    }

    #[test]
    fn ord() {
        let mut collection = repeat_with(|| {
            let node_id = NodeId::new();
            let frames = vec![vec![1]];
            OutboundMessage::new(node_id.clone(), frames.clone())
        })
        .take(5)
        .collect::<Vec<OutboundMessage>>();

        collection[1].message_frames = vec!["last".as_bytes().to_vec()];
        collection[1].scheduled_timestamp = Utc::now() + TimeDuration::seconds(100);

        collection[3].message_frames = vec!["first".as_bytes().to_vec()];
        collection[3].scheduled_timestamp = Utc::now() - TimeDuration::seconds(100);

        collection.sort();

        assert_eq!(collection[0].message_frames[0], "first".as_bytes().to_vec());
        assert_eq!(collection[4].message_frames[0], "last".as_bytes().to_vec());
    }
}
