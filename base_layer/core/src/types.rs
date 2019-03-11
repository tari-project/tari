// Copyright 2018 The Tari Project
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
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use crypto::{
    common::Blake256,
    ristretto::{
        pedersen::{PedersenBaseOnRistretto255, PedersenOnRistretto255},
        RistrettoSchnorr,
        RistrettoSecretKey,
    },
};
use crypto::ristretto::RistrettoPublicKey;

/// Define the explicit Signature implementation for the Tari base layer. A different signature scheme can be
/// employed by redefining this type.
pub type Signature = RistrettoSchnorr;

/// Define the explicit Commitment implementation for the Tari base layer.
pub type Commitment = PedersenOnRistretto255;
pub type Base = PedersenBaseOnRistretto255;

/// Define the explicit Secret key implementation for the Tari base layer.
pub type BlindingFactor = RistrettoSecretKey;

/// Define the explicit Public key implementation for the Tari base layer
pub type PublicKey = RistrettoPublicKey;

/// Define the hash function that will be used to produce a signature challenge
pub type SignatureHash = Blake256;

