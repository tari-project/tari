// Copyright 2019. The Tari Project
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

use tari_mmr::{ArrayLike, ArrayLikeExt, MemBackendVec};

#[test]
fn len_push_get_truncate_for_each_shift_clear() {
    let mut db_vec = MemBackendVec::<i32>::new();
    let mut mem_vec = vec![100, 200, 300, 400, 500, 600];
    assert_eq!(db_vec.len().unwrap(), 0);

    mem_vec.iter().for_each(|val| assert!(db_vec.push(val.clone()).is_ok()));
    assert_eq!(db_vec.len().unwrap(), mem_vec.len());

    mem_vec
        .iter()
        .enumerate()
        .for_each(|(i, val)| assert_eq!(db_vec.get(i).unwrap(), Some(val.clone())));
    assert_eq!(db_vec.get(mem_vec.len()).unwrap(), None);

    mem_vec.truncate(4);
    assert!(db_vec.truncate(4).is_ok());
    assert_eq!(db_vec.len().unwrap(), mem_vec.len());
    db_vec.for_each(|val| assert!(mem_vec.contains(&val.unwrap()))).unwrap();

    assert!(mem_vec.shift(2).is_ok());
    assert!(db_vec.shift(2).is_ok());
    assert_eq!(db_vec.len().unwrap(), 2);
    assert_eq!(db_vec.get(0).unwrap(), Some(300));
    assert_eq!(db_vec.get(1).unwrap(), Some(400));

    assert!(db_vec.clear().is_ok());
    assert_eq!(db_vec.len().unwrap(), 0);
}
