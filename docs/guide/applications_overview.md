# Tari Applications Overview

This document provides a comprehensive overview of all applications in the Tari project, their purposes, command-line options, and configuration overrides.

## Overview

The Tari project consists of several applications that work together to provide a complete cryptocurrency ecosystem:

- **minotari_node** - Full base node implementation for the Tari blockchain
- **minotari_console_wallet** - Command-line wallet for managing Tari transactions
- **minotari_miner** - Mining implementation for SHA-3 based proof-of-work
- **minotari_merge_mining_proxy** - Proxy server for merge mining with Monero/XMRig
- **minotari_ledger_wallet** - Hardware wallet implementation for Ledger devices
- **deps_only** - Development utility for building dependencies

## Applications

### 1. minotari_node

**Description**: The full base node implementation that maintains the Tari blockchain, validates transactions, and provides network services.

**Binary Name**: `minotari_node`

**Purpose**: 
- Maintains a full copy of the Tari blockchain
- Validates and processes transactions
- Provides gRPC API for other applications
- Participates in peer-to-peer network consensus
- Optionally enables mining functionality

#### Command Line Options

**Common Options** (inherited from CommonCliArgs):
- `-b, --base-path, --base-dir <PATH>` - Directory to store files (default: platform-specific, env: `TARI_BASE_DIR`)
- `-c, --config <FILE>` - Configuration file path (default: `config/config.toml`)
- `-l, --log-config <FILE>` - Log configuration file path
- `--log-path <PATH>` - Directory for log files
- `--network <NETWORK>` - Network to use (env: `TARI_NETWORK`)
- `-p <KEY=VALUE>` - Configuration property overrides (multiple allowed)

**Node-Specific Options**:
- `--init` - Create default configuration file if it doesn't exist
- `--rebuild-db` - Rebuild the database, adding blocks one by one
- `-n, --non-interactive-mode` - Run without UI (env: `TARI_NON_INTERACTIVE`)
- `--watch <COMMAND>` - Watch a command in non-interactive mode
- `--profile` - Enable Tokio Console profiling
- `--grpc-enabled` - Enable gRPC server (env: `MINOTARI_NODE_ENABLE_GRPC`)
- `--mining-enabled` - Enable mining functionality (env: `MINOTARI_NODE_ENABLE_MINING`)
- `--second-layer-grpc-enabled` - Enable second layer gRPC (env: `MINOTARI_NODE_SECOND_LAYER_GRPC_ENABLED`)
- `--disable-splash-screen` - Disable the startup splash screen
- `-z, --libtor-data-dir <PATH>` - Path to libtor data directory

#### All Configuration Overrides (-p options)

**Core Node Configuration**:
- `base_node.network=<network>` - Network type (mainnet, esmeralda, nextnet, stagenet, igor)
- `base_node.identity_file=<path>` - Node identity file path
- `base_node.use_libtor=<bool>` - Use built-in Tor instance
- `base_node.tor_identity_file=<path>` - Tor identity file path
- `base_node.db_type=<type>` - Database backend (lmdb, memory)
- `base_node.data_dir=<path>` - Persistent data directory
- `base_node.lmdb_path=<path>` - LMDB database path
- `base_node.max_randomx_vms=<num>` - Maximum RandomX VMs (default: 5)
- `base_node.bypass_range_proof_verification=<bool>` - Skip range proof verification
- `base_node.force_sync_peers=<peers>` - Force sync from specific peers
- `base_node.messaging_request_timeout=<seconds>` - Request timeout (default: 60)
- `base_node.status_line_interval=<seconds>` - CLI status update interval (default: 5)
- `base_node.buffer_size=<size>` - Publish/subscribe buffer size (default: 1500)
- `base_node.metadata_auto_ping_interval=<seconds>` - Peer ping interval (default: 30)
- `base_node.report_grpc_error=<bool>` - Obscure gRPC error responses
- `base_node.tari_pulse_interval=<seconds>` - Network sync check interval (default: 120)
- `base_node.tari_pulse_health_check=<seconds>` - Health check interval (default: 600)

**gRPC Configuration**:
- `base_node.grpc_enabled=<bool>` - Enable gRPC server
- `base_node.grpc_address=<address>` - gRPC server address
- `base_node.grpc_server_allow_methods=<methods>` - Allowed gRPC methods (comma-separated)
- `base_node.grpc_authentication=<auth>` - gRPC authentication mode
- `base_node.grpc_tls_enabled=<bool>` - Enable gRPC TLS

**LMDB Configuration**:
- `base_node.lmdb.init_size_bytes=<bytes>` - Initial LMDB size (default: 16MB)
- `base_node.lmdb.grow_size_bytes=<bytes>` - LMDB growth size (default: 16MB)
- `base_node.lmdb.resize_threshold_bytes=<bytes>` - Resize threshold (default: 4MB)

**Storage Configuration**:
- `base_node.storage.orphan_storage_capacity=<num>` - Max orphan blocks (default: 720)
- `base_node.storage.pruning_horizon=<blocks>` - Pruning horizon (default: 0)
- `base_node.storage.pruning_interval=<blocks>` - Pruning interval (default: 50)
- `base_node.storage.track_reorgs=<bool>` - Record reorgs for list-reorgs command
- `base_node.storage.cleanup_orphans_at_startup=<bool>` - Clean orphans on startup

