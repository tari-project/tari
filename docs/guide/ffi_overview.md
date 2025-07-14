# Tari Foreign Function Interface (FFI) Overview

This document provides a comprehensive overview of all FFI implementations in the Tari project, their purposes, available functions, integration patterns, and usage examples for developers integrating Tari functionality into applications.

## Overview

Tari provides Foreign Function Interface (FFI) libraries that enable integration with applications written in languages other than Rust. These C-compatible libraries expose Tari's core functionality for wallet operations, mining utilities, and blockchain interactions.

### FFI Libraries Available

- **minotari_wallet_ffi** - Comprehensive wallet functionality for mobile and desktop applications
- **minotari_mining_helper_ffi** - Mining utilities for mining pools and stratum servers

### Supported Platforms

- **iOS** - Static library (.a) integration
- **Android** - Dynamic library (.so) integration
- **Linux** - Dynamic library (.so)
- **macOS** - Dynamic library (.dylib)
- **Windows** - Dynamic library (.dll)

### Languages Supported

- **C/C++** - Direct FFI usage
- **JavaScript/Node.js** - Via ffi-napi bindings
- **Swift** - iOS integration
- **Java/Kotlin** - Android integration
- **Python** - Via ctypes or cffi
- **C#/.NET** - Via P/Invoke

---

## Wallet FFI Library (minotari_wallet_ffi)

**Library Name**: `libminotari_wallet_ffi`
**Header File**: `wallet.h`
**Source Location**: `base_layer/wallet_ffi/`
**Primary Use Cases**: Mobile wallets, desktop applications, embedded wallet functionality

### Core Concepts

#### Memory Management
All FFI functions follow C-style memory management:
- Functions that create objects return pointers that must be destroyed
- Each create function has a corresponding destroy function
- Memory leaks will occur if destroy functions are not called
- All string returns must be freed with `string_destroy()`

#### Error Handling
All functions use consistent error handling:
- Error output parameter (`error_out: *mut c_int`)
- Error codes defined in the library
- Zero (0) indicates success
- Non-zero values indicate specific error types

#### Callbacks
The wallet uses callback functions for asynchronous notifications:
- Transaction events (received, confirmed, mined)
- Balance updates
- Connectivity status changes
- Validation completion events

### Core Data Types

#### Fundamental Types
```c
// Basic wallet instance
struct TariWallet;

// Transaction types
struct TariCompletedTransactions;
struct TariPendingInboundTransactions;
struct TariPendingOutboundTransactions;

// Address and key types
struct TariAddress;
struct TariPublicKey;
struct TariPrivateKey;

// Balance information
struct Balance;

// Collection types
struct TariContacts;
struct TariPublicKeys;
struct TariSeedWords;

// Utility types
struct ByteVector;
struct TariVector;
struct TariUtxo;
```

#### Configuration Types
```c
// Transport configuration
struct TransportConfig;
struct P2pConfig;

// Base node state
struct TariBaseNodeState;
```

### Wallet Lifecycle Functions

#### Wallet Creation and Destruction

**wallet_create**
```c
TariWallet* wallet_create(
    CommsConfig* config,                    // Communications configuration
    const char* log_path,                   // Log file path
    unsigned int num_rolling_log_files,     // Number of rolling log files
    unsigned int size_per_log_file_bytes,   // Size per log file
    const char* passphrase,                 // Wallet passphrase (optional)
    const char* seed_words,                 // Seed words for recovery (optional)
    // Callback functions (16 callbacks)
    void (*callback_received_transaction)(TariPendingInboundTransaction*),
    void (*callback_received_transaction_reply)(TariCompletedTransaction*),
    void (*callback_received_finalized_transaction)(TariCompletedTransaction*),
    void (*callback_transaction_broadcast)(TariCompletedTransaction*),
    void (*callback_transaction_mined)(TariCompletedTransaction*),
    void (*callback_transaction_mined_unconfirmed)(TariCompletedTransaction*, uint64_t),
    void (*callback_faux_transaction_confirmed)(TariCompletedTransaction*),
    void (*callback_faux_transaction_unconfirmed)(TariCompletedTransaction*, uint64_t),
    void (*callback_transaction_send_result)(uint64_t, TariTransactionSendStatus*),
    void (*callback_transaction_cancellation)(TariCompletedTransaction*),
    void (*callback_txo_validation_complete)(uint64_t, uint64_t),
    void (*callback_contacts_liveness_data_updated)(TariContactsLivenessData*),
    void (*callback_balance_updated)(Balance*),
    void (*callback_transaction_validation_complete)(uint64_t, uint8_t),
    void (*callback_saf_messages_received)(),
    void (*callback_connectivity_status)(uint64_t),
    bool* recovery_in_progress,             // Output: recovery status
    int* error_out                          // Error output
);
```

