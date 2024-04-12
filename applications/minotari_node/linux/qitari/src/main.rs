// Experimental CLI for Tari setup and streamlinied console ui/ux
// Initially focused on Tor setup and configuration
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufRead, Write};
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use chrono::Local;
use clap::{App, Arg, AppSettings};
use std::os::unix::fs::PermissionsExt; 
//use backtrace; // for debug only

fn get_config_dir() -> io::Result<PathBuf> {
    env::var("HOME")
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "HOME environment variable not set"))
        .map(|home| PathBuf::from(home).join(".config/tor"))
}

const CONFIG_FILE: &str = "torrc";
const LOG_FILE: &str = "tor.log";

fn main() {
    let matches = App::new("qitari - Crypto may be hard, Tari doesn't have to be.")
        .version("1.0")
        .author("Tari and Friends")
        .about("Streamline Tari Setup, Configuration and Usage")
        .setting(AppSettings::ArgRequiredElseHelp)
        .arg(Arg::with_name("install").short("i").long("install").help("Install and configure Tor"))
        .arg(Arg::with_name("configure").short("c").long("configure").help("Configure Tor"))
        .arg(Arg::with_name("start").short("s").long("start").help("Start Tor"))
        .arg(Arg::with_name("logs").short("l").long("logs").help("Tail Tor logs"))
        .arg(Arg::with_name("update").short("u").long("update").help("Update Tor"))
        .arg(Arg::with_name("status").short("t").long("status").help("Show status of Tor service"))
        .arg(Arg::with_name("reset").short("r").long("reset").help("Reset or redo setup"))
        .arg(Arg::with_name("check").short("k").long("check").help("Check Tor configuration for potential issues"))
        .arg(Arg::with_name("monitor").short("m").long("monitor").help("Monitor Tor network status and performance"))
        .arg(Arg::with_name("optimize").short("o").long("optimize").help("Optimize Tor configuration for better performance"))
        .get_matches();

    let config_dir = get_config_dir().expect("Failed to get configuration directory");
    fs::create_dir_all(&config_dir).expect("Failed to create configuration directory");

    let log_file_path = config_dir.join(LOG_FILE);
    let mut log_file = File::create(&log_file_path).expect("Failed to create log file");

    let tor_port1 = 9050;
    let tor_port2 = 9051;

    if matches.is_present("install") {
        match install_tor() {
            Ok(_) => log(&mut log_file, "INFO", "Tor installation completed successfully.").unwrap(),
            Err(e) => {
                let error_message = format!("Failed to install Tor: {}", e);
                log(&mut log_file, "ERROR", &error_message).unwrap();
                eprintln!("Error: {}", error_message);
                eprintln!("Backtrace:\n{:?}", backtrace::Backtrace::new());
                std::process::exit(1);
            }
        }
        match configure_tor(&config_dir, tor_port1, tor_port2) {
            Ok(_) => log(&mut log_file, "INFO", "Tor configuration completed successfully.").unwrap(),
            Err(e) => {
                let error_message = format!("Failed to configure Tor: {:?} {} {} {}", config_dir, tor_port1, tor_port2, e);
                log(&mut log_file, "ERROR", &error_message).unwrap();
                eprintln!("Error: {}", error_message);
                eprintln!("Backtrace:\n{:?}", backtrace::Backtrace::new());
                std::process::exit(1);
            }
        }
    }

    if matches.is_present("start") {
        if !is_port_in_use(tor_port1) && !is_port_in_use(tor_port2) {
            launch_tor(&config_dir, &log_file_path, tor_port1, tor_port2).unwrap();
        } else {
            log(&mut log_file, "ERROR", "Tor is already running on the specified ports.").unwrap();
        }
    }

    if matches.is_present("logs") {
        tail_tor_logs(&log_file_path).unwrap();
    }

    if matches.is_present("update") {
        update_tor().unwrap();
    }

    if matches.is_present("status") {
        show_status(&config_dir, &log_file_path).unwrap();
    }
    if matches.is_present("reset") {
        reset_setup(&config_dir, 9050, 9051).unwrap_or_else(|e| {
            eprintln!("Failed to reset Tor setup: {}", e);
            std::process::exit(1);
        });
    }

    if matches.is_present("check") {
        check_tor_config(&config_dir).unwrap_or_else(|e| {
            eprintln!("Failed to check Tor configuration: {}", e);
            std::process::exit(1);
        });
    }
    

    if matches.is_present("monitor") {
        monitor_tor_network(&log_file_path).unwrap_or_else(|e| {
            eprintln!("Failed to monitor Tor network: {}", e);
            std::process::exit(1);
        });
    }

    if matches.is_present("optimize") {
        optimize_tor_config(&config_dir).unwrap();
    }
}

