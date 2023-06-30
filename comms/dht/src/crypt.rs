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

use std::{convert::TryFrom, iter, mem::size_of};

use chacha20poly1305::{aead::AeadInPlace, ChaCha20Poly1305, KeyInit, Nonce, Tag};
use digest::{generic_array::GenericArray, Digest, FixedOutput};
use prost::bytes::BytesMut;
use tari_comms::{
    message::MessageExt,
    types::{CommsDHKE, CommsPublicKey, CommsSecretKey},
    BufMut,
};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, ByteArray};
use tari_utilities::{hidden_type, safe_array::SafeArray, ByteArrayError, Hidden};
use zeroize::Zeroize;

use crate::{
    comms_dht_hash_domain_challenge,
    comms_dht_hash_domain_key_mask,
    comms_dht_hash_domain_key_message,
    envelope::{DhtMessageFlags, DhtMessageHeader, DhtMessageType, NodeDestination},
    error::DhtEncryptError,
    version::DhtProtocolVersion,
};

// `ChaCha20` key used to encrypt messages
hidden_type!(CommsMessageKey, SafeArray<u8, { size_of::<chacha20::Key>() }>);

// Mask used (as a secret key) for sender key offset; we fix it to 32 bytes for compatibility
// This isn't fully generic, but will work for 32-byte hashers and byte-to-scalar functionality
hidden_type!(CommsKeyMask, SafeArray<u8, 32>);

const MESSAGE_BASE_LENGTH: usize = 6000;

fn get_message_padding_length(message_length: usize) -> usize {
    if message_length == 0 {
        return MESSAGE_BASE_LENGTH;
    }

    if message_length % MESSAGE_BASE_LENGTH == 0 {
        0
    } else {
        MESSAGE_BASE_LENGTH - (message_length % MESSAGE_BASE_LENGTH)
    }
}

/// Pads a message to a multiple of MESSAGE_BASE_LENGTH excluding the additional prefix space.
/// This function returns the number of additional padding bytes appended to the message.
fn pad_message_to_base_length_multiple(
    message: &mut BytesMut,
    additional_prefix_space: usize,
) -> Result<usize, DhtEncryptError> {
    // We require a 32-bit length representation, and also don't want to overflow after including this encoding
    if message.len() > u32::MAX as usize {
        return Err(DhtEncryptError::PaddingError("Message is too long".to_string()));
    }
    let padding_length =
        get_message_padding_length(message.len().checked_sub(additional_prefix_space).ok_or_else(|| {
            DhtEncryptError::PaddingError("Message length shorter than the additional_prefix_space".to_string())
        })?);

    message.resize(message.len() + padding_length, 0);

    Ok(padding_length)
}

/// Returns the unpadded message. The messages must have the length prefixed to it and the nonce is removec.
fn get_original_message_from_padded_text(padded_message: &mut BytesMut) -> Result<(), DhtEncryptError> {
    // NOTE: This function can return errors relating to message length
    // It is important not to leak error types to an adversary, or to have timing differences

    // The padded message must be long enough to extract the encoded message length
    if padded_message.len() < size_of::<u32>() {
        return Err(DhtEncryptError::PaddingError(
            "Padded message is not long enough for length extraction".to_string(),
        ));
    }

    // The padded message must be a multiple of the base length
    if (padded_message.len() % MESSAGE_BASE_LENGTH) != 0 {
        return Err(DhtEncryptError::PaddingError(
            "Padded message must be a multiple of the base length".to_string(),
        ));
    }

    // Decode the message length
    let len = padded_message.split_to(size_of::<u32>());
    let mut encoded_length = [0u8; size_of::<u32>()];
    encoded_length.copy_from_slice(&len[..]);
    let message_length = u32::from_le_bytes(encoded_length) as usize;

    // The padded message is too short for the decoded length
    if message_length > padded_message.len() {
        return Err(DhtEncryptError::CipherError(
            "Claimed unpadded message length is too large".to_string(),
        ));
    }

    // Remove the padding (we don't check for valid padding, as this is offloaded to authentication)
    padded_message.truncate(message_length);

    Ok(())
}

/// Generate the key for a message
pub fn generate_key_message(data: &CommsDHKE) -> CommsMessageKey {
    let mut comms_message_key = CommsMessageKey::from(SafeArray::default());
    comms_dht_hash_domain_key_message()
        .chain(data.as_bytes())
        .finalize_into(GenericArray::from_mut_slice(comms_message_key.reveal_mut()));

    comms_message_key
}

