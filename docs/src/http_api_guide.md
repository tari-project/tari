# Minotari HTTP API Documentation

This document describes the available **HTTP endpoints** exposed by the Minotari Base Node. These are primarily REST-like GET routes, along with one JSON-RPC endpoint for structured method invocation.

- [Base URL](#base-url)
- [Endpoints](#endpoints)
  - [get_tip_info](#get_tip_info)
  - [get_header_by_height](#get_header_by_height)
  - [get_height_at_time](#get_height_at_time)
  - [get_utxos_mined_info](#get_utxos_mined_info)
  - [get_utxos_deleted_info](#get_utxos_deleted_info)
  - [transactions](#transactions)
  - [sync_utxos_by_block](#sync_utxos_by_block)
  - [get_utxos_by_block](#get_utxos_by_block)
- [Testing Endpoints with curl](#testing-endpoints-with-curl)
- [Authentication](#authentication)
- [Error Handling](#error-handling)

## Base URL

```
http://<base-node-host>:<port>
```

Default port (for example): `9000`

## Endpoints

### get_tip_info

**Method & Path**

`GET /get_tip_info`

**Description**:

Returns information about the best known current tip of the blockchain. Note that this can deviate from the actual best tip.

**Response**

| Field                  | Type          | Description                                                                                                           |
|------------------------|---------------|-----------------------------------------------------------------------------------------------------------------------|
| `best_block_height`    | `u64`         | The current chain height, or the block number of the longest valid chain.                                             |
| `best_block_hash`      | `BlockHash`   | The block hash of the current tip of the longest valid chain.                                                         |
| `pruning_horizon`      | `u64`         | The configured number of blocks back from the tip that this database tracks. `0` means pruning mode is disabled.      |
| `pruned_height`        | `u64`         | The height of the pruning horizon; blocks below this height may be pruned. Archival nodes have this as zero.          |
| `accumulated_difficulty` | `U512`      | The total accumulated proof of work (PoW) of the longest chain.                                                       |
| `timestamp`            | `u64`         | Timestamp of the tip block in the longest valid chain (Unix epoch).                                                   |

**Example**:

```json
{"metadata":
{"best_block_height":0,
"best_block_hash":[60,54,129,173,49,142,181,76,163,243,232,174,215,157,61,32,125,148,165,41,26,236,132,186,26,11,98,134,51,219,54,33],
"pruning_horizon":0,
"pruned_height":0,
"accumulated_difficulty":"0x1",
"timestamp":1746770400},
"is_synced":false}
```

### get_header_by_height

**Method & Path**

`GET /get_header_by_height?height={u64}`

**Query Parameters**:

| Name   | Type | Description           |
| ------ | ---- | --------------------- |
| height | u64  | Block height to fetch |

**Description**:
Returns the block header at the specified height.

**Response**:
Returns a full block header object, or `null` if not found.

| Field                 | Type          | Description                                                                      |
| --------------------- | ------------- | -------------------------------------------------------------------------------- |
| `hash`                | `BlockHash`   | Hash of this block header (usually SHA3 or Blake2 hash of the serialized header) |
| `version`             | `u16`         | Version of the block format (for protocol upgrades)                              |
| `height`              | `u64`         | Height of this block in the chain, starting at 0 for genesis                     |
| `prev_hash`           | `BlockHash`   | Hash of the previous block header in the chain in byte array format; must be hex-encoded for reuse                                  |
| `timestamp`           | `EpochTime`   | Time at which the block was built (Unix epoch in seconds)                        |
| `input_mr`            | `FixedHash`   | Merkle root of all inputs in the block in byte array format; must be hex-encoded for reuse.                                          |
| `output_mr`           | `FixedHash`   | Merkle root of all outputs on the blockchain at this block in byte array format; must be hex-encoded for reuse                      |
| `block_output_mr`     | `FixedHash`   | Combined output MMR root for block verification in byte array format; must be hex-encoded for reuse                                 |
| `output_smt_size`     | `u64`         | Size (number of leaves) of the output and range proof MMRs at this height        |
| `kernel_mr`           | `FixedHash`   | Merkle root of all transaction kernels in this block in byte array format; must be hex-encoded for reuse                            |
| `kernel_mmr_size`     | `u64`         | Number of leaves in the kernel MMR                                               |
| `total_kernel_offset` | `PrivateKey`  | Aggregate kernel offset — hides transaction blinding factors                     |
| `total_script_offset` | `PrivateKey`  | Aggregate script offset — used for script-based transactions                     |
| `validator_node_mr`   | `FixedHash`   | Merkle root of all active validator node identities in byte array format; must be hex-encoded for reuse                             |
| `validator_node_size` | `u64`         | Number of validator nodes at this block height                                   |
| `pow`                 | `ProofOfWork` | Summary of the proof-of-work used to mine this block                             |
| `nonce`               | `u64`         | Nonce used in mining, incremented until PoW target is met                        |


**Example**
```json
{"hash":[156,13,136,68,153,25,199,189,21,13,2,40,38,145,37,216,39,253,104,64,18,119,44,207,69,164,177,239,122,130,21,203],
"version":0,
"height":2256,
"prev_hash":[106,117,246,55,89,229,32,150,180,238,48,239,57,122,34,235,142,254,192,191,101,9,17,138,49,230,91,30,27,1,206,190],
"timestamp":1746754741,
"input_mr":[33,44,230,245,247,252,103,220,183,59,42,138,122,17,64,71,3,172,162,16,167,199,93,233,229,13,145,76,159,153,66,194],
"output_mr":[135,220,7,117,186,108,160,33,18,31,38,142,70,141,127,167,117,199,197,41,88,248,167,149,179,162,103,237,203,14,239,117],
"block_output_mr":[36,147,98,237,16,225,80,210,85,62,63,38,190,90,71,28,160,18,8,43,74,183,174,10,41,112,55,123,144,41,231,89],
"output_smt_size":300580,
"kernel_mr":[245,133,29,224,142,136,199,220,80,62,34,107,62,203,109,123,232,159,43,28,155,7,57,8,231,143,41,5,55,254,153,54],
"kernel_mmr_size":3167,
"total_kernel_offset":"0000000000000000000000000000000000000000000000000000000000000000",
"total_script_offset":"0000000000000000000000000000000000000000000000000000000000000000",
"validator_node_mr":[39,125,166,92,64,178,207,153,219,134,186,237,185,3,163,240,163,133,64,243,169,77,64,200,38,238,202,199,226,125,93,252],
"validator_node_size":0,
"pow":{
  "pow_algo":"RandomXM","pow_data":"1010b5b9f5c006ce97714d4b29e5164e7416272402d0bfa2b80af59f75c1a63041bfb0a406968150070d5c20ff7e8b8d05e491779e8fa22dde4ab0473d4049af9a9e2130d44afa08eddf866c2700e76bb20dcb47b2434cd97098ea0c13342f5c3edf804ca95ab1dbebfb56f7768b051f06456bf8716c8fe02ae5613670ae01c7760470c62375cfea6c21786a6d0e507614bc8f816ba03f880007fef06aed659f81c9151dccb98777c6855a77ad84068bc7b4f6d08753c6178ed0cad5c9ccbc4adf325b8a8d11b699976e9bfe5c2008f1e43e2727e20c4ca0b9814033a593861675f826815d73928a6540162279a3a34b226438669efb1ecdbc9874eeaf4e2c14f421a1e231816850281f5cf98df6620002d0fdcf0101ff94fdcf0101c0f3cc9bd7110327df94c4f5e1cc541f3c5c87a7a5f0c725c48bd7b554ae65342b90390eea9529c800000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000348801570000000321005a3b59ac2b59fe02f3bfa2b9db30a33f7880ac4521f9108512544e645a6aca4e0000004a771b6b99282c25d8c15bc07e5a6894c1a6f52175a6eec028b8fd9e3716021100000000000000000000000000000000000000"},
"nonce":0}
```

### get_height_at_time

**Method & Path**
`GET /get_height_at_time?time={u64}`

**Query Parameters**:

| Name | Type | Description           |
| ---- | ---- | --------------------- |
| time | u64  | Epoch time in seconds |

**Description**:
Returns the blockchain height at or just before the specified epoch time.

**Response**:

```bash
123456
```

### get_utxos_mined_info

**Method & Path**
`GET /get_utxos_mined_info?hashes={comma-separated-hashes}`

**Query Parameters**:

| Name   | Type   | Description                         |
| ------ | ------ | ----------------------------------- |
| hashes | string | Comma-separated hex hashes of UTXOs |

**Description**:
Returns mined info (e.g., block height, inclusion) for the specified UTXO hashes in an array.

**Response**

| Name                          | Type       | Description                                                                |
| ----------------------------- | ---------- | -------------------------------------------------------------------------- |
| utxos                         | array      | List of mined UTXO information                                             |
| utxos[].utxo_hash           | `[u8; 32]` | The hash of the UTXO (in byte array format; must be hex-encoded for reuse) |
| utxos[].mined_in_hash      | `[u8; 32]` | Hash of the block that mined this UTXO                                     |
| utxos[].mined_in_height    | `u64`      | Block height where UTXO was mined                                          |
| utxos[].mined_in_timestamp | `u64`      | UNIX timestamp of the block that mined the UTXO                            |
| best_block_hash             | `[u8; 32]` | Latest known block hash at the time of query                               |
| best_block_height           | `u64`      | Height of the latest known block                                           |

**Example**:
```json
{"utxos":[
  {"utxo_hash":[135,107,141,62,54,23,60,219,116,228,47,34,142,152,228,24,189,9,225,165,92,104,32,126,152,149,182,108,41,74,247,141],
  "mined_in_hash":[58,190,109,239,240,193,12,217,0,161,73,253,90,235,91,196,252,17,5,86,1,181,3,85,183,82,109,119,69,226,161,133],
  "mined_in_height":2,
  "mined_in_timestamp":1746512785}],
  "best_block_hash":[167,181,212,20,12,52,128,176,131,51,75,72,122,232,40,30,77,65,254,187,8,174,110,76,154,185,125,215,158,149,4,122],
  "best_block_height":19360
  }
```

### get_utxos_deleted_info

**Method & Path**

`GET /get_utxos_deleted_info?hashes={comma-separated-hashes}&must_include_header={hash}`

**Query Parameters**:

| Name                  | Type   | Description                                   |
| --------------------- | ------ | --------------------------------------------- |
| hashes                | string | Comma-separated hex hashes of deleted UTXOs   |
| must_include_header | string | Hex hash of a header that must include the result |

**Description**:
Returns information about deleted UTXOs and whether they are present up to a certain block header.

**Response**

| Name                      | Type                            | Description                                                                                            |
| ------------------------- | ------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `utxos`                   | Array  | A list of UTXOs that have been deleted (spent or removed).                                             |
| `utxos[].utxo_hash`       | Byte array (`Vec<u8>`)          | The unique identifier hash of the UTXO in byte array format; must be hex-encoded for reuse.                                                                |
| `utxos[].found_in_header` | Optional tuple `(u64, Vec<u8>)` | The block height and block hash where the UTXO was originally found (mined). May be absent if unknown. |
| `utxos[].spent_in_header` | Optional tuple `(u64, Vec<u8>)` | The block height and block hash where the UTXO was spent or deleted. May be absent if unknown.         |
| `best_block_hash`         | Byte array (`Vec<u8>`)          | The hash of the latest (best) block known at the time of the query.                                    |
| `best_block_height`       | `u64`                           | The height (number) of the latest known block at the time of the query.                                |

**Example**
```bash
curl "http://127.0.0.1:9000/get_utxos_deleted_info?hashes=2e56f3f2f06bb5bc3b08625106f757c637852d8dd793ffebb2a5409737c29823,92092e9772cce24df684a7f2bef2e49e99b33305d7af89f8ccb90f9d7203a6da&must_include_header=a7b5d4140c3480b083334b487ae8281e4d41febb08ae6e4c9ab97dd79e95047a"
```

```json
{"utxos":[
  {"utxo_hash":[46,86,243,242,240,107,181,188,59,8,98,81,6,247,87,198,55,133,45,141,215,147,255,235,178,165,64,151,55,194,152,35],
  "found_in_header":[24017,[146,9,46,151,114,204,226,77,246,132,167,242,190,242,228,158,153,179,51,5,215,175,137,248,204,185,15,157,114,3,166,218]],
  "spent_in_header":[24562,[78,254,63,29,110,145,74,50,244,193,126,1,199,191,16,185,36,70,250,28,237,229,120,89,213,73,106,204,181,143,242,38]]},
  {"utxo_hash":[146,9,46,151,114,204,226,77,246,132,167,242,190,242,228,158,153,179,51,5,215,175,137,248,204,185,15,157,114,3,166,218],
  "found_in_header":null,
  "spent_in_header":null}
  ],
  "best_block_hash":[39,91,214,150,175,93,166,124,73,9,33,188,198,9,175,168,219,233,231,102,185,163,183,153,201,111,51,139,12,3,23,19],
  "best_block_height":27316
  }
```

### transactions

**Method & Path**
`GET /transactions?excess_sig_nonce={nonce}&excess_sig_sig={sig}`

**Query Parameters**:

| Name               | Type   | Description            |
| ------------------ | ------ | ---------------------- |
| excess_sig_nonce | string | Signature nonce in hex |
| excess_sig_sig   | string | Signature value in hex |

**Description**:
Query for a transaction in the mempool by its excess signature.

### sync_utxos_by_block

**Method & Path**

`GET /sync_utxos_by_block?start_header_hash={hash}&limit={int}&page={int}`

**Query Parameters**:

| Name                | Type   | Description                      |
| ------------------- | ------ | -------------------------------- |
| start_header_hash | string | Starting block header hash (hex) |
| limit               | int    | Max UTXOs to return per page     |
| page                | int    | Page number (0-based index)      |

**Description**:
Fetch paginated UTXOs mined in blocks, beginning from the specified block header hash. Enables efficient synchronization of UTXO data in manageable chunks, ideal for wallets or services needing incremental blockchain state updates.

**Response**

| Name                                          | Type                   | Description                                                            |
| --------------------------------------------- | ---------------------- | ---------------------------------------------------------------------- |
| `blocks`                                      | Array                  | List of blocks with UTXO data included in the response.                |
| `blocks[].header_hash`                        | Byte array (`Vec<u8>`) | The hash of the block header.                                          |
| `blocks[].height`                             | `u64`                  | The height (position) of the block in the blockchain.                  |
| `blocks[].outputs`                            | Array                  | List of UTXOs created (mined) in this block.                           |
| `blocks[].outputs[].output_hash`              | Byte array (`Vec<u8>`) | Unique identifier hash of the UTXO output.                             |
| `blocks[].outputs[].commitment`               | Byte array (`Vec<u8>`) | Commitment associated with the UTXO output.                            |
| `blocks[].outputs[].encrypted_data`           | Byte array (`Vec<u8>`) | Encrypted data related to the UTXO output.                             |
| `blocks[].outputs[].sender_offset_public_key` | Byte array (`Vec<u8>`) | Public key used as an offset by the sender of the UTXO.                |
| `blocks[].inputs`                             | Array of byte arrays   | List of inputs (spent UTXO hashes) in this block.                      |
| `blocks[].mined_timestamp`                    | `u64`                  | Timestamp (Unix epoch) when the block was mined.                       |
| `has_next_page`                               | `bool`                 | Indicates if there are more pages of blocks/UTXOs to fetch.            |
| `next_header_to_scan`                         | Byte array (`Vec<u8>`) | The header hash of the next block to scan for pagination continuation. |

**Example**
```json
{
  "blocks": [
    {
      "header_hash": "<header_hash>",
      "height": 19360,
      "outputs": [
        {
          "output_hash": "<output_hash>",
          "commitment": "<commitment>",
          "encrypted_data": "<encrypted_data>",
          "sender_offset_public_key": "<sender_offset_public_key>"
        }
        // ...other outputs...
      ],
      "inputs": [
        "<input_hash_1>",
        "<input_hash_2>"
        // ...other inputs...
      ],
      "mined_timestamp": 1748793725
    }
  ],
  "has_next_page": true,
  "next_header_to_scan": "<next_header_to_scan>"
}
```

### get_utxos_by_block

**Method & Path**

`GET /get_utxos_by_block?header_hash={hash}`

**Query Parameters**:

| Name         | Type   | Description              |
| ------------ | ------ | ------------------------ |
| header_hash | string | Hash of the block header |

**Description**:
Returns all of the UTXOs included in the block identified by the provided hash.

> Note: This can be a significant number of UTXOs with their accompanying metadata. Please use cautiously.

**Response**
| Name                                 | Type                     | Description                                       |
| ------------------------------------ | ------------------------ | ------------------------------------------------- |
| `header_hash`                        | String (hex)             | Hash of the block header.                         |
| `height`                             | Integer                  | Block height number.                              |
| `outputs`                            | Array                    | List of UTXOs (transaction outputs) in the block. |
| `outputs[].version`                  | String                   | Version of this output structure.                 |
| `outputs[].features`                 | Object                   | Features describing output type, maturity, etc.   |
| `outputs[].commitment`               | String (hex)             | Cryptographic commitment of output value.         |
| `outputs[].proof`                    | String or null           | Range proof for the output’s value (optional).    |
| `outputs[].script`                   | String (hex)             | Spending script associated with output.           |
| `outputs[].sender_offset_public_key` | String (hex)             | Sender’s public key offset.                       |
| `outputs[].metadata_signature`       | Object                   | Signature data for output metadata.               |
| `outputs[].covenant`                 | String (hex)             | Covenant script governing output conditions.      |
| `outputs[].encrypted_data`           | Object                   | Encrypted data related to the output.             |
| `outputs[].minimum_value_promise`    | Integer                  | Minimum value promised by output (if any).        |
| `mined_timestamp`                    | Integer (unix timestamp) | Time the block was mined.                         |

**Example**
```json
{
  "header_hash": "1a8da4213566e3cda06958c7ee46b87870a587fabb1c7f050f553b6da36cccb3",
  "height": 12345,
  "outputs": [
    {
      "version": "1",
      "features": {
        "version": "1",
        "output_type": "Coinbase",
        "maturity": 0,
        "coinbase_extra": "",
        "sidechain_feature": null,
        "range_proof_type": "Bulletproof"
      },
      "commitment": "abcdef1234567890",
      "proof": null,
      "script": "abcd1234",
      "sender_offset_public_key": "deadbeef",
      "metadata_signature": {
        "ephemeral_commitment": "abcd",
        "ephemeral_pubkey": "1234",
        "u_a": "5678",
        "u_x": "9abc",
        "u_y": "def0"
      },
      "covenant": "00",
      "encrypted_data": {
        "data": "encryptedpayload"
      },
      "minimum_value_promise": 0
    }
  ],
  "mined_timestamp": 1694143200
}
```

> Note:
The example JSON shown is simplified for clarity. In a real response, the outputs array can contain multiple UTXO entries, each with detailed information as described. Depending on the block, the number of outputs can vary widely, so expect the JSON to be larger and more complex when querying actual blockchain data.

## Testing Endpoints with `curl`

**Example**:

```bash
curl "http://localhost:9000/get_tip_info"
```

---

## Authentication

As of now, these endpoints do **not** require authentication. Be careful if exposing them on a public network.