# Tor Fallback Transport

The Tor Fallback Transport is a new transport mode that automatically falls back from Tor to IPv4 TCP when Tor is unavailable or blocked. This is particularly useful in environments where Tor may be blocked by firewalls or national censorship systems like the Great Firewall of China (GFW).

## Overview

The AutoFallback transport provides the following features:

1. **Intelligent Tor Detection**: Attempts to connect to Tor with a configurable timeout
2. **Automatic Fallback**: Falls back to IPv4 TCP when Tor is unavailable
3. **Periodic Retry**: Periodically attempts to reconnect to Tor when in fallback mode
4. **Address Filtering**: Automatically filters onion addresses when in IPv4-only mode
5. **Transparent Operation**: Works transparently with the existing Tari P2P infrastructure

## Configuration

To enable the AutoFallback transport, set the transport type to "auto_fallback" in your configuration:

```toml
[p2p.transport]
type = "auto_fallback"

# Tor configuration (attempted first)
[p2p.transport.tor]
control_address = "/ip4/127.0.0.1/tcp/9051"
tor_timeout = 15          # Timeout before fallback (seconds)
allow_fallback = true     # Allow fallback to IPv4
tor_retry_interval = 300  # Retry Tor every 5 minutes

# TCP configuration (used for fallback)
[p2p.transport.tcp]
listener_address = "/ip4/0.0.0.0/tcp/18189"
```

### Configuration Options

#### Tor Settings

- **`tor_timeout`**: Maximum time to wait for Tor connection before falling back (default: 15 seconds)
- **`allow_fallback`**: Whether to allow fallback to IPv4 when Tor fails (default: true)
- **`tor_retry_interval`**: How often to retry Tor connection when in fallback mode (default: 300 seconds)

#### TCP Settings

- **`listener_address`**: IPv4 address to bind when in fallback mode

## How It Works

### Initialization Process

1. **Tor Attempt**: The transport first attempts to initialize Tor with the configured timeout
2. **Timeout Detection**: If Tor doesn't respond within the timeout, it's considered blocked
3. **Fallback**: If Tor fails and `allow_fallback` is true, switch to IPv4 TCP transport
4. **Mode Notification**: The transport broadcasts the current mode to interested components

### Connection Modes

#### Tor Mode
- Uses Tor hidden service for both inbound and outbound connections
- Can connect to both onion addresses and regular IP addresses
- Provides maximum privacy and anonymity

#### IPv4-Only Mode  
- Uses direct TCP connections
- Filters out onion addresses from peer lists
- Better performance but no anonymity
- Suitable for environments where Tor is blocked

#### Failed Mode
- No connections possible
- Occurs when Tor fails and fallback is disabled

### Background Monitoring

When in IPv4-only mode, the transport periodically tests Tor availability and automatically switches back when Tor becomes available.

## Error Handling

The transport handles the following error scenarios:

### Tor Control Port Offline
```
TorControlPortOffline: Unable to connect to the Tor control port
```
**Cause**: Tor daemon is not running or control port is blocked
**Action**: Falls back to IPv4 if enabled

### Tor Connection Timeout
```
TorConnectionTimeout: Tor connection timed out - possible network blocking
```
**Cause**: Tor connections are being filtered or blocked by firewall/GFW
**Action**: Falls back to IPv4 if enabled

### Tor Blocked
```
TorBlocked: Tor connection blocked by firewall or GFW
```
**Cause**: Network-level blocking of Tor traffic
**Action**: Falls back to IPv4 if enabled

## Usage Examples

### Complete Configuration Example

Here's a complete configuration file showing AutoFallback transport setup:

```toml
# Example configuration for Tari Node with AutoFallback transport
# This configuration will attempt to use Tor, but fallback to IPv4 TCP if Tor is blocked

[base_node]
# Enable the base node
network = "esmeralda"

# P2P transport configuration
[p2p.transport]
# Use AutoFallback transport: Try Tor first, fallback to TCP if blocked
type = "auto_fallback"

# TCP transport settings (used for fallback)
[p2p.transport.tcp]
listener_address = "/ip4/0.0.0.0/tcp/18189"

# Tor transport settings (attempted first)
[p2p.transport.tor]
# Tor control port address
control_address = "/ip4/127.0.0.1/tcp/9051"

# How long to wait for Tor connection before falling back (seconds)
tor_timeout = 15

# Whether to allow fallback to IPv4 when Tor fails
allow_fallback = true

# How often to retry Tor when in fallback mode (seconds)
tor_retry_interval = 300  # 5 minutes

# Tor proxy authentication
socks_auth = "none"

# Tor control port authentication (auto-detect)
control_auth = "auto"

# Port for the hidden service
onion_port = 18141

# Forward traffic to this address (auto-assigned if not set)
# forward_address = "/ip4/127.0.0.1/tcp/18141"

# Override the listener address
# listener_address_override = "/ip4/127.0.0.1/tcp/0"

# Addresses to bypass Tor proxy for outbound connections
proxy_bypass_addresses = []

# Bypass Tor for outbound TCP connections (better performance, less privacy)
proxy_bypass_for_outbound_tcp = true

# DHT Configuration
[p2p.dht]
# Enable auto-join to the DHT network
auto_join = true

# Minimum number of neighboring nodes
num_neighbouring_nodes = 8

# Number of random walk nodes
num_random_nodes = 4
```

