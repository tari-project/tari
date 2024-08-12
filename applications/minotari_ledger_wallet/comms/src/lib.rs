//  Copyright 2024 The Tari Project
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

pub mod accessor_methods;
pub mod error;
pub mod ledger_wallet;

#[cfg(test)]
mod test {
    use borsh::BorshSerialize;
    use minotari_ledger_wallet_common::{hex_to_bytes_serialized, PUSH_PUBKEY_IDENTIFIER};
    use rand::rngs::OsRng;
    use tari_crypto::{
        keys::{PublicKey, SecretKey},
        ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    };
    use tari_script::{script, slice_to_boxed_message};
    use tari_utilities::{hex::Hex, ByteArray};

    const NOP_IDENTIFIER: &str = "0173";
    const PUSH_ONE_IDENTIFIER: &str = "017c";
    const CHECK_SIG_VERIFY_IDENTIFIER: &str = "21ad";

    #[test]
    // This is testing the serialization of the 'PushPubKey' script and the byte representation of the script as needed
    // for native serialization in the tari ledger wallet code. Other script types where the exact hex representation of
    // the script payload could easily be determined are tested in the same way to verify the concept.
    // This test should highlight if any changes are made to the script serialization. Primary script:
    // - 'script!(PushPubKey(Box::new(<PUB_KEY>)))'
    // and additional ones used for testing:
    // - 'script!(Nop)'
    // - 'script!(PushOne)'
    // - `CheckSigVerify(<MESSAGE>))`
    fn test_push_pub_key_serialized_byte_representation() {
        let mut scripts = Vec::new();

        scripts.push((script!(Nop), NOP_IDENTIFIER, "".to_string()));
        scripts.push((script!(PushOne), PUSH_ONE_IDENTIFIER, "".to_string()));

        for pub_key in [
            RistrettoPublicKey::default(),
            RistrettoPublicKey::from_secret_key(&RistrettoSecretKey::random(&mut OsRng)),
        ] {
            scripts.push((
                script!(PushPubKey(Box::new(pub_key.clone()))),
                PUSH_PUBKEY_IDENTIFIER,
                pub_key.to_hex(),
            ));
        }

        let key = RistrettoSecretKey::random(&mut OsRng);
        let msg = slice_to_boxed_message(key.as_bytes());
        scripts.push((script!(CheckSigVerify(msg)), CHECK_SIG_VERIFY_IDENTIFIER, key.to_hex()));

        for (script, hex_identifier, hex_payload) in scripts {
            let mut serialized = Vec::new();
            script.serialize(&mut serialized).unwrap();
            let hex_data = hex_identifier.to_owned() + &hex_payload;
            assert_eq!(hex_data, serialized.to_vec().to_hex());
            assert_eq!(
                hex_to_bytes_serialized(hex_identifier, &hex_payload).unwrap(),
                serialized.as_slice(),
                "Change in script serialization detected: {:?}, expected {}",
                script,
                hex_identifier
            );
        }
    }
}
