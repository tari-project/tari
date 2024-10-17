//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{fmt, fmt::Display, str::FromStr};

use crate::TariSwarmError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolVersion {
    domain: String,
    network: String,
    version: Version,
}

impl ProtocolVersion {
    pub const fn new(domain: String, network: String, version: Version) -> Self {
        Self {
            domain,
            network,
            version,
        }
    }

    pub fn domain(&self) -> &str {
        &self.domain
    }

    pub fn network(&self) -> &str {
        &self.network
    }

    pub const fn version(&self) -> Version {
        self.version
    }

    pub fn is_compatible(&self, protocol_str: &str) -> bool {
        let Some((domain, network, version)) = parse_protocol_str(protocol_str) else {
            return false;
        };
        self.domain == domain && self.network == network && self.version.semantic_version_eq(&version)
    }
}

impl PartialEq<String> for ProtocolVersion {
    fn eq(&self, other: &String) -> bool {
        let Some((domain, network, version)) = parse_protocol_str(other) else {
            return false;
        };
        self.domain == domain && self.network == network && self.version == version
    }
}

impl Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "/{}/{}/{}", self.domain, self.network, self.version)
    }
}

impl FromStr for ProtocolVersion {
    type Err = TariSwarmError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut parts = value.split('/');
        // Must have a leading '/'
        let leading = parts.next();
        if leading.filter(|l| l.is_empty()).is_none() {
            return Err(TariSwarmError::ProtocolVersionParseFailed {
                given: value.to_string(),
            });
        }

        let Some((domain, network, version)) = parse_protocol_str(value) else {
            return Err(TariSwarmError::ProtocolVersionParseFailed {
                given: value.to_string(),
            });
        };
        Ok(Self::new(domain.to_string(), network.to_string(), version))
    }
}

fn parse_protocol_str(protocol_str: &str) -> Option<(&str, &str, Version)> {
    let mut parts = protocol_str.split('/');
    // Must have a leading '/'
    let leading = parts.next()?;
    if !leading.is_empty() {
        return None;
    }

    let domain = parts.next()?;
    let network = parts.next()?;
    let version = parts.next().and_then(|s| s.parse().ok())?;
    Some((domain, network, version))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    major: u16,
    minor: u16,
    patch: u16,
}

impl Version {
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self { major, minor, patch }
    }

    pub const fn major(&self) -> u16 {
        self.major
    }

    pub const fn minor(&self) -> u16 {
        self.minor
    }

    pub const fn patch(&self) -> u16 {
        self.patch
    }

    pub const fn semantic_version_eq(&self, other: &Version) -> bool {
        // Similar to https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#specifying-dependencies-from-cratesio
        // 0.x.y any change to x is not compatible
        if self.major == 0 {
            // 0.0.x any change to x is not compatible
            if self.minor == 0 {
                return self.patch == other.patch;
            }
            return self.minor == other.minor;
        }

        // x.y.z any change to x is not compatible
        self.major == other.major
    }
}

impl FromStr for Version {
    type Err = TariSwarmError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('.');

        let mut next = move || {
            parts
                .next()
                .ok_or(TariSwarmError::InvalidVersionString { given: s.to_string() })?
                .parse()
                .map_err(|_| TariSwarmError::InvalidVersionString { given: s.to_string() })
        };
        Ok(Self {
            major: next()?,
            minor: next()?,
            patch: next()?,
        })
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_parses_correctly() {
        let version = ProtocolVersion::from_str("/tari/igor/1.2.3").unwrap();
        assert_eq!(version.domain(), "tari");
        assert_eq!(version.network(), "igor");
        assert_eq!(version.version(), Version {
            major: 1,
            minor: 2,
            patch: 3
        });
    }
}
