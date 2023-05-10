// Copyright 2019. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use core::result::Result;

use tari_common_types::types::{PrivateKey, PublicKey};
use tari_comms::types::CommsDHKE;
use tari_crypto::{
    hash::blake2::Blake256,
    hash_domain,
    hashing::{DomainSeparatedHash, DomainSeparatedHasher},
    keys::PublicKey as PKtrait,
};
use tari_utilities::{byte_array::ByteArrayError, ByteArray};

use crate::WalletHasher;

hash_domain!(
    WalletOutputRewindKeysDomain,
    "com.tari.tari_project.base_layer.wallet.output_rewind_keys",
    1
);

hash_domain!(
    WalletOutputEncryptionKeysDomain,
    "com.tari.tari_project.base_layer.wallet.output_encryption_keys",
    1
);

hash_domain!(
    WalletOutputSpendingKeysDomain,
    "com.tari.tari_project.base_layer.wallet.output_spending_keys",
    1
);

type WalletOutputEncryptionKeysDomainHasher = DomainSeparatedHasher<Blake256, WalletOutputEncryptionKeysDomain>;
type WalletOutputSpendingKeysDomainHasher = DomainSeparatedHasher<Blake256, WalletOutputSpendingKeysDomain>;

/// Generate an output encryption key from a Diffie-Hellman shared secret
pub fn shared_secret_to_output_encryption_key(shared_secret: &CommsDHKE) -> Result<PrivateKey, ByteArrayError> {
    PrivateKey::from_bytes(
        WalletOutputEncryptionKeysDomainHasher::new()
            .chain(shared_secret.as_bytes())
            .finalize()
            .as_ref(),
    )
}

/// Generate an output spending key from a Diffie-Hellman shared secret
pub fn shared_secret_to_output_spending_key(shared_secret: &CommsDHKE) -> Result<PrivateKey, ByteArrayError> {
    PrivateKey::from_bytes(
        WalletOutputSpendingKeysDomainHasher::new()
            .chain(shared_secret.as_bytes())
            .finalize()
            .as_ref(),
    )
}

/// Stealth address domain separated hasher using Diffie-Hellman shared secret
pub fn diffie_hellman_stealth_domain_hasher(
    private_key: &PrivateKey,
    public_key: &PublicKey,
) -> DomainSeparatedHash<Blake256> {
    WalletHasher::new_with_label("stealth_address")
        .chain(CommsDHKE::new(private_key, public_key).as_bytes())
        .finalize()
}

/// Stealth payment script spending key
pub fn stealth_address_script_spending_key(
    dh_domain_hasher: &DomainSeparatedHash<Blake256>,
    destination_public_key: &PublicKey,
) -> PublicKey {
    PublicKey::from_secret_key(
        &PrivateKey::from_bytes(dh_domain_hasher.as_ref()).expect("'DomainSeparatedHash<Blake256>' has correct size"),
    ) + destination_public_key
}
