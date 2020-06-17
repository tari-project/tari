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

use crate::compat::IoCompat;
use futures::{AsyncRead, AsyncWrite, Future, StreamExt};
use tokio_util::codec::{Framed, LinesCodec, LinesCodecError};

/// Max line length accepted by the liveness session.
const MAX_LINE_LENGTH: usize = 50;

pub struct LivenessSession<TSocket> {
    framed: Framed<IoCompat<TSocket>, LinesCodec>,
}

impl<TSocket> LivenessSession<TSocket>
where TSocket: AsyncRead + AsyncWrite + Unpin
{
    pub fn new(socket: TSocket) -> Self {
        Self {
            framed: Framed::new(IoCompat::new(socket), LinesCodec::new_with_max_length(MAX_LINE_LENGTH)),
        }
    }

    pub fn run(self) -> impl Future<Output = Result<(), LinesCodecError>> {
        let (sink, stream) = self.framed.split();
        stream.forward(sink)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{memsocket::MemorySocket, runtime};
    use futures::SinkExt;
    use tokio::{time, time::Duration};

    #[tokio_macros::test_basic]
    async fn echos() {
        let (inbound, outbound) = MemorySocket::new_pair();
        let liveness = LivenessSession::new(inbound);
        let join_handle = runtime::current().spawn(liveness.run());
        let mut outbound = Framed::new(IoCompat::new(outbound), LinesCodec::new());
        for _ in 0..10usize {
            outbound.send("ECHO".to_string()).await.unwrap()
        }
        let pings = outbound.take(10).collect::<Vec<_>>().await;
        assert_eq!(pings.len(), 10);
        assert!(pings.iter().all(|a| a.as_ref().unwrap() == "ECHO"));

        time::timeout(Duration::from_secs(1), join_handle)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
