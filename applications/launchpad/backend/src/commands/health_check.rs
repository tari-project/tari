use std::{convert::TryFrom, sync::Mutex};

use bollard::{
    container::{ListContainersOptions, StatsOptions},
    image::SearchImagesOptions,
    Docker,
};
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};

use crate::{
    commands::pull_images::DEFAULT_IMAGES,
    docker::{DockerWrapper, DockerWrapperError, ImageType},
};

#[derive(Serialize, Debug, Deserialize, Clone, Copy, PartialEq)]
pub enum DockerContainerStatus {
    Running,
    Stopped,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq)]
pub struct DockerImageStatus {
    pub(crate) image: ImageType,
    pub(crate) status: DockerContainerStatus,
}

impl DockerImageStatus {
    pub fn new(image: ImageType, status: DockerContainerStatus) -> Self {
        DockerImageStatus { image, status }
    }

    pub fn get_status(&self) -> DockerContainerStatus {
        self.status
    }
}
lazy_static! {
    static ref DOCKER_IMAGES: Mutex<Vec<DockerImageStatus>> = Mutex::new(
        DEFAULT_IMAGES
            .iter()
            .map(|image| DockerImageStatus::new(image.clone(), DockerContainerStatus::Stopped))
            .collect()
    );
}

fn set_image_status(new_image_stauts: DockerImageStatus) {
    let mut images = DOCKER_IMAGES.lock().unwrap();
    if !images.contains(&new_image_stauts) {
        let image_status_index = images
            .iter()
            .position(|image_status| image_status.image.eq(&new_image_stauts.image));

        if image_status_index.is_some() {
            images.remove(image_status_index.unwrap());
        }
        images.push(new_image_stauts);
    }
}

pub enum ContainerStatus {}

/// Default network is Dibbler. This will change after mainnet launch
impl Default for DockerContainerStatus {
    fn default() -> Self {
        Self::Stopped
    }
}

impl TryFrom<&str> for DockerContainerStatus {
    type Error = DockerWrapperError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "running" => Ok(DockerContainerStatus::Running),
            "stopped" => Ok(DockerContainerStatus::Stopped),
            _ => Err(DockerWrapperError::InvalidStatus),
        }
    }
}

#[tokio::test]
async fn search_image_test() {
    let docker = Docker::connect_with_local_defaults().unwrap();
    let found = docker
        .search_images(SearchImagesOptions {
            term: "quay.io/tarilabs/tari_base_node:latest",
            ..Default::default()
        })
        .await;

    let result = found.unwrap();
    println!("Found {:?}", result.len());
    result
        .iter()
        .filter(|i| i.name.as_ref().unwrap() == "tarilabs/tari_base_node")
        .for_each(|i| println!("-> {:?}", i));
}

#[tokio::test]
async fn search_container_test() {
    let docker = Docker::connect_with_local_defaults().unwrap();
    let found = docker
        .list_containers(Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        }))
        .await
        .unwrap();

    println!("Found {:?}", found.len());
    found.iter().for_each(|i| println!("state -> {:?}", i.state));
}

#[tokio::test]
async fn get_image_stats_test() {
    let docker = Docker::connect_with_local_defaults().unwrap();
    async move {
        print!("-->>");
        let stats = &docker
            .stats(
                "docker_rig-base_node-1",
                Some(StatsOptions {
                    stream: false,
                    ..Default::default()
                }),
            )
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        for stat in stats {
            println!(
                "{} - mem total: {:?} | mem usage: {:?} | cpu usage {:?}",
                stat.name, stat.memory_stats, stat.memory_stats.usage, stat.cpu_stats.cpu_usage,
            );
        }
    }
    .await;
    assert!(true);
}

#[test]
fn init_and_setting_tor_image_status_running_test() {
    assert_eq!(DEFAULT_IMAGES.len(), DOCKER_IMAGES.lock().unwrap().len());

    let tor_image_stopped = DockerImageStatus::new(ImageType::Tor, DockerContainerStatus::Stopped);
    let tor_image_running = DockerImageStatus::new(ImageType::Tor, DockerContainerStatus::Running);

    assert!(DOCKER_IMAGES.lock().unwrap().contains(&tor_image_stopped));
    assert!(!DOCKER_IMAGES.lock().unwrap().contains(&tor_image_running));

    set_image_status(tor_image_running);

    assert!(DOCKER_IMAGES.lock().unwrap().contains(&tor_image_running));
    assert!(!DOCKER_IMAGES.lock().unwrap().contains(&tor_image_stopped));
    assert_eq!(DEFAULT_IMAGES.len(), DOCKER_IMAGES.lock().unwrap().len());
}