fn log(log_file: &mut File, level: &str, message: &str) -> io::Result<()> {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    writeln!(log_file, "{} [{}] {}", timestamp, level, message)
}

fn command_exists(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn is_port_in_use(port: u16) -> bool {
    TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).is_err()
}

fn install_tor() -> io::Result<()> {
    if !command_exists("tor") {
        if command_exists("apt-get") {
            log_command_execution("sudo", &["apt-get", "update", "-y"])?;
            log_command_execution("sudo", &["apt-get", "install", "-y", "tor", "tor-geoipdb", "torsocks"])?;
        } else if command_exists("dnf") {
            log_command_execution("sudo", &["dnf", "install", "-y", "tor", "tor-geoipdb", "torsocks"])?;
        } else if command_exists("pacman") {
            log_command_execution("sudo", &["pacman", "-Sy", "--noconfirm", "tor", "tor-geoipdb", "torsocks"])?;
        } else if command_exists("brew") {
            log_command_execution("brew", &["install", "tor", "torsocks"])?;
        } else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "No compatible package manager found to install Tor."));
        }
    }
    Ok(())
}


fn configure_tor(config_dir: &Path, tor_port1: u16, tor_port2: u16) -> io::Result<()> {
    println!("Running as user: {}", env::var("USER").unwrap_or_else(|_| "unknown user".to_string()));
    println!("Configuration directory: {:?}", config_dir);
    println!("Tor ports: {} (SOCKS), {} (Control)", tor_port1, tor_port2);

    let config_file_path = config_dir.join("torrc");

    println!("Configuration file will be located at: {}", config_file_path.display());

    if let Ok(metadata) = config_dir.metadata() {
        println!("Current permissions for the config directory: {:o}", metadata.permissions().mode() & 0o777);
    } else {
        println!("Failed to access metadata for configuration directory.");
    }

    println!("Stopping Tor service to apply new configurations.");
    Command::new("sudo").args(&["systemctl", "stop", "tor"]).status()?;

    let mut config_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&config_file_path)?;

    writeln!(config_file, "SocksPort 127.0.0.1:{}", tor_port1)?;
    writeln!(config_file, "ControlPort 127.0.0.1:{}", tor_port2)?;
    writeln!(config_file, "CookieAuthentication 0")?;
    writeln!(config_file, "ClientOnly 1")?;
    writeln!(config_file, "ClientUseIPv6 1")?;
    writeln!(config_file, "SafeLogging 1")?;  

    match fs::set_permissions(&config_file_path, fs::Permissions::from_mode(0o640)) {
        Ok(_) => println!("Permissions set to 640 for the configuration file."),
        Err(e) => {
            eprintln!("Failed to set permissions for the configuration file: {}", e);
            return Err(e);
        }
    };

    println!("Restarting Tor service to apply new configurations.");
    Command::new("sudo").args(&["systemctl", "start", "tor"]).status()?;

    println!("Configuration file has been updated and Tor service restarted. You can edit the settings any time at the specified path.");

    Ok(())
}

fn check_tor_config(config_dir: &Path) -> io::Result<()> {
    let config_file_path = config_dir.join("torrc");

    println!("Checking Tor configuration for potential issues...");
    println!("Configuration file being checked: {}", config_file_path.display());

    if !config_file_path.exists() {
        println!("Configuration file does not exist at the expected location: {}", config_file_path.display());
        return Err(io::Error::new(io::ErrorKind::NotFound, "Configuration file not found"));
    }

    let output = Command::new("tor")
        .args(&["--verify-config", "-f", config_file_path.to_str().unwrap()])
        .output()?;

    if !output.status.success() {
        println!("Tor configuration check failed:");
        println!("Output: {}", String::from_utf8_lossy(&output.stdout));
        println!("Errors: {}", String::from_utf8_lossy(&output.stderr));
        println!("\nDisplaying the contents of the configuration file for review:");
        display_file_contents(&config_file_path)?;
        return Err(io::Error::new(io::ErrorKind::Other, "Tor configuration validation failed"));
    }

    println!("Tor configuration check passed successfully.");
    Ok(())
}

fn display_file_contents(file_path: &Path) -> io::Result<()> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        println!("{}", line);
    }

    Ok(())
}

