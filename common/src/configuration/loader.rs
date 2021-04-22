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
//! struct should implements [`Deserialize`][serde::Deserialize] and [`NetworkConfigPath`] traits.
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
//! # use serde::{Deserialize};
//! # use tari_common::{NetworkConfigPath, ConfigLoader};
//! #[derive(Deserialize)]
//! struct MyNodeConfig {
//!     welcome_message: String,
//! }
//! impl NetworkConfigPath for MyNodeConfig {
//!     fn main_key_prefix() -> &'static str {
//!         "my_node"
//!     }
//! }
//!
//! # let mut config = Config::new();
//! config.set("my_node.network", "rincewind");
//! config.set("my_node.rincewind.welcome_message", "nice to see you at unseen");
//! let my_config = <MyNodeConfig as ConfigLoader>::load_from(&config).unwrap();
//! assert_eq!(my_config.welcome_message, "nice to see you at unseen");
//! ```

use super::Network;
use config::Config;
use std::{
    error::Error,
    fmt::{Display, Formatter},
};

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
    fn merge_subconfig(config: &Config) -> Result<Config, ConfigurationError> {
        use config::Value;
        match Self::overload_key_prefix(config)? {
            Some(key) => {
                let overload: Value = config.get(key.as_str()).unwrap_or_default();
                let base: Value = config.get(Self::main_key_prefix()).unwrap_or_default();
                let mut base_config = Config::new();
                base_config.set(Self::main_key_prefix(), base)?;
                let mut config = Config::new();
                // Some magic is required to make them correctly merge
                config.merge(base_config)?;
                config.set(Self::main_key_prefix(), overload)?;
                Ok(config)
            },
            None => Ok(config.clone()),
        }
    }
}

/// Load struct from config's main section and network subsection override
///
/// Network subsection will be chosen based on `network` key value
/// from the main section defined in this trait.
///
/// Wrong network value will result in Error
pub trait NetworkConfigPath {
    /// Main configuration section
    fn main_key_prefix() -> &'static str;
    /// Path for `network` key in config
    fn network_config_key() -> String {
        let main = <Self as NetworkConfigPath>::main_key_prefix();
        format!("{}.network", main)
    }
}
impl<C: NetworkConfigPath> ConfigPath for C {
    /// Returns the string representing the top level configuration category.
    /// For example, in the following TOML file, options for `main_key_prefix` would be `MainKeyOne` or `MainKeyTwo`:
    /// ```toml
    /// [MainKeyOne]
    ///   subkey1=1
    /// [MainKeyTwo]
    ///   subkey2=1
    /// ```
    fn main_key_prefix() -> &'static str {
        <Self as NetworkConfigPath>::main_key_prefix()
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
        let network_key = Self::network_config_key();
        let network_val: Option<String> = config.get_str(network_key.as_str()).ok();
        if let Some(s) = network_val {
            let network: Network = s.parse()?;
            Ok(Some(format!("{}.{}", Self::main_key_prefix(), network)))
        } else {
            Ok(None)
        }
    }
}

/// Configuration loader based on ConfigPath selectors
///
/// ```
/// # use config::Config;
/// # use serde::{Deserialize};
/// use tari_common::{ConfigLoader, NetworkConfigPath};
///
/// #[derive(Deserialize)]
/// struct MyNodeConfig {
///     #[serde(default = "welcome")]
///     welcome_message: String,
///     #[serde(default = "bye")]
///     goodbye_message: String,
/// }
/// fn welcome() -> String {
///     "welcome to tari".into()
/// }
/// fn bye() -> String {
///     "bye bye".into()
/// }
/// impl NetworkConfigPath for MyNodeConfig {
///     fn main_key_prefix() -> &'static str {
///         "my_node"
///     }
/// }
/// // Loading preset and serde default value
/// let mut config = Config::new();
/// config.set("my_node.goodbye_message", "see you later");
/// config.set("my_node.mainnet.goodbye_message", "see you soon");
/// let my_config = <MyNodeConfig as ConfigLoader>::load_from(&config).unwrap();
/// assert_eq!(my_config.goodbye_message, "see you later".to_string());
/// assert_eq!(my_config.welcome_message, welcome());
/// // Overloading from network subsection as we use NetworkConfigPath
/// config.set("my_node.network", "mainnet");
/// let my_config = <MyNodeConfig as ConfigLoader>::load_from(&config).unwrap();
/// assert_eq!(my_config.goodbye_message, "see you soon".to_string());
/// ```
pub trait ConfigLoader: ConfigPath + for<'de> serde::de::Deserialize<'de> {
    /// Try to load configuration from supplied Config by `main_key_prefix()`
    /// with values overloaded from `overload_key_prefix()`.
    ///
    /// Default values will be taken from
    /// - `#[serde(default="value")]` field attribute
    /// - value defined in Config::set_default()
    /// For automated inheritance of Default values use DefaultConfigLoader.
    fn load_from(config: &Config) -> Result<Self, ConfigurationError> {
        let merger = Self::merge_subconfig(config)?;
        Ok(merger.get(Self::main_key_prefix())?)
    }
}
impl<C> ConfigLoader for C where C: ConfigPath + for<'de> serde::de::Deserialize<'de> {}

