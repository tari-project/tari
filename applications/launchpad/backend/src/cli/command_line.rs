use core::time;
use std::{collections::HashMap, iter::Map, thread::sleep};

use bollard::{
    container::{ListContainersOptions, LogOutput, LogsOptions, StatsOptions},
    errors::Error,
    models::CreateImageInfo,
    system::EventsOptions,
    Docker,
};
use futures::{Stream, StreamExt, TryStreamExt};
use log::{debug, error, warn};
use serde::Serialize;
use thiserror::Error;
use tokio::io::AsyncReadExt;

use crate::{docker::LogMessage, DockerWrapper};

pub type DockerComposeStatus<T> = std::result::Result<T, DockerStatusError>;

pub fn docker_compose_up() -> DockerComposeStatus<String> {
    let _create_tr_dir = std::process::Command::new("mkdir")
        .arg("-p")
        .arg("~/launchpad/config/tor")
        .status()
        .expect("something went wrong");

    let _status = std::process::Command::new("docker-compose")
        .env("DATA_FOLDER", "~/launchpad/config")
        .arg("-f")
        .arg("../docker_rig/docker-compose.yml")
        .arg("up")
        .arg("tor")
        .arg("-d")
        .status()
        .expect("something went wrong");
    sleep(time::Duration::from_secs(5));

    let status = std::process::Command::new("docker-compose")
        .env("DATA_FOLDER", "~/launchpad/config")
        .env("TARI_NETWORK", "dibbler")
        .arg("-f")
        .arg("../docker_rig/docker-compose.yml")
        .arg("up")
        .arg("base_node")
        .arg("-d")
        .status()
        .expect("something went wrong");

    let whatever = status.to_string();
    Ok(whatever)
}

#[derive(Debug, Serialize, PartialEq)]
#[allow(warnings)]
pub struct DockerStatusError {
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Payload {
    image: String,
    name: String,
    info: CreateImageInfo,
}
#[test]
fn docker_compose_test() {
    let _ = docker_compose_up();
}

#[tokio::test]
async fn show_container_stats_test() {
    let docker = Docker::connect_with_local_defaults().unwrap();
    async move {
        print!("-->>");
        let stats = &docker
            .stats(
                "9df417fc59a9",
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

#[tokio::test]
async fn print_container_logs() {
    println!("start");
    let docker = Docker::connect_with_local_defaults().unwrap();
    let options = LogsOptions::<String> {
        follow: true,
        stdout: true,
        stderr: true,
        ..Default::default()
    };
    let id = "config-tor-1";
    let mut logs = docker.logs(id, Some(options.clone()));
    while let Some(Ok(msg)) = logs.next().await {
        println!("msg {:?}", msg);
    }
    // loop{
    //     let msg = logs
    //         .next()
    //         .await;
    //     println!("msg {:?}", msg);
    //     counter = counter + 1;
    //     if counter > 20 {
    //         break;
    //     } else {
    //         continue
    //     }
    // }

    assert!(true);
}

#[tokio::test]
async fn container_id_test() {
    let status = std::process::Command::new("docker-compose")
        .env("DATA_FOLDER", "/Users/matkat/launchpad/config")
        .env("TARI_NETWORK", "dibbler")
        .env("TARI_MONEROD_PASSWORD", "tari")
        .env("TARI_MONEROD_USERNAME", "tari")
        .arg("-f")
        .arg("/Users/matkat/launchpad/config/docker-compose.yml")
        .arg("ps")
        .arg("-a")
        .arg("-q")
        .output()
        .expect("something went wrong");
    let container_id  = String::from_utf8_lossy(&status.stdout).replace("\n", "");
    println!("output: {:?}", container_id);        
}
