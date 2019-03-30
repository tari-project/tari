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

use crate::keys::SecretKey;

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
/// arithmetic. Adding two commitments is the same as committing to the sum of their parts:
/// $$ \begin{aligned}
///   C_1 &= v_1.G + k_1.H \\\\
///   C_2 &= v_2.G + k_2.H \\\\
///   \therefore C_1 + C_2 &= (v_1 + v_2)G + (k_1 + k_2)H
/// \end{aligned} $$
pub trait HomomorphicCommitment {
    type K: SecretKey;

    fn open(&self, k: &Self::K, v: &Self::K) -> bool;
    fn as_bytes(&self) -> &[u8];
}

pub trait HomomorphicCommitmentFactory {
    type K: SecretKey;
    type C: HomomorphicCommitment<K = Self::K>;
    fn create(k: &Self::K, v: &Self::K) -> Self::C;
    /// return an identity point for addition using the specified base point. This is a commitment to zero with a zero
    /// blinding factor on the base point
    fn zero() -> Self::C;
}
