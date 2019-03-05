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

use crate::support::{hashvalues::HashValues, testobject::TestObject};
use blake2::Blake2b;
use merklemountainrange::{merklemountainrange::MerkleMountainRange, merklenode::Hashable};

fn create_mmr(leaves: u32) -> MerkleMountainRange<TestObject<Blake2b>, Blake2b> {
    let mut mmr: MerkleMountainRange<TestObject<Blake2b>, Blake2b> = MerkleMountainRange::new();
    for i in 1..leaves + 1 {
        let object: TestObject<Blake2b> = TestObject::new(i.to_string());
        mmr.add_single(object);
    }
    mmr
}

#[test]
fn create_small_mmr() {
    let mmr = create_mmr(2);
    assert_eq!(1, mmr.get_peak_height());
    let hash_values = HashValues::new();
    let hash0 = mmr.get_hash(0).unwrap();
    let proof = mmr.get_hash_proof(&hash0);
    let mut our_proof = Vec::new();
    our_proof.push(mmr.get_hash(0).unwrap());
    our_proof.push(mmr.get_hash(1).unwrap());
    our_proof.push(mmr.get_hash(2).unwrap());
    assert_eq!(hash_values.get_slice(2), HashValues::to_hex_multiple(&proof));
    assert_eq!(mmr.verify_proof(&our_proof), true);
}