/// Configuration loader based on ConfigPath selectors with Defaults
///
/// ```
/// use config::Config;
/// use serde::{Deserialize, Serialize};
/// use tari_common::{DefaultConfigLoader, NetworkConfigPath};
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
/// impl NetworkConfigPath for MyNodeConfig {
///     fn main_key_prefix() -> &'static str {
///         "my_node"
///     }
/// }
/// let mut config = Config::new();
/// config.set("my_node.goodbye_message", "see you later");
/// let my_config = <MyNodeConfig as DefaultConfigLoader>::load_from(&config).unwrap();
/// assert_eq!(my_config.goodbye_message, "see you later".to_string());
/// assert_eq!(my_config.welcome_message, MyNodeConfig::default().welcome_message);
/// ```
pub trait DefaultConfigLoader:
    ConfigPath + Default + serde::ser::Serialize + for<'de> serde::de::Deserialize<'de>
{
    /// Try to load configuration from supplied Config by `main_key_prefix()`
    /// with values overloaded from `overload_key_prefix()`.
    ///
    /// Default values will be taken from Default impl for struct
    fn load_from(config: &Config) -> Result<Self, ConfigurationError> {
        let default = <Self as Default>::default();
        let buf = serde_json::to_string(&default)?;
        let value: config::Value = serde_json::from_str(buf.as_str())?;
        let mut merger = Self::merge_subconfig(config)?;
        merger.set_default(Self::main_key_prefix(), value)?;
        Ok(merger.get(Self::main_key_prefix())?)
    }
}
impl<C> DefaultConfigLoader for C where C: ConfigPath + Default + serde::ser::Serialize + for<'de> serde::de::Deserialize<'de>
{}

//-------------------------------------      Configuration errors      --------------------------------------//

#[derive(Debug)]
pub struct ConfigurationError {
    field: String,
    message: String,
}

impl ConfigurationError {
    pub fn new(field: &str, msg: &str) -> Self {
        ConfigurationError {
            field: String::from(field),
            message: String::from(msg),
        }
    }
}

impl Display for ConfigurationError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "Invalid value for {}: {}", self.field, self.message)
    }
}