**Mempool Configuration**:
- `base_node.mempool.unconfirmed_pool.storage_capacity=<num>` - Max unconfirmed TXs (default: 40,000)
- `base_node.mempool.unconfirmed_pool.weight_tx_skip_count=<num>` - TX skip count (default: 20)
- `base_node.mempool.unconfirmed_pool.min_fee=<fee>` - Minimum fee (default: 50)
- `base_node.mempool.reorg_pool.expiry_height=<blocks>` - Reorg pool expiry (default: 5)
- `base_node.mempool.service.initial_sync_num_peers=<num>` - Sync peer count (default: 2)
- `base_node.mempool.service.initial_sync_max_transactions=<num>` - Max sync TXs (default: 10,000)
- `base_node.mempool.service.block_sync_trigger=<blocks>` - Sync trigger blocks (default: 5)

**State Machine Configuration**:
- `base_node.state_machine.blockchain_sync_config.initial_max_sync_latency=<seconds>` - Max sync latency (default: 240)
- `base_node.state_machine.blockchain_sync_config.max_latency_increase=<seconds>` - Latency increase (default: 10)
- `base_node.state_machine.blockchain_sync_config.ban_period=<seconds>` - Long ban period (default: 7200)
- `base_node.state_machine.blockchain_sync_config.short_ban_period=<seconds>` - Short ban period (default: 240)
- `base_node.state_machine.blockchain_sync_config.forced_sync_peers=<peers>` - Forced sync peers
- `base_node.state_machine.blockchain_sync_config.validation_concurrency=<threads>` - Validation threads (default: 6)
- `base_node.state_machine.blockchain_sync_config.rpc_deadline=<seconds>` - RPC deadline (default: 240)
- `base_node.state_machine.blocks_behind_before_considered_lagging=<blocks>` - Lagging threshold (default: 1)
- `base_node.state_machine.time_before_considered_lagging=<seconds>` - Lagging time threshold (default: 10)

**P2P Configuration**:
- `base_node.p2p.public_addresses=<addresses>` - Public addresses for discovery
- `base_node.p2p.auxiliary_tcp_listener_address=<address>` - Additional TCP listener
- `base_node.p2p.datastore_path=<path>` - Peer database path (default: "peer_db")
- `base_node.p2p.peer_database_name=<name>` - Peer database name (default: "peers")
- `base_node.p2p.max_concurrent_inbound_tasks=<num>` - Max inbound tasks (default: 100)
- `base_node.p2p.max_concurrent_outbound_tasks=<num>` - Max outbound tasks (default: 100)
- `base_node.p2p.allow_test_addresses=<bool>` - Allow test addresses
- `base_node.p2p.listener_liveness_allowlist_cidrs=<cidrs>` - Liveness allowlist CIDRs
- `base_node.p2p.listener_self_liveness_check_interval=<seconds>` - Self liveness checks
- `base_node.p2p.rpc_max_simultaneous_sessions=<num>` - Max RPC sessions (default: 100)
- `base_node.p2p.rpc_max_sessions_per_peer=<num>` - Max RPC per peer (default: 10)
- `base_node.p2p.cull_oldest_peer_rpc_connection_on_full=<bool>` - Cull old RPC connections
- `base_node.p2p.base_node_http_wallet_query_service_listen_address=<address>` - Wallet query service address

**Transport Configuration**:
- `base_node.p2p.transport.type=<type>` - Transport type (tcp, tor, socks5, memory)

**TCP Transport**:
- `base_node.p2p.transport.tcp.listener_address=<address>` - TCP listener address
- `base_node.p2p.transport.tcp.tor_socks_address=<address>` - Tor SOCKS proxy address
- `base_node.p2p.transport.tcp.tor_socks_auth=<auth>` - Tor SOCKS authentication

**Tor Transport**:
- `base_node.p2p.transport.tor.control_address=<address>` - Tor control server address
- `base_node.p2p.transport.tor.socks_auth=<auth>` - SOCKS authentication
- `base_node.p2p.transport.tor.socks_address_override=<address>` - SOCKS address override
- `base_node.p2p.transport.tor.control_auth=<auth>` - Tor control authentication
- `base_node.p2p.transport.tor.onion_port=<port>` - Onion service port
- `base_node.p2p.transport.tor.proxy_bypass_addresses=<addresses>` - Proxy bypass addresses
- `base_node.p2p.transport.tor.proxy_bypass_for_outbound_tcp=<bool>` - Bypass proxy for outbound TCP
- `base_node.p2p.transport.tor.forward_address=<address>` - Traffic forward address
- `base_node.p2p.transport.tor.listener_address_override=<address>` - Listener address override

**SOCKS5 Transport**:
- `base_node.p2p.transport.socks.proxy_address=<address>` - SOCKS5 proxy address
- `base_node.p2p.transport.socks.auth=<auth>` - SOCKS5 authentication

**Memory Transport**:
- `base_node.p2p.transport.memory.listener_address=<address>` - Memory transport address

