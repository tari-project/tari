// Copyright 2020, The Tari Project
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

use std::mem::size_of;

use chacha20::{
    cipher::{NewCipher, StreamCipher},
    ChaCha20,
    Key,
    Nonce,
};
use chacha20poly1305::{
    self,
    aead::{Aead, NewAead},
    ChaCha20Poly1305,
};
use digest::Digest;
use rand::{rngs::OsRng, RngCore};
use tari_comms::types::{CommsPublicKey, CommsSecretKey};
use tari_crypto::{
    keys::DiffieHellmanSharedSecret,
    tari_utilities::{epoch_time::EpochTime, ByteArray},
};
use zeroize::Zeroize;

use crate::{
    comms_dht_hash_domain_challenge,
    comms_dht_hash_domain_key_message,
    comms_dht_hash_domain_key_signature,
    envelope::{DhtMessageFlags, DhtMessageHeader, DhtMessageType, NodeDestination},
    outbound::DhtOutboundError,
    version::DhtProtocolVersion,
};

#[derive(Debug, Clone, Zeroize)]
#[zeroize(drop)]
pub struct CipherKey(chacha20::Key);
pub struct AuthenticatedCipherKey(chacha20poly1305::Key);

const MESSAGE_BASE_LENGTH: usize = 6000;

/// Generates a Diffie-Hellman secret `kx.G` as a `chacha20::Key` given secret scalar `k` and public key `P = x.G`.
pub fn generate_ecdh_secret(secret_key: &CommsSecretKey, public_key: &CommsPublicKey) -> [u8; 32] {
    // TODO: PK will still leave the secret in released memory. Implementing Zerioze on RistrettoPublicKey is not
    //       currently possible because (Compressed)RistrettoPoint does not implement it.
    let k = CommsPublicKey::shared_secret(secret_key, public_key);
    let mut output = [0u8; 32];

    output.copy_from_slice(k.as_bytes());
    output
}

fn pad_message_to_base_length_multiple(message: &[u8]) -> Result<Vec<u8>, DhtOutboundError> {
    // We require a 32-bit length representation, and also don't want to overflow after including this encoding
    if message.len() > ((u32::max_value() - (size_of::<u32>() as u32)) as usize) {
        return Err(DhtOutboundError::PaddingError("Message is too long".to_string()));
    }
    let message_length = message.len();
    let encoded_length = (message_length as u32).to_le_bytes();

    // Pad the message (if needed) to the next multiple of the base length
    let padding_length = if ((message_length + size_of::<u32>()) % MESSAGE_BASE_LENGTH) == 0 {
        0
    } else {
        MESSAGE_BASE_LENGTH - ((message_length + size_of::<u32>()) % MESSAGE_BASE_LENGTH)
    };

    // The padded message is the encoded length, message, and zero padding
    let mut padded_message = Vec::with_capacity(size_of::<u32>() + message_length + padding_length);
    padded_message.extend_from_slice(&encoded_length);
    padded_message.extend_from_slice(message);
    padded_message.extend(std::iter::repeat(0u8).take(padding_length));

    Ok(padded_message)
}

fn get_original_message_from_padded_text(padded_message: &[u8]) -> Result<Vec<u8>, DhtOutboundError> {
    // NOTE: This function can return errors relating to message length
    // It is important not to leak error types to an adversary, or to have timing differences

    // The padded message must be long enough to extract the encoded message length
    if padded_message.len() < size_of::<u32>() {
        return Err(DhtOutboundError::PaddingError("Padded message is not long enough for length extraction".to_string()));
    }

    // The padded message must be a multiple of the base length
    if (padded_message.len() % MESSAGE_BASE_LENGTH) != 0 {
        return Err(DhtOutboundError::PaddingError("Padded message must be a multiple of the base length".to_string()));
    }

    // Decode the message length
    let mut encoded_length = [0u8; size_of::<u32>()];
    encoded_length.copy_from_slice(&padded_message[0..size_of::<u32>()]);
    let message_length = u32::from_le_bytes(encoded_length) as usize;

    // The padded message is too short for the decoded length
    let end = message_length.checked_add(size_of::<u32>()).ok_or_else(|| DhtOutboundError::PaddingError("Claimed unpadded message length is too large".to_string()))?;
    if end > padded_message.len() {
        return Err(DhtOutboundError::CipherError(
            "Claimed unpadded message length is too large".to_string(),
        ));
    }

    // Remove the padding (we don't check for valid padding, as this is offloaded to authentication)
    let start = size_of::<u32>();
    let unpadded_message = &padded_message[start..end];

    Ok(unpadded_message.to_vec())
}

pub fn generate_key_message(data: &[u8]) -> CipherKey {
    // domain separated hash of data (e.g. ecdh shared secret) using hashing API
    let domain_separated_hash = comms_dht_hash_domain_key_message().chain(data).finalize();

    // Domain separation uses Challenge = Blake256, thus its output has 32-byte length
    CipherKey(*Key::from_slice(domain_separated_hash.as_ref()))
}

pub fn generate_key_signature_for_authenticated_encryption(data: &[u8]) -> AuthenticatedCipherKey {
    // domain separated of data (e.g. ecdh shared secret) using hashing API
    let domain_separated_hash = comms_dht_hash_domain_key_signature().chain(data).finalize();

    // Domain separation uses Challenge = Blake256, thus its output has 32-byte length
    AuthenticatedCipherKey(*chacha20poly1305::Key::from_slice(domain_separated_hash.as_ref()))
}

