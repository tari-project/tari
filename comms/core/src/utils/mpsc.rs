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

use std::collections::VecDeque;

use tokio::sync::mpsc;

pub async fn send_all<T, I: IntoIterator<Item = T>>(
    sender: &mpsc::Sender<T>,
    iter: I,
) -> Result<(), mpsc::error::SendError<T>> {
    for item in iter {
        sender.send(item).await?;
    }
    Ok(())
}

pub fn send_all_unbounded<T, I: IntoIterator<Item = T>>(
    sender: &mpsc::UnboundedSender<T>,
    iter: I,
) -> Result<(), mpsc::error::SendError<T>> {
    for item in iter {
        sender.send(item)?;
    }
    Ok(())
}

/// Tracks the bytes a receiver has not yet consumed from a bounded channel, so that the sender can stop producing once
/// the receiver falls behind by a number of bytes and not only by a number of items.
///
/// The sender records the size of each item after sending it. Items leave the channel in order, so the channel's free
/// capacity tells how many of the oldest recorded items the receiver has taken. The most recently taken item is still
/// counted until the next one is taken, because the receiver usually still holds it, e.g. a client caller is still
/// decoding it. Older taken items are not counted: a receiver that holds more than one taken item, like the RPC server
/// writing one item while it holds the next prefetched, holds those on top of the budget.
///
/// One item is always let through when the channel is empty, so the bytes counted stay below `max_bytes` plus the size
/// of the largest item. The channel must have no other sender.
#[derive(Debug)]
pub struct ChannelByteBudget {
    max_bytes: usize,
    /// The sizes of the items sent and still counted, oldest first
    sizes: VecDeque<usize>,
    num_bytes: usize,
}

impl ChannelByteBudget {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            sizes: VecDeque::new(),
            num_bytes: 0,
        }
    }

    /// Records an item of `num_bytes` that was just sent on the channel
    pub fn record_sent(&mut self, num_bytes: usize) {
        self.sizes.push_back(num_bytes);
        self.num_bytes = self.num_bytes.saturating_add(num_bytes);
    }

    /// Returns the bytes sent on `sender` that the receiver has not consumed yet
    pub fn buffered_bytes<T>(&mut self, sender: &mpsc::Sender<T>) -> usize {
        self.forget_taken(sender);
        self.num_bytes
    }

    /// Waits until fewer than `max_bytes` are buffered, or until the channel is empty. Returns an error if the
    /// receiver was dropped.
    pub async fn wait_for_room<T>(&mut self, sender: &mpsc::Sender<T>) -> Result<(), mpsc::error::SendError<()>> {
        loop {
            if sender.is_closed() {
                return Err(mpsc::error::SendError(()));
            }
            self.forget_taken(sender);
            if self.num_bytes < self.max_bytes || num_in_channel(sender) == 0 {
                return Ok(());
            }
            wait_for_one_taken(sender).await?;
        }
    }

    /// Stops counting the items that have been taken, except the most recent one
    fn forget_taken<T>(&mut self, sender: &mpsc::Sender<T>) {
        let num_counted = num_in_channel(sender).saturating_add(1);
        while self.sizes.len() > num_counted {
            if let Some(size) = self.sizes.pop_front() {
                self.num_bytes = self.num_bytes.saturating_sub(size);
            }
        }
    }
}

/// Waits for the receiver to take an item, which frees one more slot than is free now. Returns early if the channel is
/// already empty, which happens when the receiver takes the last items after the caller checked. Returns an error if
/// the receiver was dropped.
async fn wait_for_one_taken<T>(sender: &mpsc::Sender<T>) -> Result<(), mpsc::error::SendError<()>> {
    // Never ask for more slots than the channel has: reserve_many fails for a request it can never grant
    let num_permits = sender.capacity().saturating_add(1).min(sender.max_capacity());
    match sender.reserve_many(num_permits).await {
        Ok(permits) => {
            drop(permits);
            Ok(())
        },
        Err(_) if sender.is_closed() => Err(mpsc::error::SendError(())),
        Err(_) => Ok(()),
    }
}

