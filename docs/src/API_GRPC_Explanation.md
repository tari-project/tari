[wallet-proto]: https://github.com/tari-project/tari/blob/development/applications/minotari_app_grpc/proto/wallet.proto

# gRPC API for Minotari Wallet
Below is documentation regarding various gRPC methods available for the Minotari Console Wallet.

- [Introduction](#introduction)
  - [General Structure](#general-structure)
  - [Tari Address Structure](#tari-address-structure-with-optional-payment-id)
  - [Code Generation from .proto files](#understanding-code-generation-from-proto-files)
  - [Loading the Protocol Buffer Definition](#loading-the-protocol-buffer-definition)
  - [Instantiating the Client](#instantiating-the-client)
  - [Authentication with gRPC](#authentication-with-grpc)
- [Base Node gRPC Methods](#grpc-base-node-methods)
- [Wallet gRPC Methods](#grpc-wallet-methods)
- [Useful Non-gRPC Methods](#unrelated-functions-not-available-in-the-grpc-but-useful)

## Introduction
This document provides detailed descriptions of the Remote Procedure Call (RPC) methods available in the Minotari Wallet using **gRPC**. These gRPC methods allow developers and users to interact programmatically with the Minotari Wallet, enabling a wide range of operations such as querying balances, managing transactions, and retrieving wallet-related data.

Use of gRPC requires access to a full node configured to allow gRPC calls. More on this can be read in the [Adding Tari to your Exchange guide](https://www.tari.com/lessons/09_adding_tari_to_your_exchange).

### General Structure
Each gRPC method has the following general structure:

- **Protocol**: gRPC
- **Service**: Defined in the [`wallet.proto`][wallet-proto] file (e.g., `Wallet` service).
- **Request Format**: Protocol Buffers (Protobuf messages).
- **Response Format**: Protocol Buffers (Protobuf messages).
- **Endpoint**: The gRPC server address, typically defined as a host and port combination (e.g., `127.0.0.1:18183`).

To make a gRPC call, a client application must:
1. Use the generated gRPC client stubs from the [`wallet.proto`][wallet-proto] file.
2. Call the desired method (e.g., `GetBalance`).
3. Pass the appropriate Protobuf request object and handle the Protobuf response or any errors.

Below is an example of how a gRPC call would typically work for a function defined in [`wallet.proto`][wallet-proto].

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

### Tari Address Structure (with Optional Payment ID)

Tari addresses are an address scheme used by Tari. Each address includes the necessary information for identifying the network, verifying integrity, and optionally embedding an **encrypted payment identifier**. The [RFC-0155 TariAddress](https://rfc.tari.com/RFC-0155_TariAddress) can be reviewed for more information.

> We strongly recommend the use of Emoji ID as the preferred format. This is discussed in more detail in the [encoding](#encoding) section below.

#### Binary Structure

| Offset | Field            | Rule/Use                                                                 |
|--------|------------------|---------------------------------------------------------------------------|
| 0      | Network ID       | Indicates which network the address belongs to (e.g., Mainnet/Testnet).   |
| 1      | Features         | Flags whether it's a one-sided or interactive address, and if payment ID is used. |
| 2–33   | Public View Key  | Used by receivers to detect payments addressed to them.                   |
| 34–65  | Public Spend Key | Required to authorize spending from the wallet.                           |
| 66–N   | Payment ID       | *(Optional)* Encrypted tag embedded for tracking the purpose of payment. |
| N+1    | Checksum         | Calculated using the [Damm algorithm](https://en.wikipedia.org/wiki/Damm_algorithm). |

#### Payment ID Feature (Optional)

The optional Payment ID feature allows an exchange, merchant or other service to append a payment ID to the address in a manner that preserves privacy while still allowing the service to track payments, withdrawals and other activity against a particular user. This is done by requesting an address from the existing wallet via the gRPC method `GetPaymentIdAddress`, described later in the document.

When included, the payment_id is encrypted using the public keys of the address. The Features byte uses bitflag 2 (value 4) to indicate the presence of a payment ID. For example:
   
   impl TariAddressFeatures: u8 {
        // this forces a transaction to include the following payment id
        const PAYMENT_ID = 0b0000_0100;
        const INTERACTIVE = 0b0000_0010;
        ///one sided payment
        const ONE_SIDED = 0b0000_0001;
    }

The maximum allowed size for `payment_id` is **256 bytes**. Larger values will raise:
  ```rust
  TariAddressError::PaymentIdTooLarge
  ```

Please note that fees will be applicable for every bit used in the `payment_id`.

#### Encoding
After serialization, the complete byte array is encoded using **Base58**, resulting in a human-readable Tari address.

Please note that Tari supports three address formats for representation of the address:
- Hexadecimal
- Base58
- Emoji ID

**EmojiID**

The **Emoji ID** is the preferred encoding for Tari addresses. Emoji ID has a number of benefits for users:
- The address is shorter, and provides for more easily-identifiable characters, thus eliminating identification errors (0 vs O, 1 vs l)
- The alphabet used for Emoji ID is larger than hexidecimal or Base58, resulting in shorter character sequences for encoding
- The use of a checksum can verify if the address is correct and for the correct network.

The EmojiID is derived deterministically from a public view key as a 33-byte address, with the first 32-characters representing the address and the 33rd character a checksum of the address calculated from `DammSumm`. The checksum can be used to confirm the address validity and other variables/feature requirements (such as whether the address is for the correct network.) Conversion between these forms is supported, with automatic checksum validation. The public key is recoverable from the Emoji ID.

You can find more information about the Emoji ID implementation here: [emoji.rs implementation](https://github.com/tari-project/tari/blob/development/base_layer/common_types/src/emoji.rs)

The `GetAddress` gRPC call can retrieve the wallet's Emoji ID address.

The 256 emojis used are shown below:

| 🐢 | 📟 | 🌈 | 🌊 | 🎯 | 🐋 | 🌙 | 🤔 | 🌕 | ⭐ | 🎋 | 🌰 | 🌴 | 🌵 | 🌲 | 🌸 |
|-----|-----|-----|-----|-----|-----|-----|-----|-----|-----|-----|-----|-----|-----|-----|-----|
| 🌹 | 🌻 | 🌽 | 🍀 | 🍁 | 🍄 | 🥑 | 🍆 | 🍇 | 🍈 | 🍉 | 🍊 | 🍋 | 🍌 | 🍍 | 🍎 |
| 🍐 | 🍑 | 🍒 | 🍓 | 🍔 | 🍕 | 🍗 | 🍚 | 🍞 | 🍟 | 🥝 | 🍣 | 🍦 | 🍩 | 🍪 | 🍫 |
| 🍬 | 🍭 | 🍯 | 🥐 | 🍳 | 🥄 | 🍵 | 🍶 | 🍷 | 🍸 | 🍾 | 🍺 | 🍼 | 🎀 | 🎁 | 🎂 |
| 🎃 | 🤖 | 🎈 | 🎉 | 🎒 | 🎓 | 🎠 | 🎡 | 🎢 | 🎣 | 🎤 | 🎥 | 🎧 | 🎨 | 🎩 | 🎪 |
| 🎬 | 🎭 | 🎮 | 🎰 | 🎱 | 🎲 | 🎳 | 🎵 | 🎷 | 🎸 | 🎹 | 🎺 | 🎻 | 🎼 | 🎽 | 🎾 |
| 🎿 | 🏀 | 🏁 | 🏆 | 🏈 | ⚽ | 🏠 | 🏥 | 🏦 | 🏭 | 🏰 | 🐀 | 🐉 | 🐊 | 🐌 | 🐍 |
| 🦁 | 🐐 | 🐑 | 🐔 | 🙈 | 🐗 | 🐘 | 🐙 | 🐚 | 🐛 | 🐜 | 🐝 | 🐞 | 🦋 | 🐣 | 🐨 |
| 🦀 | 🐪 | 🐬 | 🐭 | 🐮 | 🐯 | 🐰 | 🦆 | 🦂 | 🐴 | 🐵 | 🐶 | 🐷 | 🐸 | 🐺 | 🐻 |
| 🐼 | 🐽 | 🐾 | 👀 | 👅 | 👑 | 👒 | 🧢 | 💅 | 👕 | 👖 | 👗 | 👘 | 👙 | 💃 | 👛 |
| 👞 | 👟 | 👠 | 🥊 | 👢 | 👣 | 🤡 | 👻 | 👽 | 👾 | 🤠 | 👃 | 💄 | 💈 | 💉 | 💊 |
| 💋 | 👂 | 💍 | 💎 | 💐 | 💔 | 🔒 | 🧩 | 💡 | 💣 | 💤 | 💦 | 💨 | 💩 | ➕ | 💯 |
| 💰 | 💳 | 💵 | 💺 | 💻 | 💼 | 📈 | 📜 | 📌 | 📎 | 📖 | 📿 | 📡 | ⏰ | 📱 | 📷 |
| 🔋 | 🔌 | 🚰 | 🔑 | 🔔 | 🔥 | 🔦 | 🔧 | 🔨 | 🔩 | 🔪 | 🔫 | 🔬 | 🔭 | 🔮 | 🔱 |
| 🗽 | 😂 | 😇 | 😈 | 🤑 | 😍 | 😎 | 😱 | 😷 | 🤢 | 👍 | 👶 | 🚀 | 🚁 | 🚂 | 🚚 |
| 🚑 | 🚒 | 🚓 | 🛵 | 🚗 | 🚜 | 🚢 | 🚦 | 🚧 | 🚨 | 🚪 | 🚫 | 🚲 | 🚽 | 🚿 | 🧲 |

### Understanding Code Generation from `.proto` Files
The `.proto` file, such as [`wallet.proto`][wallet-proto], acts as a **shared contract** that defines all available services, methods, and message structures for the Minotari Wallet's gRPC API. However, it is not executable code by itself.

To actually call these methods in your application, the `.proto` file must be **processed into usable code** through a process known as **code generation**. This step provides you with:

- **Message classes** you can use to create requests and read responses (e.g., `GetBalanceRequest`)
- **Service stubs or clients** that wrap the underlying gRPC transport and let you call methods like `GetBalance()` as regular functions
- Support for automatic serialization, deserialization, and type checking

In some languages like Java or Go, this is done ahead of time using the `protoc` compiler:

```bash
protoc --go_out=. --go-grpc_out=. wallet.proto
```

In others, like Node.js, code generation can be done **at runtime** using tools such as `@grpc/proto-loader`, which dynamically loads and interprets the `.proto` definitions.

Regardless of the language, this step is required: it transforms the `.proto` contract into concrete, usable APIs in your application.

> 🔧 **Note:** If you're using a statically typed language, make sure to run the appropriate `protoc` command to generate your language-specific files before attempting to use the gRPC client.

### Loading the Protocol Buffer Definition
In gRPC, a `.proto` file defines the **contract** between services and clients. This contract includes:

- The **RPC methods** a service exposes (e.g., `SendTransaction`, `GetBalance`)
- The **data types** used for requests and responses (e.g., `TransactionRequest`, `TransactionResponse`)

All gRPC clients and servers—regardless of the programming language—use this `.proto` file to generate code that knows how to encode, decode, and handle communication between endpoints.

Whether you're building a gRPC client in Java, Python, Go, or Node.js, one of the first steps is to **load and parse the `.proto` file**. This step:

- Converts the definitions into usable service and message classes/objects.
- Ensures consistent structure across different implementations.
- Enables automatic serialization (binary encoding) and deserialization of data between systems.

In **Node.js**, for example, this is done using a utility like `@grpc/proto-loader`:

```javascript
const packageDefinition = protoLoader.loadSync('wallet.proto', {
  keepCase: true,
  longs: String,
  enums: String,
  defaults: true,
  oneofs: true,
});
```

### Instantiating the Client
To use the methods, you will need to use a gRPC library to instantiate a gRPC client against the [`wallet.proto`][wallet-proto] file. Once done, you can then call various methods against the gRPC wallet service. Instantiating the client will differ depending on your particular language. Below is an example of a Node.js implementation.

```javascript
// Imports the gRPC library for Node.js, specifically the pure JavaScript implementation (@grpc/grpc-js), which supports HTTP/2 and is the modern, recommended one.
const grpc = require('@grpc/grpc-js');
// Imports a module that can parse .proto files into a format grpc-js can understand.
const protoLoader = require('@grpc/proto-loader');
// Loads and synchronously parses the wallet.proto file using proto-loader.
const packageDef = protoLoader.loadSync("wallet.proto", {});
// Takes the parsed proto package and feeds it into grpc.loadPackageDefinition() to make it usable with grpc-js.
const walletProto = grpc.loadPackageDefinition(packageDef).Wallet;
// Instantiates a gRPC client for the Wallet service.
const client = new walletProto('localhost:18183', grpc.credentials.createInsecure());
```

This would need to be placed before any method call.

### Authentication with gRPC
The `GrpcAuthentication` object supports two modes:
- **None**: No authentication is required.
- **Basic**: Username and password are used for authentication. Note that these are distinct from your wallet's credentials and are configured separately.

Here are step-by-step instructions for enabling and configuring basic gRPC authentication in the Tari wallet using the `config.toml` file:

#### 1. **Locate the Configuration File**
   - Navigate to the `config.toml` file for your Tari wallet.
   - Example path: `common/config/presets/d_console_wallet.toml`.

#### 2. **Enable gRPC Authentication**
   - Open the configuration file in a text editor.
   - Find the following commented-out section:
     ```toml
     # gRPC authentication method (default = "none")
     #grpc_authentication = { username = "admin", password = "xxxx" }
     ```

#### 3. **Uncomment and Configure**
   - Uncomment the `grpc_authentication` line by removing the `#` at the beginning.
   - Set a clear-text username and password. For example:
     ```toml
     grpc_authentication = { username = "admin", password = "mysecurepassword" }
     ```

#### 4. **Save the File**
   - After making the changes, save the `config.toml` file.

#### 5. **Restart the Wallet**
   - Restart your Tari wallet application to apply the updated configuration.

#### 6. **Client-Side Configuration**
   - Ensure your gRPC client connects using basic authentication. For example, in JavaScript:

     ```javascript
     const grpc = require('@grpc/grpc-js');
     const metadata = new grpc.Metadata();
     metadata.add('username', 'admin');
     metadata.add('password', 'mysecurepassword');

     const callCredentials = grpc.credentials.createFromMetadataGenerator((_, callback) => {
         callback(null, metadata);
     });

     const channelCredentials = grpc.credentials.combineChannelCredentials(
         grpc.credentials.createSsl(),
         callCredentials
     );

     const client = new walletProto('localhost:18183', channelCredentials);
     ```

#### 7. **Test the Connection**
   - Verify that the gRPC client can successfully connect using the provided username and password.

   - Ensure the username and password are secure and not shared publicly.
   - If you’re using SSL (`grpc.credentials.createSsl()`), combine it with the credentials for a secure connection.
   - If the gRPC server is running locally, you may use `grpc.credentials.createInsecure()` for testing purposes (not recommended for production).

##### Connecting with Authentication Examples
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
const client = new Client('localhost:18183', {
  authentication: {
    type: 'basic',
    username: 'my_username',
    password: 'my_password',
  },
});

console.log('Client created:', client);
```

## gRPC Base Node Methods
These methods are dependent on access to a base node and use of the [`base_node.proto`](https://github.com/tari-project/tari/blob/development/applications/minotari_app_grpc/proto/base_node.proto) file.

### Get Max Height
You can call the base node's gRPC method to get the current blockchain height.

Example:
```javascript
const client = new Client('localhost:18143'); // Connect to base node
const response = await client.getTipInfo();
console.log('Max Height:', response.chain_height);
```

## gRPC Wallet Methods
These methods are dependent on access to a wallet node and use of the [`wallet.proto`][wallet-proto] file.

### Get Balance
The wallet gRPC method `GetBalance` is used to retrieve a wallet's total available and pending balances. 

Example:
```javascript
     const balance = await client.GetBalance();
     console.log('Available Balance:', balance.available_balance);
     console.log('Pending Incoming Balance:', balance.pending_incoming_balance);
     console.log('Pending Outgoing Balance:', balance.pending_outgoing_balance);
     console.log('Time Locked Balance:', balance.timelocked_balance);
```

In addition, it is possible to retrieve the balance of a wallet's funds that are matched to a specific `payment_id` provided with any transactions to the wallet. This will provide the total of all transactions that were made into the wallet using that `payment_id`

The `payment_id` can be specified in three formats: 
- **`u256 (bytes)`**: Must be provided as a byte array.
- **`utf8_string (string)`**: Must be a valid UTF-8 string.
- **`user_bytes (bytes)`**: Must be provided as a generic byte array.

```javascript
const userPaymentId = {
         utf8_string: "your_payment_id_string" // Replace this with your actual payment ID
     };

     const balance = await client.GetBalance({ payment_id: userPaymentId });
     console.log('Available Balance:', balance.available_balance);
     console.log('Pending Incoming Balance:', balance.pending_incoming_balance);
     console.log('Pending Outgoing Balance:', balance.pending_outgoing_balance);
     console.log('Time Locked Balance:', balance.timelocked_balance);
```

**Example JSON Response:**
```json
{
  "available_balance": 1000000,
  "pending_incoming_balance": 200000,
  "pending_outgoing_balance": 50000,
  "timelocked_balance": 0,
}
```

### Get Address 
This RPC returns addresses generated for a specific payment ID. It provides both the interactive and one-sided addresses for the given payment ID, along with their respective representations in base58 and emoji formats.

 Example usage (JavaScript):

 ```javascript
 // Prepare the payment ID for the request
 const paymentId = Buffer.from('your_payment_id_here', 'hex');
 const request = { payment_id: paymentId };

 // Call the GetPaymentIdAddress RPC method
 client.GetPaymentIdAddress(request, (error, response) => {
   if (error) {
     console.error('Error:', error);
   } else {
     console.log('Payment ID Address Response:', response);
   }
 });
 ```

 **Sample JSON Response:**

 ```json
{
  "interactive_address": "0411aabbccddeeff00112233445566778899aabbccddeeff0011223344556677",
  "one_sided_address": "02ff8899aabbccddeeff00112233445566778899aabbccddeeff001122334455",
  "interactive_address_base58": "14HVCEeZC2RGE4SDn3yG.....6xouGvS5SXwEvXKwK3zLz2rgReh",
  "one_sided_address_base58": "12HVCEeZC2RGE4SDn3yGwqz.....obB1a6xouGvS5SXwEvXKwK3zLz2rgReL",
  "interactive_address_emoji": "🐢🌊💤🔌🚑🐛🏦⚽🍓🐭🚁🎢🔪🥐👛🍞.....🍐🍟💵🎉🍯🎁🎾🎼💻💄🍳🍐🤔🥝🍫👅🚀🐬🎭",
  "one_sided_address_emoji": "🐢📟💤🔌🚑🐛🏦⚽🍓🐭🚁🎢🔪🥐👛🍞📜.....🍐🍟💵🎉🍯🎁🎾🎼💻💄🍳🍐🤔🥝🍫👅🚀🐬🎭"
}
```

### Get Payment ID Address
The `GetPaymentIdAddress` gRPC method returns an address appended with a payment ID, derived from an existing address. The payment ID is an optional, additional piece of metadata (like an invoice number or customer reference).

- `payment_id` (optional) must be passed as a UTF-8 encoded byte array. If derived from a string, the `payment_id` must be encoded in UTF-8.

**Example:**
```javascript
const crypto = require('crypto');

 Generate a 32-byte random payment_id
const paymentId = crypto.randomBytes(32);  This will be a Buffer

client.GetPaymentIdAddress({ payment_id: paymentId }, (error, response) => {
  if (error) {
    console.error('gRPC Error:', error);
    return;
  }

  console.log(JSON.stringify({
    interactive_address: Buffer.from(response.interactive_address).toString('hex'),
    one_sided_address: Buffer.from(response.one_sided_address).toString('hex'),
    interactive_address_base58: response.interactive_address_base58,
    one_sided_address_base58: response.one_sided_address_base58,
    interactive_address_emoji: response.interactive_address_emoji,
    one_sided_address_emoji: response.one_sided_address_emoji
  }, null, 2));
});
```

**Example of JSON response**:
```json
{
  "interactive_address": "0411aabbccddeeff00112233445566778899aabbccddeeff0011223344556677",
  "one_sided_address": "02ff8899aabbccddeeff00112233445566778899aabbccddeeff001122334455",
  "interactive_address_base58": "14HVCEeZC2RGE4SDn3yG.....6xouGvS5SXwEvXKwK3zLz2rgReh",
  "one_sided_address_base58": "12HVCEeZC2RGE4SDn3yGwqz.....obB1a6xouGvS5SXwEvXKwK3zLz2rgReL",
  "interactive_address_emoji": "🐢🌊💤🔌🚑🐛🏦⚽🍓🐭🚁🎢🔪🥐👛🍞.....🍐🍟💵🎉🍯🎁🎾🎼💻💄🍳🍐🤔🥝🍫👅🚀🐬🎭",
  "one_sided_address_emoji": "🐢📟💤🔌🚑🐛🏦⚽🍓🐭🚁🎢🔪🥐👛🍞📜.....🍐🍟💵🎉🍯🎁🎾🎼💻💄🍳🍐🤔🥝🍫👅🚀🐬🎭"
}
```

### Get Transactions by Payment ID
The `GetCompletedTransactions` method retrieves all completed transactions against a particular wallet, which can be optionally filtered by passing the `payment_id` to show only completed transactions associated with the payment ID.

- `payment_id` (optional) must be passed as a UTF-8 encoded byte array. If derived from a string, the `payment_id` must be encoded in UTF-8 and should not contain invalid UTF-8 characters.

**Example of retrieving all transactions:**
```javascript
// Define the request without a payment_id
const request = {};

// Call GetCompletedTransactions
client.GetCompletedTransactions(request, (error, response) => {
  if (error) {
    console.error('Error:', error);
  } else {
    console.log('Completed Transactions:', response);
  }
});
```

This example retrieves completed transactions filtered by a specific `payment_id`.

```javascript
// Define the request with a payment_id
const request = {
  payment_id: {
    utf8_string: 'example_payment_id', // Replace with your payment_id
  },
};

// Call GetCompletedTransactions
client.GetCompletedTransactions(request, (error, response) => {
  if (error) {
    console.error('Error:', error);
  } else {
    console.log('Filtered Completed Transactions:', response);
  }
});
```

**Example of JSON Response**
```json
{
  "transaction": {
    "tx_id": "123456",
    "source_address": "B1a2c3d4e5f6g7h8i9j0",
    "dest_address": "A9j8h7g6f5e4d3c2b1a0",
    "status": 3,
    "amount": "1000000000",
    "is_cancelled": false,
    "direction": 1,
    "fee": "2500000",
    "timestamp": 1714328123,
    "excess_sig": "abcdef0123456789...",
    "payment_id": "4f3c2a1b",
    "mined_in_block_height": 10203
  }
}
```

### Get Transaction Info
You can use the `GetTransactionInfo` gRPC method to obtain information about transactions associated with one or more `transaction_id`.

- The `transaction_id` is defined as repeated uint64. Must be an unsigned 64-bit integer (e.g., 1234567890). Ensure it is passed as an array if querying multiple transactions.

Example:
```javascript
     const txDetails = await client.getTransactionInfo({ transaction_id: ['your-transaction-id'] });
     console.log(txDetails);
```

**Example JSON Response**
```json
{
  "transactions": [
    {
      "tx_id": "1234567890",
      "source_address": "T1a2b3c4d5e6f7g8h9i0jklmnopqrstuvwx",
      "dest_address": "T1z9y8x7w6v5u4t3s2r1q0ponmlkjihgfedcba",
      "status": 2,
      "amount": "1000000000",
      "is_cancelled": false,
      "direction": 1,
      "fee": "1000000",
      "timestamp": 1714328123,
      "excess_sig": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
      "payment_id": "4f3c2a1b",
      "mined_in_block_height": 10203
    },
    {
      "tx_id": "9876543210",
      "source_address": "T1z9y8x7w6v5u4t3s2r1q0ponmlkjihgfedcba",
      "dest_address": "T1a2b3c4d5e6f7g8h9i0jklmnopqrstuvwx",
      "status": 1,
      "amount": "500000000",
      "is_cancelled": false,
      "direction": 2,
      "fee": "500000",
      "timestamp": 1714329156,
      "excess_sig": "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
      "payment_id": "7e8f9d0c",
      "mined_in_block_height": 10204
    }
  ]
}
```

### Send a Transaction
Use the `transfer` function to perform a send transaction to a participant.

Example:
```javascript
     const transferResponse = await client.Transfer({
       destination: 'receiver-tari-address',
       amount: 1000000,           // Amount in µT
       fee_per_gram: 25,          // Fee per gram
       message: 'Payment for services'  // Maximum message size is 32 bytes (256 bits)
     });
     console.log('Transfer successful:', transferResponse);
```

## Unrelated functions not available in the gRPC but useful
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