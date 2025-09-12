// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use axum::http::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};

/// Configuration for setting Cache-Control headers on wallet HTTP responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpCacheConfig {
    /// If not enabled (false), no Cache-Control header is set anywhere.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// The Cache-Control string to use for the 'get_tip_info' handler.
    /// Default: "public, max-age=15, s-maxage=15, stale-while-revalidate=15"
    #[serde(default = "default_get_tip_info_cache_control")]
    pub get_tip_info: String,
    /// The Cache-Control string to use for the 'get_header_by_height' handler.
    /// Default: "public, max-age=120, s-maxage=60, stale-while-revalidate=15"
    #[serde(default = "default_get_header_by_height_cache_control")]
    pub get_header_by_height: String,
    /// The Cache-Control string to use for the 'get_utxos_by_block' handler
    /// Default: "public, max-age=3600, s-maxage=1800, stale-while-revalidate=60"
    #[serde(default = "default_get_utxos_by_block_cache_control")]
    pub get_utxos_by_block: String,
    /// The Cache-Control string to use for the 'sync_utxos_by_block' handler
    /// Default: "public, max-age=3600, s-maxage=1800, stale-while-revalidate=60"
    #[serde(default = "default_sync_utxos_by_block_cache_control")]
    pub sync_utxos_by_block: String,
    /// The Cache-Control string to use for the 'get_height_at_time' handler.
    /// Default: "public, max-age=60, s-maxage=30, stale-while-revalidate=15"
    #[serde(default = "default_cache_control")]
    pub get_height_at_time: String,
    /// The Cache-Control string to use for the 'transaction_query' handler
    /// Default: "public, max-age=60, s-maxage=30, stale-while-revalidate=15"
    #[serde(default = "default_cache_control")]
    pub transaction_query: String,
    /// The Cache-Control string to use for the 'get_utxos_deleted_info' handler
    /// Default: "public, max-age=60, s-maxage=30, stale-while-revalidate=15"
    #[serde(default = "default_cache_control")]
    pub get_utxos_deleted_info: String,
    /// The Cache-Control string to use for the 'get_utxos_mined_info' handler
    /// Default: "public, max-age=60, s-maxage=30, stale-while-revalidate=15"
    #[serde(default = "default_cache_control")]
    pub get_utxos_mined_info: String,
}

fn default_enabled() -> bool {
    true
}

fn default_get_tip_info_cache_control() -> String {
    "public, max-age=15, s-maxage=15, stale-while-revalidate=15".to_string()
}

fn default_get_header_by_height_cache_control() -> String {
    "public, max-age=120, s-maxage=60, stale-while-revalidate=15".to_string()
}

fn default_get_utxos_by_block_cache_control() -> String {
    "public, max-age=3600, s-maxage=1800, stale-while-revalidate=60".to_string()
}

fn default_sync_utxos_by_block_cache_control() -> String {
    "public, max-age=3600, s-maxage=1800, stale-while-revalidate=60".to_string()
}

fn default_cache_control() -> String {
    // match the most common value in your code today
    "public, max-age=60, s-maxage=30, stale-while-revalidate=15".to_string()
}

impl Default for HttpCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            get_tip_info: default_get_tip_info_cache_control(),
            get_header_by_height: default_get_header_by_height_cache_control(),
            get_utxos_by_block: default_get_utxos_by_block_cache_control(),
            sync_utxos_by_block: default_sync_utxos_by_block_cache_control(),
            get_height_at_time: default_cache_control(),
            transaction_query: default_cache_control(),
            get_utxos_deleted_info: default_cache_control(),
            get_utxos_mined_info: default_cache_control(),
        }
    }
}

/// Keys for the different HTTP routes that can have Cache-Control applied.
#[derive(Debug, Clone, Copy)]
pub enum RouteKey {
    GetTipInfo,
    GetHeaderByHeight,
    GetUtxosByBlock,
    SyncUtxosByBlock,
    GetHeightAtTime,
    TransactionQuery,
    GetUtxosDeletedInfo,
    GetUtxosMinedInfo,
}

