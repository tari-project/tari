#[macro_use]
extern crate criterion;

use criterion::Criterion;
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

fn sign_and_verify_message(c: &mut Criterion) {
    c.bench_function("Sign and verify", |b| {
        let mut rng = OsRng::new().unwrap();
        b.iter(|| {
            let mut msg = [0u8; 32];
            rng.fill_bytes(&mut msg);
            let (k, p) = RistrettoPublicKey::random_keypair(&mut rng);
            let r = RistrettoSecretKey::random(&mut rng);
            let msg_key = RistrettoSecretKey::from_bytes(&msg).unwrap();
            let sig = RistrettoSchnorr::sign(k, r, &msg).unwrap();
            assert!(sig.verify(&p, &msg_key));
        });
    });
}

criterion_group!(
    name = signatures;
    config = Criterion::default().warm_up_time(Duration::from_millis(500));
    targets = generate_secret_key, native_keypair, sign_and_verify_message
    );
criterion_main!(signatures);
