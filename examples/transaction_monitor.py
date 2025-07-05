#!/usr/bin/env python3
"""
Tari Wallet Transaction Monitor

A comprehensive Python example demonstrating real-time transaction monitoring
using the Tari wallet event bridge system. This example shows how to:

- Set up wallet and event bridge integration
- Handle all transaction event types
- Implement proper error handling and recovery
- Monitor performance and memory usage
- Gracefully handle shutdown and cleanup

Features:
- Real-time transaction event monitoring
- Comprehensive event type coverage
- Performance metrics and statistics
- Memory usage monitoring  
- Graceful shutdown handling
- Async/sync pattern examples
- Error recovery and retry logic
"""

import asyncio
import logging
import signal
import sys
import time
import traceback
from dataclasses import dataclass
from datetime import datetime
from typing import Dict, List, Optional
import argparse
import json

# Import Tari wallet components
# Note: This assumes the Tari Python bindings are installed
try:
    from tari_wallet import (
        PyTariWallet, 
        EventBridge, 
        EventType,
        TransactionData,
        BalanceData,
        WalletEvent
    )
    from tari_wallet.testing import MockEventBridge  # For testing
except ImportError as e:
    print(f"Error importing Tari wallet: {e}")
    print("Please ensure Tari Python bindings are installed:")
    print("pip install tari-wallet")
    sys.exit(1)

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
    handlers=[
        logging.StreamHandler(),
        logging.FileHandler('transaction_monitor.log')
    ]
)
logger = logging.getLogger(__name__)

@dataclass
class MonitoringStats:
    """Statistics for transaction monitoring"""
    events_processed: int = 0
    transactions_received: int = 0
    transactions_sent: int = 0
    transactions_mined: int = 0
    transactions_cancelled: int = 0
    balance_updates: int = 0
    start_time: float = 0.0
    last_activity: float = 0.0
    
    def events_per_second(self) -> float:
        """Calculate events per second"""
        elapsed = time.time() - self.start_time
        return self.events_processed / elapsed if elapsed > 0 else 0.0
    
    def to_dict(self) -> Dict:
        """Convert stats to dictionary for JSON serialization"""
        return {
            'events_processed': self.events_processed,
            'transactions_received': self.transactions_received,
            'transactions_sent': self.transactions_sent,
            'transactions_mined': self.transactions_mined,
            'transactions_cancelled': self.transactions_cancelled,
            'balance_updates': self.balance_updates,
            'events_per_second': self.events_per_second(),
            'uptime_seconds': time.time() - self.start_time,
            'last_activity': self.last_activity
        }

