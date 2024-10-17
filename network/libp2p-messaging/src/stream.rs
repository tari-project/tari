//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use libp2p::{
    futures::{channel::mpsc, SinkExt, Stream, StreamExt},
    PeerId,
};

pub type StreamId = u32;
pub fn channel<T>(stream_id: StreamId, peer_id: PeerId) -> (MessageSink<T>, MessageStream<T>) {
    let (sender, receiver) = mpsc::unbounded();
    let sink = MessageSink::new(stream_id, peer_id, sender);
    let stream = MessageStream::new(stream_id, peer_id, receiver);
    (sink, stream)
}

#[derive(Debug)]
pub struct MessageStream<TMsg> {
    stream_id: StreamId,
    peer_id: PeerId,
    receiver: mpsc::UnboundedReceiver<TMsg>,
}

impl<TMsg> MessageStream<TMsg> {
    pub fn new(stream_id: StreamId, peer_id: PeerId, receiver: mpsc::UnboundedReceiver<TMsg>) -> Self {
        Self {
            stream_id,
            peer_id,
            receiver,
        }
    }

    pub fn peer_id(&self) -> &PeerId {
        &self.peer_id
    }

    pub fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    pub async fn recv(&mut self) -> Option<TMsg> {
        self.receiver.next().await
    }
}

#[derive(Debug)]
pub struct MessageSink<TMsg> {
    stream_id: StreamId,
    peer_id: PeerId,
    sender: mpsc::UnboundedSender<TMsg>,
}

impl<TMsg> MessageSink<TMsg> {
    pub fn new(stream_id: StreamId, peer_id: PeerId, sender: mpsc::UnboundedSender<TMsg>) -> Self {
        Self {
            stream_id,
            peer_id,
            sender,
        }
    }

    pub fn peer_id(&self) -> &PeerId {
        &self.peer_id
    }

    pub fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    pub fn send(&mut self, msg: TMsg) -> Result<(), crate::Error> {
        self.sender.unbounded_send(msg).map_err(|_| crate::Error::ChannelClosed)
    }

    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }

    pub async fn send_all<TStream>(&mut self, stream: &mut TStream) -> Result<(), crate::Error>
    where TStream: Stream<Item = Result<TMsg, mpsc::SendError>> + Unpin + ?Sized {
        self.sender
            .send_all(stream)
            .await
            .map_err(|_| crate::Error::ChannelClosed)
    }
}

impl<TMsg> Clone for MessageSink<TMsg> {
    fn clone(&self) -> Self {
        Self {
            stream_id: self.stream_id,
            peer_id: self.peer_id,
            sender: self.sender.clone(),
        }
    }
}
