# Block Sync

## Overview

Block sync (`BlockSync::next_event(..)`) is triggered when the state machine (`BaseNodeStateMachine`) transitions into `BaseNodeState::BlockSync(peers)`. It utilizes an asynchronous event driven synchronizer pattern implemented in `BlockSynchronizer` to synchronize blocks with the latter employing three dynamic function closures (`Hooks`) for `on_starting`, `on_progress` and `on_complete`

### High Level Flow

```mermaid
flowchart TD

A["1. Start"]
B["2. Client: Initialise block sync"]
C["3. Client: Synchronize blocks"]
D["4. Server: Synchronize blocks"]

A --"State change to `BlockSync`"--> B
B --"Call to 
`match synchronizer.synchronize().await`"--> C
C --"block_stream.next().await"--> D
D --"response"--> C

A -..- N1>"state_machine.rs (fn next_state_event)"]
B -..- N2>"block_sync.rs (fn next_event)"]
C -..- N2>"block_sync.rs (fn next_event)"]
C -..- N3>"synchronizer.rs (fn synchronize)"]
D -..- N4>"base_node/sync/rpc/service.rs (fn sync_blocks)"]
    N1:::note
    N2:::note
    N3:::note
    N4:::note
    classDef note fill:#eee,stroke:#ccc
```

### Block Sync Sequence (Happy Path)

```mermaid
sequenceDiagram
participant State Machine
participant Client
participant Server
State Machine->>Client: 1.1 State change to `BlockSync`
Client->>Client: 2.1 Initialise block sync
alt 3.0 Synchronize
    loop 3.1 Attempt Block Sync
        loop 3.2 For all sync peers
            Client->>Client: 3.2.1 Select next sync peer
            Client->>Client: 3.2.2 On starting hook: 'StateInfo::Connecting`
            Note over Client: Only called for the first peer ('vec is drained')
            Client->>Server: 3.2.3 Connect to sync peer
            Client->>Server: 3.2.4 Obtain RPC connection
            alt 3.3 Synchronize Blocks
                Client->>Client: 3.3.1 Initialise block sync
                Client->>Server: 3.3.2 Send request `SyncBlocksRequest`
                alt 4.1 Server side
                    Server->>Server: 4.1.1 TODO: `async fn sync_blocks`
                end
                loop 3.4 Stream Blocks Until End
                    Server->>Client: 3.4.1 Send next block body
                    Client->>Client: 3.4.2 Fetch block header from db
                    Client->>Client: 3.4.3 Deserialize block body
                    Client->>Client: 3.4.4 Construct full block
                    Client->>Client: 3.4.5 Validate block body
                    Client->>Client: 3.4.6 Construct chain block
                    Client->>Client: 3.4.7 Update block chain in db
                    Client->>Client: 3.4.8a On progress hook: send event `BlockEvent::ValidBlockAdded`
                    Client->>Client: 3.4.8b On progress hook: event `StateInfo::BlockSync`
                    alt 3.4.9 if avg_latency > max_latency
                        Client->>Client: 3.4.9a Exit 3.3 with `MaxLatencyExceeded`
                    end
                end
                alt 3.3.3 if final block ok
                    Client->>Client: 3.3.3a On complete hook: send event `BlockEvent::BlockSyncComplete`
                end
            end
            alt 3.2.5 if Synchronize Blocks ok
                Client->>Client: 3.2.5a Clean up orphans db
                Client->>Client: 3.2.5b Exit loop 3.2
            end
            alt 3.2.6 if MaxLatencyExceeded && sync_peer == last_peer
                Client->>Client: 3.2.6a Exit 3.2 with `AllSyncPeersExceedLatency`
            end
        end
        alt 3.5 if Attempt Block Sync ok
            Client->>Client: 3.5.1 Exit loop 3.1
        else 3.6.1 if AllSyncPeersExceedLatency && sync_peers > 2
            Client->>Client: 3.6.1a Increase latency
        end
    end
