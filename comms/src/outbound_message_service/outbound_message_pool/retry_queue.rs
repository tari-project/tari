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
    cmp::{self, Ordering},
    collections::{BinaryHeap, HashMap, VecDeque},
    hash::Hash,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

#[derive(Eq, PartialEq)]
struct ScheduledItem<T> {
    item: T,
    scheduled_at: Instant,
}

impl<T> ScheduledItem<T> {
    pub fn new(item: T, scheduled_at: Instant) -> Self {
        Self { item, scheduled_at }
    }

    pub fn is_scheduled(&self) -> bool {
        self.scheduled_at <= Instant::now()
    }
}

impl<T> PartialOrd<ScheduledItem<T>> for ScheduledItem<T>
where ScheduledItem<T>: Ord
{
    /// Orders from least to most time remaining from being scheduled
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for ScheduledItem<T>
where ScheduledItem<T>: Eq
{
    /// Orders from least to most time remaining from being scheduled
    fn cmp(&self, other: &Self) -> Ordering {
        let now = Instant::now();
        let sub_self = self.scheduled_at.checked_duration_since(now);
        let sub_other = other.scheduled_at.checked_duration_since(now);

        match (sub_self, sub_other) {
            // Both are "scheduled"
            (None, None) => Ordering::Equal,
            // self is "scheduled", other is not
            (None, Some(_)) => Ordering::Greater,
            // other is "scheduled", self is not
            (Some(_), None) => Ordering::Less,
            // Both aren't "scheduled", whichever one is closer to being scheduled is greater
            (Some(a), Some(b)) => b.cmp(&a),
        }
    }
}

/// # RetryBucket
///
/// Represents an ordered grouping of work items, with methods which can be used to track attempts.
#[derive(Default)]
pub struct RetryBucket<T> {
    attempts: u32,
    contents: VecDeque<T>,
}

impl<T> RetryBucket<T> {
    /// Create a new RetryBucket with the given contents
    pub fn new(contents: Vec<T>) -> Self {
        Self {
            contents: contents.into(),
            attempts: 1,
        }
    }

    /// Increment the number of attempts for this bucket
    pub fn incr_attempts(&mut self) {
        self.attempts += 1;
    }

    /// Return the number of attempts for this bucket
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Pop an item from the front
    pub fn pop_front(&mut self) -> Option<T> {
        self.contents.pop_front()
    }

    /// Push an item onto the front
    pub fn push_front(&mut self, item: T) {
        self.contents.push_front(item)
    }

    /// Push an item onto the back
    pub fn push_back(&mut self, item: T) {
        self.contents.push_back(item)
    }

    /// Pour the contents of the given bucket into this one
    pub fn pour_from(&mut self, bucket: &mut Self) {
        bucket.contents.extend(self.contents.drain(..))
    }

    /// Get the future Instant in time that this bucket should be scheduled. Exponentially backing off
    /// according to the number of attempts.
    pub fn schedule(&self) -> Instant {
        Instant::now() + Duration::from_secs(self.exponential_backoff_offset())
    }

    fn exponential_backoff_offset(&self) -> u64 {
        if self.attempts == 0 {
            return 0;
        }
        let secs = 0.5 * (f32::powf(2.0, self.attempts as f32) - 1.0);
        cmp::max(2, secs.ceil() as u64)
    }
}

/// A lease represents an item which is either available for lease or already taken.
/// A taken lease may have items added.
///
/// This is used by the RetryQueue to contain the RetryBuckets and keep track of whether
/// the buckets are available for lease or not.
enum Lease<T> {
    Available(T),
    Taken(Option<T>),
}

impl<T> Lease<T> {
    #[cfg(test)]
    fn is_available(&self) -> bool {
        match self {
            Lease::Available(_) => true,
            _ => false,
        }
    }

    #[cfg(test)]
    fn is_taken(&self) -> bool {
        match self {
            Lease::Taken(_) => true,
            _ => false,
        }
    }

    fn take(self) -> Option<T> {
        match self {
            Lease::Available(t) => Some(t),
            Lease::Taken(t) => t,
        }
    }

    fn borrow_inner_mut(&mut self) -> Option<&mut T> {
        match self {
            Lease::Available(t) => Some(t),
            Lease::Taken(Some(t)) => Some(t),
            Lease::Taken(None) => None,
        }
    }

    fn borrow_inner(&self) -> Option<&T> {
        match self {
            Lease::Available(t) => Some(t),
            Lease::Taken(Some(t)) => Some(t),
            Lease::Taken(None) => None,
        }
    }
}

/// # RetryQueue
///
/// A thread-safe data structure which provides a small API for leasing buckets of items related by their key.
///
/// A common use-case for this is bundling related items of work together for sequential processing, where processing
/// related work items doesn't make sense.
///
/// Buckets of related work can be leased by workers and processed. While a bucket is leased, more related work may come
/// in. To handle this case, a "temporary" bucket may be filled while the original loaned bucket is being processed.
/// Once the worker is done processing the bucket, it may return the bucket (with remaining messages, if any)
/// and the bucket lease will be available again for other workers, albeit with the temporary buckets contents added.
/// Alternatively, the worker may want to remove the lease under that key (all messages processed). In this case,
/// a call to the `remove()` method returns the bucket (in this case the temporary one) for processing by the worker.
///
/// Each bucket is scheduled in time according to it's number of attempts (exponential backoff).
#[derive(Clone)]
pub struct RetryQueue<K, T>
where K: Hash + Eq
{
    inner: Arc<RwLock<Inner<K, T>>>,
}

impl<K, T> RetryQueue<K, T>
where
    K: Hash + Eq,
    K: Clone,
{
    /// Create a new RetryQueue
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner::new())),
        }
    }

    /// Returns `true` of the queue contains the key, otherwise `false`
    pub fn contains(&self, key: &K) -> bool {
        let inner = acquire_read_lock!(self.inner);
        inner.contains(key)
    }

    /// Returns `true` if the queue is empty, otherwise `false`
    pub fn is_empty(&self) -> bool {
        let inner = acquire_read_lock!(self.inner);
        inner.is_empty()
    }

    /// Lease the next bucket if one is scheduled and available
    pub fn lease_next(&self) -> Option<(K, RetryBucket<T>)> {
        let mut inner = acquire_write_lock!(self.inner);
        inner.take_next()
    }

    /// Return the leased bucket and make it available. Any bucket items added during the lease
    /// are added to the returned bucket. Returns true if the lease was returned, otherwise false
    pub fn return_lease(&self, key: &K, bucket: RetryBucket<T>) -> bool {
        let mut inner = acquire_write_lock!(self.inner);
        // It is an error to return a lease that doesn't exist.
        inner.return_lease(key, bucket)
    }

    /// Add an item to a bucket. If a bucket doesn't exist for the given key one is created, otherwise
    /// the message is appended to the available or temporary bucket.
    pub fn add_item(&self, key: K, item: T) {
        let mut inner = acquire_write_lock!(self.inner);
        inner.add_item(key, item);
    }

    /// Remove a bucket from the queue. If an available/temporary bucket exists, it is returned.
    pub fn remove(&self, key: &K) -> Option<RetryBucket<T>> {
        let mut inner = acquire_write_lock!(self.inner);
        inner.remove(key)
    }

    /// Clear the contents of the queue
    pub fn clear(&self) {
        let mut inner = acquire_write_lock!(self.inner);
        inner.clear();
    }
}

struct Inner<K, T>
where K: Hash + Eq
{
    buckets: HashMap<K, Lease<RetryBucket<T>>>,
    roster: BinaryHeap<ScheduledItem<K>>,
}

impl<K, T> Inner<K, T>
where
    K: Hash + Eq,
    K: Clone,
{
    fn new() -> Self {
        Self {
            buckets: HashMap::new(),
            roster: BinaryHeap::new(),
        }
    }

    fn contains(&self, key: &K) -> bool {
        self.buckets.contains_key(key)
    }

    fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }

    fn clear(&mut self) {
        self.buckets.clear();
        self.roster.clear();
    }

    fn add_item(&mut self, key: K, item: T) {
        match self.buckets.get_mut(&key) {
            // Messages have already been added for this key, so we simply add it to
            // the existing available bucket
            Some(Lease::Available(bucket)) => {
                bucket.push_back(item);
            },
            // The bucket has been taken, in the meantime other messages for the node have
            // arrived. We fill a "taken" bucket with these messages. It is the job of
            // the worker that has taken the original bucket to process these messages
            // if it is able. It does this by removing the `Taken` Booking which returns
            // the rest of the messages.
            Some(Lease::Taken(Some(bucket))) => {
                bucket.push_back(item);
            },
            Some(Lease::Taken(v @ None)) => {
                // Set the bucket as the "taken" bucket
                *v = Some(RetryBucket::new(vec![item]));
            },
            None => {
                // Create a new bucket and schedule on the roster
                let bucket = RetryBucket::new(vec![item]);
                self.add_schedule_for_key(key.clone(), bucket.schedule());
                self.buckets.insert(key, Lease::Available(bucket));
            },
        }
    }

    fn return_lease(&mut self, key: &K, mut bucket: RetryBucket<T>) -> bool {
        let is_lease_returned = match self.buckets.get_mut(key) {
            Some(lease @ Lease::Taken(Some(_))) => {
                let b = lease.borrow_inner_mut().expect("RetryBucket in Lease must be Some");
                b.pour_from(&mut bucket);
                *lease = Lease::Available(bucket);
                true
            },
            Some(lease @ Lease::Taken(None)) => {
                *lease = Lease::Available(bucket);
                true
            },
            _ => false,
        };

        // If we were able to put the bucket back, we need to rescheduled
        // the bucket
        if is_lease_returned {
            let schedule = self
                .buckets
                .get(key)
                .expect("Lease must exist for given key")
                .borrow_inner()
                .expect("RetryBucket must exist in Lease")
                .schedule();

            self.add_schedule_for_key(key.clone(), schedule);
        }

        is_lease_returned
    }

    fn add_schedule_for_key(&mut self, key: K, schedule: Instant) {
        self.roster.push(ScheduledItem::new(key, schedule));
    }

    fn remove(&mut self, key: &K) -> Option<RetryBucket<T>> {
        self.buckets.remove(key).and_then(|lease| lease.take())
    }

    fn take_next(&mut self) -> Option<(K, RetryBucket<T>)> {
        match self.roster.peek().filter(|schedule| schedule.is_scheduled()) {
            Some(_) => {
                let schedule = self.roster.pop().expect("Could not pop ScheduledItem after peek");
                let lease = self
                    .buckets
                    .insert(schedule.item.clone(), Lease::Taken(None))
                    .expect("Invariant: Item in schedule does not exist in buckets");
                lease.take().map(|bucket| (schedule.item, bucket))
            },
            None => None,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    //---------------------------------- ScheduledItem --------------------------------------------//
    #[test]
    fn scheduled_item_new() {
        let item = 123;
        let instant = Instant::now();
        let scheduled = ScheduledItem::new(item, instant);
        assert_eq!(scheduled.item, item);
        assert_eq!(scheduled.scheduled_at, instant);
    }

    #[test]
    fn scheduled_item_is_scheduled() {
        let instant = Instant::now();
        let scheduled = ScheduledItem::new(123, instant);
        assert!(scheduled.is_scheduled())
    }

    #[test]
    fn scheduled_item_ord() {
        let instant = Instant::now();
        let scheduled1 = ScheduledItem::new(123, instant);
        let scheduled2 = ScheduledItem::new(123, instant + Duration::from_secs(10));
        let scheduled3 = ScheduledItem::new(123, instant + Duration::from_secs(20));
        assert!(scheduled1 > scheduled2);
        assert!(scheduled2 > scheduled3);
        assert!(scheduled1 > scheduled3);
        assert!(scheduled2 < scheduled1);
        assert!(scheduled3 < scheduled2);
        assert!(scheduled3 < scheduled1);
    }

    //---------------------------------- RetryBucket --------------------------------------------//

    #[test]
    fn retry_bucket_new() {
        let bucket = RetryBucket::new(vec!["A"]);
        assert_eq!(bucket.contents, vec!["A"]);
        assert_eq!(bucket.attempts, 1);
    }

    #[test]
    fn retry_bucket_default() {
        let bucket = RetryBucket::<()>::default();
        assert_eq!(bucket.contents, vec![]);
        assert_eq!(bucket.attempts, 0);
    }

    #[test]
    fn retry_bucket_incr_attempts() {
        let mut bucket = RetryBucket::<()>::default();
        bucket.incr_attempts();
        assert_eq!(bucket.attempts, 1);
        bucket.incr_attempts();
        assert_eq!(bucket.attempts, 2);
    }

    #[test]
    fn retry_bucket_pop_front() {
        let mut bucket = RetryBucket::new(vec![123, 456]);
        assert_eq!(bucket.pop_front(), Some(123));
        assert_eq!(bucket.pop_front(), Some(456));
        assert_eq!(bucket.pop_front(), None);
        assert_eq!(bucket.contents.len(), 0);
    }

    #[test]
    fn retry_bucket_push_front() {
        let mut bucket = RetryBucket::new(vec![]);
        bucket.push_front(123);
        bucket.push_front(456);
        assert_eq!(bucket.contents[0], 456);
        assert_eq!(bucket.contents[1], 123);
    }

    #[test]
    fn retry_bucket_push_back() {
        let mut bucket = RetryBucket::new(vec![]);
        bucket.push_back(123);
        bucket.push_back(456);
        assert_eq!(bucket.contents[0], 123);
        assert_eq!(bucket.contents[1], 456);
    }

    #[test]
    fn retry_bucket_pour_into() {
        let mut bucket1 = RetryBucket::new(vec![123]);
        let mut bucket2 = RetryBucket::new(vec![456]);
        bucket2.pour_from(&mut bucket1);
        assert_eq!(bucket1.contents[0], 123);
        assert_eq!(bucket1.contents[1], 456);
    }

    #[test]
    fn retry_bucket_exponential_backoff_offset() {
        let mut bucket = RetryBucket::<()>::default();
        bucket.attempts = 0;
        assert_eq!(bucket.exponential_backoff_offset(), 0);
        bucket.attempts = 1;
        assert_eq!(bucket.exponential_backoff_offset(), 2);
        bucket.attempts = 2;
        assert_eq!(bucket.exponential_backoff_offset(), 2);
        bucket.attempts = 3;
        assert_eq!(bucket.exponential_backoff_offset(), 4);
        bucket.attempts = 4;
        assert_eq!(bucket.exponential_backoff_offset(), 8);
        bucket.attempts = 5;
        assert_eq!(bucket.exponential_backoff_offset(), 16);
        bucket.attempts = 6;
        assert_eq!(bucket.exponential_backoff_offset(), 32);
        bucket.attempts = 7;
        assert_eq!(bucket.exponential_backoff_offset(), 64);
        bucket.attempts = 8;
        assert_eq!(bucket.exponential_backoff_offset(), 128);
        bucket.attempts = 9;
        assert_eq!(bucket.exponential_backoff_offset(), 256);
        bucket.attempts = 10;
        assert_eq!(bucket.exponential_backoff_offset(), 512);
    }

    #[test]
    fn retry_bucket_schedule() {
        let mut bucket = RetryBucket::<()>::default();
        bucket.attempts = 1;
        assert!(bucket.schedule() > Instant::now());
        assert!(bucket.schedule() < Instant::now() + Duration::from_secs(3));
    }

    //---------------------------------- Lease --------------------------------------------//

    #[test]
    fn lease_take() {
        // Non-copy type
        let s = "A".to_string();
        let lease = Lease::Available(s.clone());
        assert_eq!(lease.take(), Some(s));

        let s = "A".to_string();
        let lease = Lease::Taken(Some(s.clone()));
        assert_eq!(lease.take(), Some(s));

        let lease = Lease::<()>::Taken(None);
        assert_eq!(lease.take(), None);
    }

    #[test]
    fn lease_borrow_inner() {
        let lease = Lease::Available(123);
        assert_eq!(lease.borrow_inner(), Some(&123));

        let lease = Lease::Taken(Some(123));
        assert_eq!(lease.borrow_inner(), Some(&123));

        let lease = Lease::<()>::Taken(None);
        assert_eq!(lease.borrow_inner(), None);
    }

    #[test]
    fn lease_borrow_inner_mut() {
        let mut lease = Lease::Available(123);
        assert_eq!(lease.borrow_inner_mut(), Some(&mut 123));

        let mut lease = Lease::Taken(Some(123));
        assert_eq!(lease.borrow_inner_mut(), Some(&mut 123));

        let mut lease = Lease::<()>::Taken(None);
        assert_eq!(lease.borrow_inner_mut(), None);
    }

    //---------------------------------- RetryBucket --------------------------------------------//

    #[test]
    fn retry_queue_new() {
        let retry_queue = RetryQueue::<(), ()>::new();
        let lock = acquire_read_lock!(retry_queue.inner);
        assert!(lock.buckets.is_empty());
        assert!(lock.roster.is_empty());
    }

    #[test]
    fn retry_queue_clear() {
        let retry_queue = RetryQueue::new();
        retry_queue.add_item(1, 1);
        retry_queue.add_item(1, 1);
        retry_queue.add_item(2, 2);
        retry_queue.add_item(2, 2);

        assert!(!retry_queue.is_empty());
        retry_queue.clear();
        assert!(retry_queue.is_empty());
        assert!(!retry_queue.contains(&1));
        assert!(!retry_queue.contains(&2));
    }

    #[test]
    fn retry_queue_contains() {
        let retry_queue = RetryQueue::new();
        assert!(!retry_queue.contains(&1));
        retry_queue.add_item(1, 1);
        assert!(retry_queue.contains(&1));
    }

    #[test]
    fn retry_queue_is_empty() {
        let retry_queue = RetryQueue::new();
        assert!(retry_queue.is_empty());
        retry_queue.add_item(1, 1);
        assert!(!retry_queue.is_empty());
    }

    fn util_modify_schedule(retry_queue: &RetryQueue<u32, u32>, scheduled_at: Instant) {
        // Ensure that the item is scheduled
        let mut lock = acquire_write_lock!(retry_queue.inner);
        let mut item = lock.roster.pop().unwrap();
        item.scheduled_at = scheduled_at;
        lock.roster.push(item);
    }

    #[test]
    fn retry_queue_lease_next() {
        let retry_queue = RetryQueue::new();
        assert!(retry_queue.lease_next().is_none());
        retry_queue.add_item(1, 2);

        util_modify_schedule(&retry_queue, Instant::now() - Duration::from_secs(1));
        let (k, mut bucket) = retry_queue.lease_next().unwrap();
        // We've leased the bucket, but retry queue still knows about the key
        assert!(retry_queue.contains(&k));

        assert_eq!(k, 1);
        assert_eq!(bucket.pop_front(), Some(2));
        assert!(retry_queue.lease_next().is_none());

        {
            let lock = acquire_read_lock!(retry_queue.inner);
            assert!(lock.buckets.get(&1).unwrap().is_taken());
        }

        // Required to remove the Taken lease - removing the Taken lease when
        // done is part of the contract of using RetryQueue
        retry_queue.remove(&1);

        retry_queue.add_item(1, 2);
        util_modify_schedule(&retry_queue, Instant::now() + Duration::from_secs(10));
        assert!(retry_queue.lease_next().is_none());
    }

    #[test]
    fn retry_queue_add_item() {
        let retry_queue = RetryQueue::new();
        retry_queue.add_item(1, 2);
        retry_queue.add_item(1, 3);
        retry_queue.add_item(2, 3);

        {
            let lock = acquire_read_lock!(retry_queue.inner);
            assert_eq!(lock.buckets.len(), 2);
            assert!(lock.buckets.contains_key(&1));
            assert!(lock.buckets.contains_key(&2));
            assert_eq!(lock.buckets.get(&1).unwrap().borrow_inner().unwrap().contents.len(), 2);
            assert_eq!(lock.buckets.get(&2).unwrap().borrow_inner().unwrap().contents.len(), 1);
        }
    }

    #[test]
    fn retry_queue_return_lease() {
        let retry_queue = RetryQueue::new();
        retry_queue.add_item(1, 2);
        retry_queue.add_item(1, 3);

        util_modify_schedule(&retry_queue, Instant::now() - Duration::from_millis(1));

        let (k, bucket) = retry_queue.lease_next().unwrap();
        {
            let lock = acquire_read_lock!(retry_queue.inner);
            assert!(lock.buckets.get(&1).unwrap().is_taken());
        }

        // Items added while lease is taken should be appended to the returning bucket
        retry_queue.add_item(1, 4);
        retry_queue.add_item(1, 5);

        assert!(retry_queue.return_lease(&k, bucket));
        {
            let lock = acquire_read_lock!(retry_queue.inner);
            assert!(lock.buckets.get(&1).unwrap().is_available());
            assert_eq!(lock.buckets.get(&1).unwrap().borrow_inner().unwrap().contents.len(), 4);
            // Check that the resulting contents of the bucket include the items added during the lease
            assert_eq!(lock.buckets.get(&1).unwrap().borrow_inner().unwrap().contents[0], 2);
            assert_eq!(lock.buckets.get(&1).unwrap().borrow_inner().unwrap().contents[1], 3);
            assert_eq!(lock.buckets.get(&1).unwrap().borrow_inner().unwrap().contents[2], 4);
            assert_eq!(lock.buckets.get(&1).unwrap().borrow_inner().unwrap().contents[3], 5);
        }
    }

    #[test]
    fn retry_queue_remove() {
        let retry_queue = RetryQueue::new();
        retry_queue.add_item(1, 2);

        retry_queue.remove(&1).unwrap();
        assert!(retry_queue.remove(&1).is_none());
        assert!(!retry_queue.contains(&1));
    }

    #[test]
    fn retry_queue_clone() {
        let retry_queue = RetryQueue::new();
        retry_queue.add_item(1, 2);
        let clone = retry_queue.clone();
        assert!(clone.contains(&1));
    }
}
