# Tari gRPC API Overview

This document provides a comprehensive overview of all gRPC services in the Tari project, their purposes, available methods, request/response schemas, and implementation details.

## Overview

Tari provides several gRPC services that enable programmatic interaction with different components of the ecosystem:

- **BaseNode service** - Core blockchain operations, mining, and chain queries
- **Wallet service** - Wallet operations, transaction management, and payment handling
- **ValidatorNode service** - Smart contract operations and sidechain interactions
- **ShaP2Pool service** - P2Pool mining operations

All services use Protocol Buffers (protobuf) for message serialization and are implemented using the tonic gRPC framework in Rust.

## Service Implementations

### Application Hosting

- **minotari_node** - Hosts BaseNode and ShaP2Pool services
- **minotari_console_wallet** - Hosts Wallet service
- **Validator Node** (planned) - Hosts ValidatorNode service

### Connection Details

**Default Addresses:**
- BaseNode gRPC: `127.0.0.1:18142`
- Wallet gRPC: `127.0.0.1:18143`
- ValidatorNode gRPC: (varies by deployment)

**Security:**
- TLS encryption support (optional)
- Method-level access control
- Network-level access restrictions

---

## BaseNode Service

**Service Definition**: `tari.rpc.BaseNode`
**Hosted by**: minotari_node
**Proto File**: `applications/minotari_app_grpc/proto/base_node.proto`

The BaseNode service provides access to blockchain data, mining operations, and network state information.

### Chain Query Methods

#### ListHeaders
**Purpose**: Lists block headers from the current best chain with pagination support.

**Request**: `ListHeadersRequest`
```protobuf
message ListHeadersRequest {
    uint64 from_height = 1;     // Starting height (optional)
    uint64 num_headers = 2;     // Number of headers to return (default: 10)
    Sorting sorting = 3;        // SORTING_DESC or SORTING_ASC
}
```

**Response**: `stream BlockHeaderResponse`
```protobuf
message BlockHeaderResponse {
    BlockHeader header = 1;     // The block header
    uint64 confirmations = 2;   // Blocks from tip (depth)
    uint64 reward = 3;          // Block reward (mining + fees)
    uint64 difficulty = 4;      // Achieved difficulty
    uint32 num_transactions = 5; // Transaction count
}
```

**Example Usage**:
```javascript
const request = {
    from_height: 1000000,
    num_headers: 50,
    sorting: 0 // DESC
};
const stream = client.listHeaders(request);
stream.on('data', (response) => {
    console.log(`Header at height ${response.header.height}`);
});
```

#### GetHeaderByHash
**Purpose**: Retrieves a specific block header by its hash.

**Request**: `GetHeaderByHashRequest`
```protobuf
message GetHeaderByHashRequest {
    bytes hash = 1;  // Block hash (32 bytes)
}
```

**Response**: `BlockHeaderResponse`

#### GetBlocks
**Purpose**: Returns complete blocks by height(s).

**Request**: `GetBlocksRequest`
```protobuf
message GetBlocksRequest {
    repeated uint64 heights = 1;  // Block heights to fetch
}
```

**Response**: `stream HistoricalBlock`
```protobuf
message HistoricalBlock {
    uint64 confirmations = 1;  // Confirmation count
    Block block = 2;           // Complete block data
}
```

#### GetBlockTiming
**Purpose**: Returns block timing statistics for a height range.

**Request**: `HeightRequest`
**Response**: `BlockTimingResponse`

### Network Information Methods

#### GetSyncInfo
**Purpose**: Returns base node synchronization status.

**Request**: `Empty`
**Response**: `SyncInfoResponse`
```protobuf
message SyncInfoResponse {
    uint64 tip_height = 1;       // Network tip height
    uint64 local_height = 2;     // Local node height
    repeated bytes peer_node_id = 3; // Connected peers
}
```

#### GetSyncProgress
**Purpose**: Detailed synchronization progress information.