end
alt 3.7.1 if Synchronize ok
    Client->>Client: 3.7.1a Set `BlockSync::is_synced = true`
    Client->>State Machine: 3.7.1b `StateEvent::BlocksSynchronized`
end
```

### Block Sync Sequence (Error Handling)

```mermaid
sequenceDiagram
participant State Machine
participant Client
participant Server
State Machine->>Client: 1.1 State change to `BlockSync`
alt 3.0 Synchronize
    loop 3.1 Attempt Block Sync
        loop 3.2 For all sync peers
            Client->>Server: 3.2.3 Connect to sync peer
            alt 3.2.3a if ConnectivityError 
                Client->>Client: Exit 3.2 with Err(BlockSyncError::ConnectivityError)
                Note over Client: Should try next peer not exit!
            end
            Client->>Server: 3.2.4 Obtain RPC connection
            alt 3.2.4a if RpcError
                Client->>Client: Exit 3.2 with Err(BlockSyncError::RpcError)
                Note over Client: Should try next peer not exit!
            end
            Client->>Server: 3.2.6 Send request `SyncBlocksRequest`
            alt 4.1 Server side
                Server->>Server: 4.1.1 TODO: `async fn sync_blocks`
            end
            alt 3.3 Synchronize Blocks
                loop 3.4 Stream Blocks Until End
                    Server->>Client: 3.4.1 Send next block body
                    alt 3.4.1a if block stream RpcStatus err
                        Client->>Client: 3.4.1b Exit 3.3 with that error
                        Note over Client: Should try next peer not exit!
                    end
                    alt 3.4.9 if ValidationError::BadBlockFound || FatalStorageError || AsyncTaskFailed || CustomError
                        Client->>Client: 3.4.9a Exit 3.3 with that error
                    end
                    alt 3.4.10 if other ValidationError
                        Client->>Client: 3.4.10a Delete orphan, Insert bad block
                        Note over Client: Some errors above also belong here, i.e. CustomError!
                        Client->>Client: 3.4.10b Exit 3.3 with other ValidationError
                    end
                    alt 3.4.11 if avg_latency > max_latency
                        Client->>Client: 3.4.11a Exit 3.3 with `MaxLatencyExceeded`
                    end
                    alt 3.4.12 if other error
                        Client->>Client: 3.4.12a Exit 3.3 with other error
                    end
                    Note over Client: All errors from synchronize_blocks try next peer not exit!
                end
            end
            alt 3.2.5 if Synchronize Blocks ok
                alt if 3.2.5c Clean up orphans db not ok
                    Client->>Client: 3.2.5d Exit loop 3.2 with that error
                    Note over Client: Sync ok, not return wrong status!
                end
            end
            alt 3.2.6 if MaxLatencyExceeded && sync_peer == last_peer
                Client->>Client: 3.2.6b Exit 3.2 with `AllSyncPeersExceedLatency`
            else 3.2.7 if other error
                Client->>Client: 3.2.7a Exit 3.2 with other error
            end
        end
        alt 3.6.2a err AllSyncPeersExceedLatency
            alt 3.6.2b if sync_peers <= 2
                Client->>Client: 3.6.2c Exit 3.1 with `AllSyncPeersExceedLatency`
                Note over Client: Remove, this negates the increasing latency logic!
            end
        end
        alt 3.6.3 if other error
            Client->>Client: 3.6.3a Exit 3.2 with other error
        end
    end
end
alt 3.7.2 if Synchronize err
    Client->>Client: 3.7.2a Send event `StateInfo::SyncFailed`
    Client->>Client: 3.7.2b Swap to highest proof-of-work chain
    Note over Client: Call clear_all_pending_headers()!
    Note over Client: Call cleanup_orphans()!
    Client->>State Machine: 3.7.2c `StateEvent::BlockSyncFailed`
end
```