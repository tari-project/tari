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

use criterion::{criterion_group, Criterion};
use futures::{future::join_all, SinkExt, StreamExt};
use std::{iter::repeat, time::Duration};
use tari_broadcast_channel::{bounded, raw_bounded};
use tokio::runtime::Runtime;

/// Raw broadcast channel benchmark
///
/// NUM_SUBSCRIBERS receiving NUM_MESSAGES messages containing ITEM_SIZE bytes
fn bench_raw_channel(c: &mut Criterion) {
    const ITEM_SIZE: usize = 1024;
    const NUM_SUBSCRIBERS: usize = 1000;
    const BUF_SIZE: usize = 1000;
    const NUM_MESSAGES: usize = 1000;

    c.bench_function("raw channel", move |b| {
        let (tx, rx) = raw_bounded(BUF_SIZE);

        let mut subscribers = repeat(&rx).cloned().take(NUM_SUBSCRIBERS).collect::<Vec<_>>();

        let mut sub_read = subscribers.iter_mut().map(|s| s.next().unwrap());

        let mut counter = 0;
        b.iter_with_setup(
            || {
                (0..)
                    .map(|v| repeat(v).take(ITEM_SIZE).collect::<Vec<u8>>())
                    .take(NUM_MESSAGES)
                    .for_each(|v| {
                        tx.broadcast(v).unwrap();
                    });
            },
            |_| {
                assert!(sub_read.all(|v| (*v)[0] == counter));
                counter += 1;
            },
        );
    });
}

/// Async broadcast channel benchmark
///
/// NUM_SUBSCRIBERS receiving NUM_MESSAGES messages containing ITEM_SIZE bytes
fn bench_async_channel(c: &mut Criterion) {
    const NUM_SUBSCRIBERS: usize = 1000;
    const BUF_SIZE: usize = 50;
    const NUM_MESSAGES: usize = 50;

    c.bench_function("async channel", move |b| {
        let rt = Runtime::new().unwrap();
        let (mut publisher, subscriber) = bounded(BUF_SIZE);

        let mut subscribers = repeat(&subscriber).cloned().take(NUM_SUBSCRIBERS).collect::<Vec<_>>();

        b.iter_with_setup(
            || {
                (0..NUM_MESSAGES).for_each(|v| {
                    rt.block_on(publisher.send(v)).unwrap();
                });
            },
            |_| {
                (0..NUM_MESSAGES).for_each(|i| {
                    let sub_read_all = subscribers.iter_mut().map(|s| s.next());
                    let values = rt.block_on(join_all(sub_read_all));
                    assert_eq!(values.len(), NUM_SUBSCRIBERS);
                    assert!(values.iter().all(|v| **(v.as_ref().unwrap()) == i));
                })
            },
        );
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_millis(500));
    targets = bench_raw_channel, bench_async_channel
);
