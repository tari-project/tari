// Copyright 2025 The Tari Project
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
//! m-of-n multisig outputs.
//!
//! # Trust model
//!
//! A multisig output's script carries a random 32-byte message fixed when the output is created. A co-signer's
//! signature covers that message and nothing else: it does not cover the recipient, amount or fee of the transaction
//! that later spends the output. Once `m` signatures exist, whoever holds them and builds the spending transaction
//! alone decides where the funds go. Co-signers trust that party (security assumption `A-MULTISIG-01`).
//!
//! Before signing with
//! [`sign_script_message_with_spend_key_trusted`](crate::key_manager::TransactionKeyManagerInterface::sign_script_message_with_spend_key_trusted),
//! a co-signer should take the message from an output they know independently (one they saw created, or their own
//! record), not only from the party asking for the signature, and confirm that the unspent output with that hash
//! ([`types::GetMultisigUtxoDataOutput`]) carries this message and their key
//! ([`script::co_signer_script_public_key`]). The console wallet's `sign-script-message --output-hash` makes this
//! check against its base node. Neither the check nor a commitment binds the signature to that output: it is valid
//! for any output whose script has the same message and key.
pub mod script;
pub mod session;
pub mod types;