**DHT Configuration**:
- `base_node.p2p.dht.database_url=<url>` - DHT database URL
- `base_node.p2p.dht.num_neighbouring_nodes=<num>` - Neighboring nodes (default: 8)
- `base_node.p2p.dht.num_random_nodes=<num>` - Random nodes (default: 4)
- `base_node.p2p.dht.minimize_connections=<bool>` - Minimize connections
- `base_node.p2p.dht.broadcast_factor=<num>` - Broadcast peer count (default: 8)
- `base_node.p2p.dht.propagation_factor=<num>` - Propagation peer count (default: 4)
- `base_node.p2p.dht.dedup_cache_capacity=<num>` - Message dedup cache size (default: 2,500)
- `base_node.p2p.dht.dedup_cache_trim_interval=<seconds>` - Cache trim interval (default: 300)
- `base_node.p2p.dht.dedup_allowed_message_occurrences=<num>` - Allowed message occurrences (default: 1)
- `base_node.p2p.dht.discovery_request_timeout=<seconds>` - Discovery timeout (default: 120)
- `base_node.p2p.dht.auto_join=<bool>` - Auto-join network
- `base_node.p2p.dht.join_cooldown_interval=<seconds>` - Join cooldown (default: 600)
- `base_node.p2p.dht.ban_duration=<seconds>` - Ban duration (default: 21,600)
- `base_node.p2p.dht.ban_duration_short=<seconds>` - Short ban duration (default: 3,600)
- `base_node.p2p.dht.flood_ban_max_msg_count=<num>` - Max messages before ban (default: 100,000)
- `base_node.p2p.dht.flood_ban_timespan=<seconds>` - Flood detection timespan (default: 100)
- `base_node.p2p.dht.offline_peer_cooldown=<seconds>` - Offline peer cooldown (default: 7,200)
- `base_node.p2p.dht.excluded_dial_addresses=<addresses>` - Excluded dial addresses
- `base_node.p2p.dht.enable_forwarding=<bool>` - Enable message forwarding

**Store and Forward (SAF) Configuration**:
- `base_node.p2p.dht.saf.msg_validity=<seconds>` - Message validity period (default: 10,800)
- `base_node.p2p.dht.saf.msg_storage_capacity=<num>` - Storage capacity (default: 100,000)
- `base_node.p2p.dht.saf.num_closest_nodes=<num>` - Closest nodes (default: 10)
- `base_node.p2p.dht.saf.max_returned_messages=<num>` - Max returned messages (default: 50)
- `base_node.p2p.dht.saf.low_priority_msg_storage_ttl=<seconds>` - Low priority TTL (default: 21,600)
- `base_node.p2p.dht.saf.high_priority_msg_storage_ttl=<seconds>` - High priority TTL (default: 259,200)
- `base_node.p2p.dht.saf.max_message_size=<bytes>` - Max message size (default: 524,288)
- `base_node.p2p.dht.saf.auto_request=<bool>` - Auto-request messages on connect
- `base_node.p2p.dht.saf.max_inflight_request_age=<seconds>` - Max inflight request age (default: 120)
- `base_node.p2p.dht.saf.num_neighbouring_nodes=<num>` - SAF neighboring nodes (default: 8)

**DHT Connectivity Configuration**:
- `base_node.p2p.dht.connectivity.update_interval=<seconds>` - Update interval (default: 120)
- `base_node.p2p.dht.connectivity.random_pool_refresh_interval=<seconds>` - Random pool refresh (default: 7,200)
- `base_node.p2p.dht.connectivity.high_failure_rate_cooldown=<seconds>` - High failure cooldown (default: 45)
- `base_node.p2p.dht.connectivity.minimum_desired_tcpv4_node_ratio=<ratio>` - Min TCPv4 ratio (default: 0.1)

**DHT Network Discovery Configuration**:
- `base_node.p2p.dht.network_discovery.enabled=<bool>` - Enable network discovery
- `base_node.p2p.dht.network_discovery.min_desired_peers=<num>` - Min desired peers (default: 50)
- `base_node.p2p.dht.network_discovery.idle_period=<seconds>` - Idle period (default: 1,800)
- `base_node.p2p.dht.network_discovery.idle_after_num_rounds=<num>` - Idle after rounds (default: 10)
- `base_node.p2p.dht.network_discovery.on_failure_idle_period=<seconds>` - Failure idle period (default: 5)
- `base_node.p2p.dht.network_discovery.max_sync_peers=<num>` - Max sync peers (default: 5)
- `base_node.p2p.dht.network_discovery.max_peers_to_sync_per_round=<num>` - Max peers per round (default: 500)
- `base_node.p2p.dht.network_discovery.initial_peer_sync_delay=<seconds>` - Initial sync delay

**Peer Validation Configuration**:
- `base_node.p2p.dht.peer_validator_config.min_peer_version=<version>` - Minimum peer version

**HTTP Wallet Query Service**:
- `base_node.http_wallet_query_service.port=<port>` - Service port (default: 9000)
- `base_node.http_wallet_query_service.external_address=<address>` - External address

---

### 2. minotari_console_wallet

**Description**: Command-line wallet application for creating transactions, managing funds, and interacting with the Tari network.

**Binary Name**: `minotari_console_wallet`

**Purpose**:
- Manage Tari wallet and private keys
- Create and broadcast transactions
- Monitor transaction status
- Provide extensive CLI commands for wallet operations
- Support for hardware wallets (Ledger)

#### Command Line Options

**Common Options** (inherited from CommonCliArgs):
- `-b, --base-path, --base-dir <PATH>` - Directory to store files
- `-c, --config <FILE>` - Configuration file path
- `-l, --log-config <FILE>` - Log configuration file path
- `--log-path <PATH>` - Directory for log files
- `--network <NETWORK>` - Network to use
- `-p <KEY=VALUE>` - Configuration property overrides