impl Error for ConfigurationError {}
impl From<config::ConfigError> for ConfigurationError {
    fn from(err: config::ConfigError) -> Self {
        use config::ConfigError;
        match err {
            ConfigError::FileParse { uri, cause } if uri.is_some() => Self {
                field: uri.unwrap(),
                message: cause.to_string(),
            },
            ConfigError::Type { ref key, .. } => Self {
                field: format!("{:?}", key),
                message: err.to_string(),
            },
            ConfigError::NotFound(key) => Self {
                field: key,
                message: "required key not found".to_string(),
            },
            x => Self::new("", x.to_string().as_str()),
        }
    }
}
impl From<serde_json::error::Error> for ConfigurationError {
    fn from(err: serde_json::error::Error) -> Self {
        Self {
            field: "".to_string(),
            message: err.to_string(),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::ConfigurationError;

    #[test]
    fn configuration_error() {
        let e = ConfigurationError::new("test", "is a string");
        assert_eq!(e.to_string(), "Invalid value for test: is a string");
    }

    use super::*;
    use serde::{Deserialize, Serialize};

    // test NetworkConfigPath both with Default and withou Default
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
    #[derive(Default, Serialize, Deserialize)]
    struct SuperTari {
        #[serde(flatten)]
        pub within: SubTari,
        pub over: SubTari,
        #[serde(default = "serde_default_string")]
        bitcoin: String,
    }
    fn serde_default_string() -> String {
        "ispublic".into()
    }
    impl NetworkConfigPath for SuperTari {
        fn main_key_prefix() -> &'static str {
            "crypto"
        }
    }

    #[test]
    fn default_network_config_loader() -> anyhow::Result<()> {
        let mut config = Config::new();

        config.set("crypto.monero", "isnottari")?;
        config.set("crypto.mainnet.monero", "isnottaritoo")?;
        config.set("crypto.mainnet.bitcoin", "isnottaritoo")?;
        let crypto = <SuperTari as DefaultConfigLoader>::load_from(&config)?;
        // no network value
        // [X] crypto.mainnet, [X] crypto = "isnottari", [X] Default
        assert_eq!(crypto.within.monero, "isnottari");
        // [ ] crypto.mainnet, [ ] crypto, [X] Default = "isprivate"
        assert_eq!(crypto.over.monero, "isprivate");
        // [X] crypto.mainnet, [ ] crypto, [X] Default = "", [X] serde(default)
        assert_eq!(crypto.bitcoin, "");

        config.set("crypto.over.monero", "istari")?;
        let crypto = <SuperTari as DefaultConfigLoader>::load_from(&config)?;
        // [ ] crypto.mainnet, [X] crypto = "istari", [X] Default
        assert_eq!(crypto.over.monero, "istari");

        config.set("crypto.network", "mainnet")?;
        // network = mainnet
        let crypto = <SuperTari as DefaultConfigLoader>::load_from(&config)?;
        // [X] crypto.mainnet = "isnottaritoo", [X] crypto, [X] Default
        assert_eq!(crypto.within.monero, "isnottaritoo");
        // [X] crypto.mainnet = "isnottaritoo", [ ] crypto, [X] serde(default), [X] Default
        assert_eq!(crypto.bitcoin, "isnottaritoo");
        // [ ] crypto.mainnet, [X] crypto = "istari", [X] Default
        assert_eq!(crypto.over.monero, "istari");

        config.set("crypto.network", "wrong_network")?;
        assert!(<SuperTari as DefaultConfigLoader>::load_from(&config).is_err());

        Ok(())
    }

    #[test]
    fn network_config_loader() -> anyhow::Result<()> {
        let mut config = Config::new();

        // no network value
        config.set("crypto.monero", "isnottari")?;
        config.set("crypto.mainnet.bitcoin", "isnottaritoo")?;
        // [X] crypto.monero [X] crypto.bitcoin(serde) [ ] crypto.over.monero
        assert!(<SuperTari as ConfigLoader>::load_from(&config).is_err());

        // [X] crypto.monero [X] crypto.bitcoin(serde) [ ] crypto.over.monero [X] mainnet.*
        config.set("crypto.mainnet.monero", "isnottaritoo")?;
        config.set("crypto.mainnet.over.monero", "istari")?;
        assert!(<SuperTari as ConfigLoader>::load_from(&config).is_err());

        // network = mainnet
        config.set("crypto.network", "mainnet")?;
        let crypto = <SuperTari as ConfigLoader>::load_from(&config)?;
        // [X] crypto.mainnet = "isnottaritoo", [X] crypto, [X] Default
        assert_eq!(crypto.within.monero, "isnottaritoo");
        // [X] crypto.mainnet = "isnottaritoo", [ ] crypto, [X] serde(default), [X] Default
        assert_eq!(crypto.bitcoin, "isnottaritoo");
        // [X] crypto.mainnet = "istari", [ ] crypto, [X] Default
        assert_eq!(crypto.over.monero, "istari");

        let mut config = Config::new();
        // no network value
        config.set("crypto.monero", "isnottari")?;
        config.set("crypto.over.monero", "istari")?;
        let crypto = <SuperTari as ConfigLoader>::load_from(&config)?;
        // [ ] crypto.mainnet, [X] crypto = "isnottari"
        assert_eq!(crypto.within.monero, "isnottari");
        // [ ] crypto.mainnet, [ ] crypto, [X] serde(default) = "ispublic"
        assert_eq!(crypto.bitcoin, "ispublic");
        // [ ] crypto.mainnet, [X] crypto = "istari"
        assert_eq!(crypto.over.monero, "istari");

        config.set("crypto.bitcoin", "isnottaritoo")?;
        let crypto = <SuperTari as ConfigLoader>::load_from(&config)?;
        // [ ] crypto.mainnet, [X] crypto = "isnottaritoo", [X] serde(default)
        assert_eq!(crypto.bitcoin, "isnottaritoo");

        Ok(())
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
        let mut config = Config::new();

        // no network value
        // [ ] one.param1(default) [X] one.param1(default) [ ] one.param2 [X] one.param2(serde)
        assert!(<OneConfig as ConfigLoader>::load_from(&config).is_err());
        // [ ] one.param1(default) [X] one.param1(default) [ ] one.param2 [X] one.param2(default)
        let one = <OneConfig as DefaultConfigLoader>::load_from(&config)?;
        assert_eq!(one.param1, OneConfig::default().param1);
        assert_eq!(one.param2, OneConfig::default().param2);

        config.set("one.param1", "can load from main section")?;
        let one = <OneConfig as DefaultConfigLoader>::load_from(&config)?;
        assert_eq!(one.param1, "can load from main section");
        assert_eq!(one.param2, "param2");

        let one = <OneConfig as ConfigLoader>::load_from(&config)?;
        assert_eq!(one.param1, "can load from main section");
        assert_eq!(one.param2, param2_serde_default());

        config.set("one.param2", "specific param overloads serde")?;
        let one = <OneConfig as ConfigLoader>::load_from(&config)?;
        assert_eq!(one.param2, "specific param overloads serde");

        Ok(())
    }
}
