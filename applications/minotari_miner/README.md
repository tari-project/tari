# Standalone miner for the Minotari network

The Minotari Miner application should be running connected to the Minotari Base Node and the Minotari Console Wallet,
allowing it insert the coinbase transactions created by the Minotari Wallet and to to mine blocks matching the
required network difficulty. This also serves as a reference implementation for custom miners that which can be derived
from it.

### Installation

Please refer to the relevant section in the main
[README installation section](https://github.com/tari-project/tari/blob/development/README.md#install-and-run).

### Configuration

When running with local versions of the Minotari Base Node and the Minotari Wallet, no additional configuration other
than what is described in the main
[README configuration section](https://github.com/tari-project/tari/blob/development/README.md#tari-sha3-mining)
is required. The Minotari Miner can also be located on a remote workstation.

Configuration options for the Minotari Miner are as follows:

- `base_node_grpc_address` - this is IPv4/IPv6 address including port number, by which the Minotari Base Node can be found;
- `num_mining_threads` - the number of mining threads, which defaults to the number of CPU cores;
- `mine_on_tip_only` - mining will only start when the Minotari Base Node reports it is in the bootstrapped state;
- `proof_of_work_algo` - The proof of work algorithm to use
- `validate_tip_timeout_sec` - the interval at which the current block height will be checked to determine if mining
   must be restarted, whereby the tip might have advanced passed the block height that is in use in the current template.
- `mining_pool_address` - Stratum Mode configuration - mining pool address
- `mining_wallet_address` - `Stratum Mode configuration - mining wallet address/public key`
- `mining_worker_name` - `Stratum Mode configuration - mining worker name`
- `coinbase_extra` - Note that this data is publicly readable, but it is suggested you populate it so that pool 
   dominance can be seen before any one party has more than 51%.
- `network` - "Selected network"
- `wait_timeout_on_error` - "Base node reconnect timeout after any gRPC or miner error"
- `wallet_payment_address` - "The Tari wallet address where the mining funds will be sent to"

### Caveats

Currently, the Minotari Miner only supports SHA3 mining; this is adequate for the current Tari protocol.
