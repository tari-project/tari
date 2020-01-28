// Copyright 2020. The Tari Project
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

// If you get a "module not found" error, see README.md for details on how to generate the node package
let tari_crypto = require('./pkg');
let assert = require('assert');

let KeyRing = tari_crypto.KeyRing;

console.log(`Tari crypto. Version ${tari_crypto.version()}`);
// The WASM module holds the keys in a vector (keyring), which means that we can get at all the cryptoey goodness
// without having to expose tons of functions with unsafe pointers, or continuously do de- and serialisation to hex
// or base64.
const keys = KeyRing.new();

console.log("Creating new keypair");
keys.new_key("Alice");
let n = keys.new_key("Bob");
console.log(`${n} keys in ring`);
console.log("kA = ", keys.private_key("Alice"));
console.log("PB = ", keys.public_key("Bob"));

console.log("Signing message");
let [r, R] = tari_crypto.generate_keypair();
let sig = keys.sign("Alice", r, "Hello Tari");
// The public nonce is returned in the signature. It should match R above
assert.equal(R, sig.public_nonce);
if (sig.error) {
    console.log(`Error getting signature ${sig.error}`);
} else {
    console.log('Signature:', sig);
    console.log("Verifying signature..");
    let pubkey = keys.public_key("Alice");
    console.log(`Pubkey: ${pubkey}`);
    let check = tari_crypto.check_signature(R, sig.signature, pubkey, "Hello Tari");
    if (check.result == true) {
        console.log("Signature is valid!");
    } else {
        console.log(`Invalid signature: ${check.error}`);
    }
}
keys.free();