/// Decrypts cipher text using ChaCha20 stream cipher given the cipher key and cipher text with integral nonce.
pub fn decrypt(cipher_key: &CipherKey, cipher_text: &[u8]) -> Result<Vec<u8>, DhtOutboundError> {
    if cipher_text.len() < size_of::<Nonce>() {
        return Err(DhtOutboundError::CipherError(
            "Cipher text is not long enough to include nonce".to_string(),
        ));
    }

    let (nonce, cipher_text) = cipher_text.split_at(size_of::<Nonce>());
    let nonce = Nonce::from_slice(nonce);
    let mut cipher_text = cipher_text.to_vec();

    let mut cipher = ChaCha20::new(&cipher_key.0, nonce);
    cipher.apply_keystream(cipher_text.as_mut_slice());

    // get original message, from decrypted padded cipher text
    let cipher_text = get_original_message_from_padded_text(cipher_text.as_slice())?;
    Ok(cipher_text)
}

pub fn decrypt_with_chacha20_poly1305(
    cipher_key: &AuthenticatedCipherKey,
    cipher_signature: &[u8],
) -> Result<Vec<u8>, DhtOutboundError> {
    let nonce = [0u8; size_of::<chacha20poly1305::Nonce>()];

    let nonce_ga = chacha20poly1305::Nonce::from_slice(&nonce);

    let cipher = ChaCha20Poly1305::new(&cipher_key.0);
    let decrypted_signature = cipher
        .decrypt(nonce_ga, cipher_signature)
        .map_err(|_| DhtOutboundError::CipherError(String::from("Authenticated decryption failed")))?;

    Ok(decrypted_signature)
}

/// Encrypt the plain text using the ChaCha20 stream cipher
pub fn encrypt(cipher_key: &CipherKey, plain_text: &[u8]) -> Result<Vec<u8>, DhtOutboundError> {
    // pad plain_text to avoid message length leaks
    let plain_text = pad_message_to_base_length_multiple(plain_text)?;

    let mut nonce = [0u8; size_of::<Nonce>()];
    OsRng.fill_bytes(&mut nonce);

    let nonce_ga = Nonce::from_slice(&nonce);
    let mut cipher = ChaCha20::new(&cipher_key.0, nonce_ga);

    let mut buf = vec![0u8; plain_text.len() + nonce.len()];
    buf[..nonce.len()].copy_from_slice(&nonce[..]);

    buf[nonce.len()..].copy_from_slice(plain_text.as_slice());
    cipher.apply_keystream(&mut buf[nonce.len()..]);
    Ok(buf)
}

/// Produces authenticated encryption of the signature using the ChaCha20-Poly1305 stream cipher,
/// refer to https://docs.rs/chacha20poly1305/latest/chacha20poly1305/# for more details.
/// Attention: as pointed in https://github.com/tari-project/tari/issues/4138, it is possible
/// to use a fixed Nonce, with homogeneous zero data, as this does not incur any security
/// vulnerabilities. However, such function is not intented to be used outside of dht scope
pub fn encrypt_with_chacha20_poly1305(
    cipher_key: &AuthenticatedCipherKey,
    signature: &[u8],
) -> Result<Vec<u8>, DhtOutboundError> {
    let nonce = [0u8; size_of::<chacha20poly1305::Nonce>()];

    let nonce_ga = chacha20poly1305::Nonce::from_slice(&nonce);
    let cipher = ChaCha20Poly1305::new(&cipher_key.0);

    // length of encrypted equals signature.len() + 16 (the latter being the tag size for ChaCha20-poly1305)
    let encrypted = cipher
        .encrypt(nonce_ga, signature)
        .map_err(|_| DhtOutboundError::CipherError(String::from("Authenticated encryption failed")))?;

    Ok(encrypted)
}

/// Generates a 32-byte hashed challenge that commits to the message header and body
pub fn create_message_domain_separated_hash(header: &DhtMessageHeader, body: &[u8]) -> [u8; 32] {
    create_message_domain_separated_hash_parts(
        header.version,
        &header.destination,
        header.message_type,
        header.flags,
        header.expires,
        header.ephemeral_public_key.as_ref(),
        body,
    )
}

/// Generates a 32-byte hashed challenge that commits to all message parts
pub fn create_message_domain_separated_hash_parts(
    protocol_version: DhtProtocolVersion,
    destination: &NodeDestination,
    message_type: DhtMessageType,
    flags: DhtMessageFlags,
    expires: Option<EpochTime>,
    ephemeral_public_key: Option<&CommsPublicKey>,
    body: &[u8],
) -> [u8; 32] {
    // get byte representation of `expires` input
    let expires = expires.map(|t| t.as_u64().to_le_bytes()).unwrap_or_default();

    // get byte representation of `ephemeral_public_key`
    let e_pk = ephemeral_public_key
        .map(|e_pk| {
            let mut buf = [0u8; 32];
            // CommsPublicKey::as_bytes returns 32-bytes
            buf.copy_from_slice(e_pk.as_bytes());
            buf
        })
        .unwrap_or_default();

    // we digest the given data into a domain independent hash function to produce a signature
    // use of the hashing API for domain separation and deal with variable length input
    let hasher = comms_dht_hash_domain_challenge()
        .chain(protocol_version.as_bytes())
        .chain(destination.to_inner_bytes())
        .chain((message_type as i32).to_le_bytes())
        .chain(flags.bits().to_le_bytes())
        .chain(expires)
        .chain(e_pk)
        .chain(body);

    Digest::finalize(hasher).into()
}

