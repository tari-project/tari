use pyo3::prelude::*;
use pyo3::types::PyFunction;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

// Import event types from the main event bridge system
use crate::event_bridge::types::{WalletEvent, EventType, EventData, TransactionData};
use crate::event_bridge::transaction::extract_transaction_data;

/// Thread-safe event callback manager
pub struct PythonEventBridge {
    callbacks: Arc<Mutex<HashMap<String, PyObject>>>,
    event_sender: mpsc::UnboundedSender<WalletEvent>,
}

impl PythonEventBridge {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<WalletEvent>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let bridge = Self {
            callbacks: Arc::new(Mutex::new(HashMap::new())),
            event_sender: sender,
        };
        (bridge, receiver)
    }

    /// Register a Python callback for a specific event type
    pub async fn register_callback(&self, event_type: &str, callback: PyObject) {
        let mut callbacks = self.callbacks.lock().await;
        callbacks.insert(event_type.to_string(), callback);
    }

    /// Send an event to Python (called from Rust async context)
    pub fn send_event(&self, event: WalletEvent) {
        if let Err(e) = self.event_sender.send(event) {
            eprintln!("Failed to send wallet event: {}", e);
        }
    }

    /// Process events and call Python callbacks (runs in dedicated task)
    pub async fn process_events(
        callbacks: Arc<Mutex<HashMap<String, PyObject>>>,
        mut receiver: mpsc::UnboundedReceiver<WalletEvent>,
    ) {
        while let Some(event) = receiver.recv().await {
            let callbacks_guard = callbacks.lock().await;
            let event_name = event.event_name();
            
            match &event.data {
                EventData::TransactionReceived(tx_data) => {
                    if let Some(callback) = callbacks_guard.get(event_name) {
                        Self::call_python_callback_safe(callback, (tx_data.tx_id, tx_data.amount)).await;
                    }
                },
                EventData::BalanceUpdated { available, pending_incoming, pending_outgoing } => {
                    if let Some(callback) = callbacks_guard.get(event_name) {
                        Self::call_python_callback_safe(callback, (*available, *pending_incoming, *pending_outgoing)).await;
                    }
                },
                EventData::ConnectivityStatus { status } => {
                    if let Some(callback) = callbacks_guard.get(event_name) {
                        Self::call_python_callback_safe(callback, status.clone()).await;
                    }
                },
                EventData::TransactionMined { tx_id, amount, block_height } => {
                    if let Some(callback) = callbacks_guard.get(event_name) {
                        Self::call_python_callback_safe(callback, (*tx_id, *amount, *block_height)).await;
                    }
                },
                EventData::BaseNodeState { is_synced, height } => {
                    if let Some(callback) = callbacks_guard.get(event_name) {
                        Self::call_python_callback_safe(callback, (*is_synced, *height)).await;
                    }
                },
                // Handle other event types as needed...
                _ => {}
            }
        }
    }

    /// Safely call Python callback with GIL management
    async fn call_python_callback_safe<T>(callback: &PyObject, args: T) 
    where 
        T: IntoPy<PyObject> + Send + 'static,
    {
        // Use tokio::task::spawn_blocking to handle GIL acquisition safely
        let callback_clone = callback.clone();
        tokio::task::spawn_blocking(move || {
            Python::with_gil(|py| {
                if let Ok(func) = callback_clone.downcast_bound::<PyFunction>(py) {
                    let py_args = args.into_py(py);
                    if let Err(e) = func.call1((py_args,)) {
                        eprintln!("Python callback error: {}", e);
                    }
                }
            });
        }).await.unwrap_or_else(|e| {
            eprintln!("Failed to execute Python callback: {}", e);
        });
    }
}

/// Safe callback wrappers that send events to the bridge
pub mod safe_callbacks {
    use super::*;
    use crate::*;
    use libc::{c_void, c_ulonglong};

    pub unsafe extern "C" fn bridged_balance_updated(
        context: *mut c_void, 
        balance: *mut TariBalance
    ) {
        if context.is_null() || balance.is_null() {
            return;
        }

        let bridge = &*(context as *const PythonEventBridge);
        let balance_ref = &*balance;
        
        let event_data = EventData::BalanceUpdated {
            available: balance_ref.available,
            pending_incoming: balance_ref.pending_incoming,
            pending_outgoing: balance_ref.pending_outgoing,
        };
        
        let event = WalletEvent::new(EventType::BalanceUpdated, 0, event_data);
        bridge.send_event(event);
    }

    pub unsafe extern "C" fn bridged_transaction_received(
        context: *mut c_void,
        tx: *mut TariPendingInboundTransaction
    ) {
        if context.is_null() || tx.is_null() {
            return;
        }

        let bridge = &*(context as *const PythonEventBridge);
        
        // Use extraction safety layer to convert C data to safe structure
        match extract_transaction_data(tx as *mut c_void) {
            Ok(tx_data) => {
                let event_data = EventData::TransactionReceived(tx_data);
                let event = WalletEvent::new(EventType::TransactionReceived, 0, event_data);
                bridge.send_event(event);
            },
            Err(e) => {
                eprintln!("Failed to extract transaction data: {}", e);
            }
        }
    }

    pub unsafe extern "C" fn bridged_connectivity_status(
        context: *mut c_void,
        status: c_ulonglong
    ) {
        if context.is_null() {
            return;
        }

        let bridge = &*(context as *const PythonEventBridge);
        let status_str = match status {
            0 => "Disconnected",
            1 => "Connecting", 
            2 => "Connected",
            _ => "Unknown",
        };
        
        let event_data = EventData::ConnectivityStatus {
            status: status_str.to_string(),
        };
        
        let event = WalletEvent::new(EventType::ConnectivityStatus, 0, event_data);
        bridge.send_event(event);
    }
} 