class TransactionMonitor:
    """
    Comprehensive transaction monitoring system using Tari wallet event bridge.
    
    This class demonstrates:
    - Event bridge setup and configuration
    - Comprehensive event handling for all transaction types
    - Error recovery and resilience patterns
    - Performance monitoring and statistics
    - Memory usage tracking
    - Graceful shutdown handling
    """
    
    def __init__(self, wallet_passphrase: str, use_mock: bool = False):
        """
        Initialize the transaction monitor.
        
        Args:
            wallet_passphrase: Passphrase for wallet creation/access
            use_mock: Use mock event bridge for testing (default: False)
        """
        self.wallet_passphrase = wallet_passphrase
        self.use_mock = use_mock
        self.wallet: Optional[PyTariWallet] = None
        self.bridge: Optional[EventBridge] = None
        self.stats = MonitoringStats()
        self.running = False
        self.shutdown_event = asyncio.Event()
        
        # Event handlers storage for cleanup
        self.event_handlers = []
        
        # Performance monitoring
        self.performance_samples = []
        self.max_samples = 1000
        
    async def initialize(self):
        """Initialize wallet and event bridge"""
        try:
            logger.info("Initializing Tari wallet transaction monitor...")
            
            # Create wallet
            if not self.use_mock:
                self.wallet = PyTariWallet.create_with_passphrase(self.wallet_passphrase)
                logger.info("Wallet created successfully")
                
                # Create event bridge
                self.bridge = EventBridge.new()
            else:
                # Use mock for testing
                self.bridge = MockEventBridge()
                logger.info("Using mock event bridge for testing")
            
            # Set up event handlers
            self._setup_event_handlers()
            
            # Initialize statistics
            self.stats.start_time = time.time()
            self.stats.last_activity = time.time()
            
            logger.info("Transaction monitor initialized successfully")
            
        except Exception as e:
            logger.error(f"Failed to initialize transaction monitor: {e}")
            logger.error(traceback.format_exc())
            raise

    def _setup_event_handlers(self):
        """Set up all event handlers for comprehensive transaction monitoring"""
        
        # Transaction Events
        @self.bridge.on_event(EventType.TransactionReceived)
        async def handle_transaction_received(event: WalletEvent):
            """Handle incoming transaction events"""
            try:
                data = event.data
                self.stats.transactions_received += 1
                self.stats.events_processed += 1
                self.stats.last_activity = time.time()
                
                logger.info(
                    f"📥 RECEIVED: Transaction {data.tx_id} | "
                    f"Amount: {data.amount:,} µT | "
                    f"From: {data.source_address[:20]}... | "
                    f"Message: {data.message[:50] if data.message else 'None'}"
                )
                
                # Example: Process high-value transactions differently
                if data.amount >= 10_000_000:  # 10 XTR
                    logger.warning(f"🚨 HIGH VALUE: Transaction {data.tx_id} received for {data.amount:,} µT")
                
                await self._record_performance_sample('transaction_received')
                
            except Exception as e:
                logger.error(f"Error handling transaction received: {e}")

        @self.bridge.on_event(EventType.TransactionBroadcast)
        async def handle_transaction_broadcast(event: WalletEvent):
            """Handle transaction broadcast events"""
            try:
                data = event.data
                self.stats.transactions_sent += 1
                self.stats.events_processed += 1
                self.stats.last_activity = time.time()
                
                logger.info(
                    f"📡 BROADCAST: Transaction {data.tx_id} | "
                    f"Amount: {data.amount:,} µT | "
                    f"To: {data.source_address[:20]}..."
                )
                
                await self._record_performance_sample('transaction_broadcast')
                
            except Exception as e:
                logger.error(f"Error handling transaction broadcast: {e}")

        @self.bridge.on_event(EventType.TransactionMined)
        async def handle_transaction_mined(event: WalletEvent):
            """Handle transaction mined events"""
            try:
                data = event.data
                self.stats.transactions_mined += 1
                self.stats.events_processed += 1
                self.stats.last_activity = time.time()
                
                logger.info(
                    f"⛏️  MINED: Transaction {data.tx_id} | "
                    f"Amount: {data.amount:,} µT | "
                    f"Status: Confirmed ✅"
                )
                
                await self._record_performance_sample('transaction_mined')
                
            except Exception as e:
                logger.error(f"Error handling transaction mined: {e}")

        @self.bridge.on_event(EventType.TransactionCancellation)
        async def handle_transaction_cancelled(event: WalletEvent):
            """Handle transaction cancellation events"""
            try:
                data = event.data
                self.stats.transactions_cancelled += 1
                self.stats.events_processed += 1
                self.stats.last_activity = time.time()
                
                logger.warning(
                    f"❌ CANCELLED: Transaction {data.tx_id} | "
                    f"Amount: {data.amount:,} µT | "
                    f"Reason: {data.message or 'Unknown'}"
                )
                
                await self._record_performance_sample('transaction_cancelled')
                
            except Exception as e:
                logger.error(f"Error handling transaction cancellation: {e}")

        @self.bridge.on_event(EventType.TransactionReply)
        async def handle_transaction_reply(event: WalletEvent):
            """Handle transaction reply events"""
            try:
                data = event.data
                self.stats.events_processed += 1
                self.stats.last_activity = time.time()
                
                logger.info(
                    f"💬 REPLY: Transaction {data.tx_id} | "
                    f"Reply: {data.message[:100] if data.message else 'No message'}"
                )
                
                await self._record_performance_sample('transaction_reply')
                
            except Exception as e:
                logger.error(f"Error handling transaction reply: {e}")

        @self.bridge.on_event(EventType.TransactionFinalized)
        async def handle_transaction_finalized(event: WalletEvent):
            """Handle transaction finalized events"""
            try:
                data = event.data
                self.stats.events_processed += 1
                self.stats.last_activity = time.time()
                
                logger.info(f"✔️  FINALIZED: Transaction {data.tx_id} ready for broadcast")
                
                await self._record_performance_sample('transaction_finalized')
                
            except Exception as e:
                logger.error(f"Error handling transaction finalized: {e}")

        # Balance Events
        @self.bridge.on_event(EventType.BalanceUpdated)
        async def handle_balance_updated(event: WalletEvent):
            """Handle balance update events"""
            try:
                data = event.data
                self.stats.balance_updates += 1
                self.stats.events_processed += 1
                self.stats.last_activity = time.time()
                
                logger.info(
                    f"💰 BALANCE: Available: {data.available_balance:,} µT | "
                    f"Pending In: {data.pending_incoming_balance:,} µT | "
                    f"Pending Out: {data.pending_outgoing_balance:,} µT"
                )
                
                # Log time-locked balance if present
                if data.time_locked_balance:
                    logger.info(f"🔒 Time Locked: {data.time_locked_balance:,} µT")
                
                await self._record_performance_sample('balance_updated')
                
            except Exception as e:
                logger.error(f"Error handling balance update: {e}")

        # Network Events
        @self.bridge.on_event(EventType.ConnectivityStatus)
        async def handle_connectivity_status(event: WalletEvent):
            """Handle connectivity status events"""
            try:
                self.stats.events_processed += 1
                self.stats.last_activity = time.time()
                
                logger.info(f"🌐 CONNECTIVITY: Status changed - {event.data}")
                
                await self._record_performance_sample('connectivity_status')
                
            except Exception as e:
                logger.error(f"Error handling connectivity status: {e}")

        # Store handlers for cleanup
        self.event_handlers = [
            handle_transaction_received,
            handle_transaction_broadcast,
            handle_transaction_mined,
            handle_transaction_cancelled,
            handle_transaction_reply,
            handle_transaction_finalized,
            handle_balance_updated,
            handle_connectivity_status
        ]

    async def _record_performance_sample(self, event_type: str):
        """Record performance sample for monitoring"""
        sample = {
            'timestamp': time.time(),
            'event_type': event_type,
            'processing_time': time.time() - self.stats.last_activity
        }
        
        self.performance_samples.append(sample)
        
        # Keep only recent samples
        if len(self.performance_samples) > self.max_samples:
            self.performance_samples = self.performance_samples[-self.max_samples:]

    async def start_monitoring(self):
        """Start the transaction monitoring system"""
        try:
            logger.info("Starting transaction monitoring...")
            self.running = True
            
            # Start the event bridge
            await self.bridge.start()
            logger.info("Event bridge started successfully")
            
            # Start background tasks
            tasks = [
                asyncio.create_task(self._statistics_reporter()),
                asyncio.create_task(self._performance_monitor()),
                asyncio.create_task(self._memory_monitor())
            ]
            
            if self.use_mock:
                # Add mock event generator for testing
                tasks.append(asyncio.create_task(self._mock_event_generator()))
            
            logger.info("🚀 Transaction monitor is now running...")
            logger.info("Press Ctrl+C to stop monitoring")
            
            # Wait for shutdown signal
            await self.shutdown_event.wait()
            
            # Cancel background tasks
            for task in tasks:
                task.cancel()
                try:
                    await task
                except asyncio.CancelledError:
                    pass
            
        except Exception as e:
            logger.error(f"Error in monitoring loop: {e}")
            logger.error(traceback.format_exc())
            raise
        finally:
            await self.stop_monitoring()

    async def stop_monitoring(self):
        """Stop the transaction monitoring system"""
        logger.info("Stopping transaction monitor...")
        self.running = False
        
        try:
            if self.bridge:
                await self.bridge.stop()
                logger.info("Event bridge stopped")
            
            # Final statistics report
            await self._print_final_statistics()
            
        except Exception as e:
            logger.error(f"Error during shutdown: {e}")
        
        logger.info("Transaction monitor stopped")

    async def _statistics_reporter(self):
        """Background task to report statistics periodically"""
        while self.running:
            try:
                await asyncio.sleep(30)  # Report every 30 seconds
                
                if self.stats.events_processed > 0:
                    stats_dict = self.stats.to_dict()
                    logger.info(
                        f"📊 STATS: Events: {stats_dict['events_processed']} | "
                        f"Rate: {stats_dict['events_per_second']:.1f}/sec | "
                        f"Received: {stats_dict['transactions_received']} | "
                        f"Mined: {stats_dict['transactions_mined']}"
                    )
                
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"Error in statistics reporter: {e}")

    async def _performance_monitor(self):
        """Background task to monitor performance"""
        while self.running:
            try:
                await asyncio.sleep(60)  # Check every minute
                
                if len(self.performance_samples) >= 10:
                    # Calculate average processing time
                    recent_samples = self.performance_samples[-100:]
                    avg_processing_time = sum(s['processing_time'] for s in recent_samples) / len(recent_samples)
                    
                    # Alert if processing time is high
                    if avg_processing_time > 0.001:  # > 1ms
                        logger.warning(f"⚠️  High processing time detected: {avg_processing_time*1000:.2f}ms")
                    
                    # Event type distribution
                    event_types = {}
                    for sample in recent_samples:
                        event_type = sample['event_type']
                        event_types[event_type] = event_types.get(event_type, 0) + 1
                    
                    logger.debug(f"Event distribution: {event_types}")
                
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"Error in performance monitor: {e}")

    async def _memory_monitor(self):
        """Background task to monitor memory usage"""
        while self.running:
            try:
                await asyncio.sleep(120)  # Check every 2 minutes
                
                try:
                    import psutil
                    import os
                    
                    process = psutil.Process(os.getpid())
                    memory_mb = process.memory_info().rss / 1024 / 1024
                    cpu_percent = process.cpu_percent()
                    
                    logger.debug(f"Memory: {memory_mb:.1f}MB | CPU: {cpu_percent:.1f}%")
                    
                    # Alert if memory usage is high
                    if memory_mb > 200:  # > 200MB
                        logger.warning(f"⚠️  High memory usage: {memory_mb:.1f}MB")
                    
                except ImportError:
                    # psutil not available, skip memory monitoring
                    pass
                
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"Error in memory monitor: {e}")

    async def _mock_event_generator(self):
        """Generate mock events for testing (only in mock mode)"""
        if not self.use_mock:
            return
        
        logger.info("Starting mock event generator for testing...")
        event_id = 1
        
        while self.running:
            try:
                await asyncio.sleep(2)  # Generate event every 2 seconds
                
                # Create mock transaction data
                mock_data = TransactionData(
                    tx_id=event_id,
                    source_address=f"mock_address_{event_id}",
                    amount=1000000 + (event_id * 100000),
                    message=f"Mock transaction {event_id}",
                    timestamp=int(time.time()),
                    status=1
                )
                
                # Emit different event types randomly
                import random
                event_types = [
                    EventType.TransactionReceived,
                    EventType.TransactionBroadcast,
                    EventType.TransactionMined,
                    EventType.BalanceUpdated
                ]
                
                event_type = random.choice(event_types)
                await self.bridge.emit_event(event_type, mock_data)
                
                event_id += 1
                
                # Occasionally generate balance update
                if event_id % 5 == 0:
                    balance_data = BalanceData(
                        available_balance=50000000 + (event_id * 1000000),
                        time_locked_balance=None,
                        pending_incoming_balance=1000000,
                        pending_outgoing_balance=500000
                    )
                    await self.bridge.emit_event(EventType.BalanceUpdated, balance_data)
                
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"Error in mock event generator: {e}")

    async def _print_final_statistics(self):
        """Print final statistics on shutdown"""
        stats_dict = self.stats.to_dict()
        
        logger.info("=" * 60)
        logger.info("FINAL TRANSACTION MONITORING STATISTICS")
        logger.info("=" * 60)
        logger.info(f"Total Events Processed: {stats_dict['events_processed']}")
        logger.info(f"Transactions Received: {stats_dict['transactions_received']}")
        logger.info(f"Transactions Sent: {stats_dict['transactions_sent']}")
        logger.info(f"Transactions Mined: {stats_dict['transactions_mined']}")
        logger.info(f"Transactions Cancelled: {stats_dict['transactions_cancelled']}")
        logger.info(f"Balance Updates: {stats_dict['balance_updates']}")
        logger.info(f"Average Rate: {stats_dict['events_per_second']:.2f} events/second")
        logger.info(f"Total Uptime: {stats_dict['uptime_seconds']:.0f} seconds")
        logger.info("=" * 60)
        
        # Save statistics to file
        try:
            with open('transaction_monitor_stats.json', 'w') as f:
                json.dump(stats_dict, f, indent=2)
            logger.info("Statistics saved to transaction_monitor_stats.json")
        except Exception as e:
            logger.error(f"Failed to save statistics: {e}")

