#[macro_use]
extern crate criterion;
#[macro_use]
extern crate serde_derive;

use blake2::Blake2b;
use criterion::Criterion;
use digest::Digest;
use merklemountainrange::{mmr::*, to_graph::ToGraph};
use rand::{
    rngs::{OsRng, StdRng},
    Rng,
    RngCore,
    SeedableRng,
};
use std::{fs, time::Duration};
use tari_storage::{keyvalue_store::DataStore, lmdb::*};
use tempdir::TempDir;

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

fn apply_state_with_storage(c: &mut Criterion) {
    use test_structures::*;

    c.bench_function_over_inputs(
        "append",
        move |b, &&size| {
            let name: String = OsRng.next_u64().to_string();

            let mut mmr = MerkleMountainRange::<H, Blake2b>::new();
            mmr.init_persistance_store(&name, 0);

            let tmp_dir = TempDir::new(&name).unwrap();
            let builder = LMDBBuilder::new();
            let mut store = builder
                .set_mapsize(5)
                .set_path(tmp_dir.path().to_str().unwrap())
                .add_database(&"mmr_mmr_checkpoints".to_string())
                .add_database(&"mmr_mmr_objects".to_string())
                .add_database(&"mmr_init".to_string())
                .build()
                .unwrap();

            b.iter(|| {
                for _ in 0..size {
                    mmr.append(vec![H(OsRng.next_u64())]).unwrap();
                    mmr.apply_state(&mut store).unwrap();
                }
            });
        },
        TEST_SIZES,
    );
}

fn get_merkle_root(c: &mut Criterion) {
    use test_structures::*;

    c.bench_function_over_inputs(
        "get_merkle_root",
        move |b, &&size| {
            let mut mmr = MerkleMountainRange::<H, Blake2b>::new();
            b.iter(|| {
                for _ in 0..size {
                    mmr.append(vec![H(OsRng.next_u64())]).unwrap();
                    let root = mmr.get_merkle_root();
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
                dbg!(index);
                let proof = mmr.get_hash_proof(&items[index]);
                dbg!(proof.len());
                // dbg!(&proof);
                assert!(!proof.is_empty());
            });
        },
        TEST_SIZES,
    );
}

criterion_group!(
name = merkle_mountain_range;
config= Criterion::default().warm_up_time(Duration::from_millis(500)).sample_size(10);
targets= append,get_merkle_root,get_object);

criterion_main!(merkle_mountain_range);
