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

mod comms_integration;
pub(super) mod greeting_service;
mod handshake;
pub(super) mod mock;
mod smoke;

#[test]
fn rpc_frame_is_superset_of_messaging_frame() {
    use crate::protocol::{messaging::MAX_FRAME_LENGTH, rpc};
    // Anything that fits in a messaging frame (e.g. a propagated block) must also fit in an RPC response payload
    // (e.g. the same block fetched during sync), and in an RPC request.
    const { assert!(rpc::RPC_MAX_FRAME_SIZE > MAX_FRAME_LENGTH) };
    const { assert!(rpc::max_response_payload_size() >= MAX_FRAME_LENGTH) };
    const { assert!(rpc::max_request_size() >= MAX_FRAME_LENGTH) };
    assert_eq!(rpc::RPC_MAX_FRAME_SIZE, 8 * 1024 * 1024 + 1024);
}