/// Generate the mask used to protect a sender public key
pub fn generate_key_mask(data: &CommsDHKE) -> Result<CommsSecretKey, ByteArrayError> {
    let mut comms_key_mask = CommsKeyMask::from(SafeArray::default());
    comms_dht_hash_domain_key_mask()
        .chain(data.as_bytes())
        .finalize_into(GenericArray::from_mut_slice(comms_key_mask.reveal_mut()));

    // This is infallible since we require 32 bytes of hash output
    CommsSecretKey::from_bytes(comms_key_mask.reveal())
}

/// Decrypt a message using the `ChaCha20Poly1305` authenticated stream cipher
/// Note that we use a fixed zero nonce here because of the use of an ephemeral key
pub fn decrypt_message(
    message_key: &CommsMessageKey,
    buffer: &mut BytesMut,
    associated_data: &[u8],
) -> Result<(), DhtEncryptError> {
    // Assert we have a tag
    if buffer.len() < size_of::<Tag>() {
        return Err(DhtEncryptError::InvalidAuthenticatedDecryption);
    }

    // We use a fixed zero nonce since the key is ephemeral
    // This is _not_ safe in general!
    let nonce = Nonce::from_slice(&[0u8; size_of::<Nonce>()]);

    // Split off the tag
    let tag = buffer.split_off(buffer.len() - size_of::<Tag>()).freeze();

    // Decrypt with authentication
    let cipher = ChaCha20Poly1305::new(GenericArray::from_slice(message_key.reveal()));
    cipher
        .decrypt_in_place_detached(nonce, associated_data, buffer, GenericArray::from_slice(&tag))
        .map_err(|_| DhtEncryptError::InvalidAuthenticatedDecryption)?;

    // Unpad the message
    get_original_message_from_padded_text(buffer)?;
    Ok(())
}

/// Encrypt a message using the `ChaCha20-Poly1305` authenticated stream cipher
/// The message is assumed to have a 32-bit length prepended to it
/// Note that we use a fixed zero nonce here because of the use of an ephemeral key
pub fn encrypt_message(
    message_key: &CommsMessageKey,
    buffer: &mut BytesMut,
    associated_data: &[u8],
) -> Result<(), DhtEncryptError> {
    // Pad the message to mitigate leaking its length
    pad_message_to_base_length_multiple(buffer, 0)?;

    // We use a fixed zero nonce since the key is ephemeral
    // This is _not_ safe in general!
    let nonce = Nonce::from_slice(&[0u8; size_of::<Nonce>()]);

    // Encrypt with authentication
    let cipher = ChaCha20Poly1305::new(GenericArray::from_slice(message_key.reveal()));
    let tag = cipher
        .encrypt_in_place_detached(nonce, associated_data, buffer)
        .map_err(|e| DhtEncryptError::CipherError(e.to_string()))?;

    // Append the tag to the buffer
    buffer.extend_from_slice(&tag);

    Ok(())
}

/// Encodes a prost Message, efficiently prepending the little-endian 32-bit length to the encoding
fn encode_with_prepended_length<T: prost::Message>(
    msg: &T,
    additional_prefix_space: usize,
) -> Result<BytesMut, DhtEncryptError> {
    let len = msg.encoded_len();
    let mut buf = BytesMut::with_capacity(size_of::<u32>() + additional_prefix_space + len);
    buf.extend(iter::repeat(0).take(additional_prefix_space));
    let len_u32 = u32::try_from(len).map_err(|_| DhtEncryptError::InvalidMessageBody)?;
    buf.put_u32_le(len_u32);
    msg.encode(&mut buf).expect(
        "prost::Message::encode documentation says it is infallible unless the buffer has insufficient capacity. This \
         buffer's capacity was set with encoded_len",
    );
    Ok(buf)
}

