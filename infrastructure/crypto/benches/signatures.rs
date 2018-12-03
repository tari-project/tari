#![feature(test)]
extern crate test;

use crypto::{
    curve25519::{Curve25519EdDSA, Curve25519PublicKey, Curve25519SecretKey},
    keys::{PublicKey, SecretKey, SecretKeyFactory},
    signatures::SchnorrSignature,
};
use ed25519_dalek::SecretKey as dsk;
use rand::OsRng;
use test::Bencher;

#[bench]
fn generate_secret_key(b: &mut Bencher) {
    let mut rng = OsRng::new().unwrap();
    b.iter(|| {
        let key = Curve25519SecretKey::random(&mut rng);
        key.to_hex();
    });
}

#[bench]
fn native_keypair(b: &mut Bencher) {
    let mut rng = OsRng::new().unwrap();
    b.iter(|| {
        dsk::generate(&mut rng);
    });
}

#[bench]
fn curve25519_eddsa(b: &mut Bencher) {
    let mut rng = OsRng::new().unwrap();
    let msg = b"This parrot is dead";
    let k = Curve25519SecretKey::random(&mut rng);
    let p = Curve25519PublicKey::from_secret_key(&k);
    b.iter(|| {
        let sig = Curve25519EdDSA::sign(&k, &p, msg);
        assert!(sig.verify(&p, msg));
    })
}