**Wallet-Specific Options**:
- `--password <PASSWORD>` - Wallet password (env: `MINOTARI_WALLET_PASSWORD`)
- `--change-password` - Change wallet password and exit
- `--recovery` - Force wallet recovery
- `--seed-words <WORDS>` - Seed words for recovery (env: `MINOTARI_WALLET_SEED_WORDS`)
- `--seed-words-file-name <FILE>` - File to save seed words
- `-n, --non-interactive-mode` - Run without UI
- `-i, --input-file <FILE>` - Input file of commands
- `--command <COMMAND>` - Single command to execute
- `--wallet-notify <SCRIPT>` - Wallet notification script
- `--command-mode-auto-exit` - Auto-exit after command execution
- `--grpc-enabled` - Enable gRPC (env: `MINOTARI_WALLET_ENABLE_GRPC`)
- `--grpc-address <ADDRESS>` - gRPC address (env: `MINOTARI_WALLET_GRPC_ADDRESS`)
- `--profile` - Enable Tokio Console profiling
- `--view-private-key <KEY>` - View private key for read-only wallet (env: `MINOTARI_WALLET_VIEW_PRIVATE_KEY`)
- `--spend-key <KEY>` - Spend key for wallet (env: `MINOTARI_WALLET_SPEND_KEY`)
- `--birthday <HEIGHT>` - Wallet birthday block height
- `-z, --libtor-data-dir <PATH>` - Path to libtor data directory

#### Wallet Commands

The wallet supports extensive subcommands for various operations:

**Basic Operations**:
- `get-balance` - Show current wallet balance
- `send-minotari <amount> <address>` - Send Tari to address
- `burn-minotari <amount>` - Burn Tari (destroy permanently)
- `sync` - Synchronize wallet with base node

**Pre-mine Operations** (for network genesis):
- `pre-mine-start` - Start pre-mine spending session
- `pre-mine-start-party` - Start party for pre-mine spending
- `pre-mine-encumber` - Encumber aggregate UTXO
- `pre-mine-sigs` - Handle pre-mine signatures
- `pre-mine-spend-tx` - Create pre-mine spend transaction
- `pre-mine-backup-utxo` - Backup pre-mine UTXO

**Advanced Operations**:
- `send-one-sided-to-stealth-address` - Send to stealth address
- `make-it-rain` - Stress test with multiple transactions
- `coin-split` - Split coins into smaller denominations
- `discover-peer` - Discover peer information
- `whois` - Get information about a public key

**Import/Export**:
- `export-utxos` - Export UTXOs to file
- `export-tx` - Export transaction to file
- `import-tx` - Import transaction from file
- `export-spent-utxos` - Export spent UTXOs
- `export-view-key-and-spend-key` - Export keys
- `import-paper-wallet` - Import from paper wallet

**Node Management**:
- `set-base-node` - Set base node connection
- `set-custom-base-node` - Set custom base node
- `clear-custom-base-node` - Clear custom base node

**Atomic Swaps**:
- `init-sha-atomic-swap` - Initialize SHA atomic swap
- `finalise-sha-atomic-swap` - Finalize atomic swap
- `claim-sha-atomic-swap-refund` - Claim refund

**Utilities**:
- `count-utxos` - Count UTXOs
- `revalidate-wallet-db` - Revalidate wallet database
- `register-validator-node` - Register as validator node
- `create-tls-certs` - Create TLS certificates

**Payment References**:
- `show-pay-ref` - Show payment reference
- `find-pay-ref` - Find payment reference
- `list-tx` - List all transactions

#### All Configuration Overrides (-p options)

**Core Wallet Configuration**:
- `wallet.buffer_size=<size>` - Message buffer size (default: 50,000)
- `wallet.data_dir=<path>` - Wallet data directory (default: "data/wallet")
- `wallet.db_file=<path>` - Database file path (default: "db/console_wallet.db")
- `wallet.db_connection_pool_size=<num>` - Database connection pool size (default: 16)
- `wallet.password=<password>` - Wallet password
- `wallet.contacts_auto_ping_interval=<seconds>` - Contact ping interval (default: 30)
- `wallet.contacts_online_ping_window=<seconds>` - Contact online window (default: 30)
- `wallet.command_send_wait_timeout=<seconds>` - Command mode timeout (default: 300)
- `wallet.command_send_wait_stage=<stage>` - Transaction wait stage (default: "Broadcast")
- `wallet.autoignore_onesided_utxos=<bool>` - Auto-ignore one-sided UTXOs
- `wallet.custom_base_node=<pubkey>::<address>` - Custom base node peer
- `wallet.base_node_service_peers=<peers>` - Base node service peers list
- `wallet.recovery_retry_limit=<num>` - Recovery retry attempts (default: 3)
- `wallet.fee_per_gram=<amount>` - Default transaction fee (default: 5)
- `wallet.num_required_confirmations=<num>` - Required confirmations (default: 3)
- `wallet.use_libtor=<bool>` - Use built-in Tor instance
- `wallet.identity_file=<path>` - Identity file path
- `wallet.notify_file=<path>` - Notification script path
- `wallet.balance_enquiry_cooldown_period=<seconds>` - Balance check cooldown (default: 5)
- `wallet.birthday_offset=<blocks>` - Scanning offset from birthday (default: 2)

**gRPC Configuration**:
- `wallet.grpc_enabled=<bool>` - Enable gRPC server
- `wallet.grpc_address=<address>` - gRPC bind address (default: "127.0.0.1:18143")
- `wallet.grpc_authentication=<auth>` - gRPC authentication (username/password)
- `wallet.grpc_tls_enabled=<bool>` - Enable gRPC TLS