**Request**: `Empty`
**Response**: `SyncProgressResponse`
```protobuf
message SyncProgressResponse {
    uint64 tip_height = 1;
    uint64 local_height = 2;
    SyncState state = 3;         // STARTUP, HEADER, BLOCK, DONE, etc.
    string short_desc = 4;
    uint64 initial_connected_peers = 5;
}
```

#### GetTipInfo
**Purpose**: Returns current blockchain tip information.

**Request**: `Empty`
**Response**: `TipInfoResponse`
```protobuf
message TipInfoResponse {
    MetaData metadata = 1;           // Chain metadata
    bool initial_sync_achieved = 2;  // Sync status
    BaseNodeState base_node_state = 3; // Node state
    bool failed_checkpoints = 4;     // Checkpoint failures
}
```

#### GetNetworkStatus
**Purpose**: Returns network connectivity and peer information.

**Request**: `Empty`
**Response**: `NetworkStatusResponse`

#### Identify
**Purpose**: Returns the node's network identity.

**Request**: `Empty`
**Response**: `NodeIdentity`

### Mining Methods

#### GetNewBlockTemplate
**Purpose**: Requests a new block template for mining.

**Request**: `NewBlockTemplateRequest`
```protobuf
message NewBlockTemplateRequest {
    PowAlgo algo = 1;        // POW_ALGOS_RANDOMXM, POW_ALGOS_SHA3X, POW_ALGOS_RANDOMXT
    uint64 max_weight = 2;   // Maximum block weight
}
```

**Response**: `NewBlockTemplateResponse`
```protobuf
message NewBlockTemplateResponse {
    NewBlockTemplate new_block_template = 1;
    bool initial_sync_achieved = 3;
    MinerData miner_data = 4;
}
```

#### GetNewBlock
**Purpose**: Constructs a complete block from a template.

**Request**: `NewBlockTemplate`
**Response**: `GetNewBlockResult`
```protobuf
message GetNewBlockResult {
    bytes block_hash = 1;        // Completed block hash
    Block block = 2;             // Complete block
    bytes merge_mining_hash = 3; // For merge mining
    bytes tari_unique_id = 4;    // Unique identifier
    MinerData miner_data = 5;
    bytes vm_key = 6;
}
```

#### SubmitBlock
**Purpose**: Submits a mined block for validation and propagation.

**Request**: `Block`
**Response**: `SubmitBlockResponse`
```protobuf
message SubmitBlockResponse {
    bytes block_hash = 1;  // Hash of submitted block
}
```

#### GetNetworkDifficulty
**Purpose**: Returns network difficulty information over time.

**Request**: `HeightRequest`
**Response**: `stream NetworkDifficultyResponse`
```protobuf
message NetworkDifficultyResponse {
    uint64 difficulty = 1;
    uint64 estimated_hash_rate = 2;
    uint64 height = 3;
    uint64 timestamp = 4;
    uint64 pow_algo = 5;
    uint64 sha3x_estimated_hash_rate = 6;
    uint64 monero_randomx_estimated_hash_rate = 7;
    uint64 tari_randomx_estimated_hash_rate = 10;
}
```

### Transaction Methods

#### SubmitTransaction
**Purpose**: Submits a transaction to the mempool.

**Request**: `SubmitTransactionRequest`
```protobuf
message SubmitTransactionRequest {
    Transaction transaction = 1;
}
```

**Response**: `SubmitTransactionResponse`
```protobuf
message SubmitTransactionResponse {
    SubmitTransactionResult result = 1;  // ACCEPTED, REJECTED, etc.
}
```

#### GetMempoolTransactions
**Purpose**: Returns all transactions currently in the mempool.

**Request**: `GetMempoolTransactionsRequest`
**Response**: `stream GetMempoolTransactionsResponse`

#### TransactionState
**Purpose**: Checks the state of a transaction.

