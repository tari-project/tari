# Block Sync

## Overview

Block sync (`BlockSync::next_event(..)`) is triggered when the state machine (`BaseNodeStateMachine`) transitions into `BaseNodeState::BlockSync(peers)`. It utilizes an asynchronous event driven synchronizer pattern implemented in `BlockSynchronizer` to synchronize blocks with the latter employing three dynamic function closures (`Hooks`) for `on_starting`, `on_progress` and `on_complete`

### Flow

```mermaid
flowchart TD
A[Start] --> B[Client: Initialise block sync]
B[Client: Initialise block sync] --> C[Client: Synchronize blocks]
C[Client: Synchronize blocks] --> D[Server: Synchronize blocks]
D[Server: Synchronize blocks] --> C[Client: Synchronize blocks]
A -..- N1>"state_machine.rs (fn next_state_event)"]
B -..- N2>"block_sync.rs (fn next_event)"]
C -..- N3>"synchronizer.rs (fn synchronize)"]
D -..- N4>"base_node/sync/rpc/service.rs (fn sync_blocks)"]
   N1:::note
   N2:::note
   N3:::note
   N4:::note
    classDef note fill:#eee,stroke:#ccc
```

### Sequence

```mermaid
sequenceDiagram
participant State Machine
participant Client
participant Server
State Machine->>Client: State change to `BlockSync`
Client->>Client: Initialise block sync
loop Attempt Block Sync
  Client->>Client: Select next sync peer
  Client->>Client: Call on starting hook
  Client->>Server: Connect to sync peer
  Note right of Client: Abandon block sync on connect error!
  Client->>Server: Obtain RPC connection
  Note right of Client: Abandon block sync on connect error!
  Client->>Client: Initialise block sync
  Client->>Server: Send request `SyncBlocksRequest`
  loop Stream Blocks Until End
    Note right of Client: Goto main loop on error, ban peer if malicious
    Server->>Client: Send next block body
    Client->>Client: Fetch block header from db
    Client->>Client: Deserialize block body
    Client->>Client: Construct full block
    Client->>Client: Validate block body
    Client->>Client: Construct chain block
    Client->>Client: Update block chain in db
    Client->>Client: Call on progress block hook
  end
  Client->>Client: If end, call on complete hook, exit loop
end
Client->>State Machine: `StateEvent::BlocksSynchronized`
```