#[cfg(test)]
mod test {
    use tari_crypto::keys::PublicKey;
    use tari_utilities::hex::from_hex;

    use super::*;

    #[test]
    fn encrypt_decrypt() {
        let pk = CommsPublicKey::default();
        let key = CipherKey(*chacha20::Key::from_slice(pk.as_bytes()));
        let plain_text = "Last enemy position 0830h AJ 9863".as_bytes().to_vec();
        let encrypted = encrypt(&key, &plain_text).unwrap();
        let decrypted = decrypt(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plain_text);
    }

    #[test]
    fn decrypt_fn() {
        let pk = CommsPublicKey::default();
        let key = CipherKey(*chacha20::Key::from_slice(pk.as_bytes()));
        let cipher_text = from_hex(
            "6063cd49c7b871c0fc9785e9b959fda553fadbb10bcaaced0958e88eb6858e05fe310b4401a78d03b52a81be49db2bffcce13765e1a64460063d33289b1a3527af3df8e292c79abca71aa9a87baa1a0a6c23532a3297dda9e0c22d4b60606db1ed02a75e7a7d21fafe1214cbf8a3a66ec319a6aafeeb0e7b06375370c52b2abe63170ce50552a697f1ff87dc03ae1df574ed8e7abf915aec6959808ec526d6da78f08f2bed24268028baeba3ebd52d0fde34b145267ced68a08c4d480c213d0ab8b9c55a1630e956ed9531d6db600537f3997d612bec5905bd3ce72f5eace475e9793e6e316349f9cbd49022e401870af357605a1c7d279d5f414a5cae13e378711f345eabf46eb7fabc9465376f027a8d2d69448243cf2d70223c2430f6ce1ff55b8c2f54e27ccf77040f70c9eb84c9da9f8176a867ebcf8cbb9dfbb9256d688a76ec02af3afe3fa8221a4876462a754bc65a15c584b8c132a48dd955821961e47d5fdce62668b8f11a42b9127d5d98414bbd026eed0b5511628b96eb8435880f38d28bad6573f90611397b7f0df46d371e29a1de3591a3aa221623a540693941d6fc22d4fa57bc612282868f024613c2f69224141ad623eb49cb2b151ff79b36ec30f9a2842c696fd94f3092989ba3f8f5136850ed1f970a559e3fe19c24b84f650da4d46a5abc3514a242e3088cae15b6012e2a3cd878a7b988cc6183e81a27fc263d3be1ef07403262b22972b4f78a38d930f3e4ec1f5c9f1a1639cd138f24c665fc39b808dfbed81f8888e8e23aac1cdd980dcd0f7d95dc71fb02abcf532be17208b8d86a5fd501c18f2b18234956617797ea78523907c46a7b558cde76d1d034d6ab36ca33c4c17ea617929e3e40af011417f9b3983912fd0ed60685261532bcca5f3b3863d92cf2e4eb33a97613d9ca55112d1bd63df9d591042f972e2da3bde7b5a572f9f4cd2633330fdb3250430f27d3a40a30a996d5a41d61dafc3fe96a1fb63e2cab5c3ec0f1084e778f303da498fc970fe3117b8513166a6c8798e00a82a6c96a61b419e12717fac57dd1c989d3a10b3ab798ee0c15a5cd04dbe83667523ad4b7587ff331513b63f15c72f1844d67c7830f723fa754969d3f4254895e71d7087617ded071a797ee5791b7a95abcb360ef504bdaa85191b09b2345c0d0096fafa85b10d1675f0c4a1fe45231fd88c715c32c38d9697cebfb712e1ce57645c8faa08b4983ab8f537b017c9898b7907c06b25c01ea0f9d736d573ec7e3b14efa84d84258131ddc11a9696e12234ab65fb4e653d8dcdf4c6ce51e0104aedf2b8089593c4b5d665ed885c2d798843cf80041657ba9d800bbb9eee7076c212cfdc56df1d63e3eb4de5c6f132376c39b0272fc35e4729988eeaf9f7748142b68b10a72a948f6db24baebe3323c11002edfbe878972f02920a9ca536ff917c37a20eb0d984ee230885950fb271e56a8640d5fff75cef501cc82a46244eca5bfed4ab180b4c4d61abb209f38dabddcb4a82581bf4990631e11cc670507f8ce88c10a7fcc06ed824d7a1aab0b4dc4a2b984f45c7447b077262dc6b90f04516a5598193e95a40e982f092a2b61fcf5203379e6b042715792915adfd0edc66a69bb7fbacf64b9c86f3b15da66f8d8eecfc373204ad10f2c666181f0facd972be208c620bb0539e396c6254f2efcc2934ae6265fdc0b8172577435758e7da46df6f175f2867e6bd734fe70416b70b643b9e4b0e0e844ccfa019c699f85313f94cc6a1bb371bb14fd65a1599355ec8cdd71a555d98a900b8cbd2b1de3378f42a137eb95b473d62be3559ed687583d6963b6857d3be5f7acc12f5f5b04b26b5c582ce6dd8d1bee6322b02c2c1dc29fcba20899d529df4fd6c1edbfd1081d6cf10b20b9451ad935da2c4cef66c160550b180ba1b668029ed15448cd288427aca7f6e6505fdfc69b8111a2a071601d78484e857705a4bc7f9423800ded4eba46e0f22ee85c48fc9e8a191355edc0868df350d627a7f1120d79ba4aa1dde1ec97f8daeb0a07be914d5f6d2e74270666d03e4ca92845957b85982761dc1ee6f7603e31681dd323a045c0ac3b06b515d7bd485bfe7f6abe31e35aac7d8536b3f9c572121fcdd44c505ccfffe514e498732cab4e70524a5281b0942f5ae861b535764f056df6a1951b3c1c261f21b3b5f0a276ed05e32879ede428683b34ac8e7ebc88c9c767bf5e7cfb0cf444c1f9fd5be9041f69f6ae9772b0e035c6a2a7d9c1c614858a506a4a4bc00cee0577561b96e98c973edfa43d52471db9c716699e52260a20150aa99f8adea872c999b66fb4395d5b8c717a2c97eb7638a1d92da2ef8b2ec80db3afa3ce83445aaccae09f38c0b84c85a8983ba4c3b9a13fed4c65fd8899333da4dbca549cd2a487eb58841881f3571dfa4821bc522b56993d657bce51dfb41caf6c2cb78e8b6beceddc44febdea144da13ae9ccd9465b3ac96b06dfe79baced35ad51763d05090dc7620c89f448134507f41828be8703fd2ab1f53370e75e55366eba1e903311313707279d5965e3343476c0a8aeef2001ad88d5e452d648dd2029a6f549809c4177d1871c88abcd1404d52ebee2dd97dc52ad1a9c018428a1a64fda6773a6ea967d4124a6cf98c7e6dc4c4d9c051a376d3e3fe2e17f6cd044dd60ee32e9d6bdbdfdcbdecc4e7306092186a7ad8ab87328f9fedb6ee8ab9417968fbaa0e582205a660fa55e1ba3c5b0c84b67017f250338125894d162c400be8d563c9f0416dc5641d31bad577543cba8c6c9a7c04064e412597d47c6272d8e087bc11397533cb1bd7feebea9feee44e1b6a5f49b937594da3b719e1982a90594277f43798a39e419c204f18a6920e5ac1a751eddeaef9392a6f84d68d73aabc6ba68750d47ad4da8bd842662226225a764661ea11ff9f13d328e0242a0b513aa5ad9fbe9d484b3d28a41890e4fc62820ef2342a90c0837b30c831eb78213e7e2cd6dfbda26a7e6103ab8b4219462ca70ca57c79638b2c49f0469ea6f68335071294257c5337ccf452ca1bfedf81610f353e7576f02a2b32aba64a4252946fda330de11990f51207817860e0d8b7c9cb58a5858155db61376a01c02aaedb7017fd3c36adf4f3c07f29f352330c6d78ab6bbb7d4aabf3725833e86523b755094273465ba57545162623036a7786f426d0a63e13bebf2205a6b488bd6da3c93469a4df4b3811e9c63d62c61e0cdd263df821adc0d1b751c1314be9fd93761b447931e425db7e09baac9083aed472de5fe6172c8e8f729ade8faa96d131e86204462e14e0411b4b7629de25a0c5dbf848c9ca8c42376f5d54bff34bf36074136bbe98228745dbc9d411d891553f0af00240e1729ce7757fba2775fa5b700e95460910008584a833fb9edc073cd4d8333643631e193040d850f87cd50d9ca2e2e5c3943787dc4a4677ac7e130c2d6739945fd3b059ebe040abb38a20d73a7669516cf8503f40642217c8580a27b127f1f33eaa7adff44c922afac813c870795563fac79d139d5b5233a26728328f88f1f9daaaea1c4e1ee64ded0b006ce46015d512e8c4a411ab788a5383563949c95846202250c5b9e0baab0bc8620327ed2aacd36e1bdc9d3a4d6b4e22627d75bd088cdd47ca204f1ce44357d1b471b37581c820f6bbdfe3da1f4f90dc353833731703b7b9bb87ff2d0cae1e2f0321994759d1a21b2075a620b58b814cd65812092891261dd7e879b65843480382f59e20d6b6c67b2fb750ff0cfce897891f976b0fae7ac31e02384b251bcbecce6ff98819cf0cd6d41fdab9ba6907742394732ed5e74bcd13aad1a188855c020f09e62540be9b2992a397b30107ad730ebe183504226b303f30032f4c0a683812d05be57961430504866bd2ee6993423ebe34ba4d2d022ce6d5b2345bbed34d6807aec473ad0701b9b8fe2db1cef57748dfcb29ddb3b253a865dd7383d04253cd70c350d02ccd2371cecfd74aae820fa91eddd89d27925c33183e03d44c7f88f8068c64d223d2d5f4ab18fae6d209e1e267395576f4f48ae056da7d6e91f94991659b4c07f44aa1c45aaf75b7274b7668753f968d5e6635f4abf238e5d44ffc38e68cad8237f7e7a25d5fc0dcd5afc2bedbac6b42e8bc8064118c9042d1159f70dfac73d65c8a9782c264445af11c878591d49d49ad46f4e6d086d55232afd234c3bceab2eef0e22e5c2875670c5125e8a172f5f2168e59fe0cb5e9e1a81bf645a2c45d115b9a3efe9fe2d1799f12b0c11f50ae5540ff4e90e6220eb62451e10ce1418929e03c751d9019d47b87847595333feb6ab4af40662d04c3ece4f93b4c2c2f2ee2078724090336f16a4f33801095036a31b557960b5d8d2552f0aadfa3dc9dcfe8f1dd6a61631b6a69ee6ce8433153f8b1ea99a9a5ac688026d6ef408f2aa958ada8baf0193b3989f359c7a913fcb9eec230568584bcda3a759c824884c9febff518c7cf312360d2c1ffd2bbdd0b2e9346cbe1bf383446bd2fec431475ec509474ef9eb06817f53d3c4ca74fba08c3b434eabf3ae9fcc2287c588fc5574bff37066705ca9a39d088cd5cbb83b385b5cf647ced0c23885295d2b24f37e4098be82edccc23e1c973b1855e2009de63408c78e570b3cec65c6d236d81adb1bc298436a1e125b99bb995a5c6df5b2a4e70b8cf1db5de38120134527ce349c32f8e35fa43837aa38cdb1d5695a34d12d27bd5ee4536d9a20e62b55e59cdc7ecca1f4398dac7a4b756d9e131a7d2c8bde32c20ef0424154c88c8276fdf3c75f08f3cd423bd648ff3520680a1f1dd956451881f6d31238c11c99a20e1d9170410c8d8eb88ce90e179fc80e23e36a28b1810383a4d0d1ef0f2db94206aa1fb25498b425e5ad1f0f0bd3eed22ca5545ef541880f37f8fea82fecd59d8c94765d3a454e81775844701412e3c01a6dcdbf277428969a7f08d67313cdd2ce3b531addee28733552ee1bf4124ad8b3e40e04b94599e04cce60f5676307b0605ad7dc73b03cc26227eab60196d37c312a01858f5ad6a901e0f1c796c52cb9690da5c712a2d36c74e65ec9a60ea41387b8a0f79697cdfd93e40ab569d6a55361be97fb7ac8d80b5a5482908d44af94df2fb09a777978f4d911008d528ff44aef960cfd25fb56e26c341850721f020f9fd112cd52fc28dd129ffc2f9a12a829dcb69b54a894d4b3d1ac3b63bc9bcd39e30a00e419c8f4d2b630c224880a7d3af9c19c8a79262818b368589e7ad03b722021306fbcbcf7bc87ad418a3eb6616e7d4ce286264554be6040e8e4cd0c5a9bbdd2367e47d1fe0a9c3eeaf2455c4f6f779bab3d5bea5284a244fc3e804fb6d0e50fec91f85b71c6ba91f43a240fa48900229e5f3038b0806f70a1cb72fdea58b664f06c04bf688183a4f22255d6976f2102aafb669ee117fa1e44ae325ad52001469fed9d26e4f8592f56e42bf5e7195f521c0beaf891e47a703075fa1948ee07add55a765346b94ae498fa96145ad8460f23248222e329398fec6ad7f323c448ce82bb706b24e07adc0681901a63d5d1c7b871a9df8009ed7bb10be4e39a987c1bf039554a016ac8693284a7248fb8a9aa440dde213c2414447727c1556d25f1fbce057652044e2350b9ef5627584d403a934dd33e8c26e20799f1dbf915705b70d66256d31ca7c407307fa18e163917635d67f742828deba4b942b5f0d916b5e737b5811d3c3b4ac386c7ebaad1a6c465ce9fb229bc6ce7ae62f8efd8632e5312db8ba213d28d19843ac7fbae105a1433921b34c216c3c2ab247080a629c7ac5507129b27ce0d38ddde06722a5a0a979894d6140c31a82bfc517adf0b57c761f75bc14d65d8701e0dea92a06584f2d877dc5fb0b32496754e6b0115e99a9623ad631ea0a76b4e7893bf0982151e1c5ed6d64a305393f6f715de333653ace204c2f03de8f36c463c937f7f23326a88337624fc606317d7c0ea2badf69e40602c2ff1e2dcc9cfca1ccef566381712af157c5e458335c8a283733a617a75fd7cef52c515c754443c8e9e1930994805e6f0b2a9a2ccbd848f6580896317dd9dbfda17d00e80d35bc58a704fbe7d6d6d45752811130f682ea9471903c9af9b5c95d074ec87b32c86dc5b29a186a60a7a03e630c7a5cd38e6ab1a2f561642d5662658fc20239233505727575e75dbbc5be630f9ebc9485f03bbf0569554a87bfb5cfd397daf5d8d92a38fc4b24b1a433edad26a12c4e29506362dec83fb9b1f31158e834cde319d40bd283f0a1f3995b3ade08bcc01c794d656583b928300a6f2be57e5bf1586a123cf28ac8e2287e0b7ab67419dc4f527f714fdee8c47088cc1857a39825524dc3f5a3777c1f906cf496dac43e3f8304ebe5d5696da5b7e5d79f176a391736ba46bc718356e1713a00ea754a52b5899ba7eb71b10bbd211cead7d1890f2d8bb981a2549e2cd53bf895f96f628c4d00061275c87f4dcbadcb5944f912d27aaa4124cadd0e2a1d82ee4d3a8b977bd1a03fc6f79caf4c306addea0bd72b754c113350655324dec3dcbb1f1de66e3e7a9f06ba0e04de0cae7af7d6e31298bf5be706038e0d8477a79ea5f8e21decdf6f5ff71090d8cb2dabb9d1a87ee526b0ec84be81ad9585b09f165cfff7a4a63e30ac7341a3a42e3e02bfc34486a2a5492a39dd31dc233c74f454584e5bd2524382a08357ba2d3ef46833551ed3fa6f672d5ba0ec75258430738c16989840b6ae6909f10340d1845bd975cf2933047a1c22c332606f76681dae8727921d4f1345f0457700b8622ea72a50c17cda201f7019d4af9dc0b67bd95317d98c2fe38ab8e12dbefe236b463014caf9c3cdd390dfcff034e90e4f51e1233bb8b341bba6f922d1b0629e261844018f39d054b26cb82592e33466354f552a14f9e6175d418cb9724fa045e723b5ab9ece5aa45800f1202b3d174fe4e129e7320a9063039f8bdea8601762ff45933503e0bd10944893e565d641a39289da67e5269dc7cfc22dc3d5f5011b66b25340cea4055bd66a7752c69e624bfc12e5cc68cd0b5cab3242860b7f40541303e228e666a6500e1e739b0b6d853b5715cf3a668facc135d133d4eefa035f36c16838f25ab4a0d2f20d18f05c0fae1a705370162cfe6f7fcf1c69654c2ce73e3820b48568c25a6d9d036c2386dee6ef14e5fc967ffccde38bf263c8c0f924bcfcdf54669dcf872724280cc4f81bb2aa993998f6312d0c6084ed823e5e6bff0ed25cc4e82b749dc11f4cf55290344d9c307d634793e81b9d3d457765dc6f81b66f1a6aaaa1079558a4892edfc342fe24856200b5dcc65c9e7809b655d3cc7bb26bcda91933f590bc61099fd0b83e04bad2174150645afd7c3ccac5417234e30da4e7574af953f8d9b7a5029417d439f1d13c4390bed2bc05d73821ff2355c33da5f95623c73abd826614572841e4777a9a0b538cef4a2c6327c75116977322a8c488f466178cdcaf3f0e10df86dbd1827ba2cc4c8fba90a1d64ad783a77704c5b1262cd11cb010f09ab04377d6e5ebed4d5dfe8eaa0cc2535a0be69bdf5e1987167b8135428ef84287aa4424c35c7a7bc94cabd553df4840121403b2ba3479e1a7f86085cce49c245af944a2ed78b77784309d05d5f5587a6e589baa6e7d279b0ab43afb5497b6d5954b3a8f66dd547d3b72565437ead511c9d5342406aace95e5cc31f2c6d618a24c219d0298f980a571ce29b999d20b3ed94a60e286ed7ddc647c439e3d421814da6d91c8b7d5b3fa70ec6a3c261ab9ee4ac779545edcc7db6df3345db26b91c5a997dac5e62b75dc05358821bab4fe65a049fe9ce8e537ae81a10dfcca0ef97c6cb95e3ff1573e5461f7b505e1678fbe97a41ab696b53f6ea09038ecda09eed34247251424b766306c0c64fd836274cf85fe0e0c19638127c2210b580c9194fe0cccac7ac80e3dde38e9cccd6a194ee923f4e73800bec0c77f60553f9c7c8413ea87d20c114d7b415fdd87fc55f273b1b3a9f9c71c4462d5b3f300daf0fc6c338278e5991e0c6de07a3c288d237df00325230be204f7b2bb7a127ac28b001e4225e910eeb9521f5af6cfeae1f18c08bf8ac9d1513c3794ba5b8ea9fb9a57825cb154fc1e9a9dddf809dd6bb11a207625b23b274344e7e0b7ca666e456735d5901f1341aca42e749183823b3debbd563aeebc68f9b15dce13d0fc1acef47d38d5967c2b6b3fe8ed69b180dbcbf17455ee6825641202ccd145c0a0a0f4091622338f48474e5838d8915f814eb87ad45e710b07f79f662c2120278ce05978d8a7aee20fc5661a08c072977ed878092e7183332b70c9c54db307c705e527f6fd2076e39c216b0490f552d52a109652958c62fc6bf7f913818dbdf5d92550779aae541d54d059d5844658422c17a24e374fa6f92e5a9fda87eee249747b9cd292043c9731d2c1d08d06eab030fb49e779cb58bf4f776d6aa0185db860007d8b2d0f7205dacb9201ac9538d2c37062f736b6b44e971e11500",
        )
        .unwrap();
        let plain_text = decrypt(&key, &cipher_text).unwrap();
        let secret_msg = "Last enemy position 0830h AJ 9863".as_bytes().to_vec();
        assert_eq!(plain_text, secret_msg);
    }