**Request**: `TransactionStateRequest`
```protobuf
message TransactionStateRequest {
    Signature excess_sig = 1;  // Transaction signature
}
```

**Response**: `TransactionStateResponse`
```protobuf
message TransactionStateResponse {
    TransactionLocation result = 1;  // UNKNOWN, MEMPOOL, MINED, NOT_STORED
}
```

### Search and Query Methods

#### SearchKernels
**Purpose**: Searches for blocks containing specific transaction kernels.

**Request**: `SearchKernelsRequest`
```protobuf
message SearchKernelsRequest {
    repeated Signature signatures = 1;  // Kernel signatures to find
}
```

**Response**: `stream HistoricalBlock`

#### SearchUtxos
**Purpose**: Searches for blocks containing specific UTXOs.

**Request**: `SearchUtxosRequest`
```protobuf
message SearchUtxosRequest {
    repeated bytes commitments = 1;  // UTXO commitments to find
}
```

**Response**: `stream HistoricalBlock`

#### FetchMatchingUtxos
**Purpose**: Fetches UTXOs that exist in the main chain.

**Request**: `FetchMatchingUtxosRequest`
```protobuf
message FetchMatchingUtxosRequest {
    repeated bytes hashes = 1;  // Output hashes to match
}
```

**Response**: `stream FetchMatchingUtxosResponse`

### Payment Reference Methods

#### SearchPaymentReferences
**Purpose**: Searches for outputs by payment reference for block explorers.

**Request**: `SearchPaymentReferencesRequest`
```protobuf
message SearchPaymentReferencesRequest {
    repeated string payment_reference_hex = 1;  // PayRef as hex (64 chars)
    bool include_spent = 2;                     // Include spent outputs
}
```

**Response**: `stream PaymentReferenceResponse`
```protobuf
message PaymentReferenceResponse {
    string payment_reference_hex = 1;
    uint64 block_height = 2;
    bytes block_hash = 3;
    uint64 mined_timestamp = 4;
    bytes commitment = 5;
    bool is_spent = 6;
    uint64 spent_height = 7;
    bytes spent_block_hash = 8;
    uint64 min_value_promise = 9;
}
```

## Wallet Service

**Service Definition**: `tari.rpc.Wallet`
**Hosted by**: minotari_console_wallet
**Proto File**: `applications/minotari_app_grpc/proto/wallet.proto`

The Wallet service provides comprehensive wallet functionality including transaction creation, balance queries, and payment management.

### Identity and Status Methods

#### GetVersion
**Purpose**: Returns the wallet software version.

**Request**: `GetVersionRequest`
**Response**: `GetVersionResponse`

#### GetState
**Purpose**: Returns comprehensive wallet operational state.

**Request**: `GetStateRequest`
**Response**: `GetStateResponse`
```protobuf
message GetStateResponse {
    uint64 scanned_height = 1;     // Latest scanned block height
    Balance balance = 2;           // Current balance breakdown
    NetworkStatus network = 3;     // Network connectivity status
}
```

#### CheckConnectivity
**Purpose**: Returns network connectivity status.

**Request**: `GetConnectivityRequest`
**Response**: `CheckConnectivityResponse`

#### Identify
**Purpose**: Returns wallet identity information.

**Request**: `GetIdentityRequest`
**Response**: `GetIdentityResponse`
```protobuf
message GetIdentityResponse {
    bytes public_key = 1;      // Wallet public key
    bytes public_address = 2;  // Public address
    bytes node_id = 3;         // Network node ID
}
```

### Address Methods

#### GetAddress
**Purpose**: Returns wallet's default addresses.

**Request**: `Empty`
**Response**: `GetAddressResponse`
```protobuf
message GetAddressResponse {
    bytes interactive_address = 1;   // Interactive transaction address
    bytes one_sided_address = 2;     // One-sided transaction address
}
```

#### GetCompleteAddress
**Purpose**: Returns addresses in multiple formats (binary, base58, emoji).

