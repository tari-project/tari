# Copyright 2024 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@payref-history @wallet
Feature: PayRef History

  @critical
  Scenario: PayRef history is preserved after a reorg
    # Set up chain 1 with wallet and miner
    Given I have a seed node SEED_A
    When I have a base node NODE_A connected to seed SEED_A
    When I have wallet WALLET_A connected to base node NODE_A
    When I have wallet WALLET_B connected to base node NODE_A
    When I have SHA3X mining node MINER_A connected to base node NODE_A and wallet WALLET_A
    # Mine enough blocks for coinbase maturity
    When mining node MINER_A mines 4 blocks with min difficulty 1 and max difficulty 1
    Then all nodes are at height 4
    When I wait for wallet WALLET_A to have at least 1002000 uT
    # Send a one-sided transaction so WALLET_B gets outputs
    When I send a one-sided transaction of 1000000 uT from wallet WALLET_A to wallet WALLET_B at fee 20
    Then wallet WALLET_A detects all transactions are at least Broadcast
    # Mine it into chain A
    When mining node MINER_A mines 1 blocks with min difficulty 1 and max difficulty 1
    Then all nodes are at height 5
    Then wallet WALLET_A detects all transactions are at least Mined_or_OneSidedUnconfirmed
    # Store the initial PayRefs for WALLET_A transactions
    Then wallet WALLET_A has PayRefs for all mined transactions
    # Now create a longer chain B (reorg)
    Given I have a seed node SEED_B
    When I have a base node NODE_B connected to seed SEED_B
    When I have wallet WALLET_C connected to base node NODE_B
    When I have SHA3X mining node MINER_B connected to base node NODE_B and wallet WALLET_C
    When mining node MINER_B mines 7 blocks with min difficulty 1 and max difficulty 1
    Then node NODE_B is at height 7
    # Connect the chains - NODE_A should reorg to NODE_B's longer chain
    When I have a base node BRIDGE connected to nodes NODE_A,NODE_B
    Then node NODE_A is at height 7
    Then node NODE_B is at height 7
    # The wallet should automatically detect the reorg via its base node connection.
    # The UTXO scanner detects missing scanned blocks and calls process_reorg,
    # which archives the old PayRefs to the payref_history table.
    Then wallet WALLET_A has historical PayRefs from before the reorg

  @critical
  Scenario: Rejected transaction shows cancellation reason
    Given I have a seed node NODE
    When I have wallet WALLET_A connected to all seed nodes
    When I have wallet WALLET_B connected to all seed nodes
    When I have SHA3X mining node MINER connected to base node NODE and wallet WALLET_A
    # A second miner that does not pay WALLET_A mines every block after WALLET_A is funded. This
    # keeps WALLET_A from receiving fresh coinbases near the tip: the later
    # "detects all transactions as Mined_or_OneSidedConfirmed" step waits on *every* completed
    # transaction, and a coinbase mined within num_confirmations of the tip can never reach
    # CoinbaseConfirmed, so it would hang until timeout (see TransactionInfo.feature).
    When I have a stealth SHA3 miner MINER2 connected to all seed nodes
    When mining node MINER mines 4 blocks
    Then all nodes are at height 4
    When I wait for wallet WALLET_A to have at least 1002000 uT
    When I send an interactive transaction of 1000000 uT from wallet WALLET_A to wallet WALLET_B at fee 20
    Then wallet WALLET_A detects all transactions are at least Broadcast
    # MINER2 mines on its own base node, so "Broadcast" (accepted by NODE's mempool) does not mean
    # MINER2's node has the transaction yet. Without this wait the block template can be built
    # before the transaction propagates, the block is mined without it, and since no further blocks
    # are mined until it is seen as mined, the next step hangs until timeout.
    Then I wait until base node MINER2 has 1 unconfirmed transactions in its mempool
    When mining node MINER2 mines 1 blocks
    Then all nodes are at height 5
    Then wallet WALLET_A detects all transactions are at least Mined_or_OneSidedUnconfirmed
    When mining node MINER2 mines 10 blocks
    Then all nodes are at height 15
    Then wallet WALLET_A detects all transactions as Mined_or_OneSidedConfirmed
    # Verify mined transactions have empty rejected_reason
    Then all mined transactions for wallet WALLET_A have empty rejected_reason
    # This wait is needed to stop base nodes from shutting down
    When I wait 1 seconds
