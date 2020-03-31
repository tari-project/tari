# Tari configuration files

This folder contains the canonical definitions for Tari configuration. 

## tari.config.json

[tari.config.json] lists all possible configuration options along with additional metadata, such as conditions for when
the options should be enabled, default value specification and some basic OS-related parameters (such as the default
home folder location).

This file is used by the [Tari Configuration Generator] to drive the configuration options.

### A warning about default values

The default values specified in [tari.config.json] _should_ correspond with the default values in the code. However it's
possible that things can go out of sync. As always the code is the ultimate source of truth; The default values in the
json file are used by [Tari Configuration Generator] to decide whether to include a given option in the `.toml` output
or not. It _does not_ set the default in the running software.
 
## Presets

The presets folder contains a set of preconfigured configuration files for common use cases. These preset files will be
loaded into and listed on [Tari Configuration Generator]'s preset config file list.

[tari.config.json]: ./tari.config.json
[Tari Configuration Generator]: https://config.tari.com