impl HttpCacheConfig {
    /// Returns a mapping of route keys to their Cache-Control strings.
    pub fn cache_control_for(&self, key: RouteKey) -> &str {
        match key {
            RouteKey::GetTipInfo => self.get_tip_info.as_str(),
            RouteKey::GetHeaderByHeight => self.get_header_by_height.as_str(),
            RouteKey::GetUtxosByBlock => self.get_utxos_by_block.as_str(),
            RouteKey::SyncUtxosByBlock => self.sync_utxos_by_block.as_str(),
            RouteKey::GetHeightAtTime => self.get_height_at_time.as_str(),
            RouteKey::TransactionQuery => self.transaction_query.as_str(),
            RouteKey::GetUtxosDeletedInfo => self.get_utxos_deleted_info.as_str(),
            RouteKey::GetUtxosMinedInfo => self.get_utxos_mined_info.as_str(),
        }
    }
}

/// Apply Cache-Control for the given handler key (no-op if disabled)
pub fn apply_cache_control(headers: &mut HeaderMap, cfg: &HttpCacheConfig, key: RouteKey) {
    if !cfg.enabled {
        return;
    }
    let value = cfg.cache_control_for(key);
    if let Ok(hv) = HeaderValue::from_str(value) {
        headers.insert("Cache-Control", hv);
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;

    use super::*;

    #[test]
    fn test_apply_cache_control_disabled() {
        let cfg = HttpCacheConfig {
            enabled: false,
            ..Default::default()
        };
        let mut headers = HeaderMap::new();

        apply_cache_control(&mut headers, &cfg, RouteKey::GetTipInfo);
        assert!(headers.get("Cache-Control").is_none());
    }

    #[test]
    fn test_apply_cache_control_all_keys() {
        let cfg = HttpCacheConfig::default();
        let mut headers = HeaderMap::new();

        for key in [
            RouteKey::GetTipInfo,
            RouteKey::GetHeaderByHeight,
            RouteKey::GetUtxosByBlock,
            RouteKey::SyncUtxosByBlock,
            RouteKey::GetHeightAtTime,
            RouteKey::TransactionQuery,
            RouteKey::GetUtxosDeletedInfo,
            RouteKey::GetUtxosMinedInfo,
        ] {
            headers.clear();
            apply_cache_control(&mut headers, &cfg, key);
            match key {
                RouteKey::GetTipInfo => {
                    assert_eq!(
                        headers.get("Cache-Control").unwrap(),
                        &HeaderValue::from_static("public, max-age=15, s-maxage=15, stale-while-revalidate=15")
                    );
                },
                RouteKey::GetHeaderByHeight => {
                    assert_eq!(
                        headers.get("Cache-Control").unwrap(),
                        &HeaderValue::from_static("public, max-age=120, s-maxage=60, stale-while-revalidate=15")
                    );
                },
                RouteKey::GetUtxosByBlock => {
                    assert_eq!(
                        headers.get("Cache-Control").unwrap(),
                        &HeaderValue::from_static("public, max-age=3600, s-maxage=1800, stale-while-revalidate=60")
                    );
                },
                RouteKey::SyncUtxosByBlock => {
                    assert_eq!(
                        headers.get("Cache-Control").unwrap(),
                        &HeaderValue::from_static("public, max-age=3600, s-maxage=1800, stale-while-revalidate=60")
                    );
                },
                RouteKey::GetHeightAtTime => {
                    assert_eq!(
                        headers.get("Cache-Control").unwrap(),
                        &HeaderValue::from_static("public, max-age=60, s-maxage=30, stale-while-revalidate=15")
                    );
                },
                RouteKey::TransactionQuery => {
                    assert_eq!(
                        headers.get("Cache-Control").unwrap(),
                        &HeaderValue::from_static("public, max-age=60, s-maxage=30, stale-while-revalidate=15")
                    );
                },
                RouteKey::GetUtxosDeletedInfo => {
                    assert_eq!(
                        headers.get("Cache-Control").unwrap(),
                        &HeaderValue::from_static("public, max-age=60, s-maxage=30, stale-while-revalidate=15")
                    );
                },
                RouteKey::GetUtxosMinedInfo => {
                    assert_eq!(
                        headers.get("Cache-Control").unwrap(),
                        &HeaderValue::from_static("public, max-age=60, s-maxage=30, stale-while-revalidate=15")
                    );
                },
            }
        }
    }
}
