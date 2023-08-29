## state machine
//todo
```mermaid
flowchart TD
    A -..- N1>state_machine.rs line 231]
    A[starting_state] --> |Initialized|B[["Listen_state (see listen_state.md)"]]
    B --> |FallenBehind| C[HeaderSync]

    
    N1:::note
    N2:::note
    N3:::note
    N4:::note
    classDef note fill:#eee,stroke:#ccc
    ```
