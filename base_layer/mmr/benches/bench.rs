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
//

use blake2::Blake2b;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use digest::Digest;
use std::time::Duration;
use tari_mmr::MerkleMountainRange;

fn get_hashes(n: usize) -> Vec<Vec<u8>> {
    (0..n).map(|i| Blake2b::digest(&i.to_le_bytes()).to_vec()).collect()
}

fn build_mmr(c: &mut Criterion) {
    c.bench_function("Build MMR", move |b| {
        let hashes = get_hashes(1000);
        let mut mmr = MerkleMountainRange::<Blake2b, _>::new(Vec::default());
        b.iter_batched(
            || hashes.clone(),
            |hashes| {
                hashes.into_iter().for_each(|hash| {
                    mmr.push(hash).unwrap();
                });
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    name = mmr;
    config= Criterion::default().warm_up_time(Duration::from_millis(500)).sample_size(10);
    targets= build_mmr
);

criterion_main!(mmr);
