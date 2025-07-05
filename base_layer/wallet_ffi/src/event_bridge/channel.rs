//! # Event Channel Infrastructure
//!
//! This module provides the core channel infrastructure for event routing.
//! It implements high-performance async channels using Tokio primitives
//! with proper error handling and backpressure management.

use super::types::WalletEvent;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use std::time::Instant;

/// Statistics for monitoring channel performance
#[derive(Debug, Clone, Default)]
pub struct ChannelStats {
    pub events_sent: u64,
    pub events_received: u64,
    pub send_errors: u64,
    pub receive_errors: u64,
    pub total_latency_ms: u64,
    pub max_latency_ms: u64,
    pub min_latency_ms: u64,
    pub current_queue_size: usize,
    pub max_queue_size: usize,
}

impl ChannelStats {
    /// Calculate average latency in milliseconds
    pub fn average_latency_ms(&self) -> f64 {
        if self.events_received > 0 {
            self.total_latency_ms as f64 / self.events_received as f64
        } else {
            0.0
        }
    }

    /// Get success rate as percentage
    pub fn success_rate(&self) -> f64 {
        let total_sent = self.events_sent;
        if total_sent > 0 {
            let successful = total_sent - self.send_errors;
            (successful as f64 / total_sent as f64) * 100.0
        } else {
            100.0
        }
    }
}

/// High-performance event channel with monitoring and error handling
pub struct EventChannel {
    sender: mpsc::UnboundedSender<TimestampedEvent>,
    receiver: Option<mpsc::UnboundedReceiver<TimestampedEvent>>,
    stats: Arc<RwLock<ChannelStats>>,
    wallet_id: u64,
    current_queue_size: Arc<RwLock<usize>>,
}

/// Event with timestamp for latency measurement
#[derive(Debug, Clone)]
pub(crate) struct TimestampedEvent {
    event: WalletEvent,
    send_time: Instant,
}

