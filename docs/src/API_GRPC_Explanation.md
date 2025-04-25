# gRPC API for Minotari Wallet
## Introduction
This document provides detailed descriptions of the Remote Procedure Call (RPC) methods available in the Minotari Wallet using **gRPC**. These gRPC methods allow developers and users to interact programmatically with the Minotari Wallet, enabling a wide range of operations such as querying balances, managing transactions, and retrieving wallet-related data.

Use of gRPC requires access to a full node configured to allow gRPC calls. More on this can be read in the [Adding Tari to your Exchange guide](https://www.tari.com/lessons/09_adding_tari_to_your_exchange).

### General Structure

Each gRPC method has the following general structure:

- **Protocol**: gRPC
- **Service**: Defined in the `wallet.proto` file (e.g., `Wallet` service).
- **Request Format**: Protocol Buffers (Protobuf messages).
- **Response Format**: Protocol Buffers (Protobuf messages).
- **Endpoint**: The gRPC server address, typically defined as a host and port combination (e.g., `127.0.0.1:50051`).

To make a gRPC call, a client application must:
1. Use the generated gRPC client stubs from the `wallet.proto` file, located [here](https://github.com/tari-project/tari/blob/development/applications/minotari_app_grpc/proto/wallet.proto).
2. Call the desired method (e.g., `GetBalance`).
3. Pass the appropriate Protobuf request object and handle the Protobuf response or any errors.

Below is an example of how a gRPC call would typically work for a function defined in `wallet.proto`.

```proto
service Wallet {
  rpc GetBalance (GetBalanceRequest) returns (GetBalanceResponse);
}

message GetBalanceRequest {
}

message GetBalanceResponse {
  uint64 available_balance = 1;
  uint64 pending_incoming_balance = 2;
  uint64 pending_outgoing_balance = 3;
}
```

## gRPC Base Node Methods
### Get Max Height
You can call the base node's gRPC method to get the current blockchain height.

Example:
```javascript
const client = new Client('localhost:18143'); // Connect to base node
const response = await client.getTipInfo();
console.log('Max Height:', response.chain_height);
```

## gRPC Wallet Methods
### Get Balance
Use the wallet gRPC method `getBalance` to retrieve the wallet's available and pending balances.

Example:
```javascript
     const balance = await client.getBalance();
     console.log('Available Balance:', balance.available_balance);
     console.log('Pending Incoming Balance:', balance.pending_incoming_balance);
     console.log('Pending Outgoing Balance:', balance.pending_outgoing_balance);
```

### Create Wallet
By default, the gRPC interface is incapable of creating a wallet at this stage. It is used for interacting with a single wallet instance.

To create a wallet, a user can use the `minotari_console_wallet` command and follow the instructions. More details on this process are available in the [Adding Tari to your Exchange guide](https://www.tari.com/lessons/09_adding_tari_to_your_exchange).

Users can, however, use the Tari Wallet FFI to create a new wallet directly. You will need to provide a seed phrase or allow the wallet to generate one.

You can find [the Wallet FFI here](https://github.com/tari-project/tari/tree/b4ba3a438a414c4c0408add103d7185d74f48ebc/base_layer/wallet_ffi): 

Example using the FFI:
```javascript
     const wallet = lib.wallet_create(
       comms,                    // Communication configuration
       "./wallet/logs",          // Log files
       5,                        // Comms buffer size
       10240,                    // Message cache size
       null,                     // Passphrase
       seedWords,                // Seed words if provided. Requires 20 words, defined in the BIP-39 mnemonic standard. If not provided, a unique seed key will be generated automatically.
       receivedTxCallback,       // Callbacks for various events
       receivedTxReplyCallback,
       receivedFinalizedCallback,
       txBroadcastCallback,
       txMinedCallback
     );
     console.log("Wallet created:", wallet);
```

### Get Transaction Info
You can use the `getTransactionInfo` gRPC method to obtain information about a specific transaction.
   
Example:
```javascript
     const txDetails = await client.getTransactionInfo({ txId: 'your-transaction-id' });
     console.log(txDetails);
```

### Fetch UTXOs by Block ID
The `fetch_unspent_utxos_in_block` function is used to fetch unspent transaction outputs (UTXOs) within a specific block by its hash. You will need to interact with the base node directly via the `BaseNodeCommsInterface`.

Example:
```rust
use tari_common_types::types::BlockHash;
use tari_core::transactions::transaction::TransactionOutput;
use tari_service_framework::reply_channel::RequestSender;
use tari_comms_dht::outbound::OutboundMessageRequester;
use std::sync::Arc;

async fn fetch_utxos_for_block(
    block_hash: BlockHash,
    request_sender: Arc<RequestSender>, // Replace with the actual `request_sender` type
) -> Result<Vec<TransactionOutput>, Box<dyn std::error::Error>> {
    // Create a mutable instance of the interface
    let mut base_node_interface = BaseNodeCommsInterface::new(
        request_sender,
        OutboundMessageRequester::default(), // Replace with the actual implementation
    );

    // Fetch the UTXOs
    match base_node_interface.fetch_unspent_utxos_in_block(block_hash).await {
        Ok(utxos) => {
            println!("Fetched {} UTXOs in block.", utxos.len());
            Ok(utxos)
        },
        Err(e) => {
            eprintln!("Error fetching UTXOs: {:?}", e);
            Err(Box::new(e))
        },
    }
}
```

Pass the block hash to fetch UTXOs.

### Send a Transaction
Use the `transfer` function to perform a send transaction to a participant.

Example:
```javascript
     const transferResponse = await client.transfer({
       destination: 'receiver-tari-address',
       amount: 1000000,           // Amount in µT
       fee_per_gram: 25,          // Fee per gram
       message: 'Payment for services'  // Maximum message size is 32 bytes (256 bits)
     });
     console.log('Transfer successful:', transferResponse);
```

#### Authentication with gRPC
The `GrpcAuthentication` object supports two modes:
- **None**: No authentication is required.
- **Basic**: Username and password are used for authentication. Note that these are distinct from your wallet's credentials and are configured separately.

Example:
```rust
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum GrpcAuthentication {
    None,
    Basic {
        username: String,
        password: SafePassword,
    },
}
```

#### Connecting with Authentication Examples
**Rust:**
```rust
use serde::{Deserialize, Serialize};
use tari_utilities::SafePassword;

// `untagged` allows matching JSON structures to either variant without an explicit tag.
// `default` marks `None` as the fallback when no fields are present.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GrpcAuthentication {
    #[default]
    None,
    Basic {
        username: String,
        #[serde(deserialize_with = "deserialize_safe_password")]
        password: SafePassword,
    },
}

fn main() {
    let auth_config = GrpcAuthentication::Basic {
        username: "my_username".to_string(),
        password: SafePassword::from("my_password".to_string()),
    };
}
```

**Node.js:**
```javascript
const { Client } = require('./path/to/clients/nodejs/wallet_grpc_client');

// Replace './path/to/clients/nodejs/wallet_grpc_client' with the installed module name or relative path
const client = new Client('localhost:18143', {
  authentication: {
    type: 'basic',
    username: 'my_username',
    password: 'my_password',
  },
});

console.log('Client created:', client);
```