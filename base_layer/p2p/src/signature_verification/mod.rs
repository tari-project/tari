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

mod error;
mod verifier;

use std::io;

pub use error::SignatureVerificationError;
use futures;
use log::{debug, warn};
use pgp::{Deserializable, SignedPublicKey, StandaloneSignature};
use reqwest::IntoUrl;
pub use verifier::SignedMessageVerifier;

const LOG_TARGET: &str = "p2p::signature_verification";

// Include GPG keys of authorized maintainers
const MAINTAINERS: &[&str] = &[
    include_str!("gpg_keys/swvheerden.asc"),
    include_str!("gpg_keys/seed_peers_http.asc"),
];

/// Returns an iterator over all configured maintainer public keys
pub fn maintainers() -> impl Iterator<Item = SignedPublicKey> {
    MAINTAINERS.iter().map(|s| {
        let (pk, _) = SignedPublicKey::from_string(s).expect("Malformed maintainer PGP signature");
        pk
    })
}

// Legacy function names kept for backward compatibility with auto_update module
/// Download a text file from the given URL (legacy name for compatibility)
pub async fn download_hashes_file<T: IntoUrl>(url: T) -> Result<String, SignatureVerificationError> {
    download_file(url).await
}

/// Download a PGP signature file from the given URL (legacy name for compatibility)
pub async fn download_hashes_sig_file<T: IntoUrl>(url: T) -> Result<StandaloneSignature, SignatureVerificationError> {
    download_signature_file(url).await
}

/// Perform an HTTP GET request and return the response
async fn http_download<T: IntoUrl>(url: T) -> Result<reqwest::Response, SignatureVerificationError> {
    let resp = reqwest::get(url).await?.error_for_status()?;
    Ok(resp)
}

/// Verify a signed hash file and extract the hash and filename for a target hash
///
/// This function:
/// 1. Verifies the signature of the hashes file using maintainer keys
/// 2. Parses the hashes file to find a matching hash
/// 3. Returns the hash and associated filename if found
pub async fn verify_signed_hash_file(
    hashes_url: &str,
    signature_url: &str,
    target_hash: &[u8],
) -> Result<(Vec<u8>, String), SignatureVerificationError> {
    let (hashes, sig) = futures::join!(download_file(hashes_url), download_signature_file(signature_url));
    let hashes = hashes?;
    let sig = sig?;
    let verifier = SignedMessageVerifier::new(maintainers().collect());
    let result = verifier.verify_signed_hashes(&sig, &hashes, target_hash);

    match &result {
        Ok((_hash, filename)) => {
            debug!(target: LOG_TARGET, "Signature verification successful for file: {}", filename);
        },
        Err(e) => {
            warn!(target: LOG_TARGET, "Signature verification failed: {}", e);
        },
    }

    result
}

/// Download and verify a generic file with its PGP signature
///
/// This function:
/// 1. Downloads the file and its signature
/// 2. Verifies the signature using maintainer keys
/// 3. Returns the file content if verification succeeds
///
/// # Example
/// ```no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let content = verify_signed_file(
///     "https://example.com/seednodes.json",
///     "https://example.com/seednodes.json.asc",
/// )
/// .await?;
/// # Ok(())
/// # }
/// ```
pub async fn verify_signed_file(file_url: &str, signature_url: &str) -> Result<String, SignatureVerificationError> {
    let (content, sig) = futures::join!(download_file(file_url), download_signature_file(signature_url));
    let content = content?;
    let sig = sig?;
    let verifier = SignedMessageVerifier::new(maintainers().collect());

    match verifier.verify_file_signature(&sig, &content) {
        Ok(_) => {
            debug!(target: LOG_TARGET, "Signature verification successful for file: {}", file_url);
            Ok(content)
        },
        Err(e) => {
            warn!(target: LOG_TARGET, "Signature verification failed for {}: {}", file_url, e);
            Err(e)
        },
    }
}

/// Download a text file from the given URL
pub async fn download_file<T: IntoUrl>(url: T) -> Result<String, SignatureVerificationError> {
    let resp = http_download(url).await?;
    let txt = resp.text().await?;
    Ok(txt)
}

/// Download a PGP signature file from the given URL
pub async fn download_signature_file<T: IntoUrl>(url: T) -> Result<StandaloneSignature, SignatureVerificationError> {
    let resp = http_download(url).await?;
    let sig_bytes = resp.bytes().await?;
    let cursor = io::Cursor::new(&sig_bytes);
    match StandaloneSignature::from_bytes(cursor) {
        Ok(sig) => {
            debug!(target: LOG_TARGET, "download_signature_file: Successfully parsed PGP signature");
            Ok(sig)
        },
        Err(e) => {
            warn!(target: LOG_TARGET, "download_signature_file: Failed to parse PGP signature: {}", e);
            Err(SignatureVerificationError::SignatureError(e))
        },
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn all_maintainers_well_formed() {
        assert_eq!(maintainers().count(), MAINTAINERS.len());
    }
}
