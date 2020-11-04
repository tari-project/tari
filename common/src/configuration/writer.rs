//! Configuration writer based on ConfigPath selectors.
//!
//! ## Example
//! ```
//! use config::Config;
//! use serde::{Deserialize, Serialize};
//! use tari_common::{
//!     configuration::writer::ConfigWriter,
//!     ConfigLoader,
//!     ConfigPath,
//!     ConfigurationError,
//!     NetworkConfigPath,
//! };
//! use toml::value::Value;
//!
//! #[derive(Serialize, Deserialize)]
//! struct MainConfig {
//!     name: String,
//! }
//! impl ConfigPath for MainConfig {
//!     fn main_key_prefix() -> &'static str {
//!         "main"
//!     }
//!
//!     fn overload_key_prefix(config: &Config) -> Result<Option<String>, ConfigurationError> {
//!         Ok(None)
//!     }
//! }
//! #[derive(Serialize, Deserialize)]
//! struct MyNodeConfig {
//!     port: u16,
//!     address: String,
//! }
//! impl NetworkConfigPath for MyNodeConfig {
//!     fn main_key_prefix() -> &'static str {
//!         "my_node"
//!     }
//! }
//! let main_config = MainConfig {
//!     name: "test_server".to_string(),
//! };
//! let node_config = MyNodeConfig {
//!     port: 3001,
//!     address: "localhost".to_string(),
//! };
//! // Merging configs into resulting structure, accounting preset use_network params
//! let mut config = Config::new();
//! config.set(&MyNodeConfig::network_config_key(), "rincewind");
//! main_config.merge_into(&mut config).unwrap();
//! node_config.merge_into(&mut config).unwrap();
//!
//! let toml_value: Value = config.try_into().unwrap();
//! let res = toml::to_string(&toml_value).unwrap();
//! assert_eq!(
//!     res,
//!     r#"[main]
//! name = "test_server"
//!
//! [my_node]
//! use_network = "rincewind"
//!
//! [my_node.rincewind]
//! address = "localhost"
//! port = 3001
//! "#
//! );
//! ```
use super::{loader::ConfigPath, ConfigurationError};
use config::Config;

/// Configuration writer based on ConfigPath selectors
///
/// It is autoimplemented for types implementing [`ConfigPath`] and [`serde::ser::Serialize`]
/// Refer to [module](crate::configuration::writer) documentation for example
pub trait ConfigWriter: ConfigPath + serde::ser::Serialize {
    /// Merges structure into configuration by `main_key_prefix()`
    /// with values overloaded from `overload_key_prefix()`.
    fn merge_into(&self, config: &mut Config) -> Result<(), ConfigurationError> {
        use serde::de::Deserialize;
        let overload = Self::overload_key_prefix(&config)?;
        let key = match overload.as_deref() {
            Some(v) => v,
            None => Self::main_key_prefix(),
        };
        let value = config::Value::deserialize(serde_json::to_value(self)?)?;
        config.set(key, value)?;
        Ok(())
    }
}
impl<C> ConfigWriter for C where C: ConfigPath + serde::ser::Serialize {}