**Request**: `Empty`
**Response**: `GetCompleteAddressResponse`
```protobuf
message GetCompleteAddressResponse {
    bytes interactive_address = 1;
    bytes one_sided_address = 2;
    string interactive_address_base58 = 3;
    string one_sided_address_base58 = 4;
    string interactive_address_emoji = 5;
    string one_sided_address_emoji = 6;
}
```

#### GetPaymentIdAddress
**Purpose**: Returns addresses for a specific payment ID.

**Request**: `GetPaymentIdAddressRequest`
```protobuf
message GetPaymentIdAddressRequest {
    bytes payment_id = 1;  // Payment identifier
}
```

**Response**: `GetCompleteAddressResponse`

### Transaction Methods

#### Transfer
**Purpose**: Creates and sends transactions (interactive, one-sided, stealth).

**Request**: `TransferRequest`
```protobuf
message TransferRequest {
    repeated PaymentRecipient recipients = 1;
}

message PaymentRecipient {
    string address = 1;           // Recipient address
    uint64 amount = 2;            // Amount to send
    uint64 fee_per_gram = 3;      // Transaction fee rate
    PaymentType payment_type = 4; // STANDARD, ONE_SIDED, STEALTH
    bytes payment_id = 5;         // Optional payment identifier
}
```

**Response**: `TransferResponse`
```protobuf
message TransferResponse {
    repeated TransferResult results = 1;
}

message TransferResult {
    string address = 1;        // Recipient address
    uint64 transaction_id = 2; // Created transaction ID
    bool is_success = 3;       // Success status
    string failure_message = 4; // Error message if failed
}
```

#### GetTransactionInfo
**Purpose**: Returns detailed transaction information by ID.

**Request**: `GetTransactionInfoRequest`
```protobuf
message GetTransactionInfoRequest {
    repeated uint64 transaction_ids = 1;  // Transaction IDs to query
}
```

**Response**: `GetTransactionInfoResponse`
```protobuf
message GetTransactionInfoResponse {
    repeated TransactionInfo transactions = 1;
}

message TransactionInfo {
    uint64 tx_id = 1;
    bytes source_address = 2;
    bytes dest_address = 3;
    TransactionStatus status = 4;          // PENDING, COMPLETED, CANCELLED, etc.
    TransactionDirection direction = 5;     // INBOUND, OUTBOUND
    uint64 amount = 6;
    uint64 fee = 7;
    bool is_cancelled = 8;
    bytes excess_sig = 9;
    uint64 timestamp = 10;
    bytes payment_id = 11;
    uint64 mined_in_block_height = 12;
}
```

#### GetCompletedTransactions
**Purpose**: Streams completed transactions, optionally filtered by payment ID.

**Request**: `GetCompletedTransactionsRequest`
```protobuf
message GetCompletedTransactionsRequest {
    UserPaymentId payment_id = 1;  // Optional filter
    string block_hash = 2;         // Optional block filter
    uint64 block_height = 3;       // Optional height filter
}

message UserPaymentId {
    oneof id {
        bytes u256 = 1;           // 32-byte hex ID
        string utf8_string = 2;   // UTF-8 string ID
        bytes user_bytes = 3;     // Raw bytes ID
    }
}
```

**Response**: `stream GetCompletedTransactionsResponse`

#### GetBlockHeightTransactions
**Purpose**: Returns all transactions mined at a specific height.

**Request**: `GetBlockHeightTransactionsRequest`
**Response**: `GetBlockHeightTransactionsResponse`

#### CancelTransaction
**Purpose**: Cancels a pending transaction.

**Request**: `CancelTransactionRequest`
```protobuf
message CancelTransactionRequest {
    uint64 tx_id = 1;  // Transaction ID to cancel
}
```

**Response**: `CancelTransactionResponse`
```protobuf
message CancelTransactionResponse {
    bool is_success = 1;       // Cancellation success
    string failure_message = 2; // Error message if failed
}
```