**Transaction Service Configuration**:
- `wallet.transactions.broadcast_monitoring_timeout=<seconds>` - Broadcast monitoring timeout (default: 180)
- `wallet.transactions.chain_monitoring_timeout=<seconds>` - Chain monitoring timeout (default: 60)
- `wallet.transactions.direct_send_timeout=<seconds>` - Direct send timeout (default: 180)
- `wallet.transactions.broadcast_send_timeout=<seconds>` - Broadcast send timeout (default: 180)
- `wallet.transactions.low_power_polling_timeout=<seconds>` - Low power polling timeout (default: 300)
- `wallet.transactions.transaction_resend_period=<seconds>` - Transaction resend period (default: 600)
- `wallet.transactions.resend_response_cooldown=<seconds>` - Resend response cooldown (default: 300)
- `wallet.transactions.pending_transaction_cancellation_timeout=<seconds>` - Pending TX cancellation timeout (default: 259,200)
- `wallet.transactions.num_confirmations_required=<num>` - Transaction confirmations required (default: 3)
- `wallet.transactions.max_tx_query_batch_size=<num>` - Max TX query batch size (default: 20)
- `wallet.transactions.transaction_routing_mechanism=<mechanism>` - Routing mechanism (DirectOnly, StoreAndForwardOnly, DirectAndStoreAndForward)
- `wallet.transactions.transaction_event_channel_size=<size>` - Event channel size (default: 25,000)
- `wallet.transactions.transaction_mempool_resubmission_window=<seconds>` - Mempool resubmission window (default: 600)

**Output Manager Configuration**:
- `wallet.outputs.prevent_fee_gt_amount=<bool>` - Prevent fee greater than amount (default: false)
- `wallet.outputs.dust_ignore_value=<amount>` - Dust ignore value (default: 100)
- `wallet.outputs.event_channel_size=<size>` - Output event channel size (default: 3,500)
- `wallet.outputs.num_confirmations_required=<num>` - Output confirmations required (default: 3)
- `wallet.outputs.tx_validator_batch_size=<num>` - TX validator batch size (default: 100)
- `wallet.outputs.num_of_seconds_to_revalidate_invalid_utxos=<seconds>` - UTXO revalidation interval (default: 259,200)

**Base Node Service Configuration**:
- `wallet.base_node.base_node_monitor_max_refresh_interval=<seconds>` - Monitor refresh interval (default: 30)
- `wallet.base_node.base_node_rpc_pool_size=<num>` - RPC client pool size (default: 5)
- `wallet.base_node.event_channel_size=<size>` - Base node event channel size (default: 250)

**P2P Configuration**:
- `wallet.p2p.public_addresses=<addresses>` - Public addresses for discovery
- `wallet.p2p.auxiliary_tcp_listener_address=<address>` - Additional TCP listener
- `wallet.p2p.datastore_path=<path>` - Peer database path (default: "peer_db")
- `wallet.p2p.peer_database_name=<name>` - Peer database name (default: "peers")
- `wallet.p2p.max_concurrent_inbound_tasks=<num>` - Max inbound tasks (default: 100)
- `wallet.p2p.max_concurrent_outbound_tasks=<num>` - Max outbound tasks (default: 100)
- `wallet.p2p.allow_test_addresses=<bool>` - Allow test addresses
- `wallet.p2p.listener_liveness_allowlist_cidrs=<cidrs>` - Liveness allowlist CIDRs
- `wallet.p2p.listener_self_liveness_check_interval=<seconds>` - Self liveness check interval
- `wallet.p2p.user_agent=<string>` - User agent string
- `wallet.p2p.rpc_max_simultaneous_sessions=<num>` - Max RPC sessions (default: 100)
- `wallet.p2p.rpc_max_sessions_per_peer=<num>` - Max RPC per peer (default: 10)
- `wallet.p2p.cull_oldest_peer_rpc_connection_on_full=<bool>` - Cull old RPC connections

**Transport Configuration**:
- `wallet.p2p.transport.type=<type>` - Transport type (tcp, tor, socks5, memory)

**TCP Transport**:
- `wallet.p2p.transport.tcp.listener_address=<address>` - TCP listener address
- `wallet.p2p.transport.tcp.tor_socks_address=<address>` - Tor SOCKS proxy address
- `wallet.p2p.transport.tcp.tor_socks_auth=<auth>` - Tor SOCKS authentication

**Tor Transport**:
- `wallet.p2p.transport.tor.control_address=<address>` - Tor control server address
- `wallet.p2p.transport.tor.socks_auth=<auth>` - SOCKS authentication
- `wallet.p2p.transport.tor.socks_address_override=<address>` - SOCKS address override
- `wallet.p2p.transport.tor.control_auth=<auth>` - Tor control authentication
- `wallet.p2p.transport.tor.onion_port=<port>` - Onion service port
- `wallet.p2p.transport.tor.proxy_bypass_addresses=<addresses>` - Proxy bypass addresses
- `wallet.p2p.transport.tor.proxy_bypass_for_outbound_tcp=<bool>` - Bypass proxy for outbound TCP
- `wallet.p2p.transport.tor.forward_address=<address>` - Traffic forward address

**SOCKS5 Transport**:
- `wallet.p2p.transport.socks.proxy_address=<address>` - SOCKS5 proxy address
- `wallet.p2p.transport.socks.auth=<auth>` - SOCKS5 authentication

**Memory Transport**:
- `wallet.p2p.transport.memory.listener_address=<address>` - Memory transport address

