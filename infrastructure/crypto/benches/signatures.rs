#[macro_use]
extern crate criterion;

use criterion::{BatchSize, Criterion};
use rand::{OsRng, RngCore};
use std::time::Duration;
use tari_crypto::{
    keys::{PublicKey, SecretKey},
    ristretto::{RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey},
};
use tari_utilities::byte_array::ByteArray;

fn generate_secret_key(c: &mut Criterion) {
    c.bench_function("generate secret key", |b| {
        let mut rng = OsRng::new().unwrap();
        b.iter(|| {
            let _ = RistrettoSecretKey::random(&mut rng);
        });
    });
}

fn native_keypair(c: &mut Criterion) {
    c.bench_function("Generate key pair", |b| {
        let mut rng = OsRng::new().unwrap();
        b.iter(|| RistrettoPublicKey::random_keypair(&mut rng));
    });
}

struct SigningData {
    k: RistrettoSecretKey,
    p: RistrettoPublicKey,
    r: RistrettoSecretKey,
    m: RistrettoSecretKey,
}

fn gen_keypair() -> SigningData {
    let mut rng = OsRng::new().unwrap();
    let mut msg = [0u8; 32];
    rng.fill_bytes(&mut msg);
    let (k, p) = RistrettoPublicKey::random_keypair(&mut rng);
    let r = RistrettoSecretKey::random(&mut rng);
    let m = RistrettoSecretKey::from_bytes(&msg).unwrap();
    SigningData { k, p, r, m }
}

fn sign_message(c: &mut Criterion) {
    c.bench_function("Create RistrettoSchnorr", move |b| {
        b.iter_batched(
            || gen_keypair(),
            |d| {
                let _ = RistrettoSchnorr::sign(d.k, d.r, &d.m.to_vec()).unwrap();
            },
            BatchSize::SmallInput,
        );
    });

    //    assert!(sig.verify(&p, &msg_key));
}

fn verify_message(c: &mut Criterion) {
    c.bench_function("Verify RistrettoSchnorr", move |b| {
        b.iter_batched(
            || {
                let d = gen_keypair();
                let s = RistrettoSchnorr::sign(d.k.clone(), d.r.clone(), &d.m.to_vec()).unwrap();
                (d, s)
            },
            |(d, s)| assert!(s.verify(&d.p, &d.m)),
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(
name = signatures;
config = Criterion::default().warm_up_time(Duration::from_millis(500));
targets = generate_secret_key, native_keypair, sign_message, verify_message
);
criterion_main!(signatures);