fn launch_tor(config_dir: &Path, log_file_path: &Path, tor_port1: u16, tor_port2: u16) -> io::Result<()> {
    let config_file_path = config_dir.join(CONFIG_FILE);
    let data_dir = config_dir.join("data");
    fs::create_dir_all(&data_dir)?;

    Command::new("tor")
        .args(&[
            "--clientonly", "1", 
            "--socksport", &tor_port1.to_string(), 
            "--controlport", &tor_port2.to_string(), 
            "--log", &format!("notice file {}", log_file_path.display()), 
            "--clientuseipv6", "1", 
            "--DataDirectory", &data_dir.display().to_string(), 
            "-f", &config_file_path.display().to_string()
        ])
        .status()?;

    Ok(())
}

fn tail_tor_logs(log_file_path: &Path) -> io::Result<()> {
    let file = File::open(log_file_path)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        println!("{}", line?);
    }
    Ok(())
}

fn update_tor() -> io::Result<()> {
    install_tor()
}

fn show_status(config_dir: &Path, log_file_path: &Path) -> io::Result<()> {
    let config_file_path = config_dir.join(CONFIG_FILE);

    println!("Tor Status Information:");
    println!("-----------------------");
    println!("Configuration file: {}", config_file_path.display());
    println!("Log file: {}", log_file_path.display());

    let file = File::open(&config_file_path)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line?;
        if line.contains("Port") {
            println!("Tor is running on ports: {}", line);
        }
    }

    let log_file = File::open(log_file_path)?;
    let log_reader = BufReader::new(log_file);
    println!("Logs snapshot:");
    for line in log_reader.lines().take(10) {
        println!("{}", line?);
    }

    Ok(())
}

fn reset_setup(config_dir: &Path, tor_port1: u16, tor_port2: u16) -> io::Result<()> {
    println!("Resetting Tor setup...");

    println!("Stopping Tor service...");
    Command::new("sudo").args(&["systemctl", "stop", "tor"]).status()?;
    println!("Reinstalling Tor...");
    install_tor()?;
    println!("Reconfiguring Tor...");
    configure_tor(config_dir, tor_port1, tor_port2)?;
    println!("Restarting Tor service...");
    Command::new("sudo").args(&["systemctl", "start", "tor"]).status()?;

    println!("Tor setup has been reset and restarted successfully.");
    Ok(())
}


fn log_command_execution(command: &str, args: &[&str]) -> io::Result<()> {
    let output = Command::new(command)
        .args(args)
        .output()?;

    if output.status.success() {
        log(&mut File::create(LOG_FILE)?, "INFO", &format!("Command executed successfully: {} {}", command, args.join(" ")))?;
    } else {
        log(&mut File::create(LOG_FILE)?, "ERROR", &format!("Command execution failed: {} {}", command, args.join(" ")))?;
        io::stdout().write_all(&output.stdout)?;
        io::stderr().write_all(&output.stderr)?;
    }

    Ok(())
}

fn monitor_tor_network(log_file_path: &Path) -> io::Result<()> {
    println!("Monitoring Tor -- Install nyx if you haven't for enhanced output.");
    if !log_file_path.exists() {
        println!("Log file does not exist: {}", log_file_path.display());
        return Err(io::Error::new(io::ErrorKind::NotFound, "Log file not found"));
    }
    println!("Tailing Tor log file: {}", log_file_path.display());
    let file = File::open(log_file_path)?;
    let reader = BufReader::new(file);

    for line in reader.lines().filter_map(|result| result.ok()) {
        println!("{}", line);
    }

    Ok(())
}


