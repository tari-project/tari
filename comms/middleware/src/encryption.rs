// Copyright 2019, The Tari Project
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

use derive_error::Error;
use futures::{task::Context, Future, Poll};
use log::*;
use std::sync::Arc;
use tari_comms::{
    message::{InboundMessage, Message, MessageEnvelopeHeader, MessageFlags},
    peer_manager::{NodeIdentity, Peer},
    types::CommsCipher,
};
use tari_crypto::keys::{DiffieHellmanSharedSecret, PublicKey};
use tari_utilities::{
    ciphers::cipher::{Cipher, CipherError},
    message_format::MessageFormat,
};
use tower::{layer::Layer, Service};

const LOG_TARGET: &'static str = "comms::middleware::encryption";

#[derive(Debug, Error)]
pub enum EncryptionError {
    /// Invalid Destination that cannot be routed
    InvalidDestination,
    /// Message destined for this node cannot be decrypted
    DecryptionFailure,
    CipherError(CipherError),
}

/// Represents a decrypted InboundMessage.
pub struct DecryptedInboundMessage {
    pub version: u8,
    pub source_peer: Peer,
    pub envelope_header: MessageEnvelopeHeader,
    pub decryption_result: Result<Message, Vec<u8>>,
}

impl DecryptedInboundMessage {
    pub fn succeed(decrypted_message: Message, message: InboundMessage) -> Self {
        Self {
            version: message.version,
            source_peer: message.source_peer,
            envelope_header: message.envelope_header,
            decryption_result: Ok(decrypted_message),
        }
    }

    pub fn fail(message: InboundMessage) -> Self {
        Self {
            version: message.version,
            source_peer: message.source_peer,
            envelope_header: message.envelope_header,
            decryption_result: Err(message.body),
        }
    }

    pub fn failed(&self) -> Option<&Vec<u8>> {
        self.decryption_result.as_ref().err()
    }

    pub fn succeeded(&self) -> Option<&Message> {
        self.decryption_result.as_ref().ok()
    }

    pub fn decryption_succeeded(&self) -> bool {
        self.decryption_result.is_ok()
    }

    pub fn decryption_failed(&self) -> bool {
        self.decryption_result.is_err()
    }
}

/// This layer is responsible for attempting to decrypt inbound messages.
pub struct DecryptionLayer {
    node_identity: Arc<NodeIdentity>,
}

impl DecryptionLayer {
    pub fn new(node_identity: Arc<NodeIdentity>) -> Self {
        Self { node_identity }
    }
}

impl<S> Layer<S> for DecryptionLayer {
    type Service = DecryptionService<S>;

    fn layer(&self, service: S) -> Self::Service {
        DecryptionService::new(service, Arc::clone(&self.node_identity))
    }
}

/// Responsible for decrypting InboundMessages and passing a DecryptedInboundMessage to the given service
pub struct DecryptionService<S> {
    node_identity: Arc<NodeIdentity>,
    inner: S,
}

impl<S> DecryptionService<S> {
    pub fn new(service: S, node_identity: Arc<NodeIdentity>) -> Self {
        Self {
            inner: service,
            node_identity,
        }
    }
}

impl<S> Service<InboundMessage> for DecryptionService<S>
where S: Service<DecryptedInboundMessage, Response = ()> + Clone + Unpin
{
    type Error = S::Error;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, msg: InboundMessage) -> Self::Future {
        Self::handle_message(self.inner.clone(), Arc::clone(&self.node_identity), msg)
    }
}

impl<S> DecryptionService<S>
where S: Service<DecryptedInboundMessage, Response = ()>
{
    async fn handle_message(
        next_service: S,
        node_identity: Arc<NodeIdentity>,
        message: InboundMessage,
    ) -> Result<(), S::Error>
    {
        let envelope_header = &message.envelope_header;
        if !envelope_header.flags.contains(MessageFlags::ENCRYPTED) {
            // TODO: message clone, can be avoided?
            let decrypted = message.body.clone();
            return Self::decryption_succeeded(next_service, message, decrypted).await;
        }

        debug!(target: LOG_TARGET, "Attempting to decrypt message");
        let shared_secret = generate_ecdh_secret(&node_identity.secret_key, &envelope_header.peer_pubkey);
        match decrypt(&shared_secret, &message.body) {
            Ok(decrypted) => Self::decryption_succeeded(next_service, message, decrypted).await,
            Err(err) => {
                debug!(target: LOG_TARGET, "Unable to decrypt message: {}", err);
                Self::decryption_failed(next_service, message).await
            },
        }
    }

    async fn decryption_succeeded(
        mut next_service: S,
        message: InboundMessage,
        decrypted: Vec<u8>,
    ) -> Result<(), S::Error>
    {
        match Message::from_binary(&decrypted) {
            Ok(decrypted_msg) => {
                debug!(target: LOG_TARGET, "Message successfully decrypted");
                let msg = DecryptedInboundMessage::succeed(decrypted_msg, message);
                next_service.call(msg).await
            },
            Err(err) => {
                debug!(target: LOG_TARGET, "Unable to deserialize message: {}", err);
                Self::decryption_failed(next_service, message).await
            },
        }
    }

    async fn decryption_failed(mut next_service: S, message: InboundMessage) -> Result<(), S::Error> {
        let msg = DecryptedInboundMessage::fail(message);
        next_service.call(msg).await
    }
}

