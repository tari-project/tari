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

use crate::{Feature, FEATURE_LIST};

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

// Identify the target network by
// 1. Checking whether --config tari-network=xxx was passed in as a config flag to cargo (or from Cargo.toml)
// 2. Checking the environment variable TARI_NETWORK is set
// 3. default to mainnet
pub fn identify_target() -> Target {
    check_envar("CARGO_CFG_TARI_NETWORK")
        .or_else(|| check_envar("TARI_NETWORK"))
        .unwrap_or(Target::TestNet)
}

pub fn check_envar(envar: &str) -> Option<Target> {
    match std::env::var(envar) {
        Ok(s) => Some(Target::from_network_str(s.to_lowercase().as_str())),
        _ => None,
    }
}

pub fn list_active_features() {
    println!("These features are ACTIVE on mainnet (no special code handling is done)");
    FEATURE_LIST
        .iter()
        .filter(|f| f.is_active())
        .for_each(|f| println!("{}", f));
}

pub fn list_removed_features() {
    println!("These features are DEPRECATED and will never be compiled");
    FEATURE_LIST
        .iter()
        .filter(|f| f.was_removed())
        .for_each(|f| println!("{}", f));
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
    println!("** Activating {} **", feature);
    println!("cargo:rustc-cfg={}", feature.attr_name());
}

pub fn build_features() {
    // Make sure to rebuild when the network changes
    println!("cargo:rerun-if-env-changed=TARI_NETWORK");

    let target = identify_target();
    println!("cargo:rustc-cfg=tari_network_{}", target.as_key_str());
    println!("Building for {}", target);
    list_active_features();
    list_removed_features();
    if let Err(e) = resolve_features(target) {
        eprintln!("Could not build Tari due to issues with the feature flag set.\n{}", e);
        panic!();
    }
}
