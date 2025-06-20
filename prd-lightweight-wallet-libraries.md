# Product Requirements Document: Lightweight Wallet Libraries

## Introduction/Overview

This document outlines the requirements for extracting wallet scanning, UTXO validation, and UTXO extraction functionality from the Tari wallet codebase into self-contained, lightweight libraries. The goal is to enable developers to build lightweight wallet applications (CLI-based, WASM-based, web wallets) without the full wallet infrastructure dependencies.

The current Tari wallet implementation is tightly coupled with networking, storage, and communication layers, making it difficult to create lightweight wallet applications. This extraction will provide modular, crypto-focused libraries that can be used independently.

## Goals

1. **Modularity**: Extract core wallet functionality into independent, reusable libraries
2. **Minimal Dependencies**: Keep dependencies limited to essential crypto libraries
3. **Cross-Platform Support**: Enable development of CLI, WASM, and web-based wallets
4. **Performance**: Provide efficient UTXO scanning and validation capabilities
5. **Compatibility**: Ensure outputs match existing wallet implementations
6. **Developer Experience**: Provide simple, intuitive APIs for wallet developers

## User Stories

1. **Console Wallet Developer**: "As a console wallet developer, I want to scan for UTXOs without the full wallet infrastructure so that I can build a lightweight app. I will provide a seed or private key and I want to recover all the transactions for that wallet."

2. **Web Wallet Developer**: "As a web wallet developer, I want to validate UTXOs independently so that I can ensure transaction integrity quickly and easily using a simple interface and providing only a seed or a private key."

3. **WASM Wallet Developer**: "As a WASM wallet developer, I want to extract and validate UTXOs in a browser environment so that I can build secure, lightweight web wallets without heavy dependencies."

4. **Mobile Wallet Developer**: "As a mobile wallet developer, I want to scan and validate UTXOs efficiently so that I can build fast, responsive mobile wallet applications with minimal resource usage."

## Functional Requirements

### 1. UTXO Scanning Library
- **1.1** The library must accept a seed phrase or private key as input
- **1.2** The library must scan blockchain data to identify UTXOs belonging to the provided keys
- **1.3** The library must support scanning from a specified block height (wallet birthday)
- **1.4** The library must return a list of discovered UTXOs with their metadata
- **1.5** The library must support incremental scanning (resume from last scanned height)
- **1.6** The library must handle different transaction output versions (V0, V1)
- **1.7** The library must support both BulletProofPlus and RevealedValue range proof types

### 2. UTXO Validation Library
- **2.1** The library must validate range proofs for transaction outputs
- **2.2** The library must verify metadata signatures on transaction outputs
- **2.3** The library must validate script signatures on transaction inputs
- **2.4** The library must verify commitment integrity and correctness
- **2.5** The library must support batch validation for performance optimization
- **2.6** The library must validate encrypted data integrity
- **2.7** The library must verify minimum value promises match actual values

### 3. UTXO Extraction Library
- **3.1** The library must decrypt encrypted data using provided keys
- **3.2** The library must extract payment IDs from encrypted data
- **3.3** The library must reconstruct WalletOutput structures from TransactionOutput data
- **3.4** The library must handle different payment ID types (Empty, U256, Open, AddressAndData, TransactionInfo, Raw)
- **3.5** The library must support key recovery for stealth addresses
- **3.6** The library must extract and validate range proofs
- **3.7** The library must handle coinbase and burn outputs appropriately

### 4. Core Data Structures
- **4.1** The library must provide clean interfaces for EncryptedData, WalletOutput, and TransactionOutput
- **4.2** The library must support serialization/deserialization of all data structures
- **4.3** The library must provide error types for all failure scenarios
- **4.4** The library must support hex encoding/decoding for data structures

### 5. Key Management Integration
- **5.1** The library must integrate with minimal key management functionality
- **5.2** The library must support deterministic key derivation from seeds
- **5.3** The library must support imported private keys
- **5.4** The library must provide key recovery capabilities

## Non-Goals (Out of Scope)

