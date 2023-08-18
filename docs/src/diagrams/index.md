  

# Top level triggers
1. New message on messaging layer
    1. TODO: breakdown into individual messages
    2. Blocks
    3. metadata?
2. New RPC messages?
    1. Wallet messages?
4. Timed triggers
    1. Are there any of these? (Maybe in state machine)
    2. Liveness maybe?
3. Start up triggers
3. New GRPC messages
4. New CLI messages  (Maybe these can be compiled out completely)


## Other diagrams
1. Important database access methods


# Schema/Diagram key/legend Notation

Data access is denoted like this, 
```mermaid
flowchart TD
  A[(database method call)]
```

Source code link
```mermaid
flowchart TD
 A -.- N1>file_name : line 460]
 N1:::note
 classDef note fill:#eee,stroke:#ccc

```

## Calling a method vs next method in flow

In a flow diagram it's not always clear if an arrow leading to another method or process  is called (as in step into) or if it is the next method called in sequence. 
We suggest using the following notation to make it clear.

For example, consider this code:

```rust
fn attempt_sync() {
    let sync_method = determine_sync_method();
    if sync_method {
        do_something();
        do_next_thing();
    }
}

fn determine_sync_method() -> bool {
    let sync_method = some_logic();
    if sync_method {
        some_other_logic();
    }
    sync_method
}
```

```mermaid
flowchart TD
 A[attempt_sync] --> B[[determine_sync_method]] --"(calls)"--> B1["s = some_logic()"]
 
```

```mermaid
flowchart TD
 C[attempt_sync] --> D["sync_method = determine_sync_method"]
 D--> E{"sync_method"}
 E --"true"--> F["some_other_logic()"]
 E --"false"--> G["return sync_method"]
 F --> G
```

> A subgraph can also be used, but sometimes it might be easier to use the "(calls)" notation to show that a process is
> inside of a method call, and not the next one in sequence.

```mermaid

# pubsub connector

```mermaid

flowchart TD
   A[Start : todo]--> B[Pubsub connector]
   B --> C[if peer_message::TariMessageType is valid ]
   C --> D[forward to publisher]
   D --> E[if topic == sub topic]
   E --> F[forward to subscription]

    N2:::note
    classDef note fill:#eee,stroke:#ccc

```

# incoming block pre.

```mermaid

flowchart TD
    A[inbound message subscription factory] --> B["From get_subscription TODO: Go deeper"]
    B --> C[[extract_block]]
    subgraph extract_block 
   
    C -..- N1>BaseNodeServiceInitializer line 118]
    C --> D["decode_message [prost]"]
    D --Failed --> S[Display warning and filter out message]
    D --> E[NewBlock try_from]
    E --Failed--> S
    
    end
    E -->E1[Base Node Service spawn_handle_incoming_block] --> F[[Check is bootstrapped]]
    F -..- N2>"base_node/service/service.rs line 290"]
    F --> G[Handle incoming block: TODO:Ref next diagram]
    N1:::note
    N2:::note
    classDef note fill:#eee,stroke:#ccc

```




# Incoming block flow

```mermaid
flowchart TD
    A[handle_incoming_block] --> B{1. is add_block_disabled}
    B -- Yes --> X((X))
    B --> C[[2. Check if exists and is not in bad block list]]
    C -->|No| X
    C --> D[[3. Check difficulty > min_difficulty]]
    D -.- N1>line 460]
    N1:::note
    D --> E[[4. Check if already reconciling]]
    E --> |yes| X
    E --> F[[5. Try to acquire write lock and insert reconciling blocks *await writelock]]
    F -- already present --> X
    F --> G[["6. Check if exist and is not in bad block list (repeat of 2)"]]
    G --> H[[7. Try to add to list of reconciling blocks]]
    H -- Already added --> X
    H --> I[[8. Reconcile and add block]]
    I -- Success --> J[[9. Remove from reconciling blocks list]]
    I -- Failed --> J
    classDef note fill:#eee,stroke:#ccc
```

