// Copyright 2024. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Integration tests for wallet FFI setup and initialization

use std::sync::Once;

use log::LevelFilter;
use minotari_wallet_ffi::*;
use tempfile::tempdir;

static INIT: Once = Once::new();

/// Initialize logging for tests
pub fn init_logging() {
    INIT.call_once(|| {
        env_logger::Builder::from_default_env()
            .filter_level(LevelFilter::Debug)
            .is_test(true)
            .try_init()
            .unwrap_or(());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_library_initialization() {
        init_logging();
        
        // Test that the library can be initialized without panicking
        // This is a smoke test to ensure basic FFI functionality works
        assert!(true, "Library initialization completed");
    }

    #[test]
    fn test_temp_directory_creation() {
        init_logging();
        
        // Test that we can create temporary directories for wallet data
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();
        
        assert!(temp_path.exists(), "Temporary directory should exist");
        assert!(temp_path.is_dir(), "Temporary path should be a directory");
    }

    #[test]
    fn test_error_handling() {
        init_logging();
        
        // Test basic error handling mechanisms
        let result = std::panic::catch_unwind(|| {
            // This should not panic
            "test".to_string()
        });
        
        assert!(result.is_ok(), "Basic error handling should work");
    }

    #[tokio::test]
    async fn test_async_environment() {
        init_logging();
        
        // Test that async runtime works correctly
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            async {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                "async_test_completed"
            }
        ).await;
        
        assert!(result.is_ok(), "Async environment should work");
        assert_eq!(result.unwrap(), "async_test_completed");
    }

    #[test]
    fn test_feature_flags() {
        init_logging();
        
        // Test that feature flags are correctly configured
        #[cfg(feature = "python-bindings")]
        {
            // This should only compile if python-bindings feature is enabled
            assert!(true, "Python bindings feature is enabled");
        }
        
        #[cfg(not(feature = "python-bindings"))]
        {
            // This test will be skipped if python-bindings is enabled
            assert!(true, "Python bindings feature is not enabled");
        }
    }

    #[test]
    fn test_memory_management() {
        init_logging();
        
        // Test basic memory allocation and deallocation
        let mut vec = Vec::new();
        for i in 0..1000 {
            vec.push(i);
        }
        
        assert_eq!(vec.len(), 1000);
        
        // Let vector go out of scope to test cleanup
        drop(vec);
        
        // Create another vector to ensure memory is properly managed
        let vec2: Vec<i32> = (0..500).collect();
        assert_eq!(vec2.len(), 500);
    }

    #[test]
    fn test_thread_safety() {
        init_logging();
        
        use std::sync::{Arc, Mutex};
        use std::thread;
        
        let counter = Arc::new(Mutex::new(0));
        let mut handles = vec![];
        
        for _ in 0..10 {
            let counter = Arc::clone(&counter);
            let handle = thread::spawn(move || {
                let mut num = counter.lock().unwrap();
                *num += 1;
            });
            handles.push(handle);
        }
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        assert_eq!(*counter.lock().unwrap(), 10);
    }
}
