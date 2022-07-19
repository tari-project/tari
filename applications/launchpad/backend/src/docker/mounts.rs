// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::path::Path;

use bollard::models::{Mount, MountTypeEnum};

pub struct Mounts {
    mounts: Vec<Mount>,
}

impl Mounts {
    pub fn empty() -> Self {
        Self { mounts: Vec::new() }
    }

    pub fn into_docker_mounts(self) -> Vec<Mount> {
        self.mounts
    }

    pub fn with_blockchain(mut self, volume_name: String) -> Self {
        let mount = Mount {
            target: Some("/blockchain".to_string()),
            source: Some(volume_name),
            typ: Some(MountTypeEnum::VOLUME),
            volume_options: None,
            ..Default::default()
        };
        self.mounts.push(mount);
        self
    }

    pub fn bind<P: AsRef<Path>>(mut self, source: P, target: &str) -> Self {
        let source = canonicalize(source);
        let mount = Mount {
            target: Some(target.to_string()),
            source: Some(source),
            typ: Some(MountTypeEnum::BIND),
            bind_options: None,
            ..Default::default()
        };
        self.mounts.push(mount);
        self
    }

    pub fn with_general<P: AsRef<Path>>(data_dir: P) -> Self {
        Mounts::empty().bind(data_dir, "/var/tari")
    }

    pub fn with_grafana(mut self, volume_name: String) -> Self {
        let mount = Mount {
            target: Some("/grafana".to_string()),
            source: Some(volume_name),
            typ: Some(MountTypeEnum::VOLUME),
            volume_options: None,
            ..Default::default()
        };
        self.mounts.push(mount);
        self
    }
}

// FIXME: This might be replaceable by std::fs::canonicalize, but I don't have a windows machine to check
fn canonicalize<P: AsRef<Path>>(path: P) -> String {
    #[cfg(target_os = "windows")]
    let path = format!(
        "//{}",
        path.as_ref()
            .iter()
            .filter_map(|part| {
                use std::{ffi::OsStr, path};

                use regex::Regex;

                if part == OsStr::new(&path::MAIN_SEPARATOR.to_string()) {
                    None
                } else {
                    let drive = Regex::new(r"(?P<letter>[A-Za-z]):").unwrap();
                    let part = part.to_string_lossy().to_string();
                    if drive.is_match(part.as_str()) {
                        Some(drive.replace(part.as_str(), "$letter").to_lowercase())
                    } else {
                        Some(part)
                    }
                }
            })
            .collect::<Vec<String>>()
            .join("/")
    );
    #[cfg(target_os = "macos")]
    let path = format!("/host_mnt{}", path.as_ref().to_string_lossy());
    #[cfg(target_os = "linux")]
    let path = path.as_ref().to_string_lossy().to_string();
    path
}
