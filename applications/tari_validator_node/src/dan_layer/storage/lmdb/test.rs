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

use crate::dan_layer::{
    models::TokenId,
    storage::{AssetStore, LmdbAssetStore},
};
use std::fs;
use tari_test_utils::paths;

fn with_store<F: FnOnce(LmdbAssetStore)>(f: F) {
    let path = paths::create_temporary_data_path();
    let store = LmdbAssetStore::initialize(&path, Default::default()).unwrap();
    f(store);
    // TODO: This will not happen on panic
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn it_replaces_the_metadata() {
    with_store(|mut store| {
        store.replace_metadata(&TokenId(b"123".to_vec()), &[4, 5, 6]).unwrap();
        let metadata = store.get_metadata(&TokenId(b"123".to_vec())).unwrap().unwrap();
        assert_eq!(metadata, vec![4, 5, 6]);

        store.replace_metadata(&TokenId(b"123".to_vec()), &[5, 6, 7]).unwrap();
        let metadata = store.get_metadata(&TokenId(b"123".to_vec())).unwrap().unwrap();
        assert_eq!(metadata, vec![5, 6, 7]);
    });
}

#[test]
fn it_returns_none_if_key_does_not_exist() {
    with_store(|mut store| {
        let metadata = store.get_metadata(&TokenId(b"123".to_vec())).unwrap();
        assert!(metadata.is_none());
    });
}