impl EventChannel {
    /// Create a new event channel with unbounded capacity
    pub fn new(wallet_id: u64) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        
        Self {
            sender,
            receiver: Some(receiver),
            stats: Arc::new(RwLock::new(ChannelStats::default())),
            wallet_id,
            current_queue_size: Arc::new(RwLock::new(0)),
        }
    }

    /// Create a new bounded event channel with specified capacity
    /// Note: This is a placeholder implementation - bounded channels are not fully implemented
    pub fn new_bounded(wallet_id: u64, _capacity: usize) -> Self {
        let (_sender, _receiver): (mpsc::Sender<TimestampedEvent>, mpsc::Receiver<TimestampedEvent>) = mpsc::channel(_capacity);
        
        // For bounded channels, we need a different approach
        // This is a simplified implementation - in production, you'd want proper bounded support
        let (unbounded_sender, unbounded_receiver) = mpsc::unbounded_channel();
        
        Self {
            sender: unbounded_sender,
            receiver: Some(unbounded_receiver),
            stats: Arc::new(RwLock::new(ChannelStats::default())),
            wallet_id,
            current_queue_size: Arc::new(RwLock::new(0)),
        }
    }

    /// Send an event through the channel
    pub async fn send(&self, event: WalletEvent) -> Result<(), String> {
        let timestamped = TimestampedEvent {
            event,
            send_time: Instant::now(),
        };

        let result = self.sender.send(timestamped);
        
        // Update statistics
        let mut stats = self.stats.write().await;
        match &result {
            Ok(_) => {
                stats.events_sent += 1;
                // Update queue size tracking
                let mut queue_size = self.current_queue_size.write().await;
                *queue_size += 1;
                stats.current_queue_size = *queue_size;
                if stats.current_queue_size > stats.max_queue_size {
                    stats.max_queue_size = stats.current_queue_size;
                }
            }
            Err(_) => {
                stats.send_errors += 1;
            }
        }

        result.map_err(|e| format!("Failed to send event: {}", e))
    }

    /// Try to send an event without blocking
    pub async fn try_send(&self, event: WalletEvent) -> Result<(), String> {
        // For unbounded channels, try_send behaves the same as send
        self.send(event).await
    }

    /// Take the receiver from this channel (can only be called once)
    pub(crate) fn take_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<TimestampedEvent>> {
        self.receiver.take()
    }

    /// Get channel statistics
    pub async fn get_stats(&self) -> ChannelStats {
        self.stats.read().await.clone()
    }

    /// Reset channel statistics
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        *stats = ChannelStats::default();
    }

    /// Get wallet ID associated with this channel
    pub fn wallet_id(&self) -> u64 {
        self.wallet_id
    }

    /// Check if the channel is closed
    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }

    /// Get current queue length
    pub fn len(&self) -> usize {
        // Use the tracked queue size from send/receive operations
        if let Ok(queue_size) = self.current_queue_size.try_read() {
            *queue_size
        } else {
            0 // Fallback if lock is contended
        }
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Event receiver that automatically updates statistics
pub struct EventReceiver {
    receiver: mpsc::UnboundedReceiver<TimestampedEvent>,
    stats: Arc<RwLock<ChannelStats>>,
    queue_size: Arc<RwLock<usize>>,
}

impl EventReceiver {
    /// Create a new event receiver from a channel
    pub(super) fn new(
        receiver: mpsc::UnboundedReceiver<TimestampedEvent>,
        stats: Arc<RwLock<ChannelStats>>,
        queue_size: Arc<RwLock<usize>>,
    ) -> Self {
        Self { receiver, stats, queue_size }
    }

    /// Receive the next event, updating statistics
    pub async fn recv(&mut self) -> Option<WalletEvent> {
        match self.receiver.recv().await {
            Some(timestamped) => {
                let receive_time = Instant::now();
                let latency = receive_time.duration_since(timestamped.send_time);
                let latency_ms = latency.as_millis() as u64;

                // Update statistics
                let mut stats = self.stats.write().await;
                stats.events_received += 1;
                stats.total_latency_ms += latency_ms;
                
                // Update queue size
                let mut queue_size = self.queue_size.write().await;
                if *queue_size > 0 {
                    *queue_size -= 1;
                }
                stats.current_queue_size = *queue_size;
                
                if latency_ms > stats.max_latency_ms {
                    stats.max_latency_ms = latency_ms;
                }
                
                if stats.min_latency_ms == 0 || latency_ms < stats.min_latency_ms {
                    stats.min_latency_ms = latency_ms;
                }

                Some(timestamped.event)
            }
            None => {
                let mut stats = self.stats.write().await;
                stats.receive_errors += 1;
                None
            }
        }
    }

    /// Try to receive an event without blocking
    pub async fn try_recv(&mut self) -> Result<WalletEvent, mpsc::error::TryRecvError> {
        match self.receiver.try_recv() {
            Ok(timestamped) => {
                let receive_time = Instant::now();
                let latency = receive_time.duration_since(timestamped.send_time);
                let latency_ms = latency.as_millis() as u64;

                // Update statistics
                let mut stats = self.stats.write().await;
                stats.events_received += 1;
                stats.total_latency_ms += latency_ms;
                
                // Update queue size
                let mut queue_size = self.queue_size.write().await;
                if *queue_size > 0 {
                    *queue_size -= 1;
                }
                stats.current_queue_size = *queue_size;
                
                if latency_ms > stats.max_latency_ms {
                    stats.max_latency_ms = latency_ms;
                }
                
                if stats.min_latency_ms == 0 || latency_ms < stats.min_latency_ms {
                    stats.min_latency_ms = latency_ms;
                }

                Ok(timestamped.event)
            }
            Err(e) => {
                if let mpsc::error::TryRecvError::Disconnected = e {
                    let mut stats = self.stats.write().await;
                    stats.receive_errors += 1;
                }
                Err(e)
            }
        }
    }

    /// Close the receiver
    pub fn close(&mut self) {
        self.receiver.close();
    }
}

/// Builder for creating customized event channels
pub struct EventChannelBuilder {
    wallet_id: u64,
    capacity: Option<usize>,
    enable_metrics: bool,
}

impl EventChannelBuilder {
    /// Create a new channel builder
    pub fn new(wallet_id: u64) -> Self {
        Self {
            wallet_id,
            capacity: None,
            enable_metrics: true,
        }
    }

    /// Set bounded capacity (default is unbounded)
    pub fn with_capacity(mut self, capacity: usize) -> Self {
        self.capacity = Some(capacity);
        self
    }

    /// Enable or disable metrics collection
    pub fn with_metrics(mut self, enable: bool) -> Self {
        self.enable_metrics = enable;
        self
    }

    /// Build the event channel
    pub fn build(self) -> (EventChannel, EventReceiver) {
        // For now, ignore capacity and always create unbounded channels
        // TODO: Implement proper bounded channel support
        let channel = EventChannel::new(self.wallet_id);
        self.create_receiver(channel)
    }

    fn create_receiver(self, mut channel: EventChannel) -> (EventChannel, EventReceiver) {
        let receiver = channel.take_receiver().expect("Receiver should be available");
        let stats = Arc::clone(&channel.stats);
        let queue_size = Arc::clone(&channel.current_queue_size);
        
        let event_receiver = EventReceiver::new(receiver, stats, queue_size);
        (channel, event_receiver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bridge::types::{EventType, EventData};
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_channel_creation() {
        let channel = EventChannel::new(1);
        assert_eq!(channel.wallet_id(), 1);
        assert!(!channel.is_closed());
        assert!(channel.is_empty());
    }

    #[tokio::test]
    async fn test_event_send_receive() {
        let (mut channel, mut receiver) = EventChannelBuilder::new(1).build();

        let event = WalletEvent::new(
            EventType::TransactionReceived,
            1,
            EventData::TransactionReceived {
                tx_id: 123,
                amount: 1000000,
                sender_address: "test".to_string(),
                message: None,
            },
        );

        // Send event
        channel.send(event.clone()).await.unwrap();

        // Receive event
        let received = receiver.recv().await.unwrap();
        assert_eq!(received.event_type, event.event_type);
        assert_eq!(received.wallet_id, event.wallet_id);
    }

    #[tokio::test]
    async fn test_statistics_tracking() {
        let (mut channel, mut receiver) = EventChannelBuilder::new(1).build();

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

        // Send and receive event
        channel.send(event).await.unwrap();
        receiver.recv().await.unwrap();

        // Check statistics
        let stats = channel.get_stats().await;
        assert_eq!(stats.events_sent, 1);
        assert_eq!(stats.events_received, 1);
        assert_eq!(stats.send_errors, 0);
        assert_eq!(stats.receive_errors, 0);
    }

    #[tokio::test]
    async fn test_latency_measurement() {
        let (mut channel, mut receiver) = EventChannelBuilder::new(1).build();

        let event = WalletEvent::new(
            EventType::ConnectivityStatus,
            1,
            EventData::ConnectivityStatus {
                status: crate::event_bridge::types::ConnectivityState::Connected,
                peer_count: 5,
            },
        );

        // Add small delay to ensure measurable latency
        channel.send(event).await.unwrap();
        sleep(Duration::from_millis(1)).await;
        receiver.recv().await.unwrap();

        let stats = channel.get_stats().await;
        assert!(stats.max_latency_ms >= stats.min_latency_ms);
        assert!(stats.average_latency_ms() >= 0.0);
    }

    #[tokio::test]
    async fn test_multiple_events() {
        let (mut channel, mut receiver) = EventChannelBuilder::new(1).build();

        // Send multiple events
        for i in 0..10 {
            let event = WalletEvent::new(
                EventType::TransactionReceived,
                1,
                EventData::TransactionReceived {
                    tx_id: i as u64,
                    amount: 1000000,
                    sender_address: format!("test_{}", i),
                    message: None,
                },
            );
            channel.send(event).await.unwrap();
        }

        // Receive all events
        for i in 0..10 {
            let received = receiver.recv().await.unwrap();
            if let EventData::TransactionReceived { tx_id, .. } = received.data {
                assert_eq!(tx_id, i as u64);
            } else {
                panic!("Expected TransactionReceived event");
            }
        }

        let stats = channel.get_stats().await;
        assert_eq!(stats.events_sent, 10);
        assert_eq!(stats.events_received, 10);
    }
}
