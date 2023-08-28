# Reconcile Block

## Overview
This flow shows us how blocks are reconciled  ...
Blocks are tirggered for reconciliation in one location via an incoming new block message.

## Prerequisites

See [Common message pipeline](common_message_pipeline.md) for details about how the messages are received and passed to this process.


## Block reconciliation and addition


```mermaid

flowchart TD
    A[fn reconcile_block] --> AA{is empty?}
    AA --yes --> AB[return Block]
    AA --no --> AC{is orphan?}
    AC --yes --> D
    AC --no --> B["fn mempool::retrieve_by_excess_sigs()"]
    
    subgraph fetching blocks
        B <--> BA[[fn mempool_storage::retrieve_by_excess_sigs]]
        BA <--> BB[[fn unconfirmed_pool::retrieve_by_excess_sigs]]
        BA <--> BC[[fn reorg_pool::retrieve_by_excess_sigs]]
    end
    
    B --> C{has all excess signatures?}
    C --no --> CA[fn outbound_interface::request_transactions_by_excess_sig]
    C --yes --> CB[fn calculate_mmr_roots]
    CA --> CC[(fn mempool::insert_all)]
    CC --> CD[BlockBuilder with transactions]
    CD --> CB
    CA --> CE{some tansactions not found?}
    CE --yes --> CF[fn request_full_block_from_peer]
    CF --> CG[return Block]
    CB --> CH{do some more}

    subgraph orphan block
        D{is empty?}
        D --yes --> DA[return Block]
        D --no --> DB[fn request_full_block_from_peer]
        DB --> DC[fn outbound_interface::request_blocks_by_hashes_from_peer]
        DC --> DD{has block?}
        DD --yes --> DE[return Block]
        DD --ConnectivityError --> DF[fn ban_peer_until]
        DD --UnexpectedApiResponse --> DG[fn ban_peer]
        DD --no --> DH[Err]
    end
    
    classDef note fill:#eee,stroke:#ccc
```