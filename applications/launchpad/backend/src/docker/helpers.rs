// Copyright 2021. The Tari Project
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
//

use std::fmt::Debug;

use futures::{Stream, StreamExt};
use log::warn;
use rand::distributions::{Alphanumeric, Distribution};
use serde::Serialize;

use super::DockerWrapperError;

/// Create a cryptographically secure password on length `len`
pub fn create_password(len: usize) -> String {
    let mut rng = rand::thread_rng();
    Alphanumeric.sample_iter(&mut rng).take(len).map(char::from).collect()
}

pub async fn process_stream<FnSendMsg, FnSendErr, T: Debug + Clone + Serialize>(
    send_message: FnSendMsg,
    send_error: FnSendErr,
    message_destination: String,
    error_destination: String,
    mut stream: impl Stream<Item = Result<T, DockerWrapperError>> + Unpin,
) where
    FnSendMsg: Fn(String, T) -> Result<(), tauri::Error>,
    FnSendErr: Fn(String, String) -> Result<(), tauri::Error>,
{
    while let Some(message) = stream.next().await {
        let emit_result = match message {
            Ok(payload) => send_message(message_destination.clone(), payload),
            Err(err) => send_error(error_destination.clone(), err.chained_message()),
        };
        if let Err(err) = emit_result {
            warn!("Error emitting event: {}", err.to_string());
        }
    }
}
