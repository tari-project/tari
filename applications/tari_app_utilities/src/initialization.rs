use crate::{consts, utilities::ExitCodes};
use config::Config;
use structopt::StructOpt;
use tari_common::{configuration::bootstrap::ApplicationType, ConfigBootstrap, GlobalConfig};

pub const LOG_TARGET: &str = "tari::application";

pub fn init_configuration(
    application_type: ApplicationType,
) -> Result<(ConfigBootstrap, GlobalConfig, Config), ExitCodes> {
    // Parse and validate command-line arguments
    let mut bootstrap = ConfigBootstrap::from_args();

    // Check and initialize configuration files
    bootstrap.init_dirs(application_type)?;

    // Load and apply configuration file
    let cfg = bootstrap.load_configuration()?;

    // Initialise the logger
    bootstrap.initialize_logging()?;

    log::info!(target: LOG_TARGET, "{} ({})", application_type, consts::APP_VERSION);

    // Populate the configuration struct
    let global_config =
        GlobalConfig::convert_from(cfg.clone()).map_err(|err| ExitCodes::ConfigError(err.to_string()))?;
    Ok((bootstrap, global_config, cfg))
}