    #[test]
    fn sanity_check() {
        let domain_separated_hash = comms_dht_hash_domain_key_signature()
            .chain(&[10, 12, 13, 82, 93, 101, 87, 28, 27, 17, 11, 35, 43])
            .finalize();

        let domain_separated_hash = domain_separated_hash.as_ref();

        // Domain separation uses Challenge = Blake256, thus its output has 32-byte length
        let key = AuthenticatedCipherKey(*chacha20poly1305::Key::from_slice(domain_separated_hash));

        let signature = b"Top secret message, handle with care".as_slice();
        let n = signature.len();
        let nonce = [0u8; size_of::<chacha20poly1305::Nonce>()];

        let nonce_ga = chacha20poly1305::Nonce::from_slice(&nonce);
        let cipher = ChaCha20Poly1305::new(&key.0);

        let encrypted = cipher
            .encrypt(nonce_ga, signature)
            .map_err(|_| DhtOutboundError::CipherError(String::from("Authenticated encryption failed")))
            .unwrap();

        assert_eq!(encrypted.len(), n + 16);
    }

    #[test]
    fn decryption_fails_in_case_tag_is_manipulated() {
        let (sk, pk) = CommsPublicKey::random_keypair(&mut OsRng);
        let key_data = generate_ecdh_secret(&sk, &pk);
        let key = generate_key_signature_for_authenticated_encryption(&key_data);

        let signature = b"Top secret message, handle with care".as_slice();

        let mut encrypted = encrypt_with_chacha20_poly1305(&key, signature).unwrap();

        // sanity check to validate that encrypted.len() = signature.len() + 16
        assert_eq!(encrypted.len(), signature.len() + 16);

        // manipulate tag and check that decryption fails
        let n = encrypted.len();
        encrypted[n - 1] += 1;

        // decryption should fail
        assert!(decrypt_with_chacha20_poly1305(&key, encrypted.as_slice())
            .unwrap_err()
            .to_string()
            .contains("Authenticated decryption failed"));
    }