pub fn prepare_message<T: prost::Message>(is_encrypted: bool, message: &T) -> Result<BytesMut, DhtEncryptError> {
    if is_encrypted {
        encode_with_prepended_length(message, 0)
    } else {
        Ok(message.encode_into_bytes_mut())
    }
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
    use prost::Message;
    use rand::rngs::OsRng;
    use tari_comms::message::MessageExt;
    use tari_crypto::keys::PublicKey;

    use super::*;

    #[test]
    fn encrypt_decrypt_message() {
        let key = CommsMessageKey::from(SafeArray::default());
        let message = "Last enemy position 0830h AJ 9863".to_string();
        let associated_data = b"Associated data";
        let mut buffer = prepare_message(true, &message).unwrap();

        encrypt_message(&key, &mut buffer, associated_data).unwrap();
        decrypt_message(&key, &mut buffer, associated_data).unwrap();
        assert_eq!(String::decode(&buffer[..]).unwrap(), message);
    }

    #[test]
    fn decryption_fails_on_evil_tag() {
        let key = CommsMessageKey::from(SafeArray::default());
        let message = "Last enemy position 0830h AJ 9863".to_string();
        let associated_data = b"Associated data";
        let mut buffer = prepare_message(true, &message).unwrap();

        encrypt_message(&key, &mut buffer, associated_data).unwrap();

        // Manipulate the tag, which is appended to the buffer
        let malleated_index = buffer.len() - 1;
        buffer[malleated_index] = !buffer[malleated_index];

        assert!(decrypt_message(&key, &mut buffer, associated_data).is_err());
    }

    #[test]
    fn decryption_fails_on_evil_message() {
        let key = CommsMessageKey::from(SafeArray::default());
        let message = "Last enemy position 0830h AJ 9863".to_string();
        let associated_data = b"Associated data";
        let mut buffer = prepare_message(true, &message).unwrap();

        encrypt_message(&key, &mut buffer, associated_data).unwrap();

        // Manipulate the message
        buffer[0] = !buffer[0];

        assert!(decrypt_message(&key, &mut buffer, associated_data).is_err());
    }

    #[test]
    fn decryption_fails_on_evil_associated_data() {
        let key = CommsMessageKey::from(SafeArray::default());
        let message = "Last enemy position 0830h AJ 9863".to_string();
        let associated_data = b"Associated data";
        let evil_associated_data = b"Evil associated data";
        let mut buffer = prepare_message(true, &message).unwrap();

        encrypt_message(&key, &mut buffer, associated_data).unwrap();

        // Decrypt using evil associated data
        assert!(decrypt_message(&key, &mut buffer, evil_associated_data).is_err());
    }

    #[test]
    // This isn't guaranteed in general by AEAD properties, but should hold on random evil keys
    // In the context of the message protocol, this is sufficient
    fn decryption_fails_on_evil_key() {
        // Generate two distinct keys
        let (sk, pk) = CommsPublicKey::random_keypair(&mut OsRng);
        let (evil_sk, evil_pk) = CommsPublicKey::random_keypair(&mut OsRng);
        let key = generate_key_message(&CommsDHKE::new(&sk, &pk));
        let evil_key = generate_key_message(&CommsDHKE::new(&evil_sk, &evil_pk));

        let message = "Last enemy position 0830h AJ 9863".to_string();
        let associated_data = b"Associated data";
        let mut buffer = prepare_message(true, &message).unwrap();

        encrypt_message(&key, &mut buffer, associated_data).unwrap();

        // Decrypt using evil key
        assert!(decrypt_message(&evil_key, &mut buffer, associated_data).is_err());
    }

    #[test]
    fn pad_message_correctness() {
        // test for small message
        let message = [0u8, 10, 22, 11, 38, 74, 59, 91, 73, 82, 75, 23, 59].as_slice();
        let pad = iter::repeat(0u8)
            .take(MESSAGE_BASE_LENGTH - message.len())
            .collect::<Vec<_>>();

        let mut pad_message = BytesMut::from(message);
        let pad_len = pad_message_to_base_length_multiple(&mut pad_message, 0).unwrap();
        // For small messages less than MESSAGE_BASE_LENGTH we can expect an exact capacity
        assert_eq!(pad_message.capacity(), message.len() + pad_len);

        // padded message is of correct length
        assert_eq!(pad_message.len(), MESSAGE_BASE_LENGTH);
        // message body is well specified
        assert_eq!(*message, pad_message[..message.len()]);
        // pad is well specified
        assert_eq!(pad, pad_message[message.len()..]);

        // test for large message
        let message = encode_with_prepended_length(&vec![100u8; MESSAGE_BASE_LENGTH * 8 - 100], 0).unwrap();
        let mut pad_message = message.clone();
        pad_message_to_base_length_multiple(&mut pad_message, 0).unwrap();
        let pad = iter::repeat(0u8)
            .take((8 * MESSAGE_BASE_LENGTH) - message.len())
            .collect::<Vec<_>>();

        // padded message is of correct length
        assert_eq!(pad_message.len(), 8 * MESSAGE_BASE_LENGTH);
        // message body is well specified
        assert_eq!(*message, pad_message[..message.len()]);
        // pad is well specified
        assert_eq!(pad, pad_message[message.len()..]);

        // test for base message of multiple base length
        let message = encode_with_prepended_length(&vec![100u8; MESSAGE_BASE_LENGTH * 9 - 123], 0).unwrap();
        let pad = std::iter::repeat(0u8)
            .take((9 * MESSAGE_BASE_LENGTH) - message.len())
            .collect::<Vec<_>>();

        let mut pad_message = message.clone();
        pad_message_to_base_length_multiple(&mut pad_message, 0).unwrap();

        // padded message is of correct length
        assert_eq!(pad_message.len(), 9 * MESSAGE_BASE_LENGTH);
        // message body is well specified
        assert_eq!(*message, pad_message[..message.len()]);
        // pad is well specified
        assert_eq!(pad, pad_message[message.len()..]);

        // test for empty message
        let message = encode_with_prepended_length(&vec![], 0).unwrap();
        let mut pad_message = message.clone();
        pad_message_to_base_length_multiple(&mut pad_message, 0).unwrap();
        let pad = [0u8; MESSAGE_BASE_LENGTH - 4];

        // padded message is of correct length
        assert_eq!(pad_message.len(), MESSAGE_BASE_LENGTH);
        // message body is well specified
        assert_eq!(message, pad_message[..message.len()]);

        // pad is well specified
        assert_eq!(pad, pad_message[message.len()..]);
    }

    #[test]
    fn unpadding_failure_modes() {
        // The padded message is empty
        let mut message = BytesMut::new();
        assert!(get_original_message_from_padded_text(&mut message)
            .unwrap_err()
            .to_string()
            .contains("Padded message is not long enough for length extraction"));

        // We cannot extract the message length
        let mut message = BytesMut::from([0u8; size_of::<u32>() - 1].as_slice());
        assert!(get_original_message_from_padded_text(&mut message)
            .unwrap_err()
            .to_string()
            .contains("Padded message is not long enough for length extraction"));

        // The padded message is not a multiple of the base length
        let mut message = BytesMut::from([0u8; 2 * MESSAGE_BASE_LENGTH + 1].as_slice());
        assert!(get_original_message_from_padded_text(&mut message)
            .unwrap_err()
            .to_string()
            .contains("Padded message must be a multiple of the base length"));
    }

    #[test]
    fn get_original_message_from_padded_text_successful() {
        // test for short message
        let message = vec![0u8, 10, 22, 11, 38, 74, 59, 91, 73, 82, 75, 23, 59];
        let mut pad_message = encode_with_prepended_length(&message, 0).unwrap();
        pad_message_to_base_length_multiple(&mut pad_message, 0).unwrap();

        //
        let mut output_message = pad_message.clone();
        get_original_message_from_padded_text(&mut output_message).unwrap();
        assert_eq!(message.to_encoded_bytes(), output_message);

        // test for large message
        let message = vec![100u8; 1024];
        let mut pad_message = encode_with_prepended_length(&message, 0).unwrap();
        pad_message_to_base_length_multiple(&mut pad_message, 0).unwrap();

        let mut output_message = pad_message.clone();
        get_original_message_from_padded_text(&mut output_message).unwrap();
        assert_eq!(message.to_encoded_bytes(), output_message);

        // test for base message of base length
        let message = vec![100u8; 984];
        let mut pad_message = encode_with_prepended_length(&message, 0).unwrap();
        pad_message_to_base_length_multiple(&mut pad_message, 0).unwrap();

        let mut output_message = pad_message.clone();
        get_original_message_from_padded_text(&mut output_message).unwrap();
        assert_eq!(message.to_encoded_bytes(), output_message);

        // test for empty message
        let message: Vec<u8> = vec![];
        let mut pad_message = encode_with_prepended_length(&message, 0).unwrap();
        pad_message_to_base_length_multiple(&mut pad_message, 0).unwrap();

        let mut output_message = pad_message.clone();
        get_original_message_from_padded_text(&mut output_message).unwrap();
        assert_eq!(message.to_encoded_bytes(), output_message);
    }

    #[test]
    fn padding_fails_if_pad_message_prepend_length_is_bigger_than_plaintext_length() {
        let message = "This is my secret message, keep it secret !".as_bytes().to_vec();
        let mut pad_message = encode_with_prepended_length(&message, 0).unwrap();
        pad_message_to_base_length_multiple(&mut pad_message, 0).unwrap();
        let mut pad_message = pad_message.to_vec();

        // we modify the prepend length, in order to assert that the get original message
        // method will output a different length message
        pad_message[0] = 1;

        let mut modified_message = BytesMut::from(pad_message.as_slice());
        get_original_message_from_padded_text(&mut modified_message).unwrap();
        assert_ne!(message.len(), modified_message.len());

        // add big number from le bytes of prepend bytes
        pad_message[0] = 255;
        pad_message[1] = 255;
        pad_message[2] = 255;
        pad_message[3] = 255;

        let mut pad_message = BytesMut::from(pad_message.as_slice());
        assert!(get_original_message_from_padded_text(&mut pad_message)
            .unwrap_err()
            .to_string()
            .contains("Claimed unpadded message length is too large"));
    }
}