**DHT Configuration**:
- `wallet.p2p.dht.database_url=<url>` - DHT database URL
- `wallet.p2p.dht.num_neighbouring_nodes=<num>` - Neighboring nodes (default: 5)
- `wallet.p2p.dht.num_random_nodes=<num>` - Random nodes (default: 1)
- `wallet.p2p.dht.minimize_connections=<bool>` - Minimize connections (default: true)
- `wallet.p2p.dht.broadcast_factor=<num>` - Broadcast peer count (default: 8)
- `wallet.p2p.dht.propagation_factor=<num>` - Propagation peer count (default: 4)
- `wallet.p2p.dht.dedup_cache_capacity=<num>` - Message dedup cache size (default: 2,500)
- `wallet.p2p.dht.dedup_cache_trim_interval=<seconds>` - Cache trim interval (default: 300)
- `wallet.p2p.dht.dedup_allowed_message_occurrences=<num>` - Allowed message occurrences (default: 1)
- `wallet.p2p.dht.discovery_request_timeout=<seconds>` - Discovery timeout (default: 120)
- `wallet.p2p.dht.auto_join=<bool>` - Auto-join network (default: true)
- `wallet.p2p.dht.join_cooldown_interval=<seconds>` - Join cooldown (default: 120)
- `wallet.p2p.dht.ban_duration=<seconds>` - Ban duration (default: 120)
- `wallet.p2p.dht.ban_duration_short=<seconds>` - Short ban duration (default: 60)
- `wallet.p2p.dht.flood_ban_max_msg_count=<num>` - Max messages before ban (default: 100,000)
- `wallet.p2p.dht.flood_ban_timespan=<seconds>` - Flood detection timespan (default: 100)
- `wallet.p2p.dht.offline_peer_cooldown=<seconds>` - Offline peer cooldown (default: 7,200)
- `wallet.p2p.dht.excluded_dial_addresses=<addresses>` - Excluded dial addresses
- `wallet.p2p.dht.enable_forwarding=<bool>` - Enable message forwarding

**Store and Forward (SAF) Configuration**:
- `wallet.p2p.dht.saf.msg_validity=<seconds>` - Message validity period (default: 10,800)
- `wallet.p2p.dht.saf.msg_storage_capacity=<num>` - Storage capacity (default: 100,000)
- `wallet.p2p.dht.saf.num_closest_nodes=<num>` - Closest nodes (default: 10)
- `wallet.p2p.dht.saf.max_returned_messages=<num>` - Max returned messages (default: 50)
- `wallet.p2p.dht.saf.low_priority_msg_storage_ttl=<seconds>` - Low priority TTL (default: 21,600)
- `wallet.p2p.dht.saf.high_priority_msg_storage_ttl=<seconds>` - High priority TTL (default: 259,200)
- `wallet.p2p.dht.saf.max_message_size=<bytes>` - Max message size (default: 524,288)
- `wallet.p2p.dht.saf.auto_request=<bool>` - Auto-request messages on connect
- `wallet.p2p.dht.saf.max_inflight_request_age=<seconds>` - Max inflight request age (default: 120)
- `wallet.p2p.dht.saf.num_neighbouring_nodes=<num>` - SAF neighboring nodes (default: 8)

**DHT Connectivity Configuration**:
- `wallet.p2p.dht.connectivity.update_interval=<seconds>` - Update interval (default: 300)
- `wallet.p2p.dht.connectivity.random_pool_refresh_interval=<seconds>` - Random pool refresh (default: 7,200)
- `wallet.p2p.dht.connectivity.high_failure_rate_cooldown=<seconds>` - High failure cooldown (default: 45)
- `wallet.p2p.dht.connectivity.minimum_desired_tcpv4_node_ratio=<ratio>` - Min TCPv4 ratio (default: 0.0)

**DHT Network Discovery Configuration**:
- `wallet.p2p.dht.network_discovery.enabled=<bool>` - Enable network discovery
- `wallet.p2p.dht.network_discovery.min_desired_peers=<num>` - Min desired peers (default: 16)
- `wallet.p2p.dht.network_discovery.idle_period=<seconds>` - Idle period (default: 1,800)
- `wallet.p2p.dht.network_discovery.idle_after_num_rounds=<num>` - Idle after rounds (default: 10)
- `wallet.p2p.dht.network_discovery.on_failure_idle_period=<seconds>` - Failure idle period (default: 5)
- `wallet.p2p.dht.network_discovery.max_sync_peers=<num>` - Max sync peers (default: 5)
- `wallet.p2p.dht.network_discovery.max_peers_to_sync_per_round=<num>` - Max peers per round (default: 500)
- `wallet.p2p.dht.network_discovery.initial_peer_sync_delay=<seconds>` - Initial sync delay (default: 25)

**Peer Validation Configuration**:
- `wallet.p2p.dht.peer_validator_config.min_peer_version=<version>` - Minimum peer version

---

### 3. minotari_miner

**Description**: SHA-3 based mining implementation that connects to base nodes to mine Tari blocks.

**Binary Name**: `minotari_miner`

**Purpose**:
- Mine Tari blocks using SHA-3 proof-of-work
- Connect to base node for block templates
- Support CPU mining with multiple threads
- Configurable mining parameters

#### Command Line Options

**Common Options** (inherited from CommonCliArgs):
- `-b, --base-path, --base-dir <PATH>` - Directory to store files
- `-c, --config <FILE>` - Configuration file path
- `-l, --log-config <FILE>` - Log configuration file path
- `--log-path <PATH>` - Directory for log files
- `--network <NETWORK>` - Network to use
- `-p <KEY=VALUE>` - Configuration property overrides

**Miner-Specific Options**:
- `--mine-until-height <HEIGHT>` - Mine until specific block height
- `--miner-max-blocks <COUNT>` - Maximum blocks to mine
- `--miner-min-diff <DIFFICULTY>` - Minimum difficulty to mine
- `--miner-max-diff <DIFFICULTY>` - Maximum difficulty to mine
- `-n, --non-interactive-mode` - Run without UI (env: `TARI_NON_INTERACTIVE`)

#### All Configuration Overrides (-p options)

