# Block Sync

## Overview

Block sync (`BlockSync::next_event(..)`) is triggered when the state machine (`BaseNodeStateMachine`) transitions into `BaseNodeState::BlockSync(peers)`. It utilizes an asynchronous event driven synchronizer pattern implemented in `BlockSynchronizer` to synchronize blocks with the latter employing three dynamic function closures (`Hooks`) for `on_starting`, `on_progress` and `on_complete`

### Flow

```mermaid
flowchart TD
A[1. Start] --> B[2. Client: Initialise block sync]
B[2. Client: Initialise block sync] --> C[3. Client: Synchronize blocks]
C[3. Client: Synchronize blocks] --> D[4. Server: Synchronize blocks]
D[4. Server: Synchronize blocks] --> C[3. Client: Synchronize blocks]
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
State Machine->>Client: 1. State change to `BlockSync`
Client->>Client: 2. Initialise block sync
loop 3. Attempt Block Sync
  Client->>Client: 3.1 Select next sync peer
  Client->>Client: 3.2 Call on starting hook
  Client->>Server: 3.3 Connect to sync peer
  Note right of Client: Abandon block sync on connect error!
  Client->>Server: 3.4 Obtain RPC connection
  Note right of Client: Abandon block sync on connect error!
  Client->>Client: 3.5 Initialise block sync
  Client->>Server: 3.6 Send request `SyncBlocksRequest`
  loop 4. Stream Blocks Until End
    Note right of Client: Goto main loop on error, ban peer if malicious
    Server->>Client: 4.1 Send next block body
    Client->>Client: 4.2 Fetch block header from db
    Client->>Client: 4.3 Deserialize block body
    Client->>Client: 4.4 Construct full block
    Client->>Client: 4.5 Validate block body
    Client->>Client: 4.6 Construct chain block
    Client->>Client: 4.7 Update block chain in db
    Client->>Client: 4.8 Call on progress block hook
  end
  Client->>Client: 3.7 If end, call on complete hook, exit loop
end
Client->>State Machine: 5. `StateEvent::BlocksSynchronized`
```