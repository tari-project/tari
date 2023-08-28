```mermaid
flowchart TD
    A -..- N1>listening.rs line 128]
    A --> |Network Silence| A
    A --> |PeerChainMetadataReceived| B{1. Is peer banned}
    B --> |True|A
    B --> |False| C[(2. save peer info)]
    C --> D{3. Is forced synced peers on}
    D --> |Yes|E{4. Is this a forced sync peer}
    E --> |No| A
    D --> |No| F[(get local chain metadata)]
    E --> |Yes| F
    F --> G{5.Is synced & 1 block behind remote peer & wait less 20s since received}
    G --> |Yes| A
    G --> |No| H[(get local chain metadata)]
    H --> I[7.determine_sync_mode]
    I -..- N2>listening.rs line 273]
    I --> J{8. Local metadat < Remote metadata}
    J --> |Yes| K[9. Local height + lagging blocks > network height]
    K --> |Yes| A 
    K --> |No| L[return FallenBehind]
    J --> |No| A



    N1:::note
    N2:::note
   classDef note fill:#eee,stroke:#ccc
   ```