### Payment Reference Methods

#### GetTransactionPayRefs
**Purpose**: Returns PayRefs for a specific transaction.

**Request**: `GetTransactionPayRefsRequest`
**Response**: `GetTransactionPayRefsResponse`
```protobuf
message GetTransactionPayRefsResponse {
    repeated bytes payment_references = 1;  // PayRef hashes
}
```


### Balance Methods

#### GetBalance
**Purpose**: Returns wallet balance, optionally filtered by payment ID.

**Request**: `GetBalanceRequest`
```protobuf
message GetBalanceRequest {
    UserPaymentId payment_id = 1;  // Optional filter
}
```

**Response**: `GetBalanceResponse`
```protobuf
message GetBalanceResponse {
    uint64 available_balance = 1;        // Spendable balance
    uint64 pending_incoming_balance = 2; // Incoming pending
    uint64 pending_outgoing_balance = 3; // Outgoing pending
    uint64 timelocked_balance = 4;       // Time-locked funds
}
```

#### GetUnspentAmounts
**Purpose**: Returns total unspent output value.

**Request**: `Empty`
**Response**: `GetUnspentAmountsResponse`

### Advanced Transaction Methods

#### CoinSplit
**Purpose**: Splits funds into multiple smaller outputs.

**Request**: `CoinSplitRequest`
```protobuf
message CoinSplitRequest {
    uint64 amount_per_split = 1;  // Value per output
    uint64 split_count = 2;       // Number of outputs
    uint64 fee_per_gram = 3;      // Transaction fee
    uint64 lock_height = 4;       // Optional lock height
    bytes payment_id = 5;         // Optional payment ID
}
```

**Response**: `CoinSplitResponse`
```protobuf
message CoinSplitResponse {
    uint64 tx_id = 1;  // Created transaction ID
}
```

#### ImportUtxos
**Purpose**: Imports UTXOs into wallet as spendable outputs.

**Request**: `ImportUtxosRequest`
**Response**: `ImportUtxosResponse`

### Atomic Swap Methods

#### SendShaAtomicSwapTransaction
**Purpose**: Initiates SHA-based atomic swap.

**Request**: `SendShaAtomicSwapRequest`
**Response**: `SendShaAtomicSwapResponse`
```protobuf
message SendShaAtomicSwapResponse {
    uint64 transaction_id = 1;
    bytes pre_image = 2;        // SHA pre-image
    bytes output_hash = 3;      // Output hash
    bool is_success = 4;
    string failure_message = 5;
}
```

#### ClaimShaAtomicSwapTransaction
**Purpose**: Claims funds from atomic swap using pre-image.

**Request**: `ClaimShaAtomicSwapRequest`
**Response**: `ClaimShaAtomicSwapResponse`

### Burn Methods

#### CreateBurnTransaction
**Purpose**: Creates transaction to permanently destroy Tari.

**Request**: `CreateBurnTransactionRequest`
**Response**: `CreateBurnTransactionResponse`

### Network Methods

#### GetNetworkStatus
**Purpose**: Returns wallet network connectivity status.

**Request**: `Empty`
**Response**: `NetworkStatusResponse`
```protobuf
message NetworkStatusResponse {
    ConnectivityStatus status = 1;  // ONLINE, DEGRADED, OFFLINE
    uint64 avg_latency_ms = 2;      // Average latency
    uint64 num_node_connections = 3; // Active connections
}
```

#### ListConnectedPeers
**Purpose**: Lists currently connected peers.

**Request**: `Empty`
**Response**: `ListConnectedPeersResponse`

### Validation Methods

#### RevalidateAllTransactions
**Purpose**: Triggers complete revalidation of all wallet outputs.

**Request**: `RevalidateRequest`
**Response**: `RevalidateResponse`

#### ValidateAllTransactions
**Purpose**: Validates all wallet outputs.

