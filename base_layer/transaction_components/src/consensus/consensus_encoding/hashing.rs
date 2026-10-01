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

    /// Create a new hasher with the specified label and network byte.
    /// NOTE: the network is generic (anything that converts to a byte) to allow for use in L2 without requiring
    /// tari_common
    pub fn new_with_network<N: Into<u8>>(label: &'static str, network: N) -> Self {
        let hasher = DomainSeparatedBorshHasher::<M, D>::new_with_label(&format!("{}.n{}", label, network.into()));
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
    use tari_crypto::hash_domain;
    use tari_script::script;

    use super::*;

    hash_domain!(TestHashDomain, "com.tari.test.test_hash", 0);

    /// Known-answer vectors for the consensus hasher on every network. The network byte is folded into the label as
    /// `{label}.n{network byte in decimal}`, so the tag is `u64_le(len) || "{domain}.v{version}.{label}.n{byte}"`,
    /// followed by the borsh encoding of the input (`[u8; 32]`, raw bytes). Label `test_vector`, input `00 01 .. 1f`.
    #[test]
    fn known_answer_vectors_per_network() {
        use tari_hashing::{BlocksHashDomain, TransactionHashDomain};

        // Exhaustive so that adding a network forces a new vector.
        fn expected(network: Network) -> (&'static str, &'static str) {
            match network {
                Network::MainNet => (
                    "feae1c469e93f306372f0a7ce64e074b5135abf6223a9bbe5637690159155eda",
                    "daea038d26a7d8123210e8237e9b9bb4215f9bbcd3b64a3d22fb9a997a88202e",
                ),
                Network::StageNet => (
                    "60473350c5314495006ac489597613d530b3da7ec4030f049984349ac42edc24",
                    "46e63e6f0113f8e3bbcade8a0c65bcb379279d04e06fae1686170c7ca171391b",
                ),
                Network::NextNet => (
                    "980cec3254a49287e65d2eb97f7e844f8293d5b68792e9bf92d4923dbc6c8706",
                    "11abf431922746eafa93d92420dd0e0261829859a3a302dc72b67bb0a78e6940",
                ),
                Network::LocalNet => (
                    "26ef18c7a8834b053f4a06eb29ca7b655a7b813e1c509b342d55002f326a29c3",
                    "1b6c832ede284d5035cd1e9fc0b4eed2dd8035fd9cc974ff8ce064c5837bf26d",
                ),
                Network::Igor => (
                    "fbaeb8c9bed844e38e5c4be7930600461d330e58536425ff651227a4298b1469",
                    "71fc587eae55cb35c4b8d0808204bc7aedb811b4c0a1932dbc1d20a54570a2bf",
                ),
                Network::Esmeralda => (
                    "a44652234bde3b3337e0ccefb1f7457d75c929e853d76789e0ea21fc1ac3be5e",
                    "0385d48b58895e74257b7ce6dab8cb85cd468db50ae475e291d5ccc6d18a772a",
                ),
            }
        }

        let mut input = [0u8; 32];
        for (i, b) in input.iter_mut().enumerate() {
            *b = u8::try_from(i).unwrap();
        }
        for network in [
            Network::MainNet,
            Network::StageNet,
            Network::NextNet,
            Network::LocalNet,
            Network::Igor,
            Network::Esmeralda,
        ] {
            let (expected_tx, expected_blocks) = expected(network);
            let tx = DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new_with_network(
                "test_vector",
                network,
            )
            .chain(&input)
            .finalize();
            let blocks = DomainSeparatedConsensusHasher::<BlocksHashDomain, Blake2b<U32>>::new_with_network(
                "test_vector",
                network,
            )
            .chain(&input)
            .finalize();
            assert_eq!(
                tari_utilities::hex::to_hex(&tx),
                expected_tx,
                "{network} TransactionHashDomain"
            );
            assert_eq!(
                tari_utilities::hex::to_hex(&blocks),
                expected_blocks,
                "{network} BlocksHashDomain"
            );
        }
    }

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
        TestHashDomain::add_domain_separation_tag(&mut hasher, format!("{}.n{}", "foo", network.as_byte()));

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
        let test_subject = script!(Nop).unwrap();
        let mut hasher = Blake2b::<U32>::default();
        TestHashDomain::add_domain_separation_tag(&mut hasher, format!("{}.n{}", "foo", network.as_byte()));

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
        // Targeted network compilations will override inferred network hashes; this only has effect if
        // `Network::set_current(<NETWORK>)` has not been called. The test may also not run if
        // `std::env::var("TARI_NETWORK")` has been set by some other test.
        if Network::is_set() {
            println!(
                "\nNote!! Static network constant is set, cannot run \
                 `it_uses_the_network_environment_variable_if_set`\n"
            );
            return;
        }
        if std::env::var("TARI_NETWORK").is_ok() {
            println!(
                "\nNote!! env_var 'TARI_NETWORK' in use, cannot run \
                 `it_uses_the_network_environment_variable_if_set`\n"
            );
            return;
        }

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
            println!("Testing network: {network:?}");
            // Generate a specific network hash
            let hash_specify_network =
                DomainSeparatedConsensusHasher::<TestHashDomain, Blake2b<U32>>::new_with_network(label, network)
                    .chain(&input)
                    .finalize();

            // Generate an inferred network hash
            // SAFETY: This test is not run in parallel, and the environment variable is removed after use.
            unsafe { std::env::set_var("TARI_NETWORK", network.as_key_str()) };
            println!(
                "TARI_NETWORK:    {:?}",
                std::env::var("TARI_NETWORK").unwrap_or_default()
            );
            println!(
                "Network:         {:?}\n",
                Network::get_current_or_user_setting_or_default()
            );
            let inferred_network_hash = DomainSeparatedConsensusHasher::<TestHashDomain, Blake2b<U32>>::new(label)
                .chain(&input)
                .finalize();
            // SAFETY: This test is not run in parallel, and the environment variable was set earlier in this test.
            unsafe { std::env::remove_var("TARI_NETWORK") };

            // They should be equal
            assert_eq!(hash_specify_network, inferred_network_hash);
        }
    }
}
