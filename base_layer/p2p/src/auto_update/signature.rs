//  Copyright 2021, The Taiji Project
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

use tari_utilities::hex::from_hex;

use crate::auto_update::dns::UpdateSpec;

pub struct SignedMessageVerifier {
    maintainers: Vec<pgp::SignedPublicKey>,
}

impl SignedMessageVerifier {
    pub fn new(maintainers: Vec<pgp::SignedPublicKey>) -> Self {
        Self { maintainers }
    }

    pub fn verify_signed_update(
        &self,
        signature: &pgp::StandaloneSignature,
        hashes: &str,
        update: &UpdateSpec,
    ) -> Option<(Vec<u8>, String)> {
        self.verify_signature(signature, hashes)?;

        hashes
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(2, ' ');
                let hash = parts.next().map(|s| s.trim()).map(from_hex)?.ok()?;
                let filename = parts.next()?;
                Some((hash, filename.trim().to_string()))
            })
            .find(|(hash, _)| update.hash == *hash)
    }

    fn verify_signature(&self, signature: &pgp::StandaloneSignature, message: &str) -> Option<&pgp::SignedPublicKey> {
        self.maintainers
            .iter()
            .find(|pk| signature.verify(pk, message.as_bytes()).is_ok())
    }
}

#[cfg(test)]
mod test {
    use pgp::Deserializable;

    use super::*;
    use crate::auto_update::{maintainers, MAINTAINERS};

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
        let (sig, _) = pgp::StandaloneSignature::from_string(VALID_SIGNATURE.trim()).unwrap();
        let verifier = SignedMessageVerifier::new(maintainers().collect());
        let signer = verifier.verify_signature(&sig, MESSAGE).unwrap();

        let (maintainer, _) = pgp::SignedPublicKey::from_string(MAINTAINERS[3]).unwrap();
        assert_eq!(*signer, maintainer);
    }

    #[test]
    fn it_does_not_validate_with_tampered_message() {
        let (sig, _) = pgp::StandaloneSignature::from_string(VALID_SIGNATURE.trim()).unwrap();
        let verifier = SignedMessageVerifier::new(maintainers().collect());
        assert!(verifier.verify_signature(&sig, "Zilip R. Phimmermann").is_none());
    }
}
