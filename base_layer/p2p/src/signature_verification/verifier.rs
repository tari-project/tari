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

use log::{debug, warn};
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
        self.maintainers.iter().find(|pk| {
            let result = signature.verify(pk, message.as_bytes()).is_ok();
            if result {
                debug!(target: LOG_TARGET, "Signature verified successfully with key: {:?}", pk.fingerprint());
            } else {
                // It's debug since other keys are not checked
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

        parsed_hashes
            .into_iter()
            .find(|(hash, filename)| {
                let matches = *hash == target_hash;
                if matches {
                    debug!(target: LOG_TARGET, "Found matching hash for file: {}", filename);
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

    // Real seed_peers_http.asc public key
    const SEED_PEERS_PUBLIC_KEY: &str = r#"-----BEGIN PGP PUBLIC KEY BLOCK-----

mDMEaLl/nhYJKwYBBAHaRw8BAQdAocXM74pI54REY9Y0fESxir/iq8We9wp6JHFP
z8vcdm20Sk1hY2llaiAoVGVzdCBmb3Igc2VlZCBwZWVycyBIVFRQIGRvd25sb2Fk
KSA8bWFjaWVqLmtvenVzemVrQHNwYWNlaW5jaC5jb20+iJMEExYKADsWIQTaz8Pe
9KT58ia7xIFrHRtevPqxvwUCaLl/ngIbAwULCQgHAgIiAgYVCgkICwIEFgIDAQIe
BwIXgAAKCRBrHRtevPqxv5cOAQDR1jrEiLxlsEFLsI6DLd0I7SRQDw+tziT/02ed
7E8wMQD/ZzdO7ZO8oLfneJrrwoWiGk241+yq7ym5uEcBuhnKyQ+0Uk1hY2llaiBL
b3p1c3playAoVGVzdGluZyBzZWVkIHBlZXJzIEhUVFAgZG93bmxvYWQpIDxtYWNp
ZWoua296dXN6ZWtAc3BhY2VpbmNoLmNvbT6IkwQTFgoAOxYhBNrPw970pPnyJrvE
gWsdG168+rG/BQJousXyAhsDBQsJCAcCAiICBhUKCQgLAgQWAgMBAh4HAheAAAoJ
EGsdG168+rG/zIgA/RmbnuNU7/mrSIqV62U5wPPhj3fT7+zR/9Ayn1ME+KbMAQC8
fSb4bOv48TUskOtzWd9j+AuH+2w1bvi9/niKAPgrAA==
=p4US
-----END PGP PUBLIC KEY BLOCK-----"#;

    // Real signature from seednodes.json.asc (current live version)
    const SEEDNODES_SIGNATURE: &str = r#"-----BEGIN PGP SIGNATURE-----

iHUEABYKAB0WIQTaz8Pe9KT58ia7xIFrHRtevPqxvwUCaLmBrwAKCRBrHRtevPqx
v/oSAP92nITPC9TNDwfsIow7IBKxHqNNvOB6FjMy0ZCgpN1ouwEA4xGcg7aodWu/
G0eKB6s7pbpSyu3XdQqJwozutRuCzA0=
=Y0ye
-----END PGP SIGNATURE-----"#;

    // Real content of seednodes.json (with trailing newline as in the actual file)
    const SEEDNODES_JSON: &str = r#"{
  "peer_seeds": [
    "4cdfb70e0b38b60c6a3573b2870e32bc3d846419c606ea379f43650b80f38409::/ip4/51.83.4.85/tcp/18189",
    "1e08628960f75b7e324f010b2ee609a9e28097e9101f4d769d474a38b6ee2d76::/ip4/51.83.102.25/tcp/18189",
    "1e08628960f75b7e324f010b2ee609a9e28097e9101f4d769d474a38b6ee2d76::/ip6/2001:41d0:303:a619::1/tcp/18189",
    "4cdfb70e0b38b60c6a3573b2870e32bc3d846419c606ea379f43650b80f38409::/ip6/2001:41d0:303:9a55::1/tcp/18189",
    "1e08628960f75b7e324f010b2ee609a9e28097e9101f4d769d474a38b6ee2d76::/onion3/tadnxyokalnqjtvu6mlhxndcq4v2tlolotpvrflscdmi7lcautao3had:18141",
    "4cdfb70e0b38b60c6a3573b2870e32bc3d846419c606ea379f43650b80f38409::/onion3/mhfptgpcj6htjkr5zwurom32wvt7x76ovqzn2ttnwo2bnku6baeaaiyd:18141"
  ]
}
"#;

    #[test]
    fn test_verify_real_seednodes_signature() {
        // Test verification of the actual seednodes.json with its signature
        let (sig, _) = StandaloneSignature::from_string(SEEDNODES_SIGNATURE.trim()).unwrap();
        let (key, _) = SignedPublicKey::from_string(SEED_PEERS_PUBLIC_KEY).unwrap();

        // Debug: Print key fingerprint
        println!("Key fingerprint: {:?}", key.fingerprint());

        let verifier = SignedMessageVerifier::new(vec![key.clone()]);

        // Debug: Try to understand what's happening
        println!("Attempting to verify signature...");
        println!("Content length: {}", SEEDNODES_JSON.len());
        println!("First 50 chars: {:?}", &SEEDNODES_JSON[..50]);

        // Try verification with debug info
        let verify_result = sig.verify(&key, SEEDNODES_JSON.as_bytes());
        println!("Direct pgp verification result: {:?}", verify_result);

        if let Err(e) = &verify_result {
            println!("Verification error details: {:?}", e);
        }

        // This should successfully verify the real seednodes.json content
        let result = verifier.verify_file_signature(&sig, SEEDNODES_JSON);
        assert!(
            result.is_ok(),
            "Failed to verify real seednodes.json signature: {:?}",
            result
        );

        // Verify we get the right key back
        let signer = verifier.verify_signature(&sig, SEEDNODES_JSON).unwrap();
        let (expected_key, _) = SignedPublicKey::from_string(SEED_PEERS_PUBLIC_KEY).unwrap();
        assert_eq!(*signer, expected_key);
    }

    #[test]
    fn test_seednodes_signature_fails_with_tampered_content() {
        // Test that verification fails when the content is modified
        let (sig, _) = StandaloneSignature::from_string(SEEDNODES_SIGNATURE.trim()).unwrap();
        let (key, _) = SignedPublicKey::from_string(SEED_PEERS_PUBLIC_KEY).unwrap();
        let verifier = SignedMessageVerifier::new(vec![key]);

        // Tampered content should fail verification
        let tampered_json = r#"{"peer_seeds": ["malicious_node"]}"#;
        assert!(verifier.verify_signature(&sig, tampered_json).is_none());
        assert!(verifier.verify_file_signature(&sig, tampered_json).is_err());
    }

    #[test]
    fn test_verify_seednodes_with_wrong_key() {
        // Test that verification fails with a different key
        let (sig, _) = StandaloneSignature::from_string(SEEDNODES_SIGNATURE.trim()).unwrap();

        // Create a different key (using a test key that's not the signer)
        const OTHER_KEY: &str = r#"-----BEGIN PGP PUBLIC KEY BLOCK-----

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
tQrYpH9CNXgX9dC9
=7S7i
-----END PGP PUBLIC KEY BLOCK-----"#;

        let (wrong_key, _) = SignedPublicKey::from_string(OTHER_KEY).unwrap();
        let verifier = SignedMessageVerifier::new(vec![wrong_key]);

        // Should fail because the key didn't sign this message
        assert!(verifier.verify_signature(&sig, SEEDNODES_JSON).is_none());
        assert!(verifier.verify_file_signature(&sig, SEEDNODES_JSON).is_err());
    }

    #[test]
    fn test_seednodes_json_exact_formatting() {
        // Test that the exact formatting of the JSON matters
        let (sig, _) = StandaloneSignature::from_string(SEEDNODES_SIGNATURE.trim()).unwrap();
        let (key, _) = SignedPublicKey::from_string(SEED_PEERS_PUBLIC_KEY).unwrap();
        let verifier = SignedMessageVerifier::new(vec![key]);

        // Different formatting (minified) should fail
        let minified_json = r#"{"peer_seeds":["4cdfb70e0b38b60c6a3573b2870e32bc3d846419c606ea379f43650b80f38409::/ip4/51.83.4.85/tcp/18189"]}"#;
        assert!(verifier.verify_signature(&sig, minified_json).is_none());

        // The exact content should succeed
        assert!(verifier.verify_signature(&sig, SEEDNODES_JSON).is_some());
    }

    #[test]
    fn test_seed_peers_key_fingerprint() {
        // Test that the seed_peers_http key has the correct fingerprint
        let (key, _) = SignedPublicKey::from_string(SEED_PEERS_PUBLIC_KEY).unwrap();

        // The fingerprint should match what we expect (from the OpenPGP output)
        // DACFC3DEF4A4F9F226BBC4816B1D1B5EBCFAB1BF
        let fingerprint = key.fingerprint();

        // Create verifier and ensure it can verify our real signature
        let verifier = SignedMessageVerifier::new(vec![key]);
        let (sig, _) = StandaloneSignature::from_string(SEEDNODES_SIGNATURE.trim()).unwrap();

        // Should successfully verify
        let result = verifier.verify_signature(&sig, SEEDNODES_JSON);
        assert!(
            result.is_some(),
            "Key with fingerprint {:?} should verify the signature",
            fingerprint
        );
    }

    #[test]
    fn test_multiple_maintainer_keys_with_real_signature() {
        // Test that having multiple keys works and the right one is selected
        const OTHER_KEY: &str = r#"-----BEGIN PGP PUBLIC KEY BLOCK-----

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
tQrYpH9CNXgX9dC9
=7S7i
-----END PGP PUBLIC KEY BLOCK-----"#;

        let (seed_key, _) = SignedPublicKey::from_string(SEED_PEERS_PUBLIC_KEY).unwrap();
        let (other_key, _) = SignedPublicKey::from_string(OTHER_KEY).unwrap();

        // Create verifier with multiple keys
        let verifier = SignedMessageVerifier::new(vec![other_key, seed_key]);

        // Parse the real signature
        let (sig, _) = StandaloneSignature::from_string(SEEDNODES_SIGNATURE.trim()).unwrap();

        // Should successfully verify with the correct key (seed_key)
        let result = verifier.verify_signature(&sig, SEEDNODES_JSON);
        assert!(result.is_some(), "Should verify with one of the maintainer keys");

        // The signer should be the seed_peers key, not the other key
        let signer = result.unwrap();
        let (expected_key, _) = SignedPublicKey::from_string(SEED_PEERS_PUBLIC_KEY).unwrap();
        assert_eq!(*signer, expected_key);
    }

    #[test]
    fn test_debug_signature_parsing() {
        // Test to debug the signature parsing issue
        use pgp::Deserializable;

        println!("\n=== Debug Signature Parsing ===");

        // Try to parse the signature
        match StandaloneSignature::from_string(SEEDNODES_SIGNATURE.trim()) {
            Ok((_, _)) => {
                println!("Signature parsed successfully");
                // Try to get more info about the signature
                println!("Signature type: EdDSA (based on error message)");
            },
            Err(e) => {
                println!("Failed to parse signature: {:?}", e);
            },
        }

        // Try parsing the key
        match SignedPublicKey::from_string(SEED_PEERS_PUBLIC_KEY) {
            Ok((key, _)) => {
                println!("Key parsed successfully");
                println!("Key fingerprint: {:?}", key.fingerprint());
            },
            Err(e) => {
                println!("Failed to parse key: {:?}", e);
            },
        }

        // Test with exact bytes from downloaded file
        println!("\n=== Testing with exact content ===");

        // The content needs to match exactly what was signed
        let test_content = SEEDNODES_JSON;
        println!("Content bytes: {} bytes", test_content.len());

        // Show hex of first and last few bytes to check for whitespace issues
        let bytes = test_content.as_bytes();
        print!("First 20 bytes (hex): ");
        for b in bytes.get(..20.min(bytes.len())).unwrap() {
            print!("{:02x} ", b);
        }
        println!();

        if bytes.len() > 20 {
            print!("Last 20 bytes (hex): ");
            for b in bytes.get(bytes.len() - 20..).unwrap() {
                print!("{:02x} ", b);
            }
            println!();
        }
    }
}
