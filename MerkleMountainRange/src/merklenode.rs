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

pub type ObjectHash = [u8; 32];

pub trait Hashable {
    fn get_hash(&self) -> ObjectHash;

    fn concat(&self, hash: ObjectHash) -> ObjectHash;
    // think about this, perhaps we should do something like this:
    // fn fun_test_impl(value: i32, f: impl Fn(i32) -> i32) -> i32 {
    // println!("{}", f(value));
    // value
    // }
    // fn fun_test_dyn(value: i32, f: &dyn Fn(i32) -> i32) -> i32 {
    // println!("{}", f(value));
    // value
    // }
    // fn fun_test_ptr(value: i32, f: fn(i32) -> i32) -> i32 {
    // println!("{}", f(value));
    // value
    // }
    //
    // fn times2(value: i32) -> i32 {
    // 2 * value
    // }
    //
    // fn main() {
    // let y = 2;
    // static dispatch
    // fun_test_impl(5, times2);
    // fun_test_impl(5, |x| 2*x);
    // fun_test_impl(5, |x| y*x);
    // dynamic dispatch
    // fun_test_dyn(5, &times2);
    // fun_test_dyn(5, &|x| 2*x);
    // fun_test_dyn(5, &|x| y*x);
    // C-like pointer to function
    // fun_test_ptr(5, times2);
    // fun_test_ptr(5, |x| 2*x); //ok: empty capture set
    // fun_test_ptr(5, |x| y*x); //error: expected fn pointer, found closure
    // }
}

/// This is the MerkleNode struct. This struct represents a merkle node,
pub struct MerkleNode {
    pub hash: ObjectHash,
    pub pruned: bool,
    // todo discuss adding height here, this will make some calculations faster, but will storage larger
}

impl MerkleNode {
    pub fn new(hash: ObjectHash) -> MerkleNode {
        MerkleNode { hash, pruned: false }
    }
}
