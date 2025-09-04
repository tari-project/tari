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

pub use error::SignatureVerificationError;
use futures;
use pgp::Deserializable;
use reqwest::IntoUrl;
use std::io;
pub use verifier::SignedMessageVerifier;

const LOG_TARGET: &str = "p2p::signature_verification";

// Include GPG keys of authorized maintainers
const MAINTAINERS: &[&str] = &[include_str!("gpg_keys/swvheerden.asc")];

/// Returns an iterator over all configured maintainer public keys
pub fn maintainers() -> impl Iterator<Item = pgp::SignedPublicKey> {
    MAINTAINERS.iter().map(|s| {
        let (pk, _) = pgp::SignedPublicKey::from_string(s).expect("Malformed maintainer PGP signature");
        pk
    })
}

/// Download a text file from the given URL
pub async fn download_hashes_file<T: IntoUrl>(url: T) -> Result<String, SignatureVerificationError> {
    let resp = http_download(url).await?;
    let txt = resp.text().await?;
    Ok(txt)
}

/// Download a PGP signature file from the given URL
pub async fn download_hashes_sig_file<T: IntoUrl>(
    url: T,
) -> Result<pgp::StandaloneSignature, SignatureVerificationError> {
    let resp = http_download(url).await?;
    let sig_bytes = resp.bytes().await?;
    let cursor = io::Cursor::new(&sig_bytes);
    let sig = pgp::StandaloneSignature::from_bytes(cursor).map_err(SignatureVerificationError::SignatureError)?;
    Ok(sig)
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
    // Download both files in parallel
    let (hashes, sig) = futures::join!(
        download_hashes_file(hashes_url),
        download_hashes_sig_file(signature_url)
    );

    let hashes = hashes?;
    let sig = sig?;

    // Verify the signature using maintainer keys
    let verifier = SignedMessageVerifier::new(maintainers().collect());
    verifier.verify_signed_hashes(&sig, &hashes, target_hash)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn all_maintainers_well_formed() {
        assert_eq!(maintainers().count(), MAINTAINERS.len());
    }
}
