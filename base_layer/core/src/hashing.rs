use digest::Digest;
use tari_crypto::{hash_domain, hashing::{DomainSeparatedHasher, LengthExtensionAttackResistant}};

hash_domain!(BaseLayerCoreDomain, "com.tari.tari-project.base_layer.core");

pub(crate) const LMDB_STORAGE_HASH_LABEL: &str = "lmdb_db";

pub(crate) fn base_layer_core_domain_separation<D: Digest + LengthExtensionAttackResistant>(label: &'static str) -> DomainSeparatedHasher<D, BaseLayerCoreDomain> {
    DomainSeparatedHasher::<D, BaseLayerCoreDomain>::new_with_label(label)
}