**wallet_destroy**
```c
void wallet_destroy(TariWallet* wallet);
```

#### Wallet Configuration

**wallet_set_base_node_peer**
```c
bool wallet_set_base_node_peer(
    TariWallet* wallet,
    unsigned char* public_key,  // Base node public key (32 bytes)
    char* address,              // Base node address
    int* error_out
);
```

**wallet_set_num_confirmations_required**
```c
void wallet_set_num_confirmations_required(
    TariWallet* wallet,
    uint64_t num_confirmations,
    int* error_out
);
```

**wallet_get_num_confirmations_required**
```c
uint64_t wallet_get_num_confirmations_required(
    TariWallet* wallet,
    int* error_out
);
```

### Wallet Information Functions

#### Balance Operations

**wallet_get_available_balance**
```c
uint64_t wallet_get_available_balance(
    TariWallet* wallet,
    int* error_out
);
```

**wallet_get_pending_incoming_balance**
```c
uint64_t wallet_get_pending_incoming_balance(
    TariWallet* wallet,
    int* error_out
);
```

**wallet_get_pending_outgoing_balance**
```c
uint64_t wallet_get_pending_outgoing_balance(
    TariWallet* wallet,
    int* error_out
);
```

#### Identity Functions

**wallet_get_public_key**
```c
TariPublicKey* wallet_get_public_key(
    TariWallet* wallet,
    int* error_out
);
```

**wallet_get_address**
```c
TariAddress* wallet_get_address(
    TariWallet* wallet,
    int* error_out
);
```

**wallet_get_seed_words**
```c
TariSeedWords* wallet_get_seed_words(
    TariWallet* wallet,
    int* error_out
);
```

### Transaction Functions

#### Sending Transactions

**wallet_send_transaction**
```c
uint64_t wallet_send_transaction(
    TariWallet* wallet,
    TariAddress* destination,       // Recipient address
    uint64_t amount,               // Amount in MicroMinotari
    uint64_t fee_per_gram,         // Fee rate
    char* message,                 // Transaction message
    bool one_sided,                // One-sided transaction flag
    int* error_out
);
```

**wallet_burn_tari**
```c
uint64_t wallet_burn_tari(
    TariWallet* wallet,
    uint64_t amount,               // Amount to burn
    uint64_t fee_per_gram,         // Fee rate
    char* message,                 // Burn message
    int* error_out
);
```

#### Transaction Queries

**wallet_get_completed_transactions**
```c
TariCompletedTransactions* wallet_get_completed_transactions(
    TariWallet* wallet,
    int* error_out
);
```

**wallet_get_pending_inbound_transactions**
```c
TariPendingInboundTransactions* wallet_get_pending_inbound_transactions(
    TariWallet* wallet,
    int* error_out
);
```

**wallet_get_pending_outbound_transactions**
```c
TariPendingOutboundTransactions* wallet_get_pending_outbound_transactions(
    TariWallet* wallet,
    int* error_out
);
```

**wallet_get_cancelled_transactions**
```c
TariCompletedTransactions* wallet_get_cancelled_transactions(
    TariWallet* wallet,
    int* error_out
);
```

#### Transaction Management

**wallet_cancel_pending_transaction**
```c
bool wallet_cancel_pending_transaction(
    TariWallet* wallet,
    uint64_t transaction_id,
    int* error_out
);
```

**wallet_coin_split**
```c
uint64_t wallet_coin_split(
    TariWallet* wallet,
    uint64_t amount_per_split,     // Amount for each output
    uint64_t split_count,          // Number of outputs to create
    uint64_t fee_per_gram,         // Fee rate
    char* msg,                     // Split message
    uint64_t lock_height,          // Lock height (0 for immediate)
    int* error_out
);
```

