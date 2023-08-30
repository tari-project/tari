flowchart TD
    A -..- N1>sync_decide.rs line 46]
    A[next_event] --> B[(get_chain_metadata)]
    B --> C{pruning mode}
    C --> |Yes|D[1. get list of sync peers </br> remote peer height > horizon sync height]
    D --> E{is empty}
    E --> |No| G[return State::ProceeedToHorizonSync]
    E --> |Yes| F[return State::continue] 
    C --> |No|H[2. get list of sync peers </br> remote peer pruned height < local longest chain]
    H --> I{is empty}
    I --> |Yes| F[return State::continue]
    I --> |No| J[return State::ProceedToBlockSync] 

    N1:::note
   classDef note fill:#eee,stroke:#ccc