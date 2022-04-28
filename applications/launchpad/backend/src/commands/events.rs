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

use std::collections::HashMap;

use bollard::{models::SystemEventsResponse, system::EventsOptions, Docker};
use futures::{Stream, StreamExt, TryStreamExt};
use log::*;
use tauri::{AppHandle, Manager, Wry};

use crate::{
    commands::{pull_images::DOCKER, AppState},
    docker::DockerWrapperError,
    error::LauncherError,
};

pub async fn stream_docker_events<F>(fun: F) -> Result<(), LauncherError>
where F: Fn(SystemEventsResponse) -> Result<(), tauri::Error> + Copy {
    let mut streams = stream_events(&DOCKER).await;
    docker_events(fun, streams).await?;
    Ok(())
}

pub async fn docker_events<F>(
    f: F,
    mut stream: impl Stream<Item = Result<SystemEventsResponse, bollard::errors::Error>> + Unpin,
) -> Result<(), LauncherError>
where
    F: Fn(SystemEventsResponse) -> Result<(), tauri::Error> + Copy,
{
    info!("Setting up event stream");
    while let Some(event_result) = stream.next().await {
        match event_result {
            Ok(event) => f(event)?,
            Err(err) => {
                warn!("Error in event stream: {:#?}", err)
            },
        };
    }
    info!("Event stream has closed.");
    Ok(())
}

/// Extract data from the event object so we know which channel to emit the payload to
pub fn event_name() -> &'static str {
    "tari://docker-system-event"
}

async fn stream_events(docker: &Docker) -> impl Stream<Item = Result<SystemEventsResponse, bollard::errors::Error>> {
    let mut type_filter = HashMap::new();
    type_filter.insert("type".to_string(), vec![
        "container".to_string(),
        "image".to_string(),
        "network".to_string(),
        "volume".to_string(),
    ]);
    let options = EventsOptions {
        since: None,
        until: None,
        filters: type_filter,
    };
    docker.events(Some(options))
}