### Contact Management Functions

**wallet_upsert_contact**
```c
bool wallet_upsert_contact(
    TariWallet* wallet,
    TariContact* contact,
    int* error_out
);
```

**wallet_remove_contact**
```c
bool wallet_remove_contact(
    TariWallet* wallet,
    TariContact* contact,
    int* error_out
);
```

**wallet_get_contacts**
```c
TariContacts* wallet_get_contacts(
    TariWallet* wallet,
    int* error_out
);
```

### Validation and Recovery Functions

**wallet_start_transaction_validation**
```c
uint64_t wallet_start_transaction_validation(
    TariWallet* wallet,
    int* error_out
);
```

**wallet_start_txo_validation**
```c
uint64_t wallet_start_txo_validation(
    TariWallet* wallet,
    int* error_out
);
```

**wallet_start_recovery**
```c
bool wallet_start_recovery(
    TariWallet* wallet,
    void (*callback_recovery_progress)(uint8_t, uint64_t, uint64_t),
    int* error_out
);
```

### UTXO Functions

**wallet_preview_coin_join**
```c
TariCoinPreview* wallet_preview_coin_join(
    TariWallet* wallet,
    TariVector* commitments,       // UTXOs to join
    uint64_t fee_per_gram,         // Fee rate
    int* error_out
);
```

**wallet_get_unspent_amounts**
```c
TariVector* wallet_get_unspent_amounts(
    TariWallet* wallet,
    int* error_out
);
```

### Utility Functions

#### Address Management

**wallet_address_from_private_key**
```c
TariAddress* wallet_address_from_private_key(
    TariPrivateKey* secret_key,
    int* network,
    int* error_out
);
```

**wallet_address_from_hex**
```c
TariAddress* wallet_address_from_hex(
    const char* hex_string,
    int* error_out
);
```

#### Base Node State Functions

**basenode_state_get_node_id**
```c
ByteVector* basenode_state_get_node_id(
    TariBaseNodeState* base_node_state,
    int* error_out
);
```

**basenode_state_get_height_of_the_longest_chain**
```c
uint64_t basenode_state_get_height_of_the_longest_chain(
    TariBaseNodeState* base_node_state,
    int* error_out
);
```

**basenode_state_get_is_node_synced**
```c
bool basenode_state_get_is_node_synced(
    TariBaseNodeState* base_node_state,
    int* error_out
);
```

#### Memory Management Functions

**string_destroy**
```c
void string_destroy(char* s);
```

**byte_vector_destroy**
```c
void byte_vector_destroy(ByteVector* bytes);
```

**tari_vector_destroy**
```c
void destroy_tari_vector(TariVector* vector);
```

---

## Mining Helper FFI Library (minotari_mining_helper_ffi)

**Library Name**: `libminotari_mining_helper_ffi`
**Header File**: `tari_mining_helper.h`
**Source Location**: `base_layer/tari_mining_helper_ffi/`
**Primary Use Cases**: Mining pools, stratum servers, mining software integration

### Core Functions

#### Public Key Validation

**public_key_hex_validate**
```c
bool public_key_hex_validate(
    const char* hex,        // Hex-encoded public key
    int* error_out
);
```

#### Block Template Manipulation

**inject_nonce**
```c
void inject_nonce(
    ByteVector* header,             // Block header bytes
    unsigned long long nonce,       // Nonce value to inject
    int* error_out
);
```

**inject_coinbase**
```c
void inject_coinbase(
    ByteVector* block_template_bytes,      // Block template
    unsigned long long coinbase_value,     // Coinbase value
    bool stealth_payment,                  // Stealth payment flag
    bool revealed_value_proof,             // Revealed value proof flag
    const char* wallet_payment_address,    // Payment address
    const char* coinbase_extra,            // Extra coinbase data
    unsigned int network,                  // Network type (0=mainnet, 1=esmeralda, etc.)
    int* error_out
);
```

#### Share Validation

**share_difficulty**
```c
unsigned long long share_difficulty(
    ByteVector* header,         // Block header
    unsigned int network,       // Network type
    int* error_out
);
```

