use serde::{Deserialize, Serialize};
use structopt::StructOpt;
use tari_common::{ConfigBootstrap, DefaultConfigLoader, NetworkConfigPath};

#[derive(StructOpt, Debug)]
/// The reference Tari cryptocurrency base node implementation
struct Arguments {
    /// Custom application parameters might eb specified as usual
    #[structopt(long, default_value = "any structopt options allowed")]
    my_param: String,
    #[structopt(flatten)]
    bootstrap: ConfigBootstrap,
}

// Following config does not require any keys customization
// and might be deserialized just as
// `let my_config: BasicConfig = config.try_into()?`
#[derive(Deserialize, Debug)]
struct BasicConfig {
    #[serde(default = "welcome")]
    welcome_message: String,
}
fn welcome() -> String {
    "welcome from tari_common".into()
}

// Following config is loading from key `my_node.{network}` where
// `{network} = my_node.use_network` parameter.
// This achieved with DefaultConfigLoader trait, which inhertis default impl
// when struct implements Serialize, Deserialize, Default and NetworkConfigPath.
// ```ignore
// let my_config = MyNodeConfig::try_from(&config)?
// ```
#[derive(Serialize, Deserialize, Debug)]
struct MyNodeConfig {
    welcome_message: String,
    goodbye_message: String,
}
impl Default for MyNodeConfig {
    fn default() -> Self {
        Self {
            welcome_message: welcome(),
            goodbye_message: "bye bye".into(),
        }
    }
}
impl NetworkConfigPath for MyNodeConfig {
    fn main_key_prefix() -> &'static str {
        "my_node"
    }
}

fn main() -> anyhow::Result<()> {
    Arguments::clap().print_help()?;
    let mut args = Arguments::from_args();
    args.bootstrap.init_dirs()?;
    println!("CLI arguments:\n");
    dbg!(&args);

    let mut config = args.bootstrap.load_configuration()?;

    // load basic config directly via Deserialize trait:
    let basic_config: BasicConfig = config.clone().try_into()?;
    assert_eq!(basic_config.welcome_message, welcome());

    let my_config: MyNodeConfig = MyNodeConfig::load_from(&config)?;
    assert_eq!(my_config.welcome_message, welcome());

    config.set("my_node.use_network", "mainnet")?;
    config.set("my_node.mainnet.welcome_message", "welcome from mainnet")?;

    let my_config = MyNodeConfig::load_from(&config)?;
    assert_eq!(my_config.welcome_message, "welcome from mainnet".to_string());
    assert_eq!(my_config.goodbye_message, "bye bye".to_string());

    Ok(())
}
