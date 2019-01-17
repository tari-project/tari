//! The Tari-compatible implementation of Curve25519 based on the curve25519-dalek implementation
use crate::{
    common::{ByteArray, ByteArrayError},
    keys::{PublicKey, SecretKey, SecretKeyFactory},
};
use ed25519_dalek::{PublicKey as DalekPK, SecretKey as DalekSK, SECRET_KEY_LENGTH};
use rand::{CryptoRng, Rng};
use sha2::Sha512;

//----------------------------------------   Secret Keys  ----------------------------------------//
#[derive(Debug)]
pub struct Curve25519SecretKey(pub(crate) DalekSK);

impl SecretKey for Curve25519SecretKey {}

impl SecretKeyFactory for Curve25519SecretKey {
    fn random<R: CryptoRng + Rng>(rng: &mut R) -> Curve25519SecretKey {
        let k = DalekSK::generate(rng);
        Curve25519SecretKey(k)
    }
}

impl PartialEq<Curve25519SecretKey> for Curve25519SecretKey {
    fn eq(&self, other: &Curve25519SecretKey) -> bool {
        other.to_bytes() == self.to_bytes()
    }
}

impl Eq for Curve25519SecretKey {}

impl ByteArray for Curve25519SecretKey {
    /// Creates a secret key from the first `SK_LEN` bytes of the byte array. Returns an error if `bytes` is less
    /// than `SK_LEN` bytes long.
    fn from_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        if bytes.len() < SECRET_KEY_LENGTH {
            return Err(ByteArrayError::ConversionError("Byte array was too short".into()));
        }
        match DalekSK::from_bytes(&bytes[0..SECRET_KEY_LENGTH]) {
            Ok(v) => Ok(Curve25519SecretKey(v)),
            Err(e) => Err(ByteArrayError::ConversionError(e.to_string())),
        }
    }

    fn to_bytes(&self) -> &[u8] {
        self.0.as_bytes().as_ref()
    }
}
//----------------------------------------   Public Keys  ----------------------------------------//

#[derive(Debug, PartialEq, Eq)]
pub struct Curve25519PublicKey(pub(crate) DalekPK);

impl PublicKey for Curve25519PublicKey {
    type K = Curve25519SecretKey;

    fn from_secret_key(k: &Curve25519SecretKey) -> Self {
        Curve25519PublicKey(DalekPK::from_secret::<Sha512>(&k.0))
    }
}

impl ByteArray for Curve25519PublicKey {
    fn from_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        match DalekPK::from_bytes(bytes) {
            Ok(v) => Ok(Curve25519PublicKey(v)),
            Err(e) => Err(ByteArrayError::ConversionError(format!("Could not convert byte array to PublicKey: {}", e))),
        }
    }

    fn to_bytes(&self) -> &[u8] {
        self.0.as_bytes().as_ref()
    }
}

#[cfg(test)]
mod test {
    use super::{Curve25519PublicKey, Curve25519SecretKey};
    use crate::{
        common::ByteArray,
        keys::{PublicKey, SecretKeyFactory},
    };
    use rand;

    const PUB_KEY: [u8; 32] = [
        130, 039, 155, 015, 062, 076, 188, 063, 124, 122, 026, 251, 233, 253, 225, 220, 014, 041, 166, 120, 108, 035,
        254, 077, 160, 083, 172, 058, 219, 042, 086, 120,
    ];

    static SEC_KEY: [u8; 32] = [
        062, 070, 027, 163, 092, 182, 011, 003, 077, 234, 098, 004, 011, 127, 079, 228, 243, 187, 150, 073, 201, 137,
        076, 022, 085, 251, 152, 002, 241, 042, 072, 054,
    ];

    fn secret() -> Curve25519SecretKey {
        Curve25519SecretKey::from_bytes(&SEC_KEY).unwrap()
    }

    fn public() -> Curve25519PublicKey {
        Curve25519PublicKey::from_bytes(&PUB_KEY).unwrap()
    }

    #[test]
    fn test_generation() {
        let mut rng = rand::OsRng::new().unwrap();
        let k1 = Curve25519SecretKey::random(&mut rng);
        let k2 = Curve25519SecretKey::random(&mut rng);
        assert_ne!(k1, k2);
    }

    #[test]
    fn create_public_key() {
        let p = Curve25519PublicKey::from_secret_key(&secret());
        assert_eq!(p, public())
    }

    #[test]
    fn secret_to_hex() {
        assert_eq!(secret().to_hex(), "3e461ba35cb60b034dea62040b7f4fe4f3bb9649c9894c1655fb9802f12a4836")
    }

    #[test]
    fn pubkey_to_hex() {
        let p = Curve25519PublicKey::from_bytes(&PUB_KEY).unwrap();
        assert_eq!(p.to_hex(), "82279b0f3e4cbc3f7c7a1afbe9fde1dc0e29a6786c23fe4da053ac3adb2a5678")
    }

    #[test]
    fn secret_from_hex() {
        let k =
            Curve25519SecretKey::from_hex("3e461ba35cb60b034dea62040b7f4fe4f3bb9649c9894c1655fb9802f12a4836").unwrap();
        assert_eq!(k, secret());
    }

    #[test]
    fn public_from_hex() {
        let p =
            Curve25519PublicKey::from_hex("82279b0f3e4cbc3f7c7a1afbe9fde1dc0e29a6786c23fe4da053ac3adb2a5678").unwrap();
        assert_eq!(p, public());
    }

    #[test]
    fn secret_to_vec() {
        assert_eq!(secret().to_vec(), SEC_KEY.to_vec());
    }

    #[test]
    fn public_to_vec() {
        assert_eq!(public().to_vec(), PUB_KEY.to_vec());
    }

    #[test]
    fn secret_from_vec() {
        assert_eq!(secret(), Curve25519SecretKey::from_vec(&SEC_KEY.to_vec()).unwrap());
    }

    #[test]
    fn public_from_vec() {
        assert_eq!(public(), Curve25519PublicKey::from_vec(&PUB_KEY.to_vec()).unwrap());
    }
}