**Request**: `ValidateRequest`
**Response**: `ValidateResponse`

---

## ValidatorNode Service

**Service Definition**: `tari.rpc.ValidatorNode`
**Hosted by**: Validator Node applications
**Proto File**: `applications/minotari_app_grpc/proto/validator_node.proto`

The ValidatorNode service provides smart contract and sidechain functionality.

### Identity Methods

#### GetIdentity
**Purpose**: Returns validator node identity.

**Request**: `GetIdentityRequest`
**Response**: `GetIdentityResponse`

#### GetMetadata
**Purpose**: Returns sidechain metadata.

**Request**: `GetMetadataRequest`
**Response**: `GetMetadataResponse`
```protobuf
message GetMetadataResponse {
    repeated SidechainMetadata sidechains = 1;
}

message SidechainMetadata {
    bytes asset_public_key = 1;
    uint64 committed_height = 2;
    bytes committed_hash = 3;
}
```

### Smart Contract Methods

#### InvokeReadMethod
**Purpose**: Invokes read-only smart contract method.

**Request**: `InvokeReadMethodRequest`
```protobuf
message InvokeReadMethodRequest {
    bytes contract_id = 1;    // Contract identifier
    uint32 template_id = 2;   // Template ID
    string method = 3;        // Method name
    bytes args = 4;           // Method arguments
    bytes sender = 5;         // Sender address
}
```

**Response**: `InvokeReadMethodResponse`
```protobuf
message InvokeReadMethodResponse {
    bytes result = 1;       // Method result
    Authority authority = 2; // Execution authority
}
```

#### InvokeMethod
**Purpose**: Invokes state-changing smart contract method.

**Request**: `InvokeMethodRequest`
**Response**: `InvokeMethodResponse`

### Contract Management Methods

#### GetConstitutionRequests
**Purpose**: Streams constitution requests.

**Request**: `GetConstitutionRequestsRequest`
**Response**: `stream TransactionOutput`

#### PublishContractAcceptance
**Purpose**: Publishes contract acceptance.

**Request**: `PublishContractAcceptanceRequest`
**Response**: `PublishContractAcceptanceResponse`

#### PublishContractUpdateProposalAcceptance
**Purpose**: Publishes contract update proposal acceptance.

**Request**: `PublishContractUpdateProposalAcceptanceRequest`
**Response**: `PublishContractUpdateProposalAcceptanceResponse`

### Token Methods

#### GetTokenData
**Purpose**: Retrieves token data.

**Request**: `GetTokenDataRequest`
**Response**: `GetTokenDataResponse`

---

## ShaP2Pool Service

**Service Definition**: `tari.rpc.ShaP2Pool`
**Hosted by**: minotari_node (when P2Pool is enabled)
**Proto File**: `applications/minotari_app_grpc/proto/p2pool.proto`

The ShaP2Pool service provides P2Pool mining functionality.

### Pool Information Methods

#### GetTipInfo
**Purpose**: Returns P2Pool tip information.

**Request**: `GetTipInfoRequest`
**Response**: `GetTipInfoResponse`
```protobuf
message GetTipInfoResponse {
    uint64 node_height = 1;          // Base node height
    bytes node_tip_hash = 2;         // Base node tip hash
    uint64 p2pool_rx_height = 3;     // P2Pool RandomX height
    bytes p2pool_rx_tip_hash = 4;    // P2Pool RandomX tip hash
    uint64 p2pool_sha_height = 5;    // P2Pool SHA height
    bytes p2pool_sha_tip_hash = 6;   // P2Pool SHA tip hash
}
```

### Mining Methods

#### GetNewBlock
**Purpose**: Gets new P2Pool block template.

**Request**: `GetNewBlockRequest`
```protobuf
message GetNewBlockRequest {
    PowAlgo pow = 1;                    // PoW algorithm
    string coinbase_extra = 2;          // Extra coinbase data
    string wallet_payment_address = 3;  // Payment address
}
```