**share_validate**
```c
int share_validate(
    ByteVector* header,                    // Block header
    const char* hash,                      // Share hash
    unsigned int network,                  // Network type
    unsigned long long share_difficulty,   // Required share difficulty
    unsigned long long template_difficulty, // Required block difficulty
    int* error_out
);
```

**Return Values for share_validate:**
- `0` - Valid Block (meets both share and block difficulty)
- `1` - Valid Share (meets share difficulty only)
- `2` - Invalid Share
- `3` - Invalid Difficulty

#### Byte Vector Utilities

**byte_vector_create**
```c
ByteVector* byte_vector_create(
    const unsigned char* byte_array,
    unsigned int element_count,
    int* error_out
);
```

**byte_vector_get_length**
```c
unsigned int byte_vector_get_length(
    const ByteVector* vec,
    int* error_out
);
```

**byte_vector_get_at**
```c
unsigned char byte_vector_get_at(
    ByteVector* ptr,
    unsigned int position,
    int* error_out
);
```

**byte_vector_destroy**
```c
void byte_vector_destroy(ByteVector* bytes);
```

---

## Integration Patterns

### JavaScript/Node.js Integration

#### Setup
```javascript
const ffi = require("ffi-napi");
const ref = require("ref-napi");

// Load the wallet library
const libWallet = ffi.Library("./libminotari_wallet_ffi.dylib", {
    // Function definitions
    wallet_create: ["pointer", [/* parameters */]],
    wallet_destroy: ["void", ["pointer"]],
    wallet_get_available_balance: ["uint64", ["pointer", "pointer"]],
    // ... more functions
});
```

#### Error Handling
```javascript
const i32 = ref.types.int32;
let err = ref.alloc(i32);

let balance = libWallet.wallet_get_available_balance(wallet, err);
if (err.deref() !== 0) {
    console.error("Error getting balance:", err.deref());
}
```

#### Callback Implementation
```javascript
const receivedTx = ffi.Callback("void", ["pointer"], function(ptr) {
    console.log("Transaction received:", ptr);
});

const balanceUpdated = ffi.Callback("void", ["pointer"], function(ptr) {
    console.log("Balance updated:", ptr);
});
```

### iOS Swift Integration

#### Library Loading
```swift
// In your bridging header
#import "wallet.h"

// Swift usage
class TariWallet {
    private var walletPtr: OpaquePointer?
    
    func create() throws {
        var error: Int32 = 0
        walletPtr = wallet_create(
            config,
            logPath,
            numLogFiles,
            logFileSize,
            passphrase,
            seedWords,
            // ... callbacks
            &error
        )
        
        if error != 0 {
            throw WalletError.creationFailed(error)
        }
    }
    
    deinit {
        if let ptr = walletPtr {
            wallet_destroy(ptr)
        }
    }
}
```

#### Callback Handling
```swift
let receivedTransactionCallback: @convention(c) (UnsafeMutablePointer<TariPendingInboundTransaction>?) -> Void = { transaction in
    // Handle received transaction
    NotificationCenter.default.post(name: .transactionReceived, object: transaction)
}
```

### Android Java/Kotlin Integration

#### JNI Wrapper
```java
public class TariWallet {
    static {
        System.loadLibrary("minotari_wallet_ffi");
    }
    
    private long walletPtr;
    
    // Native method declarations
    private native long walletCreate(
        long commsConfig,
        String logPath,
        int numLogFiles,
        int logFileSize,
        String passphrase,
        String seedWords,
        // ... parameters
    );
    
    private native void walletDestroy(long walletPtr);
    private native long walletGetAvailableBalance(long walletPtr);
    
    // Callback interface
    public interface TransactionCallback {
        void onTransactionReceived(long transactionPtr);
        void onBalanceUpdated(long balancePtr);
    }
}
```

#### Kotlin Usage
```kotlin
class TariWalletManager {
    private var wallet: TariWallet? = null
    
    fun createWallet(config: WalletConfig) {
        wallet = TariWallet().apply {
            create(
                config.commsConfig,
                config.logPath,
                // ... parameters
            )
        }
    }
    
    fun getBalance(): Long {
        return wallet?.getAvailableBalance() ?: 0L
    }
}
```

### Python Integration

