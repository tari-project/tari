// Copyright 2019, The Taiji Project
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

use std::{borrow::BorrowMut, collections::HashMap, hash::Hash, time::Duration};

use futures::{stream, Stream, StreamExt};
use tokio::sync::{broadcast, mpsc};

#[allow(dead_code)]
#[allow(clippy::mutable_key_type)] // Note: Clippy Breaks with Interior Mutability Error
pub fn get_item_counts<I>(items: I) -> HashMap<I::Item, usize>
where
    I: IntoIterator,
    I::Item: Hash + Eq,
{
    items.into_iter().fold(HashMap::new(), |mut counts, item| {
        let entry = counts.entry(item).or_insert(0);
        *entry += 1;
        counts
    })
}

/// Collect $take items from a stream or timeout for Duration $timeout.
///
/// Requires the `tokio` runtime and should be used in an async context.
///
/// ```edition2018
/// # use tokio::runtime::Runtime;
/// # use futures::stream;
/// # use std::time::Duration;
/// # use taiji_test_utils::collect_stream;
///
/// let mut rt = Runtime::new().unwrap();
/// let mut stream = stream::iter(1..10);
/// assert_eq!(
///     rt.block_on(async { collect_stream!(stream, take = 3, timeout = Duration::from_secs(1)) }),
///     vec![1, 2, 3]
/// );
/// ```
#[macro_export]
macro_rules! collect_stream {
    ($stream:expr, take=$take:expr, timeout=$timeout:expr $(,)?) => {{
        use tokio::time;

        // Evaluate $stream once, NOT in the loop 🐛🚨
        let mut stream = &mut $stream;

        let mut items = Vec::new();
        loop {
            if let Some(item) = time::timeout($timeout, futures::stream::StreamExt::next(stream))
                .await
                .expect(
                    format!(
                        "Timeout before stream could collect {} item(s). Got {} item(s).",
                        $take,
                        items.len()
                    )
                    .as_str(),
                )
            {
                items.push(item);
                if items.len() == $take {
                    break items;
                }
            } else {
                break items;
            }
        }
    }};
    ($stream:expr, timeout=$timeout:expr $(,)?) => {{
        use tokio::time;

        let mut stream = &mut $stream;
        let mut items = Vec::new();
        while let Some(item) = time::timeout($timeout, futures::stream::StreamExt::next($stream))
            .await
            .expect(format!("Timeout before stream was closed. Got {} items.", items.len()).as_str())
        {
            items.push(item);
        }
        items
    }};
}

#[macro_export]
macro_rules! collect_recv {
    ($stream:expr, take=$take:expr, timeout=$timeout:expr $(,)?) => {{
        use tokio::time;

        // Evaluate $stream once, NOT in the loop 🐛🚨
        let mut stream = &mut $stream;

        let mut items = Vec::new();
        loop {
            let item = time::timeout($timeout, stream.recv()).await.expect(&format!(
                "Timeout before stream could collect {} item(s). Got {} item(s).",
                $take,
                items.len()
            ));

            items.push(item.expect(&format!("{}/{} recv ended early", items.len(), $take)));
            if items.len() == $take {
                break items;
            }
        }
    }};
    ($stream:expr, timeout=$timeout:expr $(,)?) => {{
        use tokio::time;

        let mut stream = &mut $stream;

        let mut items = Vec::new();
        while let Some(item) = time::timeout($timeout, stream.recv())
            .await
            .expect(format!("Timeout before stream was closed. Got {} items.", items.len()).as_str())
        {
            items.push(item);
        }
        items
    }};
}

#[macro_export]
macro_rules! collect_try_recv {
    ($stream:expr, take=$take:expr, timeout=$timeout:expr $(,)?) => {{
        use tokio::time;

        // Evaluate $stream once, NOT in the loop 🐛🚨
        let mut stream = &mut $stream;

        let mut items = Vec::new();
        loop {
            let item = time::timeout($timeout, stream.recv()).await.expect(&format!(
                "Timeout before stream could collect {} item(s). Got {} item(s).",
                $take,
                items.len()
            ));

            items.push(item.expect(&format!("{}/{} recv returned unexpected result", items.len(), $take)));
            if items.len() == $take {
                break items;
            }
        }
    }};
    ($stream:expr, timeout=$timeout:expr $(,)?) => {{
        use tokio::time;

        let mut stream = &mut $stream;
        let mut items = Vec::new();
        while let Ok(item) = time::timeout($timeout, stream.recv())
            .await
            .expect(format!("Timeout before stream was closed. Got {} items.", items.len()).as_str())
        {
            items.push(item);
        }
        items
    }};
}

