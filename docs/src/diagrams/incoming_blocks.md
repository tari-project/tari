# Incoming Blocks

## Overview
This process handles any incoming blocks that are propagated to a base node. This is different from the sync process 
where the node is actively requesting blocks from other nodes. 

## Prerequisites

See [Common message pipeline](common_message_pipeline.md) for details about how the messages are received and passed to this process.


## Incoming block preprocessing
After the pubsub connector has received the message, it is extracted as block and passed to the base node service.

```mermaid

flowchart TD
    A[1. inbound message subscription factory] --> B["2. From get_subscription TODO: Go deeper"]
    B --> C[[3. extract_block]]
    subgraph extract_block 
   
    C -..- N1>BaseNodeServiceInitializer line 118]
    C --> D["4. decode_message [prost]"]
    D --Failed --> S[5. Display warning and filter out message]
    D --> E[6. NewBlock try_from]
    E --Failed--> S
    
    end
    E -->E1[7. Base Node Service spawn_handle_incoming_block] --> F[[8. Check is bootstrapped]]
    F -..- N2>"base_node/service/service.rs line 290"]
    F --> G[9. Handle incoming block: TODO:Ref next diagram]
    N1:::note
    N2:::note
    classDef note fill:#eee,stroke:#ccc

```


## Incoming block handling

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