**Response**: `GetNewBlockResponse`
```protobuf
message GetNewBlockResponse {
    GetNewBlockResult block = 1;   // Block template
    uint64 target_difficulty = 2;  // Target difficulty
}
```

#### SubmitBlock
**Purpose**: Submits mined P2Pool block.

**Request**: `SubmitBlockRequest`
**Response**: `SubmitBlockResponse`

---

## Common Data Types

### Core Types

#### Block Structure
```protobuf
message Block {
    BlockHeader header = 1;    // Block metadata
    AggregateBody body = 2;    // Transactions data
}

message BlockHeader {
    bytes hash = 1;
    uint32 version = 2;
    uint64 height = 3;
    bytes prev_hash = 4;
    uint64 timestamp = 5;
    bytes output_mr = 6;       // UTXO merkle root
    bytes kernel_mr = 8;       // Kernel merkle root
    uint64 nonce = 11;
    ProofOfWork pow = 12;
}
```

#### Transaction Structure
```protobuf
message Transaction {
    bytes offset = 1;          // Kernel offset
    AggregateBody body = 2;    // Transaction components
    bytes script_offset = 3;   // Script offset
}

message AggregateBody {
    repeated TransactionInput inputs = 1;
    repeated TransactionOutput outputs = 2;
    repeated TransactionKernel kernels = 3;
}
```

#### UTXO Structure
```protobuf
message TransactionOutput {
    OutputFeatures features = 1;           // Output metadata
    bytes commitment = 2;                  // Pedersen commitment
    RangeProof range_proof = 3;           // Value range proof
    bytes hash = 4;                       // Output hash
    bytes script = 5;                     // Tari script
    bytes sender_offset_public_key = 6;   // Sender offset key
    ComAndPubSignature metadata_signature = 7;
    bytes covenant = 8;                   // Output covenant
    uint32 version = 9;
    bytes encrypted_data = 10;            // Encrypted value/mask
    uint64 minimum_value_promise = 11;    // Minimum proven value
    bytes payment_reference = 12;         // PayRef for tracking
}
```

### Signature Types
```protobuf
message Signature {
    bytes public_nonce = 1;
    bytes signature = 2;
}

message ComAndPubSignature {
    bytes ephemeral_commitment = 1;
    bytes ephemeral_pubkey = 2;
    bytes u_a = 3;
    bytes u_x = 4;
    bytes u_y = 5;
}
```

### Proof of Work
```protobuf
message ProofOfWork {
    uint64 pow_algo = 1;    // 0=Monero, 1=Sha3X, 2=RandomXT
    bytes pow_data = 4;     // Algorithm-specific data
}

enum PowAlgos {
    POW_ALGOS_RANDOMXM = 0;  // Monero RandomX
    POW_ALGOS_SHA3X = 1;     // SHA3X
    POW_ALGOS_RANDOMXT = 2;  // Tari RandomX
}
```

### Status Enumerations

#### Transaction Status
```protobuf
enum TransactionStatus {
    TRANSACTION_STATUS_PENDING = 0;
    TRANSACTION_STATUS_COMPLETED = 1;
    TRANSACTION_STATUS_BROADCAST = 2;
    TRANSACTION_STATUS_MINED_UNCONFIRMED = 3;
    TRANSACTION_STATUS_IMPORTED = 4;
    TRANSACTION_STATUS_MINED_CONFIRMED = 5;
    TRANSACTION_STATUS_CANCELLED = 6;
    TRANSACTION_STATUS_COINBASE = 7;
    TRANSACTION_STATUS_REJECTED = 8;
}
```

#### Network Connectivity
```protobuf
enum ConnectivityStatus {
    ONLINE = 0;     // Fully connected
    DEGRADED = 1;   // Limited connectivity
    OFFLINE = 2;    // No connectivity
}
```

