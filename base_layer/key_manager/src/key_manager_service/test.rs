//  Copyright 2022, The Tari Project
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

use crate::{cipher_seed::CipherSeed, key_manager_service::KeyManagerMock};

#[tokio::test]
async fn get_next_key_test_mock() {
    let cihper = CipherSeed::new();
    let key_manager_mock = KeyManagerMock::new(cihper);
    let branch = "test_branch_1".to_string();
    let branch2 = "test_branch_2".to_string();
    key_manager_mock.add_key_manager_mock(branch.clone()).await.unwrap();
    key_manager_mock.add_key_manager_mock(branch2.clone()).await.unwrap();

    let branch_1_key_1 = key_manager_mock.get_next_key_mock(branch.clone()).await.unwrap();
    let branch_2_key_1 = key_manager_mock.get_next_key_mock(branch2.clone()).await.unwrap();
    let branch_1_key_2 = key_manager_mock.get_next_key_mock(branch.clone()).await.unwrap();
    let branch_2_key_2 = key_manager_mock.get_next_key_mock(branch2.clone()).await.unwrap();

    assert_ne!(branch_1_key_1.key, branch_1_key_2.key);
    assert_ne!(branch_2_key_1.key, branch_2_key_2.key);
    assert_ne!(branch_1_key_1.key, branch_2_key_1.key);
    assert_ne!(branch_1_key_2.key, branch_2_key_2.key);
}

#[tokio::test]
async fn get_key_at_test_mock() {
    let cipher = CipherSeed::new();
    let key_manager_mock = KeyManagerMock::new(cipher);
    let branch = "test_branch_1".to_string();
    key_manager_mock.add_key_manager_mock(branch.clone()).await.unwrap();

    let key_10 = key_manager_mock
        .get_key_at_index_mock(branch.clone(), 10)
        .await
        .unwrap();
    let key_11 = key_manager_mock
        .get_key_at_index_mock(branch.clone(), 11)
        .await
        .unwrap();

    assert_ne!(key_10, key_11);

    let key_1 = key_manager_mock.get_next_key_mock(branch.clone()).await.unwrap();
    let key_2 = key_manager_mock.get_next_key_mock(branch.clone()).await.unwrap();

    let key_1_2 = key_manager_mock.get_key_at_index_mock(branch.clone(), 1).await.unwrap();

    assert_ne!(key_10, key_1.key);
    assert_ne!(key_2.key, key_1.key);
    assert_eq!(key_1.key, key_1_2);
}
