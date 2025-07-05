//! # Event Dispatcher
//!
//! This module provides thread-safe event dispatching capabilities for routing
//! events from C callbacks to Python handlers. The dispatcher manages event
//! channels, callback registrations, and provides performance monitoring.

use super::{
    types::{WalletEvent, EventType, EventCategory, EventPriority, TransactionData},
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Mutex};
use std::time::{SystemTime, Instant};

/// Statistics for monitoring dispatcher performance
#[derive(Debug, Clone, Default)]
pub struct DispatcherStats {
    pub events_processed: u64,
    pub events_dropped: u64,
    pub callback_errors: u64,
    pub total_processing_time_ms: u64,
    pub max_processing_time_ms: u64,
    pub min_processing_time_ms: u64,
    pub active_callbacks: usize,
    pub event_type_counts: HashMap<String, u64>,
    pub category_counts: HashMap<String, u64>,
    pub priority_counts: HashMap<String, u64>,
}

impl DispatcherStats {
    /// Calculate average processing time in milliseconds
    pub fn average_processing_time_ms(&self) -> f64 {
        if self.events_processed > 0 {
            self.total_processing_time_ms as f64 / self.events_processed as f64
        } else {
            0.0
        }
    }

    /// Get event processing rate (events per second)
    pub fn events_per_second(&self, duration_secs: u64) -> f64 {
        if duration_secs > 0 {
            self.events_processed as f64 / duration_secs as f64
        } else {
            0.0
        }
    }

    /// Get error rate as percentage
    pub fn error_rate(&self) -> f64 {
        let total = self.events_processed + self.events_dropped + self.callback_errors;
        if total > 0 {
            ((self.events_dropped + self.callback_errors) as f64 / total as f64) * 100.0
        } else {
            0.0
        }
    }
}

/// Callback handler function type
pub type CallbackHandler = Box<dyn Fn(&WalletEvent) -> Result<(), Box<dyn std::error::Error + Send + Sync>> + Send + Sync>;

/// Registration information for event callbacks
#[derive(Clone)]
pub struct CallbackRegistration {
    pub event_type: EventType,
    pub callback_id: String,
    pub registered_at: SystemTime,
    pub priority: EventPriority,
}

/// Thread-safe event dispatcher for managing event routing and callbacks
pub struct EventDispatcher {
    wallet_id: u64,
    sender: mpsc::UnboundedSender<WalletEvent>,
    callbacks: Arc<RwLock<HashMap<EventType, Vec<CallbackRegistration>>>>,
    stats: Arc<RwLock<DispatcherStats>>,
    callback_handlers: Arc<RwLock<HashMap<String, CallbackHandler>>>,
    is_running: Arc<Mutex<bool>>,
}

