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

use curve25519_dalek::scalar::Scalar;

/// A commitment is like a sealed envelope. You put some information inside the envelope, and then seal (commit) it.
/// You can't change what you've said, but also, no-one knows what you've said until you're ready to open (open) the
/// envelope and reveal its contents. Also it's a special envelope that can only be opened by a special opener that
/// you keep safe in your drawer.
///
/// There are also different types of commitments that vary in their security guarantees, but all of them are
/// represented by binary data; so [HomomorphicCommitment](trait.HomomorphicCommitment.html) implements
/// [ByteArray](trait.ByteArray.html).
///
/// The Homomorphic part means, more or less, that commitments follow some of the standard rules of
/// arithmetic. Adding two commitments is the same as committing to the sum of their parts.
pub trait HomomorphicCommitment {
    type Base;
    fn new(k: &Scalar, v: &Scalar, base: &'static Self::Base) -> Self;
    /// Test whether the envelope content match the given key and value
    fn open(&self, k: &Scalar, v: &Scalar) -> bool;
    fn commit(&self) -> &[u8];
    fn to_bytes(&self) -> &[u8];
}
