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

use futures::{select, stream::FusedStream, FutureExt, Stream, StreamExt};
use rand::{distributions::Alphanumeric, CryptoRng, OsRng, Rng};
use std::{
    collections::HashMap,
    fmt::Debug,
    hash::Hash,
    iter,
    thread,
    time::{Duration, Instant},
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait},
};
use tari_transactions::{
    tari_amount::MicroTari,
    transaction::{OutputFeatures, TransactionInput, UnblindedOutput},
    types::{PrivateKey, PublicKey, COMMITMENT_FACTORY},
};

pub fn assert_change<F, T>(mut func: F, to: T, poll_count: usize)
where
    F: FnMut() -> T,
    T: Eq + Debug,
{
    let mut i = 0;
    loop {
        let last_val = func();
        if last_val == to {
            break;
        }

        i += 1;
        if i >= poll_count {
            panic!(
                "Value did not change to {:?} within {}ms (last value: {:?})",
                to,
                poll_count * 100,
                last_val,
            );
        }

        thread::sleep(Duration::from_millis(100));
    }
}

/// This method will read the specified number of items from a stream and assemble a HashMap with the received item as a
/// key and the number of occurrences as a value. If the stream does not yield the desired number of items the function
/// will return what it is has received
pub async fn event_stream_count<TStream>(
    mut stream: TStream,
    num_items: usize,
    timeout: Duration,
) -> HashMap<TStream::Item, u32>
where
    TStream: Stream + FusedStream + Unpin,
    TStream::Item: Hash + Eq + Clone,
{
    let mut result = HashMap::new();
    let mut count = 0;
    loop {
        select! {
                    item = stream.select_next_some() => {
                        let e = result.entry(item.clone()).or_insert(0);
                        *e += 1;
                        count += 1;
                        if count >= num_items {
                            break;
                        }
                     },
                    _ = tokio::timer::delay(Instant::now() + timeout).fuse() => { break; },
        //            complete => { break; },
                }
    }

    result
}

pub struct TestParams {
    pub spend_key: PrivateKey,
    pub change_key: PrivateKey,
    pub offset: PrivateKey,
    pub nonce: PrivateKey,
    pub public_nonce: PublicKey,
}

impl TestParams {
    pub fn new<R: Rng + CryptoRng>(rng: &mut R) -> TestParams {
        let r = PrivateKey::random(rng);
        TestParams {
            spend_key: PrivateKey::random(rng),
            change_key: PrivateKey::random(rng),
            offset: PrivateKey::random(rng),
            public_nonce: PublicKey::from_secret_key(&r),
            nonce: r,
        }
    }
}

pub fn make_input<R: Rng + CryptoRng>(rng: &mut R, val: MicroTari) -> (TransactionInput, UnblindedOutput) {
    let key = PrivateKey::random(rng);
    let commitment = COMMITMENT_FACTORY.commit_value(&key, val.into());
    let input = TransactionInput::new(OutputFeatures::default(), commitment);
    (input, UnblindedOutput::new(val, key, None))
}

pub fn random_string(len: usize) -> String {
    let mut rng = OsRng::new().unwrap();
    iter::repeat(()).map(|_| rng.sample(Alphanumeric)).take(len).collect()
}
