// Copyright 2019 The Tari Project
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

#[macro_use]
extern crate criterion;

#[macro_use]
extern crate serde;

use blake2::Blake2b;
use criterion::Criterion;
use merklemountainrange::mmr::*;
use rand::{rngs::OsRng, Rng, RngCore};
use std::time::Duration;

const TEST_SIZES: &[usize] = &[10, 100, 1000, 10_000, 100_000];

mod test_structures {
    use std::slice;

    use std::mem;

    use tari_utilities::hash::Hashable;

    #[derive(Serialize, Deserialize)]
    pub struct H(pub u64);

    impl Hashable for H {
        fn hash(&self) -> Vec<u8> {
            as_bytes(self).to_vec()
        }
    }

    fn as_bytes<T>(x: &T) -> &[u8] {
        unsafe { slice::from_raw_parts(x as *const T as *const u8, mem::size_of_val(x)) }
    }
}

fn append(c: &mut Criterion) {
    use test_structures::*;

    c.bench_function_over_inputs(
        "append",
        move |b, &&size| {
            let mut mmr = MerkleMountainRange::<H, Blake2b>::new();
            b.iter(|| {
                for _ in 0..size {
                    mmr.append(vec![H(OsRng.next_u64())]).unwrap();
                }
            });
        },
        TEST_SIZES,
    );
}

// TODO: Can't work out the parameters needed for storage
// fn apply_state_with_storage(c: &mut Criterion) {
//    use test_structures::*;
//
//    c.bench_function_over_inputs(
//        "apply_state_with_storage",
//        move |b, &&size| {
//            let name: String = OsRng.next_u64().to_string();
//
//
//            let tmp_dir = TempDir::new(&name).unwrap();
//            println!("{:?}", tmp_dir);
//            let builder = LMDBBuilder::new();
//            let mut store = builder
//                .set_mapsize(5)
//                .set_path(tmp_dir.path().to_str().unwrap())
//                .add_database(&format!("{}_mmr_checkpoints", &name))
//                .add_database(&format!("{}_mmr_objects", &name))
//                .add_database(&format!("{}_init", &name))
//                .build()
//                .unwrap();
//
//            let mut mmr = MerkleMountainRange::<H, Blake2b>::new();
//            mmr.init_persistance_store(&name, 1);
//
//            b.iter(|| {
//                for _ in 0..size {
//                    mmr.append(vec![H(OsRng.next_u64())]).unwrap();
//                    dbg!("checkpointing");
//                    mmr.checkpoint().unwrap();
//                    dbg!("applying");
//                    mmr.apply_state(&mut store).unwrap();
//                }
//            });
//        },
//        TEST_SIZES,
//    );
//}

fn get_merkle_root(c: &mut Criterion) {
    use test_structures::*;

    c.bench_function_over_inputs(
        "get_merkle_root",
        move |b, &&size| {
            let mut mmr = MerkleMountainRange::<H, Blake2b>::new();
            b.iter(|| {
                for _ in 0..size {
                    mmr.append(vec![H(OsRng.next_u64())]).unwrap();
                    mmr.get_merkle_root();
                }
            });
        },
        TEST_SIZES,
    );
}

fn get_object(c: &mut Criterion) {
    use test_structures::*;

    c.bench_function_over_inputs(
        "get_object",
        move |b, &&size| {
            let mut mmr = MerkleMountainRange::<H, Blake2b>::new();
            b.iter(|| {
                let mut items = vec![];

                for x in 0..size {
                    let i = H(OsRng.next_u64());
                    mmr.append(vec![i]).unwrap();
                    items.push(mmr.get_object_hash(x).unwrap());
                }

                for x in 0..size {
                    mmr.get_object(&items[size - 1 - x]).unwrap();
                }
            });
        },
        TEST_SIZES,
    );
}

fn get_proof(c: &mut Criterion) {
    use test_structures::*;

    c.bench_function_over_inputs(
        "get_proof",
        move |b, &&size| {
            let mut items = vec![];
            let mut mmr = MerkleMountainRange::<H, Blake2b>::new();

            for x in 0..size {
                let i = H(OsRng.next_u64());
                mmr.push(i).unwrap();
                items.push(mmr.get_object_hash(x).unwrap());
            }

            b.iter(|| {
                let index = OsRng.gen_range(0, size);
                let proof = mmr.get_hash_proof(&items[index]);
                assert!(!proof.is_empty());
            });
        },
        TEST_SIZES,
    );
}

criterion_group!(
name = merkle_mountain_range;
config= Criterion::default().warm_up_time(Duration::from_millis(500)).sample_size(10);
targets= append,get_merkle_root,get_object,get_proof);

criterion_main!(merkle_mountain_range);
