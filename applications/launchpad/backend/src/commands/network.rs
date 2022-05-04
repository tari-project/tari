// Copyright 2021. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, a&re permitted provided that the
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

use log::*;
use tari_common::configuration::Network;
use tauri::{AppHandle, Manager, Wry};

use super::pull_images::DEFAULT_IMAGES;
use crate::{commands::AppState, docker::TariNetwork};

pub static TARI_NETWORKS: [TariNetwork; 3] = [TariNetwork::Dibbler, TariNetwork::Igor, TariNetwork::Mainnet];

pub fn enum_to_list<T: Sized + ToString + Clone>(enums: &[T]) -> Vec<String> {
    enums
    .into_iter()
    .map(|enum_value| enum_value.to_string())
    .collect()
}

#[test]
fn network_list_test() {
    let networks = ["dibbler".to_string(), "igor".to_string(), "mainnet".to_string()].to_vec();
    assert_eq!(networks, enum_to_list(&TARI_NETWORKS));
}