pub fn generate_ecdh_secret<PK>(dest_secret_key: &PK::K, source_public_key: &PK) -> Vec<u8>
where PK: PublicKey + DiffieHellmanSharedSecret<PK = PK> {
    PK::shared_secret(dest_secret_key, source_public_key).to_vec()
}

pub fn decrypt(cipher_key: &[u8], cipher_text: &[u8]) -> Result<Vec<u8>, EncryptionError> {
    CommsCipher::open_with_integral_nonce(cipher_text, cipher_key).map_err(Into::into)
}

pub fn encrypt(cipher_key: &[u8], plain_text: &Vec<u8>) -> Result<Vec<u8>, EncryptionError> {
    CommsCipher::seal_with_integral_nonce(plain_text, cipher_key).map_err(Into::into)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::{make_inbound_message, make_node_identity, service_fn};
    use futures::{executor::block_on, future};
    use std::sync::Mutex;
    use tari_comms::types::CommsSecretKey;
    use tari_test_utils::counter_context;
    use tari_utilities::{byte_array::ByteArray, hex::from_hex};

    #[test]
    fn poll_ready() {
        let inner = service_fn(|_: DecryptedInboundMessage| future::ready(Result::<(), ()>::Ok(())));
        let node_identity = Arc::new(make_node_identity());
        let mut service = DecryptionService::new(inner, node_identity);

        counter_context!(cx, counter);

        assert!(service.poll_ready(&mut cx).is_ready());

        assert_eq!(counter.get(), 0);
    }

    #[test]
    fn decrypt_inbound_success() {
        let result = Mutex::new(None);
        let inner = service_fn(|msg: DecryptedInboundMessage| {
            *result.lock().unwrap() = Some(msg);
            future::ready(Result::<(), ()>::Ok(()))
        });
        let node_identity = Arc::new(make_node_identity());
        let mut service = DecryptionService::new(inner, Arc::clone(&node_identity));

        let plain_text_msg = Message::from_message_format((), ()).unwrap();
        let secret_key = generate_ecdh_secret(&node_identity.secret_key, &node_identity.identity.public_key);
        let encrypted = encrypt(&secret_key.to_vec(), &plain_text_msg.to_binary().unwrap()).unwrap();
        let inbound_msg = make_inbound_message(&node_identity, encrypted, MessageFlags::ENCRYPTED);

        block_on(service.call(inbound_msg)).unwrap();
        let decrypted = result.lock().unwrap().take().unwrap();
        assert_eq!(decrypted.decryption_succeeded(), true);
        assert_eq!(decrypted.decryption_result.unwrap(), plain_text_msg);
    }

    #[test]
    fn decrypt_inbound_fail() {
        let result = Mutex::new(None);
        let inner = service_fn(|msg: DecryptedInboundMessage| {
            *result.lock().unwrap() = Some(msg);
            future::ready(Result::<(), ()>::Ok(()))
        });
        let node_identity = Arc::new(make_node_identity());
        let mut service = DecryptionService::new(inner, Arc::clone(&node_identity));

        let nonsense = "Cannot Decrypt this".as_bytes().to_vec();
        let inbound_msg = make_inbound_message(&node_identity, nonsense.clone(), MessageFlags::ENCRYPTED);

        block_on(service.call(inbound_msg)).unwrap();
        let decrypted = result.lock().unwrap().take().unwrap();
        assert_eq!(decrypted.decryption_succeeded(), false);
        assert_eq!(decrypted.decryption_result.unwrap_err(), nonsense);
    }

    #[test]
    fn encrypt_decrypt() {
        let secret_key = CommsSecretKey::default().to_vec();
        let plain_text = "Last enemy position 0830h AJ 9863".as_bytes().to_vec();
        let encrypted = encrypt(&secret_key, &plain_text).unwrap();
        let decrypted = decrypt(&secret_key, &encrypted).unwrap();
        assert_eq!(decrypted, plain_text);
    }

    #[test]
    fn decrypt_fn() {
        let secret_key = CommsSecretKey::default().to_vec();
        let cipher_text =
            from_hex("7ecafb4c0a88325c984517fca1c529b3083e9976290a50c43ff90b2ccb361aeaabfaf680e744b96fc3649a447b")
                .unwrap();
        let plain_text = decrypt(&secret_key, &cipher_text).unwrap();
        let secret_msg = "Last enemy position 0830h AJ 9863".as_bytes().to_vec();
        assert_eq!(plain_text, secret_msg);
    }
}
