# Standalone miner for the MinoTari network

The MinoTari Miner application should be running connected to the MinoTari Base Node and the MinoTari Console Wallet,
allowing it insert the coinbase transactions created by the MinoTari Wallet and to to mine blocks matching the
required network difficulty. This also serves as a reference implementation for custom miners that which can be derived
from it.

### Installation

Please refer to the relevant section in the main
[README installation section](https://github.com/tari-project/tari/blob/development/README.md#install-and-run).

### Configuration

When running with local versions of the MinoTari Base Node and the MinoTari Wallet, no additional configuration other
than what is described in the main
[README configuration section](https://github.com/tari-project/tari/blob/development/README.md#tari-sha3-mining)
is required. The MinoTari Miner can also be located on a remote workstation.

Configuration options for the MinoTari Miner are as follows:

- `base_node_grpc_address` - this is IPv4/IPv6 address including port number, by which the MinoTari Base Node can be found;
- `wallet_grpc_address` - this is IPv4/IPv6 address including port number, by which the MinoTari Wallet can be
  found;
- `num_mining_threads` - the number of mining threads, which defaults to the number of CPU cores;
- `mine_on_tip_only` - mining will only start when the MinoTari Base Node reports it is in the bootstrapped state;
- `validate_tip_timeout_sec` - the interval at which the current block height will be checked to determine if mining
  must be restarted, whereby the tip might have advanced passed the block height that is in use in the current template.

### Caveats

Currently, the MinoTari Miner only supports SHA3 mining; this is adequate for the current Tari protocol.