    #[test]
    fn decryption_fails_in_case_body_message_is_manipulated() {
        let (sk, pk) = CommsPublicKey::random_keypair(&mut OsRng);
        let key_data = generate_ecdh_secret(&sk, &pk);
        let key = generate_key_signature_for_authenticated_encryption(&key_data);

        let signature = b"Top secret message, handle with care".as_slice();

        let mut encrypted = encrypt_with_chacha20_poly1305(&key, signature).unwrap();

        // manipulate encrypted message body and check that decryption fails
        encrypted[0] += 1;

        // decryption should fail
        assert!(decrypt_with_chacha20_poly1305(&key, encrypted.as_slice())
            .unwrap_err()
            .to_string()
            .contains("Authenticated decryption failed"));
    }

    #[test]
    fn decryption_fails_if_message_send_to_incorrect_node() {
        let (sk, pk) = CommsPublicKey::random_keypair(&mut OsRng);
        let (other_sk, other_pk) = CommsPublicKey::random_keypair(&mut OsRng);

        let key_data = generate_ecdh_secret(&sk, &pk);
        let other_key_data = generate_ecdh_secret(&other_sk, &other_pk);

        let key = generate_key_signature_for_authenticated_encryption(&key_data);
        let other_key = generate_key_signature_for_authenticated_encryption(&other_key_data);

        let signature = b"Top secret message, handle with care".as_slice();

        let encrypted = encrypt_with_chacha20_poly1305(&key, signature).unwrap();

        // decryption should fail
        assert!(decrypt_with_chacha20_poly1305(&other_key, encrypted.as_slice())
            .unwrap_err()
            .to_string()
            .contains("Authenticated decryption failed"));
    }

