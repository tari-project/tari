#[cfg(feature = "winservice")]
#[cfg(target_os = "windows")]
fn main() -> windows_service::Result<()> {
    use std::ffi::OsString;
    use windows_service::{
        service::{ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceType},
        service_manager::{ServiceManager, ServiceManagerAccess},
    };

    let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

    let service_binary_path = ::std::env::current_exe()
        .unwrap()
        .with_file_name("tari_merge_mining_proxy_service.exe");

    let service_info = ServiceInfo {
        name: OsString::from("tari_merge_mining_proxy_service"),
        display_name: OsString::from("Tari Merge Mining Proxy"),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::OnDemand,
        error_control: ServiceErrorControl::Normal,
        executable_path: service_binary_path,
        launch_arguments: vec![],
        dependencies: vec![],
        account_name: None, // run as System
        account_password: None,
    };
    let service = service_manager.create_service(&service_info, ServiceAccess::CHANGE_CONFIG)?;
    service.set_description("Monero Merge Mining Proxy for Tari Cryptocurrency")?;
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
