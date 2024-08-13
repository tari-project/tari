// Copyright 2019. The Tari Project
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
//! # Application configuration
//!
//! Tari is using config crate which allows to extend config file with application level configs.
//! To allow deriving configuration from a Config via [`ConfigLoader`] trait application configuration
//! struct should implements [`Deserialize`][serde::Deserialize] and [`SubConfigPath`] traits.
//!
//! [`ConfigLoader::load_from`] logic will include automated overloading of parameters from [application.{network}]
//! subsection, where network is specified in `application.network` parameter.
//!
//! [`ConfigPath`] allows to customize overloading logic event further and [`DefaultConfigLoader`] trait accounts
//! for struct [`Default`]s when loading values.
//!
//! ## Example
//!
//! ```
//! # use config::Config;
//! # use serde::{Deserialize, Serialize};
//! # use tari_common::{SubConfigPath, DefaultConfigLoader};
//! #[derive(Deserialize, Serialize, Default)]
//! struct MyNodeConfig {
//!     welcome_message: String,
//! }
//! impl SubConfigPath for MyNodeConfig {
//!     fn main_key_prefix() -> &'static str {
//!         "my_node"
//!     }
//! }
//!
//! # let mut config = Config::builder()
//! #    .set_override("my_node.override_from", "weatherwax").unwrap()
//! #    .set_override("weatherwax.my_node.welcome_message", "nice to see you at unseen").unwrap().build().unwrap();
//! let my_config = MyNodeConfig::load_from(&config).unwrap();
//! assert_eq!(my_config.welcome_message, "nice to see you at unseen");
//! ```

use std::{
    error::Error,
    fmt::{Display, Formatter},
};

use config::{Config, ValueKind};

//-------------------------------------------    ConfigLoader trait    ------------------------------------------//

/// Load struct from config's main section and subsection override
///
/// Implementation of this trait along with Deserialize grants `ConfigLoader` implementation
pub trait ConfigPath {
    /// Main configuration section
    fn main_key_prefix() -> &'static str;
    /// Overload values from a key prefix based on some configuration value.
    ///
    /// Should return a path to configuration table with overloading values.
    /// Returns `ConfigurationError` if key_prefix field has wrong value.
    /// Returns Ok(None) if no overload is required
    fn overload_key_prefix(config: &Config) -> Result<Option<String>, ConfigurationError>;
    /// Merge and produce sub-config from overload_key_prefix to main_key_prefix,
    /// which can be used to deserialize Self struct
    /// If overload key is not present in config it won't make effect
    fn merge_subconfig(config: &Config, defaults: config::Value) -> Result<Config, ConfigurationError> {
        match Self::overload_key_prefix(config)? {
            Some(key) => {
                let overload: config::Value = config.get(key.as_str()).unwrap_or_default();
                let mut config = Config::builder()
                    .set_default(Self::main_key_prefix(), defaults)?
                    .add_source(config.clone());
                // If the override is not set, ignore it
                if !matches!(overload.kind, ValueKind::Nil) {
                    config = config.set_override(Self::main_key_prefix(), overload)?;
                }

                let config = config.build()?;
                Ok(config)
            },
            None => {
                let config = Config::builder()
                    .set_default(Self::main_key_prefix(), defaults)?
                    .add_source(config.clone())
                    .build()?;
                Ok(config)
            },
        }
    }
}

/// Load struct from config's main section and subsection override
///
/// The subsection will be chosen based on `config` key value
/// from the main section defined in this trait.
///
/// So for example
///
/// ```toml
/// [section]
/// config = "local"
///
/// [section.local]
/// selected = true
///
/// [section.remote]
/// selected = false
/// ```
pub trait SubConfigPath {
    /// Main configuration section
    fn main_key_prefix() -> &'static str;
    /// Path for `override_from` key in config
    fn subconfig_key() -> String {
        let main = <Self as SubConfigPath>::main_key_prefix();
        format!("{}.override_from", main)
    }
}
impl<C: SubConfigPath> ConfigPath for C {
    /// Returns the string representing the top level configuration category.
    /// For example, in the following TOML file, options for `main_key_prefix` would be `MainKeyOne` or `MainKeyTwo`:
    /// ```toml
    /// [MainKeyOne]
    ///   subkey1=1
    /// [MainKeyTwo]
    ///   subkey2=1
    /// ```
    fn main_key_prefix() -> &'static str {
        <Self as SubConfigPath>::main_key_prefix()
    }

    /// Loads the desired subsection from the config file into the provided `config` and merges the results. The
    /// subsection that is selected for merging is determined by the value of the `network` sub key of the "main"
    /// section. For example, if a TOML configuration file contains the following:
    ///
    /// ```toml
    /// [SectionA]
    ///   network=foo
    ///   subkey=1
    /// [SectionA.foo]
    ///   subkey=2
    /// [SectionA.baz]
    ///   subkey=3
    /// ```
    ///
    /// the result after calling `merge_config` would have the struct's `subkey` value set to 2. If `network`
    /// were omitted, `subkey` would be 1, and if `network` were set to `baz`, `subkey` would be 3.
    fn overload_key_prefix(config: &Config) -> Result<Option<String>, ConfigurationError> {
        let subconfig_key = Self::subconfig_key();
        let network_val: Option<String> = config
            .get_string(subconfig_key.as_str())
            .ok()
            .map(|network| format!("{}.{}", network, Self::main_key_prefix()));
        Ok(network_val)
    }
}

