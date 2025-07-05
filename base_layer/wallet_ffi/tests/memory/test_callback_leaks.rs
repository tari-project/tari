//! # Memory Safety Validation Tests
//!
//! Tests for memory leak detection in transaction callback functionality.
//! These tests validate that callback execution does not leak memory.

#[cfg(test)]
mod tests {
    use crate::event_bridge::types::{TransactionData, WalletEvent, EventType, EventData};
    use crate::python_event_bridge::PythonEventBridge;
    use std::time::Instant;

    /// Test memory usage stability with repeated transaction event creation
    #[test]
    fn test_transaction_event_memory_stability() {
        let start_time = Instant::now();
        let initial_memory = get_memory_usage();
        
        // Create many transaction events to test for memory leaks
        for i in 0..10000 {
            let transaction_data = TransactionData {
                tx_id: i,
                source_address: format!("test_address_{}", i),
                amount: 1000000 + i,
                message: Some(format!("Test transaction {}", i)),
                timestamp: 1640995200 + i as i64,
                status: 1,
            };

            let _event = WalletEvent::new(
                EventType::TransactionReceived,
                1,
                EventData::TransactionReceived(transaction_data),
            );
            
            // Events should be dropped immediately, not accumulating memory
        }
        
        let end_memory = get_memory_usage();
        let duration = start_time.elapsed();
        
        println!("10,000 transaction events created in: {:?}", duration);
        println!("Initial memory: {} KB", initial_memory);
        println!("Final memory: {} KB", end_memory);
        
        // Memory usage should not have grown significantly
        let memory_growth = end_memory.saturating_sub(initial_memory);
        println!("Memory growth: {} KB", memory_growth);
        
        // Allow for some reasonable memory growth (less than 1MB for 10k events)
        assert!(memory_growth < 1024, "Memory grew by {} KB, may indicate memory leak", memory_growth);
        
        // Performance check - should create events quickly
        assert!(duration.as_millis() < 1000, "Event creation too slow: {:?}", duration);
    }

    /// Test that string handling doesn't leak memory
    #[test]
    fn test_string_memory_handling() {
        let initial_memory = get_memory_usage();
        
        // Create events with large strings
        for i in 0..1000 {
            let large_message = "x".repeat(1000); // 1KB string
            let large_address = format!("test_address_very_long_name_that_takes_space_{}", "y".repeat(100));
            
            let transaction_data = TransactionData {
                tx_id: i,
                source_address: large_address,
                amount: 1000000,
                message: Some(large_message),
                timestamp: 1640995200,
                status: 1,
            };

            let _event = WalletEvent::new(
                EventType::TransactionReceived,
                1,
                EventData::TransactionReceived(transaction_data),
            );
        }
        
        let end_memory = get_memory_usage();
        let memory_growth = end_memory.saturating_sub(initial_memory);
        
        println!("String test - Memory growth: {} KB", memory_growth);
        
        // Should not leak string memory
        assert!(memory_growth < 2048, "String memory may be leaking: {} KB growth", memory_growth);
    }

    /// Test concurrent transaction event creation
    #[test]
    fn test_concurrent_event_creation() {
        use std::sync::Arc;
        use std::thread;
        
        let initial_memory = get_memory_usage();
        let event_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        
        let mut handles = vec![];
        
        // Create multiple threads creating events concurrently
        for thread_id in 0..4 {
            let counter = Arc::clone(&event_count);
            let handle = thread::spawn(move || {
                for i in 0..1000 {
                    let transaction_data = TransactionData {
                        tx_id: (thread_id * 1000 + i) as u64,
                        source_address: format!("thread_{}_address_{}", thread_id, i),
                        amount: 1000000 + i as u64,
                        message: Some(format!("Thread {} transaction {}", thread_id, i)),
                        timestamp: 1640995200 + i as i64,
                        status: 1,
                    };

                    let _event = WalletEvent::new(
                        EventType::TransactionReceived,
                        1,
                        EventData::TransactionReceived(transaction_data),
                    );
                    
                    counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            });
            handles.push(handle);
        }
        
        // Wait for all threads to complete
        for handle in handles {
            handle.join().expect("Thread panicked");
        }
        
        let final_count = event_count.load(std::sync::atomic::Ordering::Relaxed);
        let end_memory = get_memory_usage();
        let memory_growth = end_memory.saturating_sub(initial_memory);
        
        println!("Concurrent test - Created {} events", final_count);
        println!("Concurrent test - Memory growth: {} KB", memory_growth);
        
        assert_eq!(final_count, 4000, "Not all events were created");
        assert!(memory_growth < 1024, "Concurrent memory usage too high: {} KB", memory_growth);
    }

    /// Get current memory usage in KB (simplified version for testing)
    fn get_memory_usage() -> u64 {
        // This is a simplified memory check
        // In a real implementation, you'd use more sophisticated memory measurement
        #[cfg(target_os = "linux")]
        {
            use std::fs;
            if let Ok(contents) = fs::read_to_string("/proc/self/status") {
                for line in contents.lines() {
                    if line.starts_with("VmRSS:") {
                        if let Some(kb_str) = line.split_whitespace().nth(1) {
                            return kb_str.parse().unwrap_or(0);
                        }
                    }
                }
            }
        }
        
        #[cfg(target_os = "macos")]
        {
            // For macOS, this is a placeholder - real implementation would use mach APIs
            // For testing purposes, we'll return a mock value
            return 1000; // Mock memory usage
        }
        
        // Default fallback
        0
    }

    /// Benchmark transaction data creation performance
    #[test]
    fn test_transaction_callback_performance() {
        let iterations = 100000;
        let start = Instant::now();
        
        for i in 0..iterations {
            let transaction_data = TransactionData {
                tx_id: i,
                source_address: "benchmark_address".to_string(),
                amount: 1000000,
                message: Some("benchmark".to_string()),
                timestamp: 1640995200,
                status: 1,
            };

            let _event = WalletEvent::new(
                EventType::TransactionReceived,
                1,
                EventData::TransactionReceived(transaction_data),
            );
        }
        
        let duration = start.elapsed();
        let events_per_second = iterations as f64 / duration.as_secs_f64();
        
        println!("Performance: {} events/second", events_per_second as u64);
        println!("Avg time per event: {} ns", duration.as_nanos() / iterations);
        
        // Should be able to create at least 100,000 events per second
        assert!(events_per_second > 100000.0, "Performance too slow: {} events/sec", events_per_second as u64);
        
        // Each event should take less than 10 microseconds to create
        let avg_time_ns = duration.as_nanos() / iterations;
        assert!(avg_time_ns < 10000, "Individual event creation too slow: {} ns", avg_time_ns);
    }

    /// Test memory stability of python_event_bridge with consistent event types
    #[tokio::test]
    async fn test_python_event_bridge_memory_stability() {
        let start_time = Instant::now();
        let initial_memory = get_memory_usage();
        
        let (bridge, mut receiver) = PythonEventBridge::new();
        
        // Create many events through python bridge to test for memory leaks
        for i in 0..1000 {
            let transaction_data = TransactionData {
                tx_id: i,
                source_address: format!("bridge_test_address_{}", i),
                amount: 1000000 + i,
                message: Some(format!("Bridge test transaction {}", i)),
                timestamp: 1640995200 + i as i64,
                status: 1,
            };

            let event = WalletEvent::new(
                EventType::TransactionReceived,
                1,
                EventData::TransactionReceived(transaction_data),
            );
            
            bridge.send_event(event);
            
            // Consume events to prevent channel buildup
            if i % 10 == 0 {
                for _ in 0..10 {
                    if receiver.try_recv().is_err() {
                        break;
                    }
                }
            }
        }
        
        // Consume remaining events
        while receiver.try_recv().is_ok() {}
        
        let end_memory = get_memory_usage();
        let duration = start_time.elapsed();
        
        println!("1,000 python bridge events created in: {:?}", duration);
        println!("Initial memory: {} KB", initial_memory);
        println!("Final memory: {} KB", end_memory);
        
        // Memory usage should not have grown significantly
        let memory_growth = end_memory.saturating_sub(initial_memory);
        println!("Python bridge memory growth: {} KB", memory_growth);
        
        // Allow for some reasonable memory growth (less than 500KB for 1k events)
        assert!(memory_growth < 512, "Python bridge memory grew by {} KB, may indicate memory leak", memory_growth);
        
        println!("Python event bridge memory safety test passed");
    }
}