#### Using ctypes
```python
import ctypes
from ctypes import POINTER, c_char_p, c_void_p, c_uint64, c_int

# Load library
lib = ctypes.CDLL('./libminotari_wallet_ffi.so')

# Define function signatures
lib.wallet_create.argtypes = [
    c_void_p,  # config
    c_char_p,  # log_path
    # ... more parameters
]
lib.wallet_create.restype = c_void_p

lib.wallet_get_available_balance.argtypes = [c_void_p, POINTER(c_int)]
lib.wallet_get_available_balance.restype = c_uint64

class TariWallet:
    def __init__(self):
        self.wallet_ptr = None
    
    def create(self, config):
        error = c_int(0)
        self.wallet_ptr = lib.wallet_create(
            config,
            b"/tmp/wallet.log",
            # ... parameters
            ctypes.byref(error)
        )
        
        if error.value != 0:
            raise Exception(f"Wallet creation failed: {error.value}")
    
    def get_balance(self):
        error = c_int(0)
        balance = lib.wallet_get_available_balance(
            self.wallet_ptr,
            ctypes.byref(error)
        )
        
        if error.value != 0:
            raise Exception(f"Balance query failed: {error.value}")
        
        return balance
```

### C# .NET Integration

#### P/Invoke Declarations
```csharp
using System;
using System.Runtime.InteropServices;

public class TariWallet
{
    const string DLL_NAME = "minotari_wallet_ffi.dll";
    
    [DllImport(DLL_NAME, CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wallet_create(
        IntPtr config,
        [MarshalAs(UnmanagedType.LPStr)] string logPath,
        uint numLogFiles,
        uint logFileSize,
        [MarshalAs(UnmanagedType.LPStr)] string passphrase,
        [MarshalAs(UnmanagedType.LPStr)] string seedWords,
        // ... callback parameters
        out int error
    );
    
    [DllImport(DLL_NAME, CallingConvention = CallingConvention.Cdecl)]
    public static extern void wallet_destroy(IntPtr wallet);
    
    [DllImport(DLL_NAME, CallingConvention = CallingConvention.Cdecl)]
    public static extern ulong wallet_get_available_balance(
        IntPtr wallet,
        out int error
    );
    
    // Callback delegates
    public delegate void TransactionReceivedCallback(IntPtr transaction);
    public delegate void BalanceUpdatedCallback(IntPtr balance);
}
```

#### Usage Example
```csharp
public class WalletManager
{
    private IntPtr walletPtr;
    
    public void CreateWallet(WalletConfig config)
    {
        int error;
        walletPtr = TariWallet.wallet_create(
            config.CommsConfigPtr,
            config.LogPath,
            config.NumLogFiles,
            config.LogFileSize,
            config.Passphrase,
            config.SeedWords,
            // ... callbacks
            out error
        );
        
        if (error != 0)
        {
            throw new WalletException($"Failed to create wallet: {error}");
        }
    }
    
    public ulong GetBalance()
    {
        int error;
        var balance = TariWallet.wallet_get_available_balance(walletPtr, out error);
        
        if (error != 0)
        {
            throw new WalletException($"Failed to get balance: {error}");
        }
        
        return balance;
    }
}
```

---

## Mining Integration Examples

### Mining Pool Integration

#### Share Validation
```c
#include "tari_mining_helper.h"

int validate_share_submission(
    const unsigned char* header_bytes,
    size_t header_length,
    const char* share_hash,
    unsigned int network,
    unsigned long long pool_difficulty,
    unsigned long long network_difficulty
) {
    int error = 0;
    
    // Create byte vector from header
    ByteVector* header = byte_vector_create(header_bytes, header_length, &error);
    if (error != 0) {
        return -1; // Creation failed
    }
    
    // Validate the share
    int result = share_validate(
        header,
        share_hash,
        network,
        pool_difficulty,
        network_difficulty,
        &error
    );
    
    // Cleanup
    byte_vector_destroy(header);
    
    if (error != 0) {
        return -1; // Validation failed
    }
    
    return result; // 0=valid block, 1=valid share, 2=invalid
}
```

