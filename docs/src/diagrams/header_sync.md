## header sync
```mermaid
flowchart TD
    A -..- N1>header_sync.rs line 130]
    A -..- N2>header_sync/synchronizer.rs line 105]
    A[synchronize] --> B[1. try sync from all peers]
    B --> C[2. connect and attempt sync]
    C --> D[3. attemp sync]
    D -..- N3>header_sync/synchronizer.rs line 284]
    D --> E[(get local tip header)]
    E --> F[["Determine sync status"]]
    F--> G{SyncStatus?}    
    G --> |Lagging|M[["synchronize_headers (see synchronize_headers.md)"]]
    G --> |InSync|H[(Get local chain metadata height)]
    G --> |WereAhead|H[(Get local chain metadata height)]
    H --> I{metadata < local_chain_header}
    I --> |true|J[return event:HeadersSynchronized]
    I --> |false|K{starting local metdata < remote peer metadata}
    K --> |true|J
    K --> |False| L[ban remote peer]
    L --> N[return event::Continue]
    M --> J


    N1:::note
    N2:::note
    N3:::note
   classDef note fill:#eee,stroke:#ccc
```
## determine sync status
```mermaid
flowchart TD
    A -..- N1>header_sync/synchronizer.rs line 434]
    A[determine sync status] --> B[1. find chain split]
    B -..- N2>header_sync/synchronizer.rs line 364]
    B --> C[(fetch 500 headers back)]
    C --> D[2.Send headers to remote peer for matching]
    D --> E{Result back}
    E -->|RPC::RequestFailed|C  
    E -->|Some headers returned|F{3.Too much headers returned}
    F --> |Yes| G[ban peer]
    F --> |No|I{4.bad data returned}
    I --> |Yes| G
    I --> |No|J{5.No headers returned}
    J --> |Yes|K{6.returned forked inde > 0}
    K --> |Yes|L[return WereAhead]
    K --> |No|M[return Insync]
    J --> |No|N[7.validater returned headers]
    N --> O{8. remote peer tip height < split height}
    O --> |Yes| G
    O --> |No|P[return Lagging]


    N1:::note
    N2:::note
   classDef note fill:#eee,stroke:#ccc
```

## synchronise headers
```mermaid
flowchart TD
    A -..- N1>header_sync/synchronizer.rs line 566]
    A[synchronie_headers] --> B{1. Compare PoW}
    B --> |Yes| C[2.Switch chain to pending chain]
    C --> D[Rewind chain if desired]
    D --> E[Commit headers to database and main chain]
    B --> |No|F{3. pending header len < max request num}
    E --> |No|F
    F --> |Yes|G{4. Compare PoW}
    G --> |No|H[ban peer]
    F --> |No| I[5. Sync headers stream]
    G --> |Yes| I
    I --> J[6. Stream get next header]
    J -->|some header| K[(Get local chain metadata height)]
    K --> L{7.existing header?}
    L --> |Yes|J
    L --> |No|M[8. validate header]
    M --> N{has switched to new chain}
    N --> |Yes|O[commit new header]
    N --> |No| P{9.compare chains}
    P --> |Yes| Q[swap chains -> C]
    P --> |No| J
    J --> |None| R{has switched to new chain}
    R --> |No|S{Remote peer metadata < current temp metdata}
    R --> |Yes|V[commit last headers, return ok]
    S --> |Yes|T[ban peer]
    S --> |No| U[Return Ok]

    N1:::note
   classDef note fill:#eee,stroke:#ccc
```

```mermaid
flowchart TD
    A[attempt_sync] -->B[determine_sync_status]
    A -..- DB>DB access featch last header]
    B -..- N1>synchronizer.rs line 434]

    B --> C[fn find_chain_split]
    C -..- N2>synchronizer.rs line 364]

    C -->|remote peer| D[fn find_chain_split]
    D -..- N3>sync/rpc/service.rs line 364]

    D -->|GRAPH REMOTE|E[[fn determine_sync_status]]    
    E -..- N1>synchronizer.rs line 434]

    E -->|empty_headers|K[Is fork hash > 0]
    K --> |true| L[Return we are ahread]
    K --> |false|G[Return insync]
    E -->|too many returned|F[ban peer]
    E -->|index >= hashes returned|F[Return ban peer]
    L --> |Check if our meta data has changed|M[ban peer on true]
    G --> |Check if our meta data has changed|M[ban peer on true]
    E -->|iterate over returned headers| H[Validate initial headers]
    H --> I[is split height greater than claimed height]
    I -->|No|F[Return ban peer]
    I --> J[Lagging state]
    J --> N[fn Synchroize_headers]
    N -..- N4>synchronizer.rs line 566]

    N --> O[is intial headers pow's higher than current chain]
    O --> |Yes|P[swap to chain to intial headers]    
    O --> |No|Q[Has peer sent max headers]
    P --> Q[Has peer sent max headers]
    Q --> |No|R[is intial headers pow's higher than current chain]
    R --> |No|S[ban peer]
    R --> |Yes|T[Exit header sync, go to block sync]
    Q --> |Yes|U[iterate over header stream]
    U --> V[Does it follow prev header]
    V --> |No|W[ban peer]
    V --> |Yes|X[Does it already exist in chain] 
    X --> |Yes|W[ban peer]
    X --> |No|Y[validate] 
    Y --> |No|W[ban peer]
    Y --> |Yes|Z[Have we swapped to new chain]
    Z --> |Yes|AA[Add to main chain]
    Z --> |No|AB[is the new temp chain's pow higher than the local chain]
    AB --> |Yes|AC[Swap to new chain]
    AC --> AD[End of header stream]
    Z --> AD[End of header stream]
    AD --> |No|U
    AD --> |Yes|AE[Have we swapped chains]
    AE --> |No|AF[is the temp chains'pow less than originally claimed by the peer]
    AF --> |Yes|W
    AF --> |No|AG[local chain is higher, exit sync]
    AE --> AH[is new chains'pow less than originally claimed by the peer]
    AH --> |Yes|W
    AH --> |No|AI[Go on to block sync]

    N1:::note
    N2:::note
    N3:::note
    N4:::note
    DB:::note
    classDef note fill:#eee,stroke:#ccc
    ```