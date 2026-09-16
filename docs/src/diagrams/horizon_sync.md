# Horizon sync

## Overview

Horizon sync triggers when the state machine (`state_machine_service/state_machine.rs`) transitions from `DecideNextSync` to `ProceedToHorizonSync`. This happens after the header is successfully synchronized (`HeaderSynchronized` event happens) and the base node is in **pruning mode** (i.e. `pruning_horizon` is > `0`).

Then, the state machine transitions from `ProceedToHorizonSync` to the `HorizonStateSync` state, where we check that we are not already synchronized (in that case we exit the horizon state with a successful `HorizonStateSynchronized` event).

If we are not already synchronized, the `HorizonStateSync` starts an event driven synchronizer (`HorizonStateSynchronization`) to synchronize with peers from tip header back to pruning horizon. At a high level, the horizon synchronizer performs:
* **Kernel** synchronization: fetches all required kernels from peers (RPC `sync_kernels`) and validates the merkle root
* **Output** synchronization: fetches all required outputs from peers (RPC `sync_utxos`) and validates the rangeproofs

After the horizon is synchronized, the state machine will transition to:
* `BlockSync` state if the horizon sync was successful (`HorizonStateSynchronized` event)
* `Waiting` state if the horizon sync failed (`HorizonStateSyncFailure` event)


## Flow diagram

```mermaid
flowchart TD
A[1. Start] --> B[2. Client: Initialise horizon sync]
B[2. Client: Initialise horizon sync] --> C[3. Client: Horizon synchronization]
C[3. Client: Horizon synchronization] --> D[4. Server: Synchronize kernels]
C[3. Client: Horizon synchronization] --> E[5. Server: Synchronize utxos]
D[4. Server: Synchronize kernels] --> C[3. Horizon synchronization]
E[5. Server: Synchronize utxos] --> C[3. Horizon synchronization]
A -..- N1>"state_machine.rs (fn next_state_event)"]
B -..- N2>"states/horizon_state_sync.rs (fn next_event)"]
C -..- N3>"horizon_state_sync/synchronizer.rs (fn synchronize)"]
D -..- N4>"base_node/sync/rpc/service.rs (fn sync_kernels)"]
E -..- N5>"base_node/sync/rpc/service.rs (fn sync_utxtos)"]
   N1:::note
   N2:::note
   N3:::note
   N4:::note
   N5:::note
    classDef note fill:#eee,stroke:#ccc
```


## Sequence diagram

```mermaid
sequenceDiagram
participant State Machine
participant HorizonStateSync
participant HorizonStateSynchronization
participant Peer
State Machine->>HorizonStateSync: 1. State change to `HorizonStateSync`
HorizonStateSync->>HorizonStateSynchronization: 2. Synchronize
HorizonStateSynchronization->>HorizonStateSynchronization: 3. Check that there are sync peers
HorizonStateSynchronization->>HorizonStateSynchronization: 4. Fetch header from db
loop 5. Attempt syncing with all peers
    loop 6. Attempt syncing with each peer
        HorizonStateSynchronization->>Peer: 6.1 Connect with sync peer
        HorizonStateSynchronization->>Peer: 6.2 Send RPC request to `sync_kernels` stream method
        loop 6.3. Process each received kernel 
            HorizonStateSynchronization->>HorizonStateSynchronization: 6.3.1 Validate kernel signature
            HorizonStateSynchronization->>HorizonStateSynchronization: 6.3.2 Insert kernel in database
            opt Kernel is the last in kernel in the MMR
                HorizonStateSynchronization->>HorizonStateSynchronization: 6.3.3 Validate MMR
	            HorizonStateSynchronization->>HorizonStateSynchronization: 6.3.4 Update block accumulated data
	            HorizonStateSynchronization->>HorizonStateSynchronization: 6.3.5 Update current header
            end
        end
        HorizonStateSynchronization->>Peer: 6.4 Send RPC request to `sync_utxos` stream method
        loop 6.5. Process each received output 
            opt UTXO is a regular tx output
	            HorizonStateSynchronization->>HorizonStateSynchronization: 6.5.1 Validate TariScript byte size
                HorizonStateSynchronization->>HorizonStateSynchronization: 6.5.2 Insert output in database
            end
            opt UTXO is a pruned output
                HorizonStateSynchronization->>HorizonStateSynchronization: 6.5.3 Insert pruned output in database
            end
            opt UTXO is deleted
                HorizonStateSynchronization->>HorizonStateSynchronization: 6.5.4 Validate against MMR
                HorizonStateSynchronization->>HorizonStateSynchronization: 6.5.5 Validate rangeproofs
                HorizonStateSynchronization->>HorizonStateSynchronization: 6.5.6 Update deleted bitmap
                HorizonStateSynchronization->>HorizonStateSynchronization: 6.5.7 Update block accumulated data
            end
        end
        HorizonStateSynchronization->>HorizonStateSynchronization: 6.6 Update database with the best block, new pruned height and new horizon data
        Note right of HorizonStateSynchronization: Ignore any latency or timeout error on the peer, and loop to the next peer
        Note right of HorizonStateSynchronization: Exit loop with error on any non-latency or non-timeout error of the peer
    end
    Note right of HorizonStateSynchronization: Increase `max_latency` and loop again if all peers exceed latency
    Note right of HorizonStateSynchronization: Exit loop with error on any non-latency error
end     
HorizonStateSync->>State Machine: 7. `StateEvent::HorizonStateSynchronized`
```