fn optimize_tor_config(config_dir: &Path) -> io::Result<()> {
    let config_file_path = config_dir.join(CONFIG_FILE);
    println!("Optimizing Tor configuration for better performance...");

    let mut config_file = OpenOptions::new()
        .write(true)
        .append(true)
        .open(&config_file_path)?;

    writeln!(config_file, "NumEntryGuards 3")?;
    writeln!(config_file, "NumDirectoryGuards 3")?;
    writeln!(config_file, "PreferTunnelledDirConns 1")?;
    writeln!(config_file, "AvoidDiskWrites 1")?;
    writeln!(config_file, "DisableAllSwap 1")?;

    println!("Tor configuration optimized.");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::remove_file;
    use std::path::Path;

    fn get_test_config_dir() -> PathBuf {
        let home_dir = env::var("HOME").unwrap();
        Path::new(&home_dir).join(".config/tor/tmp")
    }

    #[test]
    fn test_get_config_dir() {
        let config_dir = get_config_dir().unwrap();
        assert!(config_dir.exists());
        assert!(config_dir.is_dir());
    }

    #[test]
    fn test_log() {
        let log_file_path = Path::new("test_log.log");
        let mut log_file = File::create(&log_file_path).unwrap();
        log(&mut log_file, "INFO", "Test log message").unwrap();
        assert!(log_file_path.exists());
        remove_file(log_file_path).unwrap();
    }

    #[test]
    fn test_command_exists() {
        assert!(command_exists("ls"));
        assert!(!command_exists("non_existent_command"));
    }

    #[test]
    fn test_is_port_in_use() {
        assert!(!is_port_in_use(9999)); 
    }

    #[test]
    fn test_install_tor() {
        println!("Running as user: {}", env::var("USER").unwrap_or_else(|_| "unknown user".to_string()));
        assert!(install_tor().is_ok());
    }

    #[test]
    fn test_configure_tor() {
        println!("---- DO NOT BE ALARMED BY PORT CONFLICTS - NORMAL IDEAL RESPONSE IF TOR IS RUNNING ----");
        let config_dir = get_test_config_dir();
        fs::create_dir_all(&config_dir).unwrap();
        let tor_port1 = 9050;
        let tor_port2 = 9051;
        println!("Running as user: {}", env::var("USER").unwrap_or_else(|_| "unknown user".to_string()));
        assert!(configure_tor(&config_dir, tor_port1, tor_port2).is_ok());
        println!("Configuration file contents:");
        display_file_contents(&config_dir.join("torrc")).unwrap();
        println!("{}", "-".repeat(80));
    }

    #[test]
    fn test_check_tor_config() {
        println!("Running as user: {}", env::var("USER").unwrap_or_else(|_| "unknown user".to_string()));
        let config_dir = get_test_config_dir();
        fs::create_dir_all(&config_dir).unwrap();
        let config_file_path = config_dir.join("torrc");
        File::create(&config_file_path).unwrap();
        assert!(check_tor_config(&config_dir).is_ok());
    }

    #[test]
    fn test_display_file_contents() {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let file_path = temp_file.path();
        std::fs::write(file_path, "Test content").unwrap();
        assert!(display_file_contents(file_path).is_ok());
    }

    #[test]
    fn test_launch_tor() {
        let config_dir = get_test_config_dir();
        fs::create_dir_all(&config_dir).unwrap();
        let log_file_path = config_dir.join("tor.log");
        let tor_port1 = 9050;
        let tor_port2 = 9051;
        assert!(launch_tor(&config_dir, &log_file_path, tor_port1, tor_port2).is_ok());
    }

    #[test]
    fn test_tail_tor_logs() {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let log_file_path = temp_file.path();
        std::fs::write(log_file_path, "Test log content").unwrap();
        assert!(tail_tor_logs(log_file_path).is_ok());
    }

    #[test]
    fn test_update_tor() {
        assert!(update_tor().is_ok());
    }

    #[test]
    fn test_show_status() {
        let config_dir = get_test_config_dir();
        fs::create_dir_all(&config_dir).unwrap();
        let log_file_path = config_dir.join("tor.log");
        let config_file_path = config_dir.join("torrc");
        File::create(&log_file_path).unwrap();
        File::create(&config_file_path).unwrap();
        assert!(show_status(&config_dir, &log_file_path).is_ok());
    }

    #[test]
    fn test_reset_setup() {
        let config_dir = get_test_config_dir();
        fs::create_dir_all(&config_dir).unwrap();
        let tor_port1 = 9050;
        let tor_port2 = 9051;
        assert!(reset_setup(&config_dir, tor_port1, tor_port2).is_ok());
    }

    #[test]
    fn test_log_command_execution() {
        assert!(log_command_execution("echo", &["test"]).is_ok());
    }

    #[test]
    fn test_monitor_tor_network() {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let log_file_path = temp_file.path();
        std::fs::write(log_file_path, "Test log content").unwrap();
        assert!(monitor_tor_network(log_file_path).is_ok());
    }

    #[test]
    fn test_optimize_tor_config() {
        let config_dir = get_test_config_dir();
        fs::create_dir_all(&config_dir).unwrap();
        let config_file_path = config_dir.join("torrc");
        File::create(&config_file_path).unwrap();
        assert!(optimize_tor_config(&config_dir).is_ok());
    }
}