/// Configuration loader based on ConfigPath selectors
///
/// ```
/// # use config::Config;
/// # use serde::{Deserialize, Serialize};
/// use tari_common::{DefaultConfigLoader, SubConfigPath};
///
/// #[derive(Deserialize, Serialize)]
/// struct MyNodeConfig {
///     #[serde(default = "welcome")]
///     welcome_message: String,
///     #[serde(default = "bye")]
///     goodbye_message: String,
/// }
/// impl Default for MyNodeConfig {
///     fn default() -> Self {
///         Self {
///             welcome_message: welcome(),
///             goodbye_message: bye(),
///         }
///     }
/// }
/// fn welcome() -> String {
///     "welcome to tari".into()
/// }
/// fn bye() -> String {
///     "bye bye".into()
/// }
/// impl SubConfigPath for MyNodeConfig {
///     fn main_key_prefix() -> &'static str {
///         "my_node"
///     }
/// }
/// // Loading preset and serde default value
/// let mut config = Config::builder().build().unwrap();
/// config.set("my_node.goodbye_message", "see you later");
/// config.set("mainnet.my_node.goodbye_message", "see you soon");
/// let my_config = MyNodeConfig::load_from(&config).unwrap();
/// assert_eq!(my_config.goodbye_message, "see you later".to_string());
/// assert_eq!(my_config.welcome_message, welcome());
/// // Overloading from network subsection as we use SubConfigPath
/// config.set("my_node.override_from", "mainnet");
/// let my_config = MyNodeConfig::load_from(&config).unwrap();
/// assert_eq!(my_config.goodbye_message, "see you soon".to_string());
/// ```
pub trait ConfigLoader: ConfigPath + Sized {
    /// Try to load configuration from supplied Config by `main_key_prefix()`
    /// with values overloaded from `overload_key_prefix()`. For automated inheritance of Default values use
    /// DefaultConfigLoader
    ///
    /// Default values will be taken from
    /// - `#[serde(default="value")]` field attribute
    /// - value defined in Config::set_default()
    fn load_from(config: &Config) -> Result<Self, ConfigurationError>;
}

/// Configuration loader based on ConfigPath selectors with Defaults
///
/// ```
/// use config::Config;
/// use serde::{Deserialize, Serialize};
/// use tari_common::{DefaultConfigLoader, SubConfigPath};
///
/// #[derive(Serialize, Deserialize)]
/// struct MyNodeConfig {
///     welcome_message: String,
///     goodbye_message: String,
/// }
/// impl Default for MyNodeConfig {
///     fn default() -> Self {
///         Self {
///             welcome_message: "welcome from tari".into(),
///             goodbye_message: "bye bye".into(),
///         }
///     }
/// }
/// impl SubConfigPath for MyNodeConfig {
///     fn main_key_prefix() -> &'static str {
///         "my_node"
///     }
/// }
/// let mut config = Config::builder().build().unwrap();
/// config.set("my_node.goodbye_message", "see you later");
/// let my_config = MyNodeConfig::load_from(&config).unwrap();
/// assert_eq!(my_config.goodbye_message, "see you later".to_string());
/// assert_eq!(
///     my_config.welcome_message,
///     MyNodeConfig::default().welcome_message
/// );
/// ```
pub trait DefaultConfigLoader: ConfigPath + Sized {
    /// Try to load configuration from supplied Config by `main_key_prefix()`
    /// with values overloaded from `overload_key_prefix()`.
    ///
    /// Default values will be taken from Default impl for the struct.
    fn load_from(config: &Config) -> Result<Self, ConfigurationError>;
}

impl<C> DefaultConfigLoader for C
where C: ConfigPath + Default + serde::ser::Serialize + for<'de> serde::de::Deserialize<'de>
{
    fn load_from(config: &Config) -> Result<Self, ConfigurationError> {
        let default = <Self as Default>::default();
        let buf = serde_json::to_value(&default)?;
        let value: config::Value = serde_json::from_value(buf)?;
        let merger = Self::merge_subconfig(config, value)?;
        let final_value: config::Value = merger.get(Self::main_key_prefix())?;
        final_value
            .try_deserialize()
            .map_err(|ce| ConfigurationError::new(Self::main_key_prefix(), None, ce.to_string()))
    }
}

