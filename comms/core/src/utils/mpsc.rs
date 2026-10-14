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

use std::{
    collections::VecDeque,
    fmt,
    sync::Mutex,
    time::{Duration, Instant},
};

use log::warn;
use tokio::sync::{mpsc, mpsc::error::SendTimeoutError};

const LOG_TARGET: &str = "comms::utils::mpsc";

/// How long [send_or_drop] waits for room on a full channel before it drops the item
pub const SEND_OR_DROP_TIMEOUT: Duration = Duration::from_secs(1);

/// The shortest interval between two warnings about items dropped at the same site
const DROP_WARNING_INTERVAL: Duration = Duration::from_secs(10);

/// The most peers whose drops are counted per site between two warnings. Drops from further peers are still counted
/// in the total.
const MAX_PEERS_PER_WARNING_INTERVAL: usize = 32;

/// The number of peers (those with the most drops) named in a warning
const NUM_PEERS_IN_WARNING: usize = 3;

/// The drops at one site since its last warning
struct SiteDrops {
    site: &'static str,
    last_warning: Option<Instant>,
    num_dropped: usize,
    /// Drops per peer, for the first [MAX_PEERS_PER_WARNING_INTERVAL] peers
    peers: Vec<(String, usize)>,
}

static DROP_WARNINGS: Mutex<Vec<SiteDrops>> = Mutex::new(Vec::new());

pub async fn send_all<T, I: IntoIterator<Item = T>>(
    sender: &mpsc::Sender<T>,
    iter: I,
) -> Result<(), mpsc::error::SendError<T>> {
    for item in iter {
        sender.send(item).await?;
    }
    Ok(())
}

/// Sends `item` on an internal bounded channel, waiting at most [SEND_OR_DROP_TIMEOUT] for room. If the receiver does
/// not make room in time, or is gone, the item is dropped instead: one stalled consumer must not stall the actor that
/// feeds it, and with it everything else that actor serves. `site` names the call site in the warning (logged at most
/// once per 10 s per site, naming the peers with the most drops since the last one) and in the
/// `comms_channel_drop_total` metric. `peer` is the peer the item is about, if any.
///
/// The dropped item is handed back in the error, for a caller that has to clean up after it.
pub async fn send_or_drop<T>(
    sender: &mpsc::Sender<T>,
    item: T,
    site: &'static str,
    peer: Option<&(dyn fmt::Display + Sync)>,
) -> Result<(), SendTimeoutError<T>> {
    let result = sender.send_timeout(item, SEND_OR_DROP_TIMEOUT).await;
    if let Err(err) = &result {
        let reason = match err {
            SendTimeoutError::Timeout(_) => "the receiver did not take it in time",
            SendTimeoutError::Closed(_) => "the receiver is gone",
        };
        record_drop(site, reason, peer);
    }
    result
}

fn record_drop(site: &'static str, reason: &str, peer: Option<&(dyn fmt::Display + Sync)>) {
    #[cfg(feature = "metrics")]
    channel_drop_counter(site).inc();

    let now = Instant::now();
    let mut warnings = match DROP_WARNINGS.lock() {
        Ok(warnings) => warnings,
        Err(poisoned) => poisoned.into_inner(),
    };
    if !warnings.iter().any(|drops| drops.site == site) {
        warnings.push(SiteDrops {
            site,
            last_warning: None,
            num_dropped: 0,
            peers: Vec::new(),
        });
    }
    let Some(drops) = warnings.iter_mut().find(|drops| drops.site == site) else {
        return;
    };

    drops.num_dropped = drops.num_dropped.saturating_add(1);
    if let Some(peer) = peer {
        let peer = peer.to_string();
        match drops.peers.iter_mut().find(|(known, _)| *known == peer) {
            Some((_, count)) => *count = count.saturating_add(1),
            None => {
                if drops.peers.len() < MAX_PEERS_PER_WARNING_INTERVAL {
                    drops.peers.push((peer, 1));
                }
            },
        }
    }

    if drops
        .last_warning
        .is_some_and(|last| now.saturating_duration_since(last) < DROP_WARNING_INTERVAL)
    {
        return;
    }
    drops.last_warning = Some(now);
    let num_dropped = std::mem::take(&mut drops.num_dropped);
    let mut peers = std::mem::take(&mut drops.peers);
    drop(warnings);

    peers.sort_by(|a, b| b.1.cmp(&a.1));
    let mut top_peers = Vec::new();
    for (peer, count) in peers.iter().take(NUM_PEERS_IN_WARNING) {
        top_peers.push(format!("{peer} ({count})"));
    }
    if top_peers.is_empty() {
        warn!(
            target: LOG_TARGET,
            "Dropped {num_dropped} item(s) sent to `{site}` because {reason} within {SEND_OR_DROP_TIMEOUT:.0?}"
        );
    } else {
        warn!(
            target: LOG_TARGET,
            "Dropped {num_dropped} item(s) sent to `{site}` because {reason} within {SEND_OR_DROP_TIMEOUT:.0?}. Most \
             for: {}",
            top_peers.join(", ")
        );
    }
}

#[cfg(feature = "metrics")]
fn channel_drop_counter(site: &'static str) -> tari_metrics::IntCounter {
    use once_cell::sync::Lazy;
    use tari_metrics::IntCounterVec;

    static METER: Lazy<IntCounterVec> = Lazy::new(|| {
        tari_metrics::register_int_counter_vec(
            "comms_channel_drop_total",
            "The number of items dropped because an internal channel's receiver did not take them within 1s, per site",
            &["site"],
        )
        .unwrap()
    });

    METER.with_label_values(&[site])
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
    async fn send_or_drop_delivers_when_there_is_room() {
        let (tx, mut rx) = mpsc::channel(1);
        send_or_drop(&tx, 1, "test", None).await.unwrap();
        assert_eq!(rx.recv().await, Some(1));
    }

    #[tokio::test(start_paused = true)]
    async fn send_or_drop_drops_the_item_when_the_receiver_stalls() {
        let (tx, mut rx) = mpsc::channel(1);
        tx.send(1).await.unwrap();
        let started = time::Instant::now();
        let err = send_or_drop(&tx, 2, "test", None).await.unwrap_err();
        assert_eq!(started.elapsed(), SEND_OR_DROP_TIMEOUT);
        assert!(matches!(err, SendTimeoutError::Timeout(2)));
        // Only the first item was delivered
        assert_eq!(rx.recv().await, Some(1));
        assert!(rx.try_recv().is_err());

        drop(rx);
        let err = send_or_drop(&tx, 3, "test", None).await.unwrap_err();
        assert!(matches!(err, SendTimeoutError::Closed(3)));
    }

    #[test]
    fn drops_are_counted_for_a_bounded_number_of_peers() {
        const SITE: &str = "test_bounded_peers";
        for peer in 0..100 {
            record_drop(SITE, "test", Some(&peer));
        }
        let warnings = DROP_WARNINGS.lock().unwrap();
        let drops = warnings.iter().find(|drops| drops.site == SITE).unwrap();
        // The first drop was warned about at once; the other 99 wait for the next warning
        assert_eq!(drops.num_dropped, 99);
        assert_eq!(drops.peers.len(), MAX_PEERS_PER_WARNING_INTERVAL);
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
