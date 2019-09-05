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

use derive_error::Error;
use futures::{executor::block_on, select, stream::FusedStream, Stream, StreamExt};
use std::{fmt::Debug, thread, time::Duration};
#[derive(Debug, Error, PartialEq)]
pub enum StreamAssertError {
    /// Did not reach the specified number of items from the stream in the specified time
    AssertCountError,
}

/// A utility function that accepts an Arc of a stream, expected minimum number of items to be received by that stream
/// and the time frame in which they should be received. The Stream is then polled over the course of the timeframe and
/// the received items collected. If after the time frame OR after the stream ends and there are less than the expected
/// count and error is thrown. If at least as many items as the count were received the items are returned and an Option
/// of the Arc of the channel is returned. However, if the stream ended after the expected number of items were received
/// then a None is returned instead of the Arc reference.
pub fn stream_assert_count<S>(
    mut stream: S,
    expected_count: usize,
    timeout_in_millis: u32,
) -> Result<(S, Vec<S::Item>), StreamAssertError>
where
    S: Stream + Send + Sync + Unpin + FusedStream + 'static,
    S::Item: Send + Clone,
{
    let mut result = Vec::new();
    let iterations = timeout_in_millis / 50;

    for _ in 0..iterations {
        let (mut items, completed) = block_on(async {
            let mut buffer = Vec::new();
            let mut complete = false;
            loop {
                select!(
                    item = stream.next() => {
                        if let Some(item) = item {
                            buffer.push(item)
                        }
                    },
                    complete => {
                        complete = true;
                        break
                    },
                    default => break,
                );
            }
            (buffer, complete)
        });

        result.append(&mut items);

        if result.len() >= expected_count || completed {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    if result.len() < expected_count {
        return Err(StreamAssertError::AssertCountError);
    }

    Ok((stream, result))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn stream_assert_count_test() {
        let (mut tx, rx) = futures::channel::mpsc::channel(10);

        for i in 0..4 {
            tx.try_send(i).unwrap()
        }

        let rx = rx.fuse();

        let (rx, _items) = stream_assert_count(rx, 3, 200).unwrap();

        let result = stream_assert_count(rx, 5, 200);

        assert_eq!(result.unwrap_err(), StreamAssertError::AssertCountError);

        // Test what happens when a stream closes before the timeout is done.
        let (mut tx2, rx2) = futures::channel::mpsc::channel(10);
        for i in 0..2 {
            tx2.try_send(i).unwrap()
        }

        let result2 = stream_assert_count(rx2.take(2).fuse(), 5, 200);
        assert_eq!(result2.unwrap_err(), StreamAssertError::AssertCountError);
    }
}