**Core Mining Configuration**:
- `miner.network=<network>` - Mining network (mainnet, esmeralda, nextnet, stagenet)
- `miner.base_node_grpc_address=<address>` - Base node gRPC address (default: "http://127.0.0.1:18142")
- `miner.base_node_grpc_authentication=<auth>` - Base node gRPC authentication (username/password)
- `miner.base_node_grpc_tls_domain_name=<domain>` - gRPC TLS domain name
- `miner.base_node_grpc_ca_cert_filename=<file>` - CA certificate file
- `miner.num_mining_threads=<count>` - Number of mining threads (default: number of CPU cores)
- `miner.mine_on_tip_only=<bool>` - Mine only on network tip (default: true)
- `miner.validate_tip_timeout_sec=<seconds>` - Tip validation timeout (default: 30)
- `miner.wait_timeout_on_error=<seconds>` - Error timeout (default: 10)
- `miner.proof_of_work_algo=<algo>` - PoW algorithm (Sha3X, RandomXT, RandomXM)

**Pool Mining Configuration**:
- `miner.stratum_mining_pool_address=<address>` - Mining pool address (e.g., "miningcore.tari.com:3052")
- `miner.stratum_mining_wallet_address=<address>` - Mining wallet address/public key
- `miner.stratum_mining_worker_name=<name>` - Mining worker name

**Payment Configuration**:
- `miner.wallet_payment_address=<address>` - Tari wallet address for mining rewards
- `miner.coinbase_extra=<data>` - Extra coinbase data (default: "minotari_miner")
- `miner.range_proof_type=<type>` - Range proof type (revealed_value, bullet_proof_plus)

**P2Pool Configuration**:
- `miner.sha_p2pool_enabled=<bool>` - Enable SHA p2pool mining

---

### 4. minotari_merge_mining_proxy

**Description**: Proxy server that enables merge mining Tari with Monero using XMRig or other Monero miners.

**Binary Name**: `minotari_merge_mining_proxy`

**Purpose**:
- Act as proxy between Monero miners (XMRig) and Tari base node
- Enable simultaneous mining of Monero and Tari
- Handle block template conversion between networks
- Provide Monero-compatible mining interface

#### Command Line Options

**Common Options** (inherited from CommonCliArgs):
- `-b, --base-path, --base-dir <PATH>` - Directory to store files
- `-c, --config <FILE>` - Configuration file path
- `-l, --log-config <FILE>` - Log configuration file path
- `--log-path <PATH>` - Directory for log files
- `--network <NETWORK>` - Network to use
- `-p <KEY=VALUE>` - Configuration property overrides

**Proxy-Specific Options**:
- `-n, --non-interactive-mode` - Run without UI (env: `TARI_NON_INTERACTIVE`)

#### All Configuration Overrides (-p options)

**Core Proxy Configuration**:
- `merge_mining_proxy.network=<network>` - Proxy network (mainnet, esmeralda, nextnet, stagenet)
- `merge_mining_proxy.listener_address=<address>` - Proxy listener address (default: "/ip4/127.0.0.1/tcp/18081")
- `merge_mining_proxy.submit_to_origin=<bool>` - Submit to Monero blockchain (default: true)
- `merge_mining_proxy.wait_for_initial_sync_at_startup=<bool>` - Wait for base node sync (default: true)
- `merge_mining_proxy.check_tari_difficulty_before_submit=<bool>` - Check difficulty before submit (default: true)

**Base Node Configuration**:
- `merge_mining_proxy.base_node_grpc_address=<address>` - Base node gRPC address (default: "http://127.0.0.1:18142")
- `merge_mining_proxy.base_node_grpc_authentication=<auth>` - Base node gRPC authentication (username/password)

**Monero Configuration**:
- `merge_mining_proxy.use_dynamic_fail_data=<bool>` - Use dynamic monerod URLs from monero.fail (default: true)
- `merge_mining_proxy.monero_fail_url=<url>` - Monero fail URL for dynamic URLs
- `merge_mining_proxy.monerod_url=<urls>` - Static monerod URLs list (when dynamic disabled)
- `merge_mining_proxy.monerod_username=<user>` - Monerod username
- `merge_mining_proxy.monerod_password=<pass>` - Monerod password
- `merge_mining_proxy.monerod_use_auth=<bool>` - Enable monerod authentication (default: false)
- `merge_mining_proxy.monerod_connection_timeout=<seconds>` - Connection timeout (default: 2)

**Mining Configuration**:
- `merge_mining_proxy.max_randomx_vms=<num>` - Maximum RandomX VMs (default: 5)
- `merge_mining_proxy.coinbase_extra=<data>` - Extra coinbase data (default: "tari_merge_mining_proxy")
- `merge_mining_proxy.wallet_payment_address=<address>` - Tari wallet address for mining rewards
- `merge_mining_proxy.range_proof_type=<type>` - Range proof type (revealed_value, bullet_proof_plus)

**P2Pool Configuration**:
- `merge_mining_proxy.p2pool_enabled=<bool>` - Enable P2Pool support (default: false)

---

### 5. minotari_ledger_wallet

**Description**: Hardware wallet implementation for Ledger devices, providing secure key management and transaction signing.

**Binary Name**: `minotari_ledger_wallet` (Ledger app, not a CLI application)

**Purpose**:
- Secure key storage on Ledger hardware
- Transaction signing on hardware device
- Integration with Tari wallet ecosystem
- Support for multiple Ledger device models

**Device Support**:
- Ledger Nano X
- Ledger Nano S Plus
- Ledger Stax
- Ledger Flex

**Features**:
- Hardware-based private key generation and storage
- Secure transaction signing
- BIP44 derivation path: `44'/535348'`
- Curve25519 cryptography support

