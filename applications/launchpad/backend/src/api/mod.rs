// Copyright 2021. The Tari Project
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

use std::{convert::TryFrom, fmt::format};

use log::warn;
use tauri::http::status;

use crate::{
    commands::{status, DEFAULT_IMAGES},
    docker::{ContainerState, ImageType, TariNetwork},
};

pub static TARI_NETWORKS: [TariNetwork; 3] = [TariNetwork::Dibbler, TariNetwork::Igor, TariNetwork::Mainnet];

pub fn enum_to_list<T: Sized + ToString + Clone>(enums: &[T]) -> Vec<String> {
    enums.iter().map(|enum_value| enum_value.to_string()).collect()
}

#[tauri::command]
pub fn network_list() -> Vec<String> {
    enum_to_list::<TariNetwork>(&TARI_NETWORKS)
}

/// Provide a list of image names in the Tari "ecosystem"
#[tauri::command]
pub fn image_list() -> Vec<String> {
    enum_to_list::<ImageType>(&DEFAULT_IMAGES)
}

#[tauri::command]
pub async fn health_check(image: &str) -> String {
    match ImageType::try_from(image) {
        Ok(img) => status(img).await,
        Err(_err) => format!("image {} not found", image),
    }
}
