#[cfg(feature = "winservice")]
#[cfg(target_os = "windows")]
fn main() -> windows_service::Result<()> {
    use std::{thread, time::Duration};
    use windows_service::{
        service::{ServiceAccess, ServiceState},
        service_manager::{ServiceManager, ServiceManagerAccess},
    };

    let manager_access = ServiceManagerAccess::CONNECT;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

    let service_access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;
    let service = service_manager.open_service("tari_merge_mining_proxy_service", service_access)?;

    let service_status = service.query_status()?;
    if service_status.current_state != ServiceState::Stopped {
        service.stop()?;
        // Wait for service to stop
        thread::sleep(Duration::from_secs(1));
    }

    service.delete()?;
    Ok(())
}

#[cfg(feature = "winservice")]
#[cfg(not(target_os = "windows"))]
fn main() {
    println!("No service to install");
}

#[cfg(not(feature = "winservice"))]
fn main() {
    println!("No service to install");
}