impl EventDispatcher {
    /// Create a new event dispatcher
    pub fn new(wallet_id: u64, sender: mpsc::UnboundedSender<WalletEvent>) -> Self {
        Self {
            wallet_id,
            sender,
            callbacks: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(DispatcherStats::default())),
            callback_handlers: Arc::new(RwLock::new(HashMap::new())),
            is_running: Arc::new(Mutex::new(false)),
        }
    }

    /// Send an event through the dispatcher
    pub async fn send_event(&self, event: WalletEvent) -> Result<(), mpsc::error::SendError<WalletEvent>> {
        let result = self.sender.send(event.clone());
        
        // Update statistics
        let mut stats = self.stats.write().await;
        match &result {
            Ok(_) => {
                stats.events_processed += 1;
                
                // Update event type counts
                let event_name = event.event_name().to_string();
                *stats.event_type_counts.entry(event_name).or_insert(0) += 1;
                
                // Update category counts
                let category = format!("{:?}", event.event_type.category());
                *stats.category_counts.entry(category).or_insert(0) += 1;
                
                // Update priority counts
                let priority = format!("{:?}", event.event_type.priority());
                *stats.priority_counts.entry(priority).or_insert(0) += 1;
            }
            Err(_) => {
                stats.events_dropped += 1;
            }
        }

        result
    }

    /// Register a callback for a specific event type
    pub async fn register_callback<F>(&self, event_type: EventType, callback_id: String, handler: F)
    where
        F: Fn(&WalletEvent) -> Result<(), Box<dyn std::error::Error + Send + Sync>> + Send + Sync + 'static,
    {
        let registration = CallbackRegistration {
            event_type: event_type.clone(),
            callback_id: callback_id.clone(),
            registered_at: SystemTime::now(),
            priority: event_type.priority(),
        };

        // Add to callback registrations
        let mut callbacks = self.callbacks.write().await;
        callbacks.entry(event_type).or_insert_with(Vec::new).push(registration);

        // Add handler function
        let mut handlers = self.callback_handlers.write().await;
        handlers.insert(callback_id, Box::new(handler));

        // Update stats
        let mut stats = self.stats.write().await;
        stats.active_callbacks = handlers.len();
    }

    /// Unregister a callback
    pub async fn unregister_callback(&self, event_type: &EventType, callback_id: &str) -> bool {
        let mut callbacks = self.callbacks.write().await;
        let mut handlers = self.callback_handlers.write().await;

        let removed = if let Some(registrations) = callbacks.get_mut(event_type) {
            let initial_len = registrations.len();
            registrations.retain(|reg| reg.callback_id != callback_id);
            registrations.len() < initial_len
        } else {
            false
        };

        if removed {
            handlers.remove(callback_id);
            
            // Update stats
            let mut stats = self.stats.write().await;
            stats.active_callbacks = handlers.len();
        }

        removed
    }

    /// Get all registered callbacks for an event type
    pub async fn get_callbacks(&self, event_type: &EventType) -> Vec<CallbackRegistration> {
        let callbacks = self.callbacks.read().await;
        callbacks.get(event_type).cloned().unwrap_or_default()
    }

    /// Get all registered event types
    pub async fn get_registered_event_types(&self) -> Vec<EventType> {
        let callbacks = self.callbacks.read().await;
        callbacks.keys().cloned().collect()
    }

    /// Get dispatcher statistics
    pub async fn get_stats(&self) -> DispatcherStats {
        self.stats.read().await.clone()
    }

    /// Reset dispatcher statistics
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        *stats = DispatcherStats::default();
        
        // Preserve active callback count
        let handlers = self.callback_handlers.read().await;
        stats.active_callbacks = handlers.len();
    }

    /// Get wallet ID
    pub fn wallet_id(&self) -> u64 {
        self.wallet_id
    }

    /// Check if dispatcher is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.lock().await
    }

    /// Process events from the channel (main event processing loop)
    pub async fn process_events(
        dispatcher: Arc<EventDispatcher>,
        mut receiver: mpsc::UnboundedReceiver<WalletEvent>,
    ) {
        // Mark as running
        {
            let mut running = dispatcher.is_running.lock().await;
            *running = true;
        }

        log::info!("Event dispatcher started for wallet {}", dispatcher.wallet_id);

        while let Some(event) = receiver.recv().await {
            let start_time = Instant::now();
            
            // Get registered callbacks for this event type
            let callbacks = {
                let callbacks_guard = dispatcher.callbacks.read().await;
                callbacks_guard.get(&event.event_type).cloned().unwrap_or_default()
            };

            if !callbacks.is_empty() {
                // Sort callbacks by priority (higher priority first)
                let mut sorted_callbacks = callbacks;
                sorted_callbacks.sort_by(|a, b| b.priority.cmp(&a.priority));

                // Execute callbacks
                for registration in sorted_callbacks {
                    let handler_result = {
                        let handlers = dispatcher.callback_handlers.read().await;
                        if let Some(handler) = handlers.get(&registration.callback_id) {
                            handler(&event)
                        } else {
                            log::warn!(
                                "Handler not found for callback_id: {} (event: {})",
                                registration.callback_id,
                                event.event_name()
                            );
                            continue;
                        }
                    };

                    if let Err(e) = handler_result {
                        log::error!(
                            "Callback error for {} ({}): {}",
                            event.event_name(),
                            registration.callback_id,
                            e
                        );
                        
                        let mut stats = dispatcher.stats.write().await;
                        stats.callback_errors += 1;
                    }
                }
            } else {
                log::debug!("No callbacks registered for event: {}", event.event_name());
            }

            // Update processing time statistics
            let processing_time = start_time.elapsed();
            let processing_time_ms = processing_time.as_millis() as u64;

            let mut stats = dispatcher.stats.write().await;
            stats.total_processing_time_ms += processing_time_ms;
            
            if processing_time_ms > stats.max_processing_time_ms {
                stats.max_processing_time_ms = processing_time_ms;
            }
            
            if stats.min_processing_time_ms == 0 || processing_time_ms < stats.min_processing_time_ms {
                stats.min_processing_time_ms = processing_time_ms;
            }
        }

        // Mark as stopped
        {
            let mut running = dispatcher.is_running.lock().await;
            *running = false;
        }

        log::info!("Event dispatcher stopped for wallet {}", dispatcher.wallet_id);
    }

    /// Start event processing in a background task
    pub fn start_processing(&self, receiver: mpsc::UnboundedReceiver<WalletEvent>) -> tokio::task::JoinHandle<()> {
        let dispatcher = Arc::new(self.clone());
        tokio::spawn(async move {
            Self::process_events(dispatcher, receiver).await;
        })
    }

    /// Filter events by category
    pub async fn get_events_by_category(&self, category: EventCategory) -> Vec<EventType> {
        let callbacks = self.callbacks.read().await;
        callbacks
            .keys()
            .filter(|event_type| event_type.category() == category)
            .cloned()
            .collect()
    }

    /// Filter events by priority
    pub async fn get_events_by_priority(&self, priority: EventPriority) -> Vec<EventType> {
        let callbacks = self.callbacks.read().await;
        callbacks
            .keys()
            .filter(|event_type| event_type.priority() == priority)
            .cloned()
            .collect()
    }
}

