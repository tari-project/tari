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

//! # Tor Hidden Services and Control Port Client
//!
//! These modules interact with the Tor Control Port to create hidden services on-the-fly.
//!
//! The [client](crate::tor::client) module contains the client library for the Tor Control Port. You can find the spec
//! [here](https://gitweb.torproject.org/torspec.git/tree/control-spec.txt).
//!
//! The [hidden_service](crate::tor::hidden_service) module contains code which sets up hidden services required for
//! `tari_comms` to function over Tor.

mod control_client;
pub use control_client::{
    Authentication,
    KeyBlob,
    KeyType,
    PortMapping,
    PrivateKey,
    TorClientError,
    TorControlPortClient,
};

mod hidden_service;
pub use hidden_service::{
    HiddenService,
    HiddenServiceBuilder,
    HiddenServiceBuilderError,
    HiddenServiceControllerError,
    HsFlags,
    TorIdentity,
};
