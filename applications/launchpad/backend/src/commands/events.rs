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

use tauri::{AppHandle, Manager, Wry};
use futures::StreamExt;
use crate::commands::AppState;
use log::*;
use crate::docker::ContainerId;

#[tauri::command]
// Return a Result until https://github.com/tauri-apps/tauri/issues/2533 is fixed
pub async fn events(app: AppHandle<Wry>) -> Result<(), ()> {
    info!("Setting up event stream");
    let state = app.state::<AppState>();
    let docker = state.docker.read().await;
    let mut stream = docker.events().await;
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event_result) = stream.next().await {
            match event_result {
                Ok(event) => {
                    debug!("Event received: {:?}", event);
                    let id = event.actor.as_ref()
                        .map(|a| a.id.as_ref())
                        .flatten()
                        .map(|s| s.as_str())
                        .unwrap_or("unknown");
                    let id = ContainerId(id.to_string());
                    if let Err(err) = app_clone.emit_all(event_name(), event) {
                        warn!("Could not emit event to front-end, {:?}", err);
                    }
                },
                Err(err) => { warn!("Error in event stream: {:#?}", err) },
            };
        }
        info!("Event stream has closed.");
    });
    Ok(())
}

/// Extract data from the event object so we know which channel to emit the payload to
pub fn event_name() -> &'static str {
    "tari://docker-system-event"
}