1. **Networking/Communication**: The libraries will not include networking, RPC communication, or peer-to-peer functionality
2. **Storage**: The libraries will not include database storage or persistence mechanisms
3. **Transaction Broadcasting**: The libraries will not include transaction submission or broadcasting capabilities
4. **Full Wallet UI**: The libraries will not include user interface components
5. **Transaction Construction**: The libraries will focus on scanning and validation, not transaction building
6. **Blockchain Sync**: The libraries will not include full blockchain synchronization

## Design Considerations

### Architecture
- **Single Crate Structure**: All functionality will be contained in a single crate with multiple modules
- **Module Organization**:
  - `scanner`: UTXO scanning functionality
  - `validator`: UTXO validation functionality  
  - `extractor`: UTXO extraction and key recovery
  - `types`: Core data structures and types
  - `keys`: Key management and derivation
  - `errors`: Error types and handling

### API Design
- **Async Support**: All I/O operations should be async for performance
- **Error Handling**: Comprehensive error types with clear failure reasons
- **Builder Pattern**: Use builder patterns for complex operations
- **Iterator Support**: Provide iterators for large datasets
- **Batch Operations**: Support batch processing for performance

### Data Flow
1. **Input**: Seed phrase or private key
2. **Scanning**: Identify relevant UTXOs from blockchain data
3. **Validation**: Verify cryptographic proofs and signatures
4. **Extraction**: Decrypt and reconstruct wallet data
5. **Output**: Validated WalletOutput structures

## Technical Considerations

### Dependencies
- **Minimal Crypto Dependencies**: Only essential cryptographic libraries
- **No Network Dependencies**: Avoid networking and communication libraries
- **No Storage Dependencies**: Avoid database and storage libraries
- **Cross-Platform**: Ensure compatibility with WASM, CLI, and web environments

### Performance Requirements
- **Scanning Speed**: Support scanning thousands of blocks per second
- **Memory Efficiency**: Minimize memory usage for large UTXO sets
- **Validation Speed**: Support batch validation of multiple UTXOs
- **WASM Compatibility**: Ensure efficient operation in browser environments

### Security Requirements
- **Key Security**: Secure handling of private keys and seeds
- **Validation Integrity**: Thorough validation of all cryptographic proofs
- **Error Handling**: Secure error handling without information leakage
- **Memory Safety**: Zeroization of sensitive data in memory

## Success Metrics

1. **Compatibility**: 100% compatibility with existing wallet outputs
2. **Performance**: Scanning speed within 10% of current wallet implementation
3. **Dependency Reduction**: Reduce dependencies by at least 70% compared to full wallet
4. **Developer Adoption**: Successful integration in at least 3 different wallet types (CLI, WASM, Web)
5. **Error Handling**: Comprehensive error coverage for all failure scenarios
6. **Documentation**: Complete API documentation with examples

## Open Questions

1. **Blockchain Data Source**: How will the libraries receive blockchain data? (File-based, memory-based, or callback-based?)
2. **Progress Reporting**: Should the libraries provide progress callbacks during scanning?
3. **Configuration**: What configuration options should be exposed for different use cases?
4. **Testing Strategy**: How should the libraries be tested for different environments (CLI, WASM, Web)?
5. **Versioning**: How should the libraries handle different transaction output versions?
6. **Error Recovery**: What level of error recovery should be provided for corrupted data?

## Implementation Phases

### Phase 1: Core Data Structures
- Extract and clean up EncryptedData, WalletOutput, and TransactionOutput
- Implement serialization/deserialization
- Create comprehensive error types

### Phase 2: Key Management
- Implement minimal key derivation functionality
- Support seed phrase and private key inputs
- Add key recovery capabilities

### Phase 3: UTXO Validation
- Implement range proof validation
- Add signature verification
- Support batch validation

### Phase 4: UTXO Extraction
- Implement encrypted data decryption
- Add payment ID extraction
- Support WalletOutput reconstruction

### Phase 5: UTXO Scanning
- Implement blockchain data scanning
- Add incremental scanning support
- Optimize for performance

### Phase 6: Integration and Testing
- Comprehensive testing across platforms
- Performance optimization
- Documentation and examples 