#### Base Node State
```protobuf
enum BaseNodeState {
    START_UP = 0;
    HEADER_SYNC = 1;
    HORIZON_SYNC = 2;
    CONNECTING = 3;
    BLOCK_SYNC = 4;
    LISTENING = 5;
    SYNC_FAILED = 6;
}
```

---

## Client Implementation Guidelines

### Connection Management

#### TLS Configuration
```javascript
// Node.js gRPC client with TLS
const grpc = require('@grpc/grpc-js');
const credentials = grpc.credentials.createSsl();
const client = new BaseNodeClient('node.example.com:18142', credentials);
```

#### Insecure Connection (Development)
```javascript
const credentials = grpc.credentials.createInsecure();
const client = new BaseNodeClient('localhost:18142', credentials);
```

### Error Handling

#### Standard gRPC Errors
- `UNAVAILABLE` - Service not available
- `UNAUTHENTICATED` - Authentication required
- `PERMISSION_DENIED` - Insufficient permissions
- `INVALID_ARGUMENT` - Invalid request parameters
- `NOT_FOUND` - Resource not found
- `ALREADY_EXISTS` - Resource already exists
- `RESOURCE_EXHAUSTED` - Rate limit exceeded

#### Tari-Specific Errors
- Transaction validation failures
- Insufficient funds
- Network synchronization issues
- Contract execution errors

### Streaming Patterns

#### Server Streaming
```javascript
const stream = client.listHeaders(request);
stream.on('data', (response) => {
    // Process each header
});
stream.on('error', (error) => {
    // Handle stream errors
});
stream.on('end', () => {
    // Stream completed
});
```

### Authentication

#### Method Allowlisting
Configure allowed methods in base node:
```toml
[base_node]
grpc_server_allow_methods = [
    "get_tip_info",
    "get_constants", 
    "get_mempool_stats"
]
```

### Performance Considerations

#### Pagination
Use pagination for large result sets:
```javascript
const request = {
    from_height: startHeight,
    num_headers: 100  // Reasonable batch size
};
```

#### Streaming vs Unary
- Use streaming for large datasets
- Use unary calls for single items
- Consider memory constraints with large streams

#### Connection Pooling
Reuse gRPC connections when possible to avoid connection overhead.

---

## Integration Examples

### Block Explorer Integration
```javascript
// Get recent blocks with full transaction data
const heights = Array.from({length: 10}, (_, i) => tipHeight - i);
const blocksStream = client.getBlocks({heights});

blocksStream.on('data', (historicalBlock) => {
    const block = historicalBlock.block;
    console.log(`Block ${block.header.height}: ${block.body.outputs.length} outputs`);
});
```

### Mining Pool Integration
```javascript
// Get mining template
const template = await client.getNewBlockTemplate({
    algo: {pow_algo: 1}, // SHA3X
    max_weight: 21000
});

// Submit completed block
const result = await client.submitBlock(completedBlock);
console.log(`Block submitted: ${Buffer.from(result.block_hash).toString('hex')}`);
```

### Wallet Integration
```javascript
// Send transaction
const transfer = await walletClient.transfer({
    recipients: [{
        address: "recipient_address",
        amount: 1000000, // 1 T
        fee_per_gram: 25,
        payment_type: 0, // STANDARD
        payment_id: Buffer.from("invoice_123", "utf-8")
    }]
});

console.log(`Transaction created: ${transfer.results[0].transaction_id}`);
```

### Payment Verification
```javascript
// Search for payment by PayRef
const payRefStream = client.searchPaymentReferences({
    payment_reference_hex: ["1234567890abcdef..."],
    include_spent: false
});

payRefStream.on('data', (response) => {
    if (response.block_height > 0) {
        console.log(`Payment confirmed at height ${response.block_height}`);
    }
});
```

This comprehensive overview provides all the essential information for integrating with Tari's gRPC APIs, including detailed method descriptions, message schemas, usage examples, and best practices for client implementation.