//-------------------------------------      Configuration errors      --------------------------------------//

#[derive(Debug)]
pub struct ConfigurationError {
    field: String,
    value: Option<String>,
    message: String,
}

impl ConfigurationError {
    pub fn new<F: Into<String>, M: Into<String>>(field: F, value: Option<String>, msg: M) -> Self {
        ConfigurationError {
            field: field.into(),
            value,
            message: msg.into(),
        }
    }
}

impl Display for ConfigurationError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        match &self.value {
            Some(v) => write!(f, "Invalid value `{}` for {}: {}", v, self.field, self.message),
            None => write!(f, "Invalid value for `{}`: {}", self.field, self.message),
        }
    }
}

impl Error for ConfigurationError {}
impl From<config::ConfigError> for ConfigurationError {
    fn from(err: config::ConfigError) -> Self {
        use config::ConfigError;
        match err {
            ConfigError::FileParse { uri, cause } if uri.is_some() => Self {
                field: uri.unwrap(),
                value: None,
                message: cause.to_string(),
            },
            ConfigError::Type {
                ref unexpected,
                ref key,
                ..
            } => Self {
                field: format!("{:?}", key),
                value: Some(unexpected.to_string()),
                message: err.to_string(),
            },
            ConfigError::NotFound(key) => Self {
                field: key,
                value: None,
                message: "required key not found".to_string(),
            },
            x => Self::new("", None, x.to_string().as_str()),
        }
    }
}
impl From<serde_json::error::Error> for ConfigurationError {
    fn from(err: serde_json::error::Error) -> Self {
        Self {
            field: "".to_string(),
            value: None,
            message: err.to_string(),
        }
    }
}

#[cfg(test)]
mod test {
    use config::ConfigError;

    #[test]
    fn configuration_error() {
        let e = ConfigurationError::new("test", None, "is a string");
        assert_eq!(e.to_string(), "Invalid value for `test`: is a string");

        let frozen_e = ConfigurationError::from(ConfigError::Frozen);
        assert_eq!(frozen_e.to_string(), "Invalid value for ``: configuration is frozen");
    }

    use serde::{Deserialize, Serialize};

    use super::*;

    // test SubConfigPath both with Default and without Default
    #[allow(dead_code)]
    #[derive(Serialize, Deserialize)]
    struct SubTari {
        monero: String,
    }
    impl Default for SubTari {
        fn default() -> Self {
            Self {
                monero: "isprivate".into(),
            }
        }
    }

    #[allow(dead_code)]
    #[derive(Default, Serialize, Deserialize)]
    struct SuperTari {
        #[serde(flatten)]
        pub within: SubTari,
        pub over: SubTari,
        #[serde(default = "serde_default_string")]
        bitcoin: String,
    }
    #[allow(dead_code)]
    fn serde_default_string() -> String {
        "ispublic".into()
    }
    impl SubConfigPath for SuperTari {
        fn main_key_prefix() -> &'static str {
            "crypto"
        }
    }

    // test ConfigPath reading only from main section
    #[derive(Serialize, Deserialize)]
    struct OneConfig {
        param1: String,
        #[serde(default = "param2_serde_default")]
        param2: String,
    }
    impl Default for OneConfig {
        fn default() -> Self {
            Self {
                param1: "param1".into(),
                param2: "param2".into(),
            }
        }
    }
    fn param2_serde_default() -> String {
        "alwaysset".into()
    }
    impl ConfigPath for OneConfig {
        fn main_key_prefix() -> &'static str {
            "one"
        }

        fn overload_key_prefix(_: &Config) -> Result<Option<String>, ConfigurationError> {
            Ok(None)
        }
    }

    #[test]
    fn config_loaders() -> anyhow::Result<()> {
        let config = Config::default();

        // no network value
        // [ ] one.param1(default) [X] one.param1(default) [ ] one.param2 [X] one.param2(default)
        let one = <OneConfig as DefaultConfigLoader>::load_from(&config)?;
        assert_eq!(one.param1, OneConfig::default().param1);
        assert_eq!(one.param2, OneConfig::default().param2);

        let config = Config::builder()
            .add_source(config)
            .set_override("one.param1", "can load from main section")
            .unwrap()
            .build()
            .unwrap();
        let one = <OneConfig as DefaultConfigLoader>::load_from(&config)?;
        assert_eq!(one.param1, "can load from main section");
        assert_eq!(one.param2, "param2");

        Ok(())
    }
}
