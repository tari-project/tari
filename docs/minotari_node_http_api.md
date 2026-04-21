# Minotari Node HTTP API Reference

The Minotari base node exposes an HTTP REST API built on [Axum](https://github.com/tokio-rs/axum). All endpoints return JSON responses and include configurable `Cache-Control` headers.

## Server Configuration

| Setting | Description |
|---------|-------------|
| **Listen address** | `0.0.0.0` (configurable) |
| **Authentication** | None (relies on network/firewall security) |
| **Body size limit** | ~10 MB default (`25 * 4 * 1024 * 1024 / 10` bytes) |
| **Swagger UI** | Available at `/swagger-ui` |
| **OpenAPI spec** | Available at `/openapi.json` |

### Default Ports by Network

| Network | Port |
|---------|------|
| MainNet | 9000 |
| StageNet | 9001 |
| NextNet | 9002 |
| LocalNet | 9003 |
| Igor | 9004 |
| Esmeralda | 9005 |

## Error Response Format

All endpoints return errors in the following JSON format:

```json
{
  "error": "Human-readable error description"
}
```

Common HTTP status codes:
- `200` - Success
- `400` - Bad request / invalid parameters
- `404` - Resource not found
- `500` - Internal server error

---

## Endpoints

### GET `/get_tip_info`

Returns the current chain tip information including metadata and sync status.

**Parameters:** None

**Example Request:**

```bash
curl http://localhost:9000/get_tip_info
```

**Example Response:**

```json
{
  "metadata": {
    "best_block_height": 123456,
    "best_block_hash": "abcdef0123456789...",
    "pruning_horizon": 0,
    "pruned_height": 0,
    "accumulated_difficulty": "123456789",
    "timestamp": 1700000000
  },
  "is_synced": true
}
```

**Cache-Control:** `public, max-age=15, s-maxage=15, stale-while-revalidate=15`

---

### GET `/get_header_by_height`

Fetch a block header at a specific height.

**Query Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `height` | `u64` | Yes | The block height to retrieve the header for |

**Example Request:**

```bash
curl "http://localhost:9000/get_header_by_height?height=1000"
```

**Example Response:**

```json
{
  "version": 1,
  "height": 1000,
  "prev_hash": "abcdef...",
  "timestamp": 1700000000,
  "output_mr": "abcdef...",
  "kernel_mr": "abcdef...",
  "input_mr": "abcdef...",
  "total_kernel_offset": "abcdef...",
  "nonce": 12345678,
  "pow": { ... },
  "kernel_mmr_size": 1000,
  "output_mmr_size": 2000,
  "total_script_offset": "abcdef...",
  "hash": "abcdef...",
  "validator_node_mr": "abcdef..."
}
```

**Cache-Control:** Dynamic based on distance from chain tip (see [Dynamic Caching](#dynamic-cache-control)).

**Error Responses:**
- `404` - Header not found at the specified height

---

### GET `/get_height_at_time`

Find the block height at a specific Unix timestamp.

**Query Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `time` | `u64` | Yes | Unix timestamp (seconds since epoch) |

**Example Request:**

```bash
curl "http://localhost:9000/get_height_at_time?time=1700000000"
```

**Example Response:**

```json
12345
```

**Cache-Control:** `public, max-age=60, s-maxage=30, stale-while-revalidate=15`

**Error Responses:**
- `404` - No header found at the specified time
- `500` - Failed to get chain metadata

---

### GET `/get_utxos_mined_info`

Get mining information for one or more UTXOs.

**Query Parameters:**

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `hashes` | `string` | Yes | - | Comma-separated hex-encoded UTXO hashes |
| `version` | `u32` | No | `1` | Request version. Version 2 also checks the mempool for unmined outputs |

**Example Request:**

```bash
curl "http://localhost:9000/get_utxos_mined_info?hashes=abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789,fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210&version=1"
```

**Example Response:**

```json
{
  "utxo_infos": [
    {
      "output_hash": "abcdef...",
      "mined_height": 1000,
      "header_hash": "abcdef...",
      "mined_timestamp": 1700000000
    }
  ]
}
```

**Cache-Control:** `public, max-age=60, s-maxage=30, stale-while-revalidate=15`

---

### GET `/fetch_utxo`

Fetch information about a specific UTXO by its output hash.

**Query Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `utxo` | `string` | Yes | Hex-encoded output hash |

**Example Request:**

```bash
curl "http://localhost:9000/fetch_utxo?utxo=abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
```

**Example Response:**

```json
{
  "output": { ... },
  "mined_at_height": 1000,
  "mined_block_hash": "abcdef...",
  "mined_timestamp": 1700000000
}
```

**Cache-Control:** `public, max-age=3600, s-maxage=1800, stale-while-revalidate=60`

**Error Responses:**
- `404` - Output not found

---

### GET `/get_utxos_deleted_info`

Get information about deleted/spent UTXOs.

**Query Parameters:**

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `hashes` | `string` | Yes | - | Comma-separated hex-encoded UTXO hashes |
| `must_include_header` | `string` | Yes | - | Hex-encoded header hash that must be included in the check |
| `version` | `u8` | No | `0` | Response format version. `0` = standard, `1` = includes `spent_timestamp` field |

**Example Request:**

```bash
curl "http://localhost:9000/get_utxos_deleted_info?hashes=abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789&must_include_header=fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210&version=0"
```

**Example Response (v0):**

```json
{
  "utxo_infos": [
    {
      "output_hash": "abcdef...",
      "deleted_height": 1500,
      "header_hash": "abcdef..."
    }
  ]
}
```

**Example Response (v1):**

```json
{
  "utxo_infos": [
    {
      "output_hash": "abcdef...",
      "deleted_height": 1500,
      "header_hash": "abcdef...",
      "spent_timestamp": 1700005000
    }
  ]
}
```

**Cache-Control:** `public, max-age=60, s-maxage=30, stale-while-revalidate=15`

---

### GET `/transactions`

Query a transaction by its excess signature.

**Query Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `excess_sig_nonce` | `string` | Yes | Hex-encoded public nonce of the excess signature |
| `excess_sig_sig` | `string` | Yes | Hex-encoded signature value |

**Example Request:**

```bash
curl "http://localhost:9000/transactions?excess_sig_nonce=abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789&excess_sig_sig=fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
```

**Example Response:**

```json
{
  "transaction": { ... },
  "mined_at_height": 1000,
  "confirmations": 500
}
```

**Cache-Control:** `public, max-age=60, s-maxage=30, stale-while-revalidate=15`

---

### GET `/sync_utxos_by_block`

Paginated sync of UTXOs starting from a specific block header. Useful for wallet synchronisation.

**Query Parameters:**

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `start_header_hash` | `string` | Yes | - | Hex-encoded starting block header hash |
| `limit` | `u64` | Yes | - | Number of UTXOs to return per page |
| `page` | `u64` | Yes | - | Page number (0-indexed) |
| `exclude_spent` | `bool` | No | `false` | Exclude already-spent UTXOs from the response |
| `exclude_inputs` | `bool` | No | `false` | Exclude transaction inputs from the response |
| `version` | `u8` | No | `0` | Response version selector (`0` or `1`) |

**Example Request:**

```bash
curl "http://localhost:9000/sync_utxos_by_block?start_header_hash=1a8da4213566e3cda06958c7ee46b87870a587fabb1c7f050f553b6da36cccb3&limit=5&page=0&exclude_spent=false"
```

**Example Response (v0):**

```json
{
  "blocks": [
    {
      "height": 1000,
      "header_hash": "abcdef...",
      "outputs": [ ... ],
      "inputs": [ ... ]
    }
  ]
}
```

**Cache-Control:** Dynamic based on distance from chain tip (see [Dynamic Caching](#dynamic-cache-control)).

**Error Responses:**
- `404` - Header not found
- `500` - Start/end header hash not found or header height mismatch

---

### GET `/get_utxos_by_block`

Get all UTXOs in a specific block.

**Query Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `header_hash` | `string` | Yes | Hex-encoded block header hash |

**Example Request:**

```bash
curl "http://localhost:9000/get_utxos_by_block?header_hash=1a8da4213566e3cda06958c7ee46b87870a587fabb1c7f050f553b6da36cccb3"
```

**Example Response:**

```json
{
  "height": 1000,
  "header_hash": "abcdef...",
  "outputs": [ ... ]
}
```

**Cache-Control:** Dynamic based on distance from chain tip (see [Dynamic Caching](#dynamic-cache-control)).

**Error Responses:**
- `404` - Header not found

---

### GET `/generate_kernel_merkle_proof`

Generate a Merkle proof for a transaction kernel identified by its excess signature.

**Query Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `excess_sig_public_nonce` | `string` | Yes | Hex-encoded public nonce (32 bytes compressed public key) |
| `excess_sig_signature` | `string` | Yes | Hex-encoded signature |

**Example Request:**

```bash
curl "http://localhost:9000/generate_kernel_merkle_proof?excess_sig_public_nonce=abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789&excess_sig_signature=fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
```

**Example Response:**

```json
{
  "mmr_size": 50000,
  "mmr_path": ["abcdef...", "012345...", "..."],
  "mined_height": 1000
}
```

**Cache-Control:** `public, max-age=120, s-maxage=60, stale-while-revalidate=15`

**Error Responses:**
- `400` - Invalid signature public nonce length or invalid signature
- `404` - Kernel not found

---

### GET `/get_mempool_fee_per_gram_stats`

Get fee-per-gram statistics from the mempool, useful for fee estimation.

**Query Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `count` | `u64` | Yes | Number of fee buckets to return (max 20) |

**Example Request:**

```bash
curl "http://localhost:9000/get_mempool_fee_per_gram_stats?count=5"
```

**Example Response:**

```json
{
  "stats": [
    {
      "order": 0,
      "min_fee_per_gram": 5,
      "avg_fee_per_gram": 25,
      "max_fee_per_gram": 100
    },
    {
      "order": 1,
      "min_fee_per_gram": 10,
      "avg_fee_per_gram": 50,
      "max_fee_per_gram": 200
    }
  ]
}
```

**Error Responses:**
- `400` - `count` must be less than or equal to 20
- `500` - Failed to get mempool stats

---

### POST `/json_rpc`

JSON-RPC 2.0 endpoint. Currently supports the `submit_transaction` method.

**Content-Type:** `application/json`

#### Method: `submit_transaction`

Submit a signed transaction to the mempool.

**Request Body:**

```json
{
  "jsonrpc": "2.0",
  "id": "1",
  "method": "submit_transaction",
  "params": {
    "transaction": {
      "offset": "...",
      "body": {
        "inputs": [ ... ],
        "outputs": [ ... ],
        "kernels": [ ... ]
      },
      "script_offset": "..."
    }
  }
}
```

**Example Request:**

```bash
curl -X POST http://localhost:9000/json_rpc \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": "1",
    "method": "submit_transaction",
    "params": {
      "transaction": { ... }
    }
  }'
```

**Success Response:**

```json
{
  "result": {
    "accepted": true,
    "rejection_reason": "None",
    "is_synced": true
  },
  "error": null,
  "id": "1"
}
```

**Rejection Response (transaction rejected but request succeeded):**

```json
{
  "result": null,
  "error": "Failed to submit transaction: ...",
  "id": "1"
}
```

**Rejection Reasons:**

| Reason | Description |
|--------|-------------|
| `None` | Transaction was accepted into the mempool |
| `Orphan` | Transaction references unknown inputs |
| `FeeTooLow` | Transaction fee is below the minimum threshold |
| `TimeLocked` | Transaction is time-locked and not yet valid |
| `ValidationFailed` | Transaction failed consensus validation |
| `AlreadyMined` | Transaction has already been mined or spent |

**Error Responses:**
- `400` - Missing `transaction` parameter, invalid JSON, or unknown method

---

## Swagger UI / OpenAPI

The server includes built-in API documentation:

- **Swagger UI:** `GET /swagger-ui` - Interactive API documentation and testing interface
- **OpenAPI Spec:** `GET /openapi.json` - Machine-readable OpenAPI 3.0 specification

```bash
# Open in browser
open http://localhost:9000/swagger-ui

# Download the OpenAPI spec
curl http://localhost:9000/openapi.json
```

---

## Dynamic Cache Control

For endpoints that return block-specific data (`get_header_by_height`, `get_utxos_by_block`, `sync_utxos_by_block`), cache durations scale based on how far the requested block is from the chain tip:

| Distance from Tip | max-age | s-maxage | stale-while-revalidate |
|-------------------|---------|----------|------------------------|
| 0 - 10 blocks | 30s | 30s | 15s |
| 11 - 100 blocks | 300s | 5 min | 60s |
| 101 - 1,000 blocks | 360s | 20 min | 60s |
| 1,001 - 2,000 blocks | 360s | 30 min | 60s |
| 2,001 - 10,000 blocks | 360s | 1 day | 60s |
| 10,001+ blocks | 360s | 30 days | 60s |

The rationale is that older blocks are immutable and can be cached aggressively, while blocks near the tip may be affected by reorgs.

Dynamic caching can be disabled via the `HttpCacheConfig` configuration, in which case the static defaults are used.

---

## Configuration

The HTTP cache behaviour is configurable via the `HttpCacheConfig` section in the node configuration:

```toml
[base_node.http_cache]
enabled = true          # Enable/disable Cache-Control headers entirely
dynamic = true          # Enable/disable dynamic cache scaling based on block depth

# Per-route static Cache-Control defaults (used when dynamic = false, or for non-dynamic routes)
get_tip_info = "public, max-age=15, s-maxage=15, stale-while-revalidate=15"
get_header_by_height = "public, max-age=120, s-maxage=60, stale-while-revalidate=15"
get_utxos_by_block = "public, max-age=3600, s-maxage=1800, stale-while-revalidate=60"
sync_utxos_by_block = "public, max-age=3600, s-maxage=1800, stale-while-revalidate=60"
get_height_at_time = "public, max-age=60, s-maxage=30, stale-while-revalidate=15"
transaction_query = "public, max-age=60, s-maxage=30, stale-while-revalidate=15"
get_utxos_deleted_info = "public, max-age=60, s-maxage=30, stale-while-revalidate=15"
get_utxos_mined_info = "public, max-age=60, s-maxage=30, stale-while-revalidate=15"
```