### Basic Configuration (Recommended)

For a minimal setup, you only need:

```toml
[p2p.transport]
type = "auto_fallback"

[p2p.transport.tor]
control_address = "/ip4/127.0.0.1/tcp/9051"
tor_timeout = 15
allow_fallback = true
tor_retry_interval = 300

[p2p.transport.tcp]
listener_address = "/ip4/0.0.0.0/tcp/18189"
```

### Alternative Transport Configurations

For comparison, here are other transport configurations:

```toml
# Tor-only mode (no fallback)
[p2p.transport]
type = "tor"

# TCP-only mode  
[p2p.transport]
type = "tcp"

# Memory transport (for testing)
[p2p.transport]
type = "memory"

# SOCKS5 proxy
[p2p.transport]
type = "socks5"
[p2p.transport.socks]
proxy_address = "/ip4/127.0.0.1/tcp/1080"
auth = "none"
```

### Aggressive Tor Retry

For environments with intermittent Tor blocking:

```toml
[p2p.transport.tor]
tor_timeout = 30          # Longer timeout
tor_retry_interval = 60   # Retry every minute
```

### Conservative Fallback

For maximum privacy with fallback only as last resort:

```toml
[p2p.transport.tor]
tor_timeout = 60          # Very long timeout
tor_retry_interval = 900  # Retry every 15 minutes
```

### No Fallback (Tor Only)

To disable fallback and require Tor:

```toml
[p2p.transport.tor]
allow_fallback = false
```

## Monitoring and Logging

The AutoFallback transport provides detailed logging to help diagnose connectivity issues:

### Log Messages

- **Tor Initialization**: `"Attempting to initialize Tor transport..."`
- **Tor Success**: `"Tor transport initialized successfully"`
- **Tor Timeout**: `"Tor transport initialization timed out after 15s"`
- **Fallback**: `"Falling back to IPv4 TCP transport"`
- **Mode Change**: Transport mode changes are broadcast to subscribers

### Monitoring Transport Mode

Applications can subscribe to transport mode changes:

```rust
let mode_subscription = transport.subscribe_mode_changes();
tokio::spawn(async move {
    while let Ok(mode) = mode_subscription.recv().await {
        match mode {
            TransportMode::Tor => info!("Switched to Tor mode"),
            TransportMode::IPv4Only => warn!("Switched to IPv4-only mode"),
            TransportMode::Failed => error!("Transport failed"),
        }
    }
});
```

## Security Considerations

### Privacy Impact

- **Tor Mode**: Full anonymity and privacy protection
- **IPv4 Mode**: No anonymity, IP address is visible to peers and network observers
- **Address Leakage**: When in IPv4 mode, the node's real IP address may be recorded by peers

### Recommendations

1. **Monitor Mode Changes**: Always monitor transport mode to be aware of privacy level
2. **Firewall Configuration**: Ensure IPv4 ports are properly firewalled when needed
3. **Peer Selection**: Consider implementing peer filtering based on transport mode
4. **Logging**: Be careful not to log sensitive information when in IPv4 mode

## Troubleshooting

### Common Issues

1. **Constant Fallback**: If the node constantly falls back to IPv4, check Tor daemon status
2. **No Fallback**: If fallback doesn't work, verify TCP port availability
3. **Connection Issues**: In IPv4 mode, ensure firewall allows incoming connections

### Diagnostic Commands

Check Tor daemon status:
```bash
# Check if Tor is running
ps aux | grep tor

# Check Tor control port
netstat -an | grep 9051

# Test Tor control connection
telnet 127.0.0.1 9051
```

Check network connectivity:
```bash
# Test if port is accessible
nc -zv 127.0.0.1 9051

# Check for network filtering
curl --socks5 127.0.0.1:9050 http://check.torproject.org
```

### Log Analysis

Look for these patterns in logs:

- Frequent timeout errors: Network-level Tor blocking
- Connection refused: Tor daemon not running
- Successful fallback: Expected behavior in blocked environments

## Implementation Details

The AutoFallback transport is implemented as a wrapper around existing transports:

- **HiddenServiceTransport**: Used for Tor mode
- **TcpTransport**: Used for IPv4 fallback mode
- **FallbackTransport**: Coordinates between the two

The transport maintains internal state and uses Arc/RwLock for thread-safe sharing between async tasks.

## Future Enhancements

Potential future improvements include:

1. **Bridge Support**: Integration with Tor bridges for enhanced censorship resistance
2. **Pluggable Transports**: Support for alternative censorship circumvention tools
3. **Smart Peer Selection**: Preferred onion peers in Tor mode, IPv4 peers in fallback mode
4. **Connection Quality Metrics**: Monitor and report connection quality by transport type
5. **Automatic Bridge Discovery**: Discover and use Tor bridges automatically when needed