    #[test]
    fn pad_message_correctness() {
        // test for small message
        let message = &[0u8, 10, 22, 11, 38, 74, 59, 91, 73, 82, 75, 23, 59];
        let prepend_message = (message.len() as u32).to_le_bytes();
        let pad = std::iter::repeat(0u8)
            .take(MESSAGE_BASE_LENGTH - message.len() - prepend_message.len())
            .collect::<Vec<_>>();

        let pad_message = pad_message_to_base_length_multiple(message).unwrap();

        // padded message is of correct length
        assert_eq!(pad_message.len(), MESSAGE_BASE_LENGTH);
        // prepend message is well specified
        assert_eq!(prepend_message, pad_message[..prepend_message.len()]);
        // message body is well specified
        assert_eq!(
            *message,
            pad_message[prepend_message.len()..prepend_message.len() + message.len()]
        );
        // pad is well specified
        assert_eq!(pad, pad_message[prepend_message.len() + message.len()..]);

        // test for large message
        let message = &[100u8; MESSAGE_BASE_LENGTH * 8 - 100];
        let prepend_message = (message.len() as u32).to_le_bytes();
        let pad_message = pad_message_to_base_length_multiple(message).unwrap();
        let pad = std::iter::repeat(0u8)
            .take((8 * MESSAGE_BASE_LENGTH) - message.len() - prepend_message.len())
            .collect::<Vec<_>>();

        // padded message is of correct length
        assert_eq!(pad_message.len(), 8 * MESSAGE_BASE_LENGTH);
        // prepend message is well specified
        assert_eq!(prepend_message, pad_message[..prepend_message.len()]);
        // message body is well specified
        assert_eq!(
            *message,
            pad_message[prepend_message.len()..prepend_message.len() + message.len()]
        );
        // pad is well specified
        assert_eq!(pad, pad_message[prepend_message.len() + message.len()..]);

        // test for base message of multiple base length
        let message = &[100u8; MESSAGE_BASE_LENGTH * 9 - 123];
        let prepend_message = (message.len() as u32).to_le_bytes();
        let pad = std::iter::repeat(0u8)
            .take((9 * MESSAGE_BASE_LENGTH) - message.len() - prepend_message.len())
            .collect::<Vec<_>>();

        let pad_message = pad_message_to_base_length_multiple(message).unwrap();

        // padded message is of correct length
        assert_eq!(pad_message.len(), 9 * MESSAGE_BASE_LENGTH);
        // prepend message is well specified
        assert_eq!(prepend_message, pad_message[..prepend_message.len()]);
        // message body is well specified
        assert_eq!(
            *message,
            pad_message[prepend_message.len()..prepend_message.len() + message.len()]
        );
        // pad is well specified
        assert_eq!(pad, pad_message[prepend_message.len() + message.len()..]);

        // test for empty message
        let message: [u8; 0] = [];
        let prepend_message = (message.len() as u32).to_le_bytes();
        let pad_message = pad_message_to_base_length_multiple(&message).unwrap();
        let pad = [0u8; MESSAGE_BASE_LENGTH - 4];

        // padded message is of correct length
        assert_eq!(pad_message.len(), MESSAGE_BASE_LENGTH);
        // prepend message is well specified
        assert_eq!(prepend_message, pad_message[..prepend_message.len()]);
        // message body is well specified
        assert_eq!(
            message,
            pad_message[prepend_message.len()..prepend_message.len() + message.len()]
        );

        // pad is well specified
        assert_eq!(pad, pad_message[prepend_message.len() + message.len()..]);
    }

