// Copyright 2023. The Tari Project
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

use std::fmt::Display;

use crate::{FEATURE_LIST, Feature};

pub enum Target {
    TestNet,
    NextNet,
    MainNet,
}

impl Target {
    pub const fn as_key_str(&self) -> &'static str {
        match self {
            Target::MainNet => "mainnet",
            Target::NextNet => "nextnet",
            Target::TestNet => "testnet",
        }
    }

    pub fn from_network_str(value: &str) -> Self {
        // The duplication of network names here isn't great but we're being lazy and non-exhaustive
        // regarding the endless testnet possibilities. This minor MainNet, StageNet, and NextNet
        // duplication allows us to leave the crate dependency free.
        match value.to_lowercase().as_str() {
            "mainnet" | "stagenet" => Target::MainNet,
            "nextnet" => Target::NextNet,
            _ => Target::TestNet,
        }
    }
}

impl Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Target::TestNet => f.write_str("TestNet"),
            Target::NextNet => f.write_str("NextNet"),
            Target::MainNet => f.write_str("MainNet"),
        }
    }
}

// Identify the target network by:
// 1. Checking whether CARGO_CFG_TARI_TARGET_NETWORK was passed in as a cargo configuration.
// 2. Checking whether the TARI_TARGET_NETWORK environment variable is set.
// 3. Defaulting to testnet with a cargo warning for development builds when neither is set.
pub fn identify_target() -> Target {
    check_envar("CARGO_CFG_TARI_TARGET_NETWORK")
        .or_else(|| check_envar("TARI_TARGET_NETWORK"))
        .unwrap_or_else(|| {
            println!("cargo:warning=TARI_TARGET_NETWORK is not set; defaulting to testnet");
            Target::TestNet
        })
}

pub fn check_envar(envar: &str) -> Option<Target> {
    match std::env::var(envar) {
        Ok(s) if !s.trim().is_empty() => Some(Target::from_network_str(s.to_lowercase().as_str())),
        _ => None,
    }
}

pub fn list_active_features() {
    println!("tari_feature:These features are ACTIVE on mainnet (no special code handling is done)");
    FEATURE_LIST
        .iter()
        .filter(|f| f.is_active())
        .for_each(|f| println!("tari_feature:{f}"));
    println!("tari_feature:End of ACTIVE feature list");
}

pub fn list_removed_features() {
    println!("tari_feature:These features are DEPRECATED and will never be compiled");
    FEATURE_LIST
        .iter()
        .filter(|f| f.was_removed())
        .for_each(|f| println!("tari_feature:{f}"));
    println!("tari_feature:End of DEPRECATED feature list");
}

pub fn resolve_features(target: Target) -> Result<(), String> {
    match target {
        Target::MainNet => { /* No features are active at all */ },
        Target::NextNet => FEATURE_LIST
            .iter()
            .filter(|f| f.is_active_in_nextnet())
            .for_each(activate_feature),
        Target::TestNet => FEATURE_LIST
            .iter()
            .filter(|f| f.is_active_in_testnet())
            .for_each(activate_feature),
    }
    Ok(())
}

pub fn activate_feature(feature: &Feature) {
    println!("tari_feature:** Activating {feature} **");
    println!("cargo:rustc-cfg={}", feature.attr_name());
}

pub fn build_features() {
    // Make sure to rebuild when the network changes
    println!("cargo:rerun-if-env-changed=TARI_TARGET_NETWORK");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARI_TARGET_NETWORK");

    // Declare all recognized cfg flags to rustc to catch typos
    println!("cargo::rustc-check-cfg=cfg(tari_target_network_testnet)");
    println!("cargo::rustc-check-cfg=cfg(tari_target_network_nextnet)");
    println!("cargo::rustc-check-cfg=cfg(tari_target_network_mainnet)");
    for f in FEATURE_LIST {
        println!("cargo::rustc-check-cfg=cfg({})", f.attr_name());
    }

    let target = identify_target();
    println!("cargo:rustc-cfg=tari_target_network_{}", target.as_key_str());
    println!("tari_feature:Building for {target}");
    list_active_features();
    list_removed_features();
    if let Err(e) = resolve_features(target) {
        eprintln!("Could not build Tari due to issues with the feature flag set.\n{e}");
        panic!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_as_key_str() {
        assert_eq!(Target::MainNet.as_key_str(), "mainnet");
        assert_eq!(Target::NextNet.as_key_str(), "nextnet");
        assert_eq!(Target::TestNet.as_key_str(), "testnet");
    }

    #[test]
    fn test_from_network_str() {
        assert!(matches!(Target::from_network_str("mainnet"), Target::MainNet));
        assert!(matches!(Target::from_network_str("MAINNET"), Target::MainNet));
        assert!(matches!(Target::from_network_str("stagenet"), Target::MainNet));
        assert!(matches!(Target::from_network_str("nextnet"), Target::NextNet));
        assert!(matches!(Target::from_network_str("testnet"), Target::TestNet));
        assert!(matches!(Target::from_network_str("esmeralda"), Target::TestNet));
        assert!(matches!(Target::from_network_str(""), Target::TestNet));
    }

    #[test]
    fn test_check_envar() {
        let key = "TARI_TEST_RESOLVER_ENVAR";
        unsafe {
            std::env::remove_var(key);
        }
        assert!(check_envar(key).is_none());

        unsafe {
            std::env::set_var(key, "mainnet");
        }
        assert!(matches!(check_envar(key), Some(Target::MainNet)));

        unsafe {
            std::env::set_var(key, "nextnet");
        }
        assert!(matches!(check_envar(key), Some(Target::NextNet)));

        unsafe {
            std::env::set_var(key, "   ");
        }
        assert!(check_envar(key).is_none());

        unsafe {
            std::env::remove_var(key);
        }
    }
}
