//  Copyright 2022. The Tari Project
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

use borsh::BorshSerialize;
use digest::Digest;
use tari_common::configuration::Network;
use tari_crypto::hashing::DomainSeparation;
use tari_hashing::DomainSeparatedBorshHasher;

/// Domain separated consensus encoding hasher.
/// This is a thin wrapper around the domain-separated Borsh hasher but adds the network byte in its constructor
/// functions
pub struct DomainSeparatedConsensusHasher<M, D> {
    hasher: DomainSeparatedBorshHasher<M, D>,
}

impl<M: DomainSeparation, D: Digest> DomainSeparatedConsensusHasher<M, D>
where D: Default
{
    pub fn new(label: &'static str) -> Self {
        Self::new_with_network(label, Network::get_current_or_user_setting_or_default())
    }

    pub fn new_with_network(label: &'static str, network: Network) -> Self {
        let hasher = DomainSeparatedBorshHasher::<M, D>::new_with_label(&format!("{}.n{}", label, network.as_byte()));
        Self { hasher }
    }

    pub fn finalize(self) -> digest::Output<D> {
        self.hasher.finalize()
    }

    pub fn update_consensus_encode<T: BorshSerialize>(&mut self, data: &T) {
        self.hasher.update_consensus_encode(data);
    }

    pub fn chain<T: BorshSerialize>(mut self, data: &T) -> Self {
        self.update_consensus_encode(data);
        self
    }
}

impl<M: DomainSeparation, D: Digest + Default> Default for DomainSeparatedConsensusHasher<M, D> {
    /// This `default` implementation is provided for convenience, but should not be used as the de-facto consensus
    /// hasher, rather specify a specific label
    fn default() -> Self {
        DomainSeparatedConsensusHasher::<M, D>::new("default")
    }
}

#[cfg(test)]
mod tests {
    use blake2::Blake2b;
    use digest::consts::U32;
    use tari_common::configuration::Network;
    use tari_crypto::hash_domain;
    use tari_script::script;

    use super::*;

    hash_domain!(TestHashDomain, "com.tari.test.test_hash", 0);

    #[test]
    fn network_yields_distinct_hash() {
        let label = "test";
        let input = [1u8; 32];

        // Generate a mainnet hash
        let hash_mainnet =
            DomainSeparatedConsensusHasher::<TestHashDomain, Blake2b<U32>>::new_with_network(label, Network::MainNet)
                .chain(&input)
                .finalize();

        // Generate a stagenet hash
        let hash_stagenet =
            DomainSeparatedConsensusHasher::<TestHashDomain, Blake2b<U32>>::new_with_network(label, Network::StageNet)
                .chain(&input)
                .finalize();

        // They should be distinct
        assert_ne!(hash_mainnet, hash_stagenet);
    }

    #[test]
    fn it_hashes_using_the_domain_hasher() {
        let network = Network::get_current_or_user_setting_or_default();

        // Script is chosen because the consensus encoding impl for TariScript has 2 writes
        let mut hasher = Blake2b::<U32>::default();
        TestHashDomain::add_domain_separation_tag(&mut hasher, &format!("{}.n{}", "foo", network.as_byte()));

        let expected_hash = hasher.chain_update(b"\xff\x00\x00\x00\x00\x00\x00\x00").finalize();
        let hash = DomainSeparatedConsensusHasher::<TestHashDomain, Blake2b<U32>>::new("foo")
            .chain(&255u64)
            .finalize();

        assert_eq!(hash, expected_hash);
    }

    #[test]
    fn it_adds_to_hash_challenge_in_complete_chunks() {
        let network = Network::get_current_or_user_setting_or_default();

        // Script is chosen because the consensus encoding impl for TariScript has 2 writes
        let test_subject = script!(Nop);
        let mut hasher = Blake2b::<U32>::default();
        TestHashDomain::add_domain_separation_tag(&mut hasher, &format!("{}.n{}", "foo", network.as_byte()));

        let expected_hash = hasher.chain_update(b"\x01\x73").finalize();
        let hash = DomainSeparatedConsensusHasher::<TestHashDomain, Blake2b<U32>>::new("foo")
            .chain(&test_subject)
            .finalize();

        assert_eq!(hash, expected_hash);
    }

    #[test]
    fn default_consensus_hash_is_not_blake_default_hash() {
        let blake_hasher = Blake2b::<U32>::default();
        let blake_hash = blake_hasher.chain_update(b"").finalize();

        let default_consensus_hasher = DomainSeparatedConsensusHasher::<TestHashDomain, Blake2b<U32>>::default();
        let default_consensus_hash = default_consensus_hasher.chain(b"").finalize();

        assert_ne!(blake_hash.as_slice(), default_consensus_hash.as_slice());
    }

    #[test]
    fn it_uses_the_network_environment_variable_if_set() {
        let label = "test";
        let input = [1u8; 32];

        for network in [
            Network::MainNet,
            Network::StageNet,
            Network::NextNet,
            Network::LocalNet,
            Network::Igor,
            Network::Esmeralda,
        ] {
            // Generate a specific network hash
            let hash_specify_network =
                DomainSeparatedConsensusHasher::<TestHashDomain, Blake2b<U32>>::new_with_network(label, network)
                    .chain(&input)
                    .finalize();

            // Generate an inferred network hash
            std::env::set_var("TARI_NETWORK", network.as_key_str());
            let inferred_network_hash = DomainSeparatedConsensusHasher::<TestHashDomain, Blake2b<U32>>::new(label)
                .chain(&input)
                .finalize();
            std::env::remove_var("TARI_NETWORK");

            // They should be equal
            assert_eq!(hash_specify_network, inferred_network_hash);
        }
    }
}