// Manual Clone implementation to handle the complex types
impl Clone for EventDispatcher {
    fn clone(&self) -> Self {
        Self {
            wallet_id: self.wallet_id,
            sender: self.sender.clone(),
            callbacks: Arc::clone(&self.callbacks),
            stats: Arc::clone(&self.stats),
            callback_handlers: Arc::clone(&self.callback_handlers),
            is_running: Arc::clone(&self.is_running),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bridge::types::{EventData, ConnectivityState};
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_dispatcher_creation() {
        let (sender, _receiver) = mpsc::unbounded_channel();
        let dispatcher = EventDispatcher::new(1, sender);
        
        assert_eq!(dispatcher.wallet_id(), 1);
        assert!(!dispatcher.is_running().await);
    }

    #[tokio::test]
    async fn test_callback_registration() {
        let (sender, _receiver) = mpsc::unbounded_channel();
        let dispatcher = EventDispatcher::new(1, sender);

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = Arc::clone(&counter);

        dispatcher
            .register_callback(
                EventType::TransactionReceived,
                "test_callback".to_string(),
                move |_event| {
                    counter_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
            )
            .await;

        let callbacks = dispatcher.get_callbacks(&EventType::TransactionReceived).await;
        assert_eq!(callbacks.len(), 1);
        assert_eq!(callbacks[0].callback_id, "test_callback");

        let stats = dispatcher.get_stats().await;
        assert_eq!(stats.active_callbacks, 1);
    }

    #[tokio::test]
    async fn test_callback_unregistration() {
        let (sender, _receiver) = mpsc::unbounded_channel();
        let dispatcher = EventDispatcher::new(1, sender);

        // Register callback
        dispatcher
            .register_callback(
                EventType::BalanceUpdated,
                "test_callback".to_string(),
                |_event| Ok(()),
            )
            .await;

        // Verify registration
        let callbacks = dispatcher.get_callbacks(&EventType::BalanceUpdated).await;
        assert_eq!(callbacks.len(), 1);

        // Unregister callback
        let removed = dispatcher
            .unregister_callback(&EventType::BalanceUpdated, "test_callback")
            .await;
        assert!(removed);

        // Verify unregistration
        let callbacks = dispatcher.get_callbacks(&EventType::BalanceUpdated).await;
        assert_eq!(callbacks.len(), 0);

        let stats = dispatcher.get_stats().await;
        assert_eq!(stats.active_callbacks, 0);
    }

    #[tokio::test]
    async fn test_event_processing() {
        let (sender, receiver) = mpsc::unbounded_channel();
        let dispatcher = Arc::new(EventDispatcher::new(1, sender.clone()));

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = Arc::clone(&counter);

        // Register callback
        dispatcher
            .register_callback(
                EventType::ConnectivityStatus,
                "connectivity_callback".to_string(),
                move |_event| {
                    counter_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
            )
            .await;

        // Start processing
        let _handle = tokio::spawn({
            let dispatcher_clone = Arc::clone(&dispatcher);
            async move {
                EventDispatcher::process_events(dispatcher_clone, receiver).await;
            }
        });

        // Send event
        let event = WalletEvent::new(
            EventType::ConnectivityStatus,
            1,
            EventData::ConnectivityStatus {
                status: ConnectivityState::Connected,
                peer_count: 5,
            },
        );

        dispatcher.send_event(event).await.unwrap();

        // Wait for processing
        sleep(Duration::from_millis(10)).await;

        // Verify callback was called
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        let stats = dispatcher.get_stats().await;
        assert_eq!(stats.events_processed, 1);
        assert_eq!(stats.callback_errors, 0);
    }

    #[tokio::test]
    async fn test_multiple_callbacks_same_event() {
        let (sender, receiver) = mpsc::unbounded_channel();
        let dispatcher = Arc::new(EventDispatcher::new(1, sender.clone()));

        let counter1 = Arc::new(AtomicU32::new(0));
        let counter2 = Arc::new(AtomicU32::new(0));
        
        let counter1_clone = Arc::clone(&counter1);
        let counter2_clone = Arc::clone(&counter2);

        // Register multiple callbacks for same event type
        dispatcher
            .register_callback(
                EventType::TransactionReceived,
                "callback1".to_string(),
                move |_event| {
                    counter1_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
            )
            .await;

        dispatcher
            .register_callback(
                EventType::TransactionReceived,
                "callback2".to_string(),
                move |_event| {
                    counter2_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
            )
            .await;

        // Start processing
        let _handle = tokio::spawn({
            let dispatcher_clone = Arc::clone(&dispatcher);
            async move {
                EventDispatcher::process_events(dispatcher_clone, receiver).await;
            }
        });

        // Send event
        let event = WalletEvent::new(
            EventType::TransactionReceived,
            1,
            EventData::TransactionReceived(TransactionData {
                tx_id: 123,
                source_address: "test".to_string(),
                amount: 1000000,
                message: None,
                timestamp: 1640995200,
                status: 1,
            }),
        );

        dispatcher.send_event(event).await.unwrap();

        // Wait for processing
        sleep(Duration::from_millis(10)).await;

        // Verify both callbacks were called
        assert_eq!(counter1.load(Ordering::SeqCst), 1);
        assert_eq!(counter2.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_statistics_tracking() {
        let (sender, _receiver) = mpsc::unbounded_channel();
        let dispatcher = EventDispatcher::new(1, sender);

        let event = WalletEvent::new(
            EventType::BalanceUpdated,
            1,
            EventData::BalanceUpdated {
                available: 1000000,
                pending_incoming: 0,
                pending_outgoing: 0,
                timelocked: None,
            },
        );

        dispatcher.send_event(event).await.unwrap();

        let stats = dispatcher.get_stats().await;
        assert_eq!(stats.events_processed, 1);
        assert_eq!(stats.events_dropped, 0);
        assert!(stats.event_type_counts.contains_key("balance_updated"));
        assert!(stats.category_counts.contains_key("Balance"));
    }
}