#### Block Template Processing
```c
void process_block_template(
    unsigned char* template_bytes,
    size_t template_length,
    const char* miner_address,
    unsigned long long coinbase_value
) {
    int error = 0;
    
    // Create byte vector
    ByteVector* template = byte_vector_create(template_bytes, template_length, &error);
    if (error != 0) return;
    
    // Inject coinbase
    inject_coinbase(
        template,
        coinbase_value,
        false,          // not stealth
        false,          // use bulletproof+
        miner_address,
        "Pool mining",  // coinbase extra
        1,              // esmeralda network
        &error
    );
    
    if (error == 0) {
        // Template ready for mining
        distribute_to_miners(template);
    }
    
    byte_vector_destroy(template);
}
```

---

## Build Integration

### Building the Libraries

#### Wallet FFI
```bash
# Build for current platform
cargo build --release --package minotari_wallet_ffi

# Cross-compile for iOS
cargo build --release --target aarch64-apple-ios --package minotari_wallet_ffi

# Cross-compile for Android
cargo build --release --target aarch64-linux-android --package minotari_wallet_ffi
```

#### Mining Helper FFI
```bash
# Build mining helper
cargo build --release --package minotari_mining_helper_ffi
```

### Header Generation

The C headers are automatically generated using `cbindgen`:

```toml
# In Cargo.toml
[build-dependencies]
cbindgen = "0.24"
```

```rust
// In build.rs
use cbindgen::Config;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    
    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_language(cbindgen::Language::C)
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file("wallet.h");
}
```

### Mobile Build Scripts

#### iOS Build
```bash
#!/bin/bash
# mobile_build.sh for iOS

# Set targets
TARGETS="aarch64-apple-ios x86_64-apple-ios-sim"

# Build for each target
for TARGET in $TARGETS; do
    cargo build --release --target $TARGET --package minotari_wallet_ffi
done

# Create universal library
lipo -create \
    target/aarch64-apple-ios/release/libminotari_wallet_ffi.a \
    target/x86_64-apple-ios-sim/release/libminotari_wallet_ffi.a \
    -output libminotari_wallet_ffi_universal.a
```

#### Android Build
```bash
#!/bin/bash
# Android build script

# Set up NDK
export ANDROID_NDK_HOME="/path/to/ndk"
export PATH="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin:$PATH"

# Build for Android architectures
TARGETS="aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android"

for TARGET in $TARGETS; do
    cargo build --release --target $TARGET --package minotari_wallet_ffi
done
```

---

## Error Codes Reference

### Wallet FFI Error Codes
```c
// Common error codes
#define WALLET_SUCCESS 0
#define WALLET_NULL_ERROR 1
#define WALLET_ALLOCATION_ERROR 2
#define WALLET_INVALID_ARGUMENT 3
#define WALLET_INSUFFICIENT_FUNDS 4
#define WALLET_NETWORK_ERROR 5
#define WALLET_STORAGE_ERROR 6
#define WALLET_CRYPTO_ERROR 7
#define WALLET_INVALID_ADDRESS 8
#define WALLET_TRANSACTION_ERROR 9
#define WALLET_NOT_INITIALIZED 10
```

### Mining Helper Error Codes
```c
// Mining helper error codes
#define MINING_SUCCESS 0
#define MINING_NULL_ERROR 1
#define MINING_INVALID_HEADER 2
#define MINING_INVALID_DIFFICULTY 3
#define MINING_NETWORK_ERROR 4
#define MINING_VALIDATION_ERROR 5
```

---

## Best Practices

### Memory Management
1. Always call destroy functions for created objects
2. Check for null pointers before use
3. Handle error codes consistently
4. Free strings returned from FFI functions

### Error Handling
1. Always check error output parameters
2. Implement proper error recovery
3. Log errors for debugging
4. Provide meaningful error messages to users

### Threading Considerations
1. FFI functions are not thread-safe unless documented
2. Use proper synchronization when accessing from multiple threads
3. Callback functions may be called from different threads

### Performance Optimization
1. Minimize FFI crossing overhead
2. Batch operations when possible
3. Cache frequently accessed data
4. Use appropriate data structures for collections

### Security Considerations
1. Validate all input parameters
2. Protect sensitive data in memory
3. Use secure random number generation
4. Implement proper key management

This comprehensive overview provides all the essential information for integrating Tari's FFI libraries into applications across different platforms and programming languages, with detailed function references, integration patterns, and best practices.
