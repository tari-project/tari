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

/// ! This module defines all the Tauri commands we expose to the front-end.
/// ! These are generally constructed as wrappers around the lower-level methods in the `docker` module.
/// ! All the commands follow roughly the same pattern:
/// ! - handle input parameters
/// ! - call the the underlying function
/// ! - Map results to JSON and errors to String.
mod create_workspace;
mod events;
mod health_check;
mod host;
mod launch_docker;
mod pull_images;
mod service;
mod shutdown;
mod state;

pub use create_workspace::create_new_workspace;
pub use events::events;
pub use health_check::status;
pub use host::{check_docker, check_internet_connection, open_terminal};
pub use launch_docker::launch_docker;
pub use pull_images::{pull_image, pull_images, DEFAULT_IMAGES};
pub use service::{create_default_workspace, start_service, stop_service, ServiceSettings};
pub use shutdown::shutdown;
pub use state::AppState;
