// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#[cfg(test)]
#[cfg(feature = "grpc")]
mod tests {
    use super::*;
    use crate::scanning::{ScanConfig, ExtractionConfig};

    #[tokio::test]
    async fn test_grpc_scanner_builder() {
        let builder = GrpcScannerBuilder::new()
            .with_base_url("http://127.0.0.1:18142".to_string())
            .with_timeout(std::time::Duration::from_secs(10));

        // This will fail if no base node is running, but that's expected
        let result = builder.build().await;
        assert!(result.is_err()); // Should fail because no base node is running
    }

    #[tokio::test]
    async fn test_grpc_scanner_creation() {
        let result = GrpcBlockchainScanner::new("http://127.0.0.1:18142".to_string()).await;
        assert!(result.is_err()); // Should fail because no base node is running
    }

    #[tokio::test]
    async fn test_grpc_scanner_with_timeout() {
        let result = GrpcBlockchainScanner::with_timeout(
            "http://127.0.0.1:18142".to_string(),
            std::time::Duration::from_secs(5)
        ).await;
        assert!(result.is_err()); // Should fail because no base node is running
    }

    #[test]
    fn test_grpc_scanner_debug() {
        // Test that the scanner can be created and debugged (even if connection fails)
        let scanner = GrpcBlockchainScanner;
        let debug_str = format!("{:?}", scanner);
        assert!(debug_str.contains("GrpcBlockchainScanner"));
    }
}

#[cfg(test)]
#[cfg(not(feature = "grpc"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_grpc_feature_disabled() {
        let result = GrpcBlockchainScanner::new("http://127.0.0.1:18142".to_string()).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::errors::LightweightWalletError::OperationNotSupported(_)
        ));
    }

    #[tokio::test]
    async fn test_grpc_builder_feature_disabled() {
        let builder = GrpcScannerBuilder::new();
        let result = builder.build().await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::errors::LightweightWalletError::OperationNotSupported(_)
        ));
    }
} 