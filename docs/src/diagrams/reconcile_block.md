# Reconcile Block

## Overview
This flow shows us how blocks are reconciled  ...
Blocks are tirggered for reconciliation in one location via an incoming new block message.

## Prerequisites

See [Common message pipeline](common_message_pipeline.md) for details about how the messages are received and passed to this process.


## Block reconciliation and addition


```mermaid

flowchart TD
    A[fn reconcile_block] --> AA{1. is empty?}
    AA --yes --> AB[return Block]
    AA --no --> AC{2. is orphan?}
    AC --yes --> D
    AC --no --> B[9. fn mempool::retrieve_by_excess_sigs]
    
    subgraph fetching blocks
        B <--> BA[[10. fn mempool_storage::retrieve_by_excess_sigs]]
        BA <--> BB[[11. fn unconfirmed_pool::retrieve_by_excess_sigs]]
        BA <--> BC[[12. fn reorg_pool::retrieve_by_excess_sigs]]
    end
    
    B --> C{13. has all excess signatures?}
    C --no --> CA[14. fn outbound_interface::request_transactions_by_excess_sig]
    CA --> CC[(15. fn mempool::insert_all)]
    CC --> CE{16. all transactions found?}
    CE --no --> CJ
    CE --yes --> CD[18. BlockBuilder with transactions]
    C --yes --> CB[19. fn calculate_mmr_roots]
    CB --invalid --> CJ[17. fn outbound_interface::request_full_block_from_peer]
    CJ --> CK[return Block]
    CB --valid --> CL[20. fn check_mmr_roots]
    CL --invalid --> CJ
    CD --> CB
    CL --valid --> CM[return Block]

    subgraph orphan block
        D{3. is empty?}
        D --yes --> DA[return Block]
        D --no --> DB[4. fn outbound_interface::request_full_block_from_peer]
        DB --> DC[5. fn outbound_interface::request_blocks_by_hashes_from_peer]
        DC --> DD{6. has block?}
        DD --yes --> DE[return Block]
        DD --ConnectivityError --> DF[7. fn ban_peer_until]
        DD --UnexpectedApiResponse --> DG[8. fn ban_peer]
        DD --no --> DH[Err]
    end
    
    classDef note fill:#eee,stroke:#ccc
```