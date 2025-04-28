use blake2::Blake2b;
use digest::consts::U32;
use jmt::SimpleHasher;
use tari_crypto::{
    hash_domain,
    hashing::{AsFixedBytes, DomainSeparatedHasher},
};
hash_domain!(OutputSmtHashDomain, "com.tari.base_layer.core.output_smt", 1);
pub type OutputSmtHasherBlake256 = DomainSeparatedHasher<Blake2b<U32>, OutputSmtHashDomain>;
pub struct SmtHasher {
    hasher: OutputSmtHasherBlake256,
}

impl SimpleHasher for SmtHasher {
    fn new() -> Self {
        Self {
            hasher: OutputSmtHasherBlake256::new(),
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }

    fn finalize(self) -> [u8; 32] {
        self.hasher.finalize().as_fixed_bytes().expect("Hash is 32 bytes")
    }
}
