// Copyright 2019, The Tari Project
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

use std::{collections::HashMap, hash::Hash};

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
/// # use tari_test_utils::collect_stream;
///
/// let mut rt = Runtime::new().unwrap();
/// let mut stream = stream::iter(1..10);
/// assert_eq!(rt.block_on(async { collect_stream!(stream, take=3, timeout=Duration::from_secs(1)) }), vec![1,2,3]);
/// ```
#[macro_export]
macro_rules! collect_stream {
    ($stream:expr, take=$take:expr, timeout=$timeout:expr $(,)?) => {{
        use futures::{Stream, StreamExt};
        use tokio::time;

        // Evaluate $stream once, NOT in the loop 🐛🚨
        let mut stream = &mut $stream;

        let mut items = Vec::new();
        loop {
            if let Some(item) = time::timeout($timeout, stream.next()).await.expect(
                format!(
                    "Timeout before stream could collect {} item(s). Got {} item(s).",
                    $take,
                    items.len()
                )
                .as_str(),
            ) {
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
        use futures::StreamExt;
        use tokio::time;

        let mut items = Vec::new();
        while let Some(item) = time::timeout($timeout, $stream.next())
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
/// # use tari_test_utils::collect_stream_count;
///
/// let mut rt = Runtime::new().unwrap();
/// let mut stream = stream::iter(vec![1,2,2,3,2]);
/// assert_eq!(rt.block_on(async { collect_stream_count!(stream, timeout=Duration::from_secs(1)) }).get(&2), Some(&3));
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
