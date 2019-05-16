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

use criterion::Criterion;

use std::iter::repeat;
use tari_comms::connection::{Connection, Context, Direction, InprocAddress};

/// Connection benchmark
///
/// Benchmark a message being sent and received between two connections
fn bench_connection(c: &mut Criterion) {
    let ctx = Context::new();
    let addr = InprocAddress::random();

    let bytes = repeat(88u8).take(1 * 1024 * 1024 /* 10Mb */).collect::<Vec<u8>>();
    let receiver = Connection::new(&ctx, Direction::Inbound)
        .set_receive_hwm(0)
        .establish(&addr)
        .unwrap();

    let sender = Connection::new(&ctx, Direction::Outbound)
        .set_send_hwm(0)
        .establish(&addr)
        .unwrap();

    c.bench_function("connection: send/recv", move |b| {
        b.iter(|| {
            sender.send(&[bytes.as_slice()]).unwrap();
            receiver.receive(100).unwrap();
        });
    });
}

criterion_group!(benches, bench_connection);
