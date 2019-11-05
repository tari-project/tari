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

use futures::{select, stream::FusedStream, FutureExt, Stream, StreamExt};
use std::{
    hash::Hash,
    time::{Duration, Instant},
};

/// This method will read the specified number of items from a stream and assemble a HashMap with the received item as a
/// key and the number of occurences as a value. If the stream does not yield the desired number of items the function
/// will return what it is has received
pub async fn event_stream_count<TStream, I>(mut stream: TStream, num_items: usize, timeout: Duration) -> Vec<I>
where
    TStream: Stream<Item = I> + FusedStream + Unpin,
    I: Hash + Eq + Clone,
{
    let mut result = Vec::new();
    let mut count = 0;
    loop {
        select! {
            item = stream.select_next_some() => {
                result.push(item.clone());
                count += 1;
                if count >= num_items {
                    break;
                }
             },
            _ = tokio::timer::delay(Instant::now() + timeout).fuse() => { break; },
            complete => { break; },
        }
    }

    result
}
