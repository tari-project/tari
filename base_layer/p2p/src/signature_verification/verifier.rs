//  Copyright 2021, The Tari Project
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

use log::{debug, info, warn};
use pgp::{types::PublicKeyTrait, SignedPublicKey, StandaloneSignature};
use tari_utilities::hex::from_hex;

use crate::signature_verification::error::SignatureVerificationError;

const LOG_TARGET: &str = "p2p::signature_verification::verifier";

pub struct SignedMessageVerifier {
    maintainers: Vec<SignedPublicKey>,
}

impl SignedMessageVerifier {
    pub fn new(maintainers: Vec<SignedPublicKey>) -> Self {
        Self { maintainers }
    }

    /// Verify a standalone signature against a message using the configured maintainers' public keys
    pub fn verify_signature(&self, signature: &StandaloneSignature, message: &str) -> Option<&SignedPublicKey> {
        info!(target: LOG_TARGET, "Starting signature verification, maintainers count: {}", self.maintainers.len());
        self.maintainers.iter().find(|pk| {
            debug!(target: LOG_TARGET, "Verifying signature with public key fingerprint: {:?}", pk.fingerprint());
            let result = signature.verify(pk, message.as_bytes()).is_ok();
            if result {
                info!(target: LOG_TARGET, "Signature verified successfully with key: {:?}", pk.fingerprint());
            } else {
                debug!(target: LOG_TARGET, "Signature verification failed with key: {:?}", pk.fingerprint());
            }
            result
        })
    }

    /// Verify a file's content against its signature
    /// Returns Ok with the signing key if verification succeeds
    pub fn verify_file_signature(
        &self,
        signature: &StandaloneSignature,
        file_content: &str,
    ) -> Result<&SignedPublicKey, SignatureVerificationError> {
        debug!(target: LOG_TARGET, "Verifying file signature, content length: {} bytes", file_content.len());
        self.verify_signature(signature, file_content).ok_or_else(|| {
            warn!(target: LOG_TARGET, "File signature verification failed - no matching maintainer key found");
            SignatureVerificationError::VerificationFailed
        })
    }

    /// Verify a signed hash file and return the matching hash and filename for a given target hash
    /// This function expects the file to contain lines in the format: "HASH filename"
    pub fn verify_signed_hashes(
        &self,
        signature: &StandaloneSignature,
        hashes: &str,
        target_hash: &[u8],
    ) -> Result<(Vec<u8>, String), SignatureVerificationError> {
        info!(target: LOG_TARGET, "Verifying signed hashes file, looking for target hash");
        debug!(target: LOG_TARGET, "Target hash: {:?}", target_hash);
        debug!(target: LOG_TARGET, "Hashes file content length: {} bytes", hashes.len());

        self.verify_signature(signature, hashes).ok_or_else(|| {
            warn!(target: LOG_TARGET, "Hash file signature verification failed - no matching maintainer key found");
            SignatureVerificationError::VerificationFailed
        })?;

        let parsed_hashes: Vec<(Vec<u8>, String)> = hashes
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(2, ' ');
                let hash = parts.next().map(|s| s.trim()).map(from_hex)?.ok()?;
                let filename = parts.next()?;
                Some((hash, filename.trim().to_string()))
            })
            .collect();

        debug!(target: LOG_TARGET, "Parsed {} hash entries from file", parsed_hashes.len());

        parsed_hashes
            .into_iter()
            .find(|(hash, filename)| {
                let matches = *hash == target_hash;
                if matches {
                    info!(target: LOG_TARGET, "Found matching hash for file: {}", filename);
                }
                matches
            })
            .ok_or_else(|| {
                warn!(target: LOG_TARGET, "No matching hash found in the signed hashes file");
                SignatureVerificationError::InvalidHashFormat
            })
    }
}

#[cfg(test)]
mod test {
    use pgp::{Deserializable, StandaloneSignature};

    use super::*;

    const PUBLIC_KEY: &str = r#"-----BEGIN PGP PUBLIC KEY BLOCK-----

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
tQrYpH9CNXgX9dC9
=7S7i
-----END PGP PUBLIC KEY BLOCK-----"#;

    const VALID_SIGNATURE: &str = r#"-----BEGIN PGP SIGNATURE-----
iQIzBAEBCAAdFiEEM3uR78XxAn2K7fY9GIWxSVBMCmQFAmDYhicACgkQGIWxSVBM
CmRVuBAAkdFqPmJAHAu03CBTC6RjHlN+dxVgZ2UjfHzY80pVbiKTLeRoz7bMdVyZ
nVnf7QEcBMrK21LA/sBp/QmSGhym3AN3QjrFvOLJMWcfKj0gMdFV+z1TxNpZoKhD
EZheXNf+/Sy8sTdBJQhbGnD/Rs8+7IZbxKCCD43w26Z/Re+BOOeSFcARu4pka1e2
EUJRUbV6UAB21TO/A+fAl4FuOgyWrNnrF/4Fy7Fk0jLaqf5kpYpvgC6SAKlkOhBz
x0zleJAxzvIBIomGJsS2FrV17mEATJiflgMslCeZAzoggnmlbv9tDOIXnYKA46+T
O7krar5DnHHLrLOVoAOQrfLVHVbp7Z4IdBegzer3Q7FE6Sgt+hscrw/nq37OOVjL
cj6S7+IsM4Vlsrwvu5E3VHt5DBvoFszxPq4eP6MRCoO6QvuYhB5L1sT1bvdhs+qM
DMe11D0lQakx1240GJK0J0fFEvlPPG+F+Q6bHXSGDu7D0bUNk2siSKy+IdpUrvwa
HFwxr8+CkSk5pNVZdusBZabXDnLxJz9k+rEvrB1F/9ZbLP3PzV9nyWcu3htxjcPo
Ckvq+QUz80XM69HPwpAgFW6QORZdxv4ED/ek4gth3fqmu/bkQ4/vYKozMtr6Rx7D
l9smp8LtJcXkw4cNgE4MB9VKdx+NhdbvWemt7ccldeL22hmyS24=
=vcW8
-----END PGP SIGNATURE-----"#;