fn num_in_channel<T>(sender: &mpsc::Sender<T>) -> usize {
    sender.max_capacity().saturating_sub(sender.capacity())
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use tokio::time;

    use super::*;

    /// Sends items of `item_size` until the budget makes the sender wait. Returns the number of items sent.
    async fn send_until_waiting(budget: &mut ChannelByteBudget, tx: &mpsc::Sender<usize>, item_size: usize) -> usize {
        let mut num_sent = 0;
        loop {
            let wait = time::timeout(Duration::from_millis(50), budget.wait_for_room(tx)).await;
            match wait {
                Ok(result) => result.unwrap(),
                Err(_) => return num_sent,
            }
            tx.send(item_size).await.unwrap();
            budget.record_sent(item_size);
            num_sent = num_sent.saturating_add(1);
        }
    }

    #[tokio::test]
    async fn it_stops_the_sender_at_the_byte_limit() {
        let (tx, mut rx) = mpsc::channel(100);
        let mut budget = ChannelByteBudget::new(100);

        // 0, 30, 60 and 90 bytes are under the limit, so a fourth item is let through
        assert_eq!(send_until_waiting(&mut budget, &tx, 30).await, 4);
        assert_eq!(budget.buffered_bytes(&tx), 120);

        // The item the receiver took is still counted while it holds it
        rx.recv().await.unwrap();
        assert_eq!(budget.buffered_bytes(&tx), 120);
        assert_eq!(send_until_waiting(&mut budget, &tx, 30).await, 0);

        // Taking the next item releases the first
        rx.recv().await.unwrap();
        assert_eq!(budget.buffered_bytes(&tx), 90);
        assert_eq!(send_until_waiting(&mut budget, &tx, 30).await, 1);
        assert!(budget.buffered_bytes(&tx) <= 100 + 30);
    }

    #[tokio::test]
    async fn an_item_larger_than_the_limit_passes_when_nothing_is_buffered() {
        let (tx, mut rx) = mpsc::channel(10);
        let mut budget = ChannelByteBudget::new(100);

        assert_eq!(send_until_waiting(&mut budget, &tx, 1000).await, 1);
        assert_eq!(budget.buffered_bytes(&tx), 1000);

        // Once it has been taken the channel is empty, so the next item is let through even though the taken item is
        // still counted
        assert_eq!(rx.recv().await, Some(1000));
        assert_eq!(send_until_waiting(&mut budget, &tx, 1000).await, 1);
    }

    #[tokio::test]
    async fn it_does_not_wait_when_the_receiver_took_everything() {
        let (tx, mut rx) = mpsc::channel(4);
        let mut budget = ChannelByteBudget::new(100);
        for _ in 0..4 {
            tx.send(1000).await.unwrap();
            budget.record_sent(1000);
        }
        // Drain the channel: every slot is free while the taken item still counts above the limit
        for _ in 0..4 {
            rx.recv().await.unwrap();
        }
        assert_eq!(tx.capacity(), tx.max_capacity());
        assert_eq!(budget.buffered_bytes(&tx), 1000);
        let result = time::timeout(Duration::from_secs(1), budget.wait_for_room(&tx)).await;
        assert!(result.expect("wait_for_room waited on an empty channel").is_ok());
    }

    /// The receiver can take the last items between the check for an empty channel and the wait. The wait must then
    /// return straight away, not report the receiver as dropped.
    #[tokio::test]
    async fn waiting_for_an_item_to_be_taken_from_an_empty_channel_returns_at_once() {
        let (tx, mut rx) = mpsc::channel::<usize>(4);
        tx.send(1).await.unwrap();
        rx.recv().await.unwrap();
        assert_eq!(tx.capacity(), tx.max_capacity());
        let result = time::timeout(Duration::from_secs(1), wait_for_one_taken(&tx)).await;
        assert!(result.expect("waited on an empty channel").is_ok());

        drop(rx);
        assert!(wait_for_one_taken(&tx).await.is_err());
    }

    #[tokio::test]
    async fn it_returns_an_error_when_the_receiver_is_dropped() {
        let (tx, rx) = mpsc::channel(10);
        let mut budget = ChannelByteBudget::new(100);
        assert_eq!(send_until_waiting(&mut budget, &tx, 1000).await, 1);
        drop(rx);
        assert!(budget.wait_for_room(&tx).await.is_err());
    }
}
