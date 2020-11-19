// Copyright 2020. The Tari Project
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
#![feature(type_alias_impl_trait)]
#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]

mod block_template_data;
mod error;
mod helpers;
mod proxy;
#[cfg(test)]
mod test;

use crate::{block_template_data::BlockTemplateRepository, error::MmProxyError};
use futures::future;
use hyper::{service::make_service_fn, Server};
use proxy::{MergeMiningProxyConfig, MergeMiningProxyService};
use std::convert::Infallible;
use structopt::StructOpt;
use tari_common::{configuration::bootstrap::ApplicationType, ConfigBootstrap, GlobalConfig};

#[cfg(target_os = "windows")]
use futures::FutureExt;
#[cfg(target_os = "windows")]
use std::{env, ffi::OsString, fs::File, io::Write, time::Duration};
#[cfg(target_os = "windows")]
use tari_shutdown::Shutdown;
#[cfg(target_os = "windows")]
use tokio::runtime::Runtime;
#[cfg(target_os = "windows")]
use windows_service::{
    self,
    define_windows_service,
    service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType},
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
    Result as ServiceResult,
};
#[cfg(target_os = "windows")]
const SERVICE_NAME: &str = "tari_merge_mining_proxy_service";
#[cfg(target_os = "windows")]
const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;
#[cfg(target_os = "windows")]
define_windows_service!(ffi_service_main, service_main);

#[cfg(target_os = "windows")]
fn log_service_detail(service_log: &mut File, detail: &str) {
    service_log.write_all(detail.as_bytes()).expect("unable to write");
    service_log.write_all("\r\n\r\n".as_bytes()).expect("unable to write");
}

#[cfg(target_os = "windows")]
pub fn service_main(_arguments: Vec<OsString>) {
    // Windows Services do not start in the executables location as with normal applications.
    // We need to set the current working dir back to the directory of the executable
    let mut dir = env::current_exe().unwrap();
    dir.pop(); // get rid of executable name from the path
    env::set_current_dir(&dir).unwrap();

    // Set default environment variables
    dir.pop();
    env::set_var("TARI_BASE_PATH", dir.to_str().unwrap());
    dir.push("config\\windows.toml");
    env::set_var("TARI_CONFIGURATION", dir.to_str().unwrap());
    dir.pop();
    dir.push("log4rs.yml");
    env::set_var("TARI_LOG_CONFIGURATION", dir.to_str().unwrap());

    // Create Service Log
    let mut service_log = File::create("tari_merge_mining_proxy_service.log").expect("unable to create file");
    log_service_detail(&mut service_log, "Starting Service");
    log_service_detail(
        &mut service_log,
        &*format!("TARI_BASE_PATH: {}", env::var("TARI_BASE_PATH").unwrap()),
    );
    log_service_detail(
        &mut service_log,
        &*format!("TARI_CONFIGURATION: {}", env::var("TARI_CONFIGURATION").unwrap()),
    );
    log_service_detail(
        &mut service_log,
        &*format!(
            "TARI_LOG_CONFIGURATION: {}",
            env::var("TARI_LOG_CONFIGURATION").unwrap()
        ),
    );
    log_service_detail(
        &mut service_log,
        &*format!(
            "Executable Directory: {}",
            env::current_dir().unwrap().to_str().unwrap()
        ),
    );

    if let Err(e) = run_service(&mut service_log) {
        log_service_detail(&mut service_log, &*format!("Service Encountered an Error: {:?}", e));
    }
}

#[cfg(target_os = "windows")]
pub fn run_service(service_log: &mut File) -> ServiceResult<()> {
    log_service_detail(service_log, "Creating Runtime");
    let mut rt = Runtime::new().unwrap();
    log_service_detail(service_log, "Creating Shutdown Signal");
    let mut shutdown = Shutdown::new();
    let signal = shutdown.to_signal();

    log_service_detail(service_log, "Creating event handler for SCM");
    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop => {
                let _result = shutdown.trigger();
                ServiceControlHandlerResult::NoError
            },
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    log_service_detail(service_log, "Registering event handler to SCM");
    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

    let mut next_status = ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    };
    log_service_detail(service_log, "Alerting SCM that service is running");
    status_handle.set_service_status(next_status)?;

    log_service_detail(service_log, "Initializing global config");
    let config = initialize().unwrap();
    log_service_detail(service_log, &*format!("Using global config: \r\n {:?}", config));

    let addr = config.proxy_host_address;
    log_service_detail(service_log, &*format!("Listening on {}...", addr));

    log_service_detail(service_log, "Initializing merge mining config from global config");
    let config = MergeMiningProxyConfig::from(config);
    log_service_detail(service_log, &*format!("Using merge mining config: \r\n {:?}", config));

    let xmrig_service = MergeMiningProxyService::new(config, BlockTemplateRepository::new());
    let service = make_service_fn(|_conn| future::ready(Result::<_, Infallible>::Ok(xmrig_service.clone())));

    log_service_detail(service_log, "Starting server");
    let _result = rt.block_on(async {
        let _result = Server::bind(&addr)
            .serve(service)
            .with_graceful_shutdown(signal.map(|_| ()))
            .await;
    });
    log_service_detail(service_log, "Stopping server");

    next_status = ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    };
    log_service_detail(service_log, "Alerting SCM that service is stopped");
    status_handle.set_service_status(next_status)?;

    log_service_detail(service_log, "Exiting normally");
    Ok(())
}

#[cfg(target_os = "windows")]
fn main() -> windows_service::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[tokio_macros::main]
async fn main() -> Result<(), MmProxyError> {
    // tracing_subscriber::fmt::init();
    let config = initialize()?;

    let addr = config.proxy_host_address;
    println!("Listening on {}...", addr);

    let config = MergeMiningProxyConfig::from(config);
    let xmrig_service = MergeMiningProxyService::new(config, BlockTemplateRepository::new());
    let service = make_service_fn(|_conn| future::ready(Result::<_, Infallible>::Ok(xmrig_service.clone())));

    Server::bind(&addr).serve(service).await?;

    Ok(())
}

/// Loads the configuration and sets up logging
fn initialize() -> Result<GlobalConfig, MmProxyError> {
    // Parse and validate command-line arguments
    let mut bootstrap = ConfigBootstrap::from_args();
    // Check and initialize configuration files
    bootstrap.init_dirs(ApplicationType::MergeMiningProxy)?;

    // Load and apply configuration file
    let cfg = bootstrap.load_configuration()?;

    #[cfg(feature = "envlog")]
    let _ = env_logger::try_init();
    // Initialise the logger
    #[cfg(not(feature = "envlog"))]
    bootstrap.initialize_logging()?;

    let cfg = GlobalConfig::convert_from(cfg)?;
    Ok(cfg)
}