---

### 6. deps_only

**Description**: Development utility that builds all project dependencies for Docker layer caching.

**Binary Name**: `deps_only`

**Purpose**:
- Build all project dependencies without functionality
- Create cached Docker layers with compiled dependencies
- Speed up subsequent builds in containerized environments

**Usage**: This is typically used in Docker build processes and not run directly by users.

---

## Common Configuration System

All applications share a common configuration system with the following features:

### Configuration Hierarchy

1. **Default values** - Built-in defaults
2. **Configuration files** - TOML files (typically `config/config.toml`)
3. **Environment variables** - Various `TARI_*` and `MINOTARI_*` variables
4. **Command-line overrides** - Using `-p` parameters

### Universal -p Override Format

All applications support configuration overrides using the `-p` parameter:

```bash
-p "section.subsection.property=value"
```

### Common Configuration Overrides

**Common Section** (applies to all applications):
- `common.base_path=<path>` - Common Tari data path
- `common.message_cache_size=<mb>` - Message cache size in MB (default: 10)
- `common.message_cache_ttl=<minutes>` - Message cache TTL (default: 1440)
- `common.denylist_ban_period=<minutes>` - Blacklist period (default: 1440)

**Auto Update Configuration**:
- `auto_update.update_uris=<uris>` - Update URIs list
- `auto_update.hashes_url=<url>` - Update hashes URL
- `auto_update.hashes_sig_url=<url>` - Update signature URL
- `auto_update.check_interval=<seconds>` - Check interval
- `auto_update.download_base_url=<url>` - Download base URL
- `auto_update.override_from=<network>` - Network override

**Metrics Configuration**:
- `metrics.enabled=<bool>` - Enable metrics
- `metrics.bind_address=<address>` - Metrics server address
- `metrics.push_endpoint=<url>` - Metrics push endpoint
- `metrics.override_from=<network>` - Network override

**Peer Seeds Configuration** (Network-specific):
- `<network>.p2p.seeds.dns_seeds=<domains>` - DNS seed domains
- `<network>.p2p.seeds.peer_seeds=<peers>` - Static peer seeds
- `<network>.p2p.seeds.dns_name_server=<servers>` - DNS name servers

**Examples**:
```bash
# Set network
-p "base_node.network=esmeralda"

# Configure gRPC
-p "base_node.grpc_enabled=true"
-p "base_node.grpc_address=127.0.0.1:18142"

# Set peer seeds for esmeralda network
-p "esmeralda.p2p.seeds.peer_seeds=pubkey1::address1,pubkey2::address2"

# Configure wallet
-p "wallet.custom_base_node=pubkey::address"
-p "wallet.fee_per_gram=5"

# Set common configuration
-p "common.base_path=/custom/tari/path"
-p "common.message_cache_size=20"

# Enable metrics
-p "metrics.enabled=true"
-p "metrics.bind_address=127.0.0.1:9998"

# Configure auto-update for specific network
-p "stagenet.auto_update.update_uris=updates.stagenet.taripulse.com"
```

### Network Types

All applications support these networks:
- `mainnet` - Production network
- `esmeralda` - Testnet
- `nextnet` - Development network
- `localnet` - Local testing

### Common Directories

- **Base Path**: Platform-specific default, customizable with `-b`
  - Linux: `~/.tari/`
  - macOS: `~/Library/Application Support/tari/`
  - Windows: `%APPDATA%\tari\`
- **Config**: `{base_path}/{network}/config/`
- **Data**: `{base_path}/{network}/data/`
- **Logs**: `{base_path}/{network}/logs/`

### Environment Variables

**Universal**:
- `TARI_BASE_DIR` - Base directory path
- `TARI_NETWORK` - Network selection
- `TARI_NON_INTERACTIVE` - Non-interactive mode

**Application-specific**:
- `MINOTARI_NODE_ENABLE_GRPC` - Enable node gRPC
- `MINOTARI_NODE_ENABLE_MINING` - Enable node mining
- `MINOTARI_WALLET_PASSWORD` - Wallet password
- `MINOTARI_WALLET_ENABLE_GRPC` - Enable wallet gRPC

---

## Build Instructions

### Building All Applications

```bash
# Build all applications
cargo build --release

# Build specific application
cargo build --release --bin minotari_node
cargo build --release --bin minotari_console_wallet
cargo build --release --bin minotari_miner
cargo build --release --bin minotari_merge_mining_proxy
```

### Building Dependencies Only

```bash
# Build dependencies for Docker layer caching
cargo build --bin deps_only
```

### Cross-compilation

The project supports cross-compilation to various targets. See the `Cross.toml` file for supported targets.

---

## Features and Variants

Many applications support optional features that can be enabled during compilation:

### minotari_node Features
- `default` - Standard features including libtor
- `metrics` - Enable Prometheus metrics
- `safe` - Safe mode with additional checks
- `libtor` - Tor networking support
- `dhat-heap` - Heap profiling support

### minotari_console_wallet Features
- `default` - Standard features (grpc, ledger, libtor)
- `grpc` - gRPC API support
- `ledger` - Ledger hardware wallet support
- `libtor` - Tor networking support
- `dhat-heap` - Heap profiling support

### Example Feature Usage

```bash
# Build node with metrics
cargo build --release --bin minotari_node --features metrics

# Build wallet without ledger support
cargo build --release --bin minotari_console_wallet --no-default-features --features grpc,libtor
```

This comprehensive overview provides all the essential information about Tari applications, their command-line interfaces, and configuration options. Each application is designed to work together in the Tari ecosystem while being independently configurable and deployable.