/// Returns a HashMap of the number of occurrences of a particular item in a stream.
///
/// ```edition2018
/// # use tokio::runtime::Runtime;
/// # use futures::stream;
/// # use std::time::Duration;
/// # use taiji_test_utils::collect_stream_count;
///
/// let rt = Runtime::new().unwrap();
/// let mut stream = stream::iter(vec![1, 2, 2, 3, 2]);
/// assert_eq!(
///     rt.block_on(async { collect_stream_count!(&mut stream, timeout = Duration::from_secs(1)) })
///         .get(&2),
///     Some(&3)
/// );
/// ```
#[macro_export]
macro_rules! collect_stream_count {
    ($stream:expr, take=$take:expr, timeout=$timeout:expr$(,)?) => {{
        use std::collections::HashMap;
        let items = $crate::collect_stream!($stream, take = $take, timeout = $timeout);
        $crate::streams::get_item_counts(items)
    }};

    ($stream:expr, timeout=$timeout:expr $(,)?) => {{
        use std::collections::HashMap;
        let items = $crate::collect_stream!($stream, timeout = $timeout);
        $crate::streams::get_item_counts(items)
    }};
}

pub async fn assert_in_stream<S, P, R>(stream: &mut S, mut predicate: P, timeout: Duration) -> R
where
    S: Stream + Unpin,
    P: FnMut(S::Item) -> Option<R>,
{
    loop {
        if let Some(item) = tokio::time::timeout(timeout, stream.next())
            .await
            .expect("Timeout before stream emitted")
        {
            if let Some(r) = (predicate)(item) {
                break r;
            }
        } else {
            panic!("Predicate did not return true before the stream ended");
        }
    }
}

pub async fn assert_in_mpsc<T, P, R>(rx: &mut mpsc::Receiver<T>, mut predicate: P, timeout: Duration) -> R
where P: FnMut(T) -> Option<R> {
    loop {
        if let Some(item) = tokio::time::timeout(timeout, rx.recv())
            .await
            .expect("Timeout before stream emitted")
        {
            if let Some(r) = (predicate)(item) {
                break r;
            }
        } else {
            panic!("Predicate did not return true before the mpsc stream ended");
        }
    }
}

pub async fn assert_in_broadcast<T, P, R>(rx: &mut broadcast::Receiver<T>, mut predicate: P, timeout: Duration) -> R
where
    P: FnMut(T) -> Option<R>,
    T: Clone,
{
    loop {
        if let Ok(item) = tokio::time::timeout(timeout, rx.recv())
            .await
            .expect("Timeout before stream emitted")
        {
            if let Some(r) = (predicate)(item) {
                break r;
            }
        } else {
            panic!("Predicate did not return true before the broadcast channel ended");
        }
    }
}

pub fn convert_mpsc_to_stream<T>(rx: &mut mpsc::Receiver<T>) -> impl Stream<Item = T> + '_ {
    stream::unfold(rx, |rx| async move { rx.recv().await.map(|t| (t, rx)) })
}

pub fn convert_unbounded_mpsc_to_stream<T>(rx: &mut mpsc::UnboundedReceiver<T>) -> impl Stream<Item = T> + '_ {
    stream::unfold(rx, |rx| async move { rx.recv().await.map(|t| (t, rx)) })
}

pub fn convert_broadcast_to_stream<'a, T, S>(rx: S) -> impl Stream<Item = Result<T, broadcast::error::RecvError>> + 'a
where
    T: Clone + Send + 'static,
    S: BorrowMut<broadcast::Receiver<T>> + 'a,
{
    stream::unfold(rx, |mut rx| async move {
        Some(rx.borrow_mut().recv().await).map(|t| (t, rx))
    })
}