    #[test]
    fn unpadding_failure_modes() {
        // The padded message is empty
        let message: [u8; 0] = [];
        assert!(get_original_message_from_padded_text(&message)
            .unwrap_err()
            .to_string()
            .contains("Padded message is not long enough for length extraction"));

        // We cannot extract the message length
        let message = [0u8; size_of::<u32>() - 1];
        assert!(get_original_message_from_padded_text(&message)
            .unwrap_err()
            .to_string()
            .contains("Padded message is not long enough for length extraction"));

        // The padded message is not a multiple of the base length
        let message = [0u8; 2 * MESSAGE_BASE_LENGTH + 1];
        assert!(get_original_message_from_padded_text(&message)
            .unwrap_err()
            .to_string()
            .contains("Padded message must be a multiple of the base length"));
    }

    #[test]
    fn get_original_message_from_padded_text_successful() {
        // test for short message
        let message = vec![0u8, 10, 22, 11, 38, 74, 59, 91, 73, 82, 75, 23, 59];
        let pad_message = pad_message_to_base_length_multiple(message.as_slice()).unwrap();

        let output_message = get_original_message_from_padded_text(pad_message.as_slice()).unwrap();
        assert_eq!(message, output_message);

        // test for large message
        let message = vec![100u8; 1024];
        let pad_message = pad_message_to_base_length_multiple(message.as_slice()).unwrap();

        let output_message = get_original_message_from_padded_text(pad_message.as_slice()).unwrap();
        assert_eq!(message, output_message);

        // test for base message of base length
        let message = vec![100u8; 984];
        let pad_message = pad_message_to_base_length_multiple(message.as_slice()).unwrap();

        let output_message = get_original_message_from_padded_text(pad_message.as_slice()).unwrap();
        assert_eq!(message, output_message);

        // test for empty message
        let message: Vec<u8> = vec![];
        let pad_message = pad_message_to_base_length_multiple(message.as_slice()).unwrap();

        let output_message = get_original_message_from_padded_text(pad_message.as_slice()).unwrap();
        assert_eq!(message, output_message);
    }