    const MESSAGE: &str = "Philip R. Zimmermann";

    #[test]
    fn it_verifies_signed_message() {
        let (sig, _) = StandaloneSignature::from_string(VALID_SIGNATURE.trim()).unwrap();
        let (key, _) = SignedPublicKey::from_string(PUBLIC_KEY).unwrap();
        let verifier = SignedMessageVerifier::new(vec![key]);
        let signer = verifier.verify_signature(&sig, MESSAGE).unwrap();

        let (maintainer, _) = SignedPublicKey::from_string(PUBLIC_KEY).unwrap();
        assert_eq!(*signer, maintainer);
    }

    #[test]
    fn it_does_not_validate_with_tampered_message() {
        let (sig, _) = StandaloneSignature::from_string(VALID_SIGNATURE.trim()).unwrap();
        let (key, _) = SignedPublicKey::from_string(PUBLIC_KEY).unwrap();
        let verifier = SignedMessageVerifier::new(vec![key]);
        assert!(verifier.verify_signature(&sig, "Zilip R. Phimmermann").is_none());
    }

    #[test]
    fn it_verifies_file_signature() {
        let (sig, _) = StandaloneSignature::from_string(VALID_SIGNATURE.trim()).unwrap();
        let (key, _) = SignedPublicKey::from_string(PUBLIC_KEY).unwrap();
        let verifier = SignedMessageVerifier::new(vec![key]);

        // Test valid file signature
        let result = verifier.verify_file_signature(&sig, MESSAGE);
        assert!(result.is_ok());

        // Test invalid file signature
        let result = verifier.verify_file_signature(&sig, "Wrong content");
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_json_file() {
        // Test that we can verify a JSON file content
        let json_content = r#"{"peer_seeds": ["node1", "node2"]}"#;
        let (key, _) = SignedPublicKey::from_string(PUBLIC_KEY).unwrap();
        let verifier = SignedMessageVerifier::new(vec![key]);

        // This would work with a real signature of the JSON content
        // For now, we just test that the function accepts JSON strings
        let (sig, _) = StandaloneSignature::from_string(VALID_SIGNATURE.trim()).unwrap();
        let result = verifier.verify_file_signature(&sig, json_content);
        // This will fail because the signature is for a different message
        assert!(result.is_err());
    }

    #[test]
    fn test_seed_peers_http_key() {
        // Test that the seed_peers_http key can be parsed and used for verification
        const SEED_PEERS_KEY: &str = r#"-----BEGIN PGP PUBLIC KEY BLOCK-----

mDMEaLl/nhYJKwYBBAHaRw8BAQdAocXM74pI54REY9Y0fESxir/iq8We9wp6JHFP
z8vcdm20Sk1hY2llaiAoVGVzdCBmb3Igc2VlZCBwZWVycyBIVFRQIGRvd25sb2Fk
KSA8bWFjaWVqLmtvenVzemVrQHNwYWNlaW5jaS5jb20+iJMEExYKADsWIQTaz8Pe
9KT58ia7xIFrHRtevPqxvwUCaLl/ngIbAwULCQgHAgIiAgYVCgkICwIEFgIDAQIe
BwIXgAAKCRBrHRtevPqxv5cOAQDR1jrEiLxlsEFLsI6DLd0I7SRQDw+tziT/02ed
7E8wMQD/ZzdO7ZO8oLfneJrrwoWiGk241+yq7ym5uEcBuhnKyQ8=
=rjiS
-----END PGP PUBLIC KEY BLOCK-----"#;

        // Parse the key
        let (key, _) = SignedPublicKey::from_string(SEED_PEERS_KEY).unwrap();

        // Create verifier with the seed peers key
        let verifier = SignedMessageVerifier::new(vec![key]);

        // The verifier should be created successfully
        assert_eq!(verifier.maintainers.len(), 1);
    }

    #[test]
    fn test_multiple_maintainer_keys() {
        // Test with both the original test key and the seed peers key
        const SEED_PEERS_KEY: &str = r#"-----BEGIN PGP PUBLIC KEY BLOCK-----

mDMEaLl/nhYJKwYBBAHaRw8BAQdAocXM74pI54REY9Y0fESxir/iq8We9wp6JHFP
z8vcdm20Sk1hY2llaiAoVGVzdCBmb3Igc2VlZCBwZWVycyBIVFRQIGRvd25sb2Fk
KSA8bWFjaWVqLmtvenVzemVrQHNwYWNlaW5jaC5jb20+iJMEExYKADsWIQTaz8Pe
9KT58ia7xIFrHRtevPqxvwUCaLl/ngIbAwULCQgHAgIiAgYVCgkICwIEFgIDAQIe
BwIXgAAKCRBrHRtevPqxv5cOAQDR1jrEiLxlsEFLsI6DLd0I7SRQDw+tziT/02ed
7E8wMQD/ZzdO7ZO8oLfneJrrwoWiGk241+yq7ym5uEcBuhnKyQ8=
=rjiS
-----END PGP PUBLIC KEY BLOCK-----"#;

        let (key1, _) = SignedPublicKey::from_string(PUBLIC_KEY).unwrap();
        let (key2, _) = SignedPublicKey::from_string(SEED_PEERS_KEY).unwrap();

        let verifier = SignedMessageVerifier::new(vec![key1, key2]);

        // Should have both keys
        assert_eq!(verifier.maintainers.len(), 2);
    }
}
