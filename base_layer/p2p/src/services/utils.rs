// Copyright 2019 The Tari Project
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

use crate::{comms_connector::PeerMessage, domain_message::DomainMessage, tari_message::TariMessageType};
use std::{fmt::Debug, sync::Arc};
use tari_utilities::message_format::{MessageFormat, MessageFormatError};

const LOG_TARGET: &'static str = "base_layer::p2p::services";

/// For use with `StreamExt::filter_map`. Log and filter any errors.
pub async fn ok_or_skip_result<T, E>(res: Result<T, E>) -> Option<T>
where E: Debug {
    match res {
        Ok(t) => Some(t),
        Err(err) => {
            tracing::error!(target: LOG_TARGET, "{:?}", err);
            None
        },
    }
}

pub fn map_deserialized<T>(
    serialized: Arc<PeerMessage<TariMessageType>>,
) -> Result<DomainMessage<T>, MessageFormatError>
where T: MessageFormat {
    Ok(DomainMessage {
        source_peer: serialized.source_peer.clone(),
        // TODO: origin_pubkey should be used when the DHT middleware is hooked up
        origin_pubkey: serialized.comms_header.message_public_key.clone(),
        inner: serialized.deserialize_message()?,
    })
}

#[cfg(test)]
mod test {
    use futures::executor::block_on;

    #[test]
    fn ok_or_skip_result() {
        block_on(async {
            let res = Result::<_, ()>::Ok(());
            assert_eq!(super::ok_or_skip_result(res).await.unwrap(), ());

            let res = Result::<(), _>::Err(());
            assert!(super::ok_or_skip_result(res).await.is_none());
        });
    }
}
