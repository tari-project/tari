use crate::{
    curve25519::{Curve25519PublicKey, Curve25519SecretKey},
    keys::{PublicKey as TariPublicKey, SecretKey as TariSecretKey},
    signatures::SchnorrSignature,
};
use ed25519_dalek::{PublicKey, Signature, PUBLIC_KEY_LENGTH};
/// ! The Tari-compatible implementation of Curve25519 signatures based on the curve25519-dalek
/// implementation
use sha2::Sha512;

/// The EdDSA algorithm for Curve25519 using SHA512 as hash algorithm.
///
/// The author of the underlying Rust library for Curve25519 has some
/// [interesting comments](https://github.com/dalek-cryptography/ed25519-dalek/blob/12c5777b239b6033ab393cf9ce6a1919fa6fbb73/src/ed25519.rs#L351).
///
/// EdDSA uses a 'deterministic nonce' for the Schnorr signature. This is achieved by hashing the
/// secret key with a 512-bit digest algo (e.g. SHA-512) and then splitting the result in half.
/// The lower half is the actual key used to sign messages, after twiddling with some bits.
pub struct Curve25519EdDSA(pub(crate) Signature);

impl SchnorrSignature for Curve25519EdDSA {
    type K = Curve25519SecretKey;
    type P = Curve25519PublicKey;

    fn R(&self) -> Curve25519PublicKey {
        let b = self.0.to_bytes();
        Curve25519PublicKey::from_bytes(&b[0..PUBLIC_KEY_LENGTH]).unwrap()
    }

    fn s(&self) -> Curve25519SecretKey {
        let b = self.0.to_bytes();
        Curve25519SecretKey::from_bytes(&b[PUBLIC_KEY_LENGTH..]).unwrap()
    }

    //
    fn sign(secret: &Curve25519SecretKey, public: &Curve25519PublicKey, m: &[u8]) -> Self {
        let sig = secret.0.expand::<Sha512>().sign::<Sha512>(m, &public.0);
        Curve25519EdDSA(sig)
    }

    fn verify(&self, public: &Curve25519PublicKey, m: &[u8]) -> bool {
        PublicKey::verify::<Sha512>(&public.0, m, &self.0).is_ok()
    }
}

#[cfg(test)]
mod test {
    use crate::{
        curve25519::{
            curve25519_keys::{Curve25519PublicKey, Curve25519SecretKey},
            curve25519_sig::Curve25519EdDSA,
        },
        hex::from_hex,
        keys::{PublicKey, SecretKey},
        signatures::SchnorrSignature,
    };
    use std::{
        fs::File,
        io::{BufRead, BufReader},
    };

    /// A reproduction of the test set in ed25519-dalek, taken in turn from agl's ed25519 Golang
    /// package. There are 128 test cases in this set
    #[test]
    fn eddsa_signatures() {
        let mut line: String;
        let mut lineno: usize = 0;

        let f = File::open("test_vectors/edDSA_testvectors");
        assert!(f.is_ok(), "edDSA_testvectors not found");
        let file = BufReader::new(f.unwrap());

        for l in file.lines() {
            lineno += 1;
            line = l.unwrap();

            let parts: Vec<&str> = line.split(':').collect();
            assert_eq!(parts.len(), 5, "wrong number of fields in line {}", lineno);

            let sec_vec = from_hex(&parts[0]).unwrap();
            let pub_vec = from_hex(&parts[1]).unwrap();
            let msg_vec = from_hex(&parts[2]).unwrap();
            let msg = msg_vec.as_slice();
            // The test vectors add the message at the end of the sig, so just take the Sig itself
            let sig = &from_hex(&parts[3]).unwrap()[..64];
            let sig_r = &sig[..32];
            let sig_s = &sig[32..];

            println!("{}:  {:?}", lineno, parts);

            let secret = Curve25519SecretKey::from_bytes(&sec_vec[..32]).unwrap();
            let public = Curve25519PublicKey::from_bytes(&pub_vec[..32]).unwrap();

            let sig = Curve25519EdDSA::sign(&secret, &public, msg);

            assert_eq!(sig.R().to_bytes(), sig_r, "Public nonce mismatch for test {}", lineno);
            assert_eq!(sig.s().to_bytes(), sig_s, "Signature mismatch for test {}", lineno);

            assert!(sig.verify(&public, msg), "Signature verification failed on line {}", lineno);
        }
    }
}