    #[test]
    fn padding_fails_if_pad_message_prepend_length_is_bigger_than_plaintext_length() {
        let message = "This is my secret message, keep it secret !".as_bytes();
        let mut pad_message = pad_message_to_base_length_multiple(message).unwrap();

        // we modify the prepend length, in order to assert that the get original message
        // method will output a different length message
        pad_message[0] = 1;

        let modified_message = get_original_message_from_padded_text(pad_message.as_slice()).unwrap();
        assert!(message.len() != modified_message.len());

        // add big number from le bytes of prepend bytes
        pad_message[0] = 255;
        pad_message[1] = 255;
        pad_message[2] = 255;
        pad_message[3] = 255;

        assert!(get_original_message_from_padded_text(pad_message.as_slice())
            .unwrap_err()
            .to_string()
            .contains("Claimed unpadded message length is too large"));
    }

    #[test]
    fn check_decryption_succeeds_if_pad_message_padding_is_modified() {
        // this should not be problematic as any changes in the content of the encrypted padding, should not affect
        // in any way the value of the decrypted content, by applying a cipher stream
        let pk = CommsPublicKey::default();
        let key = CipherKey(*chacha20::Key::from_slice(pk.as_bytes()));
        let message = "My secret message, keep it secret !".as_bytes().to_vec();
        let mut encrypted = encrypt(&key, &message).unwrap();

        let n = encrypted.len();
        encrypted[n - 1] += 1;

        assert!(decrypt(&key, &encrypted).unwrap() == message);
    }

    #[test]
    fn decryption_fails_if_message_body_is_modified() {
        let pk = CommsPublicKey::default();
        let key = CipherKey(*chacha20::Key::from_slice(pk.as_bytes()));
        let message = "My secret message, keep it secret !".as_bytes().to_vec();
        let mut encrypted = encrypt(&key, &message).unwrap();

        encrypted[size_of::<Nonce>() + size_of::<u32>() + 1] += 1;

        assert!(decrypt(&key, &encrypted).unwrap() != message);
    }
}
