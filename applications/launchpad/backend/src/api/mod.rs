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
