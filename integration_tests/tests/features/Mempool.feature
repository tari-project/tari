# Copyright 2022 The Taiji Project
# SPDX-License-Identifier: BSD-3-Clause

@mempool @base-node
Feature: Mempool

  @critical
  Scenario: Transactions are propagated through a network
    #
    # The randomness of the TX1 propagation can result in this test not passing.
    # The probability of not passing (at least 2 nodes are not aware of TX1) is ~0.01%.
    #
    Given I have 8 seed nodes
    When I have a base node SENDER connected to all seed nodes
    When I have 8 base nodes connected to all seed nodes
    When I mine a block on SENDER with coinbase CB1
    When I mine 2 blocks on SENDER
    Then all nodes are at height 3
    When I create a transaction TX1 spending CB1 to UTX1
    When I submit transaction TX1 to SENDER
    Then SENDER has TX1 in MEMPOOL state
    Then TX1 is in the MEMPOOL of all nodes, where 1% can fail

    @broken
  Scenario: Transactions are synced
    Given I have 2 seed nodes
    When I have a base node SENDER connected to all seed nodes
    When I have 2 base nodes connected to all seed nodes
    When I mine a block on SENDER with coinbase CB1
    When I mine 6 blocks on SENDER
    Then all nodes are at height 7
    When I create a transaction TX1 spending CB1 to UTX1
    When I submit transaction TX1 to SENDER
    Then SENDER has TX1 in MEMPOOL state
    Then TX1 is in the MEMPOOL of all nodes
    When I have a base node NODE1 connected to all seed nodes
    # Keeps returning not stored. Maybe initial sync ins't receiving it.
    # mempool needs to sync more than 5 blocks before it starts syncing
    Then NODE1 has TX1 in MEMPOOL state
    When I mine 1 blocks on SENDER
    Then all nodes are at height 8
    Then SENDER has TX1 in MINED state
    Then TX1 is in the MINED of all nodes

  @critical
  Scenario: Clear out mempool
    Given I have 1 seed nodes
    When I have a base node SENDER connected to all seed nodes
    When I mine a block on SENDER with coinbase CB1
    When I mine a block on SENDER with coinbase CB2
    When I mine a block on SENDER with coinbase CB3
    When I mine 4 blocks on SENDER
    When I create a custom fee transaction TX1 spending CB1 to UTX1 with fee per gram 1600
    When I create a custom fee transaction TX2 spending CB2 to UTX2 with fee per gram 2000
    When I create a custom fee transaction TX3 spending CB3 to UTX3 with fee per gram 1800
    When I submit transaction TX1 to SENDER
    When I submit transaction TX2 to SENDER
    When I submit transaction TX3 to SENDER
    Then SENDER has TX1 in MEMPOOL state
    Then SENDER has TX2 in MEMPOOL state
    Then SENDER has TX3 in MEMPOOL state
    # Note: The block weight should only allow one transaction to be included in the block template
    When I mine 1 custom weight blocks on SENDER with weight 80
    Then SENDER has TX1 in MEMPOOL state
    Then SENDER has TX2 in MINED state
    Then SENDER has TX3 in MEMPOOL state
    # Note: The block weight should only allow one transaction to be included in the block template
    When I mine 1 custom weight blocks on SENDER with weight 80
    Then SENDER has TX1 in MEMPOOL state
    Then SENDER has TX2 in MINED state
    Then SENDER has TX3 in MINED state

  @long-running
  Scenario: Double spend eventually ends up as not stored
    Given I have 1 seed nodes
    When I have a base node SENDER connected to all seed nodes
    When I mine a block on SENDER with coinbase CB1
    When I mine 4 blocks on SENDER
    When I create a custom fee transaction TX1 spending CB1 to UTX1 with fee per gram 16
    When I create a custom fee transaction TX2 spending CB1 to UTX2 with fee per gram 20
    When I submit transaction TX1 to SENDER
    When I submit transaction TX2 to SENDER
    Then SENDER has TX1 in MEMPOOL state
    Then SENDER has TX2 in MEMPOOL state
    When I mine 1 blocks on SENDER
    # A transaction that was removed from the pool will be reported as unknown as long as it is stored in the reorg pool
    # for 5 minutes
    Then SENDER has TX1 in UNKNOWN state
    When I mine 8 blocks on SENDER
    Then SENDER has TX1 in NOT_STORED state
    Then SENDER has TX2 in MINED state

  Scenario: Mempool clearing out invalid transactions after a reorg
    Given I have a seed node SEED_A
    When I have a base node NODE_A connected to seed SEED_A
    When I have wallet WALLET_A connected to base node NODE_A
    When I have mining node MINING_A connected to base node NODE_A and wallet WALLET_A
    When I mine a block on NODE_A with coinbase CB_A
    When mining node MINING_A mines 3 blocks with min difficulty 1 and max difficulty 2
    Then node SEED_A is at height 4
    Given I have a seed node SEED_B
    When I have a base node NODE_B connected to seed SEED_B
    When I have wallet WALLET_B connected to base node NODE_B
    When I have mining node MINING_B connected to base node NODE_B and wallet WALLET_B
    When I mine a block on NODE_B with coinbase CB_B
    When mining node MINING_B mines 10 blocks with min difficulty 20 and max difficulty 9999999999
    Then node SEED_B is at height 11
    When I create a custom fee transaction TXA spending CB_A to UTX1 with fee per gram 16
    When I create a custom fee transaction TXB spending CB_B to UTX1 with fee per gram 16
    When I submit transaction TXA to NODE_A
    When I submit transaction TXB to NODE_B
    Then NODE_A has TXA in MEMPOOL state
    Then NODE_B has TXB in MEMPOOL state
    When mining node MINING_A mines 1 blocks with min difficulty 1 and max difficulty 2
    When mining node MINING_B mines 1 blocks with min difficulty 20 and max difficulty 9999999999
    Then node SEED_A is at height 5
    Then node SEED_B is at height 12
    When I connect node NODE_A to node NODE_B
    Then all nodes are at height 12
    Then NODE_A has TXA in NOT_STORED state
    Then NODE_A has TXB in MINED state

  @critical
  Scenario: Zero-conf transactions
    Given I have 1 seed nodes
    When I have a base node SENDER connected to all seed nodes
    When I mine a block on SENDER with coinbase CB1
    When I mine a block on SENDER with coinbase CB2
    When I mine 4 blocks on SENDER
    When I create a custom fee transaction TX01 spending CB1 to UTX01 with fee per gram 20
    When I create a custom fee transaction TX02 spending UTX01 to UTX02 with fee per gram 20
    When I create a custom fee transaction TX03 spending UTX02 to UTX03 with fee per gram 20
    When I create a custom fee transaction TX11 spending CB2 to UTX11 with fee per gram 20
    When I create a custom fee transaction TX12 spending UTX11 to UTX12 with fee per gram 20
    When I create a custom fee transaction TX13 spending UTX12 to UTX13 with fee per gram 20
    When I submit transaction TX01 to SENDER
    When I submit transaction TX02 to SENDER
    When I submit transaction TX03 to SENDER
    When I submit transaction TX11 to SENDER
    When I submit transaction TX12 to SENDER
    When I submit transaction TX13 to SENDER
    Then SENDER has TX01 in MEMPOOL state
    Then SENDER has TX02 in MEMPOOL state
    Then SENDER has TX03 in MEMPOOL state
    Then SENDER has TX11 in MEMPOOL state
    Then SENDER has TX12 in MEMPOOL state
    Then SENDER has TX13 in MEMPOOL state
    When I mine 1 blocks on SENDER
    Then SENDER has TX01 in MINED state
    Then SENDER has TX02 in MINED state
    Then SENDER has TX03 in MINED state
    Then SENDER has TX11 in MINED state
    Then SENDER has TX12 in MINED state
    Then SENDER has TX13 in MINED state

  Scenario: Mempool unconfirmed transactions
    Given I have 1 seed nodes
    When I have a base node BN1 connected to all seed nodes
    When I mine a block on BN1 with coinbase CB1
    When I mine 5 blocks on BN1
    When I create a custom fee transaction TX1 spending CB1 to UTX1 with fee per gram 16
    When I create a custom fee transaction TX2 spending CB1 to UTX1 with fee per gram 16
    When I create a custom fee transaction TX3 spending CB1 to UTX1 with fee per gram 16
    When I create a custom fee transaction TX4 spending CB1 to UTX1 with fee per gram 16
    When I create a custom fee transaction TX5 spending CB1 to UTX1 with fee per gram 16
    When I submit transaction TX1 to BN1
    When I submit transaction TX2 to BN1
    When I submit transaction TX3 to BN1
    When I submit transaction TX4 to BN1
    When I submit transaction TX5 to BN1
    Then I wait until base node BN1 has 5 unconfirmed transactions in its mempool

  Scenario: Mempool unconfirmed transaction to mined transaction
    Given I have 1 seed nodes
    When I have a base node BN1 connected to all seed nodes
    When I mine a block on BN1 with coinbase CB1
    When I mine 2 blocks on BN1
    When I create a custom fee transaction TX1 spending CB1 to UTX1 with fee per gram 16
    When I submit transaction TX1 to BN1
    Then I wait until base node BN1 has 1 unconfirmed transactions in its mempool
    When I mine 1 blocks on BN1
    Then I wait until base node BN1 has 0 unconfirmed transactions in its mempool

  Scenario: Mempool should not accept locked transactions
    Given I have 1 seed nodes
    When I have a base node BN1 connected to all seed nodes
    When I mine a block on BN1 with coinbase CB1
    When I mine 2 blocks on BN1
    When I create a custom locked transaction TX1 spending CB1 to UTX1 with lockheight 5
    When I submit transaction TX1 to BN1 and it does not succeed
    Then BN1 has TX1 in NOT_STORED state
    When I mine 4 blocks on BN1
    When I submit transaction TX1 to BN1