def setup_signal_handlers(monitor: TransactionMonitor):
    """Set up signal handlers for graceful shutdown"""
    
    def signal_handler(signum, frame):
        logger.info(f"Received signal {signum}, initiating shutdown...")
        monitor.shutdown_event.set()
    
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)

async def main():
    """Main function to run the transaction monitor"""
    parser = argparse.ArgumentParser(description='Tari Wallet Transaction Monitor')
    parser.add_argument(
        '--passphrase', 
        default='transaction_monitor_wallet',
        help='Wallet passphrase (default: transaction_monitor_wallet)'
    )
    parser.add_argument(
        '--mock', 
        action='store_true',
        help='Use mock event bridge for testing'
    )
    parser.add_argument(
        '--log-level',
        choices=['DEBUG', 'INFO', 'WARNING', 'ERROR'],
        default='INFO',
        help='Set logging level (default: INFO)'
    )
    
    args = parser.parse_args()
    
    # Configure log level
    logging.getLogger().setLevel(getattr(logging, args.log_level))
    
    # Create monitor
    monitor = TransactionMonitor(args.passphrase, args.mock)
    
    # Set up signal handlers
    setup_signal_handlers(monitor)
    
    try:
        # Initialize and start monitoring
        await monitor.initialize()
        await monitor.start_monitoring()
        
    except KeyboardInterrupt:
        logger.info("Interrupted by user")
    except Exception as e:
        logger.error(f"Fatal error: {e}")
        logger.error(traceback.format_exc())
        return 1
    
    return 0

if __name__ == '__main__':
    try:
        exit_code = asyncio.run(main())
        sys.exit(exit_code)
    except Exception as e:
        logger.error(f"Failed to start transaction monitor: {e}")
        sys.exit(1)
