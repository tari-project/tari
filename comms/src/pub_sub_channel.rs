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
// use super::async_::channel as async_channel;
// use super::async_::Publisher;
// use super::async_::Subscriber;
use bus_queue::async_::{channel as async_channel, Publisher, Subscriber};
use derive_error::Error;
use futures::prelude::*;
use log::*;
use std::{fmt::Debug, sync::Arc};

const LOG_TARGET: &str = "comms::pub_sub_channel";

#[derive(Debug, Error)]
pub enum TopicPublisherSubscriberError {
    /// Subscription Arc Reference error occurred, this means there was a problem getting a mutable ref to the arc
    /// during a read.
    SubscriptionArcReferenceError,
    /// Subscription Error, an error was returned from the subscription stream,
    SubscriptionError,
}

/// Create a Topic filtered Pub-Sub channel pair.
pub fn pubsub_channel<T: Send, M: Send>(size: usize) -> (TopicPublisher<T, M>, TopicSubscriber<T, M>) {
    let (publisher, subscriber) = async_channel(size);
    (publisher, TopicSubscriber::new(subscriber))
}

/// A message that is passed along the channel contains a Topic to define the type of message and the message itself.
pub struct TopicPayload<T, M> {
    topic: T,
    message: M,
}

impl<T: Send + Debug, M: Send> TopicPayload<T, M> {
    pub fn new(topic: T, message: M) -> Self {
        Self { topic, message }
    }
}

pub type TopicPublisher<T, M> = Publisher<TopicPayload<T, M>>;

pub struct TopicSubscriber<T: Send, M: Send> {
    inner: Subscriber<TopicPayload<T, M>>,
}

impl<T: Send, M: Send> TopicSubscriber<T, M> {
    fn new(subscriber: Subscriber<TopicPayload<T, M>>) -> Self {
        Self { inner: subscriber }
    }
}

impl<T: Eq + Send, M: Clone + Send> TopicSubscriber<T, M> {
    pub fn subscription(&self, topic: T) -> TopicSubscription<T, M> {
        TopicSubscription::<_, M>::new(topic, self.inner.clone())
    }
}

/// A TopicSubscription will receive messages of the Topic it is initialized with and ignore other topics.
pub struct TopicSubscription<T: Send, M: Send> {
    topic: T,
    inner: Subscriber<TopicPayload<T, M>>,
}

impl<T: Eq + Send, M: Clone + Send> TopicSubscription<T, M> {
    fn new(topic: T, subscriber: Subscriber<TopicPayload<T, M>>) -> Self {
        Self {
            topic,
            inner: subscriber,
        }
    }
}

impl<T, M> Stream for TopicSubscription<T, M>
where
    T: Eq + Send + Debug,
    M: Clone + Send,
{
    type Error = ();
    type Item = M;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            match try_ready!(self.inner.poll()) {
                Some(payload) => {
                    if payload.topic == self.topic {
                        debug!(
                            target: LOG_TARGET,
                            "Subscriber yielded message of type: {:?}", payload.topic
                        );
                        return Ok(Async::Ready(Some(payload.message.clone())));
                    }
                },
                None => return Ok(Async::Ready(None)),
            }
        }
    }
}

/// The Subscription reader holds a reference to a subscription stream and provides a Future that will read any items
/// that are currently in the stream and yield them to the synchronous caller WITHOUT ending the stream. The Stream is
/// passed back to the caller so that it can checked for new messages in the future. This is allows non-futures code to
/// read interrim values in a stream without the stream needing to end like the `.collect()` combinator requires.
pub struct SubscriptionReader<T, M>
where
    T: Eq + Send,
    M: Clone + Send,
{
    stream: Arc<TopicSubscription<T, M>>,
}

impl<T, M> SubscriptionReader<T, M>
where
    T: Eq + Send,
    M: Clone + Send,
{
    pub fn new(stream: Arc<TopicSubscription<T, M>>) -> SubscriptionReader<T, M> {
        SubscriptionReader { stream }
    }
}

impl<T, M> Future for SubscriptionReader<T, M>
where
    T: Eq + Send + Debug,
    M: Clone + Send,
{
    type Error = TopicPublisherSubscriberError;
    type Item = (Vec<M>, Option<Arc<TopicSubscription<T, M>>>);

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let mut items = Vec::new();
        loop {
            match Arc::get_mut(&mut self.stream) {
                Some(s) => match s.poll() {
                    Ok(Async::Ready(Some(v))) => items.push(v),
                    Ok(Async::Ready(None)) => return Ok(Async::Ready((items, None))),
                    Ok(Async::NotReady) => return Ok(Async::Ready((items, Some(self.stream.clone())))),
                    Err(_) => return Err(TopicPublisherSubscriberError::SubscriptionError),
                },
                None => return Err(TopicPublisherSubscriberError::SubscriptionArcReferenceError),
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    #[test]
    fn topic_pub_sub() {
        let mut rt = Runtime::new().unwrap();

        let (mut publisher, subscriber) = pubsub_channel(10);
        #[derive(Debug, Clone)]
        struct Dummy {
            a: u32,
            b: String,
        }

        let messages = vec![
            TopicPayload::new("Topic1", Dummy {
                a: 1u32,
                b: "one".to_string(),
            }),
            TopicPayload::new("Topic2", Dummy {
                a: 2u32,
                b: "two".to_string(),
            }),
            TopicPayload::new("Topic1", Dummy {
                a: 3u32,
                b: "three".to_string(),
            }),
            TopicPayload::new("Topic2", Dummy {
                a: 4u32,
                b: "four".to_string(),
            }),
            TopicPayload::new("Topic1", Dummy {
                a: 5u32,
                b: "five".to_string(),
            }),
            TopicPayload::new("Topic2", Dummy {
                a: 6u32,
                b: "size".to_string(),
            }),
            TopicPayload::new("Topic1", Dummy {
                a: 7u32,
                b: "seven".to_string(),
            }),
        ];

        for m in messages {
            publisher = rt.block_on(publisher.send(m)).unwrap();
        }

        let sub1 = subscriber.subscription("Topic1");
        let mut sub1_arc = Arc::new(sub1);
        let sr = SubscriptionReader::new(sub1_arc);

        let (topic1a, returned_arc) = rt.block_on(sr).unwrap();
        sub1_arc = returned_arc.unwrap();
        assert_eq!(topic1a.len(), 4);
        assert_eq!(topic1a[0].a, 1);
        assert_eq!(topic1a[1].a, 3);
        assert_eq!(topic1a[2].a, 5);
        assert_eq!(topic1a[3].a, 7);

        let messages2 = vec![
            TopicPayload::new("Topic1", Dummy {
                a: 11u32,
                b: "one one".to_string(),
            }),
            TopicPayload::new("Topic2", Dummy {
                a: 22u32,
                b: "two two".to_string(),
            }),
            TopicPayload::new("Topic1", Dummy {
                a: 33u32,
                b: "three three".to_string(),
            }),
        ];

        for m in messages2 {
            publisher = rt.block_on(publisher.send(m)).unwrap();
        }

        let sr = SubscriptionReader::new(sub1_arc);
        let (topic1b, _) = rt.block_on(sr).unwrap();
        assert_eq!(topic1b.len(), 2);
        assert_eq!(topic1b[0].a, 11);
        assert_eq!(topic1b[1].a, 33);

        let sub2 = subscriber.subscription("Topic2");
        let sr = SubscriptionReader::new(Arc::new(sub2));
        let (topic2, _) = rt.block_on(sr).unwrap();

        assert_eq!(topic2.len(), 4);
        assert_eq!(topic2[0].a, 2);
        assert_eq!(topic2[1].a, 4);
        assert_eq!(topic2[2].a, 6);
        assert_eq!(topic2[3].a, 22);
    }
}
