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
    /// If true, dynamic Cache-Control headers will be overridden by dynamic values depending on the height
    #[serde(default = "default_enabled")]
    dynamic: bool,
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
            dynamic: true,
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
    pub fn cache_control_for(&self, key: RouteKey, tip_height: u64, height: u64) -> String {
        let default = match key {
            RouteKey::GetTipInfo => self.get_tip_info.clone(),
            RouteKey::GetHeaderByHeight => self.get_header_by_height.clone(),
            RouteKey::GetUtxosByBlock => self.get_utxos_by_block.clone(),
            RouteKey::SyncUtxosByBlock => self.sync_utxos_by_block.clone(),
            RouteKey::GetHeightAtTime => self.get_height_at_time.clone(),
            RouteKey::TransactionQuery => self.transaction_query.clone(),
            RouteKey::GetUtxosDeletedInfo => self.get_utxos_deleted_info.clone(),
            RouteKey::GetUtxosMinedInfo => self.get_utxos_mined_info.clone(),
        };
        if !self.dynamic {
            return default;
        }
        // The variables are as follows:
        // max_age: how long the client can cache the response
        // s_maxage: how long shared caches (e.g., CDNs) can cache
        // stale_while_revalidate: how long the client/shared cache can use a stale response while revalidating
        // So for us the important one is the s_maxage, as that determines how long intermediaries can cache responses.
        // The max_age is more about client-side caching, which is less critical for us as our wallets wont do caching
        // as it will only be called once.
        let (max_age, s_maxage, stale_while_revalidate) = match tip_height.saturating_sub(height) {
            0..=10 => (30, 30, 15),             // within 10 blocks of tip (30s)
            11..=100 => (300, 60 * 5, 60),      // within 100 blocks of tip (11 blocks = 22min, cache 5 mins)
            101..=1000 => (360, 60 * 20, 60),   // within 1000 blocks of tip (101 blocks = 6.7 hours, cache 20 mins)
            1001..=2000 => (360, 60 * 30, 60),  // within 2000 blocks of tip (1001 blocks = 2.7 days, cache 30 mins)
            2001..=10000 => (360, 60 * 60, 60), // within 10000 blocks of tip (11 blocks = 5.5 days, cache 1 hour)
            _ => (360, 60 * 60 * 24 * 30, 60),  /* more than 10000 blocks from tip (10001 blocks = 27 days, cache 30
                                                  * days) */
        };
        match key {
            RouteKey::GetTipInfo => default,
            RouteKey::GetHeaderByHeight => format!(
                "public, max-age={max_age}, s-maxage={s_maxage}, stale-while-revalidate={stale_while_revalidate}"
            ),
            RouteKey::GetUtxosByBlock => format!(
                "public, max-age={max_age}, s-maxage={s_maxage}, stale-while-revalidate={stale_while_revalidate}"
            ),
            RouteKey::SyncUtxosByBlock => format!(
                "public, max-age={max_age}, s-maxage={s_maxage}, stale-while-revalidate={stale_while_revalidate}"
            ),
            RouteKey::GetHeightAtTime => default,
            RouteKey::TransactionQuery => default,
            RouteKey::GetUtxosDeletedInfo => default,
            RouteKey::GetUtxosMinedInfo => default,
        }
    }
}

/// Apply Cache-Control for the given handler key (no-op if disabled)
pub fn apply_cache_control(
    headers: &mut HeaderMap,
    cfg: &HttpCacheConfig,
    key: RouteKey,
    tip_height: u64,
    height: u64,
) {
    if !cfg.enabled {
        return;
    }
    let value = cfg.cache_control_for(key, tip_height, height);
    if let Ok(hv) = HeaderValue::from_str(&value) {
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

        apply_cache_control(&mut headers, &cfg, RouteKey::GetTipInfo, 0, 0);
        assert!(headers.get("Cache-Control").is_none());
    }

    #[test]
    fn test_apply_cache_control_all_keys() {
        let cfg = HttpCacheConfig {
            dynamic: false,
            ..Default::default()
        };
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
            apply_cache_control(&mut headers, &cfg, key, 0, 0);
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

    #[test]
    fn test_cache_control_for_dynamic_buckets_boundaries() {
        let cfg = HttpCacheConfig {
            dynamic: true,
            ..Default::default()
        };
        // (delta_from_tip, expected max-age, s-maxage, swr)
        let cases = [
            (0u64, 30, 30, 15),
            (10, 30, 30, 15),
            (11, 300, 300, 60),
            (100, 300, 300, 60),
            (101, 360, 1200, 60),
            (1000, 360, 1200, 60),
            (1001, 360, 1800, 60),
            (2000, 360, 1800, 60),
            (2001, 360, 3600, 60),
            (10000, 360, 3600, 60),
            (10001, 360, 2592000, 60),
        ];
        let tip = 50_000u64;
        for (delta, max_age, s_maxage, swr) in cases {
            let height = tip.saturating_sub(delta);
            for key in [
                RouteKey::GetHeaderByHeight,
                RouteKey::GetUtxosByBlock,
                RouteKey::SyncUtxosByBlock,
            ] {
                let got = cfg.cache_control_for(key, tip, height);
                let expected = format!(
                    "public, max-age={}, s-maxage={}, stale-while-revalidate={}",
                    max_age, s_maxage, swr
                );
                assert_eq!(got, expected, "key={:?} delta={}", key, delta);
            }
        }
    }

    #[test]
    fn test_cache_control_for_height_greater_than_tip_uses_lowest_bucket() {
        let cfg = HttpCacheConfig {
            dynamic: true,
            ..Default::default()
        };
        // height > tip should saturate to 0 delta and thus use the 0..=10 bucket
        let tip = 100;
        let height = 200; // greater than tip
        let expected = "public, max-age=30, s-maxage=30, stale-while-revalidate=15";
        let got = cfg.cache_control_for(RouteKey::GetHeaderByHeight, tip, height);
        assert_eq!(got, expected);
    }

    #[test]
    fn test_cache_control_for_dynamic_keeps_default_for_non_dynamic_routes() {
        let cfg = HttpCacheConfig {
            dynamic: true,
            ..Default::default()
        };
        let tip = 10_000u64;
        let height = 1;
        // These keys should always use their configured defaults even when dynamic is enabled
        assert_eq!(
            cfg.cache_control_for(RouteKey::GetTipInfo, tip, height),
            cfg.get_tip_info
        );
        assert_eq!(
            cfg.cache_control_for(RouteKey::GetHeightAtTime, tip, height),
            cfg.get_height_at_time
        );
        assert_eq!(
            cfg.cache_control_for(RouteKey::TransactionQuery, tip, height),
            cfg.transaction_query
        );
        assert_eq!(
            cfg.cache_control_for(RouteKey::GetUtxosDeletedInfo, tip, height),
            cfg.get_utxos_deleted_info
        );
        assert_eq!(
            cfg.cache_control_for(RouteKey::GetUtxosMinedInfo, tip, height),
            cfg.get_utxos_mined_info
        );
    }
}
