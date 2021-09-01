@mempool
Feature: Mempool

  Scenario: Transactions are propagated through a network
    #
    # The randomness of the TX1 propagation can result in this test not passing.
    # The probability of not passing (at least 2 nodes are not aware of TX1) is ~0.01%.
    #
    Given I have 8 seed nodes
    And I have a base node SENDER connected to all seed nodes
    And I have 8 base nodes connected to all seed nodes
    When I mine a block on SENDER with coinbase CB1
    When I mine 2 blocks on SENDER
    When I create a transaction TX1 spending CB1 to UTX1
    When I submit transaction TX1 to SENDER
    Then SENDER has TX1 in MEMPOOL state
    Then TX1 is in the MEMPOOL of all nodes, where 1% can fail


  Scenario: Transactions are synced
    Given I have 2 seed nodes
    And I have a base node SENDER connected to all seed nodes
    And I have 2 base nodes connected to all seed nodes
    When I mine a block on SENDER with coinbase CB1
    When I mine 2 blocks on SENDER
    When I create a transaction TX1 spending CB1 to UTX1
    When I submit transaction TX1 to SENDER
    Then SENDER has TX1 in MEMPOOL state
    Then TX1 is in the MEMPOOL of all nodes
    Given I have a base node NODE1 connected to all seed nodes
    Then NODE1 has TX1 in MEMPOOL state
    When I mine 1 blocks on SENDER
    Then SENDER has TX1 in MINED state
    Then TX1 is in the MINED of all nodes

  @flaky
  Scenario: Clear out mempool
    Given I have 1 seed nodes
    And I have a base node SENDER connected to all seed nodes
    When I mine a block on SENDER with coinbase CB1
    When I mine a block on SENDER with coinbase CB2
    When I mine a block on SENDER with coinbase CB3
    When I mine 4 blocks on SENDER
    When I create a custom fee transaction TX1 spending CB1 to UTX1 with fee 80
    When I create a custom fee transaction TX2 spending CB2 to UTX2 with fee 100
    When I create a custom fee transaction TX3 spending CB3 to UTX3 with fee 90
    When I submit transaction TX1 to SENDER
    When I submit transaction TX2 to SENDER
    When I submit transaction TX3 to SENDER
    Then SENDER has TX1 in MEMPOOL state
    Then SENDER has TX2 in MEMPOOL state
    Then SENDER has TX3 in MEMPOOL state
    When I mine 1 custom weight blocks on SENDER with weight 17
    Then SENDER has TX1 in MEMPOOL state
    Then SENDER has TX2 in MINED state
    Then SENDER has TX3 in MEMPOOL state
    When I mine 1 custom weight blocks on SENDER with weight 17
    Then SENDER has TX1 in MEMPOOL state
    Then SENDER has TX2 in MINED state
    Then SENDER has TX3 in MINED state


  @critical @broken
  Scenario: Double spend
    Given I have 1 seed nodes
    And I have a base node SENDER connected to all seed nodes
    When I mine a block on SENDER with coinbase CB1
    When I mine 4 blocks on SENDER
    When I create a custom fee transaction TX1 spending CB1 to UTX1 with fee 80
    When I create a custom fee transaction TX2 spending CB1 to UTX2 with fee 100
    When I submit transaction TX1 to SENDER
    When I submit transaction TX2 to SENDER
    Then SENDER has TX1 in MEMPOOL state
    Then SENDER has TX2 in MEMPOOL state
    When I mine 1 blocks on SENDER
    Then SENDER has TX1 in NOT_STORED state
    Then SENDER has TX2 in MINED state
    When I mine 1 blocks on SENDER
    Then SENDER has TX1 in NOT_STORED state
    Then SENDER has TX2 in MINED state

  @critical
  Scenario: Mempool clearing out invalid transactions after a reorg
    #
    # Chain 1:
    #   Collects 7 coinbases into one wallet, send 7 transactions
    #   Stronger chain
    #
    Given I do not expect all automated transactions to succeed
    Given I have a seed node SEED_A
    And I have a base node NODE_A1 connected to seed SEED_A
    And I have wallet WALLET_A1 connected to seed node SEED_A
    And I have wallet WALLET_A2 connected to seed node SEED_A
    And I have mining node MINER_A1 connected to base node SEED_A and wallet WALLET_A1
    When I wait 5 seconds
    When mining node MINER_A1 mines 7 blocks with min difficulty 200 and max difficulty 100000
    Then node SEED_A is at height 7
    Then node NODE_A1 is at height 7
    When I mine 3 blocks on SEED_A
    Then wallet WALLET_A1 detects at least 7 coinbase transactions as Mined_Confirmed
    Then node SEED_A is at height 10
    Then node NODE_A1 is at height 10
    And I multi-send 7 transactions of 1000000 uT from wallet WALLET_A1 to wallet WALLET_A2 at fee 100
    Then wallet WALLET_A1 detects all transactions are at least Broadcast
    When I wait 1 seconds
    #
    # Chain 2:
    #   Collects 7 coinbases into one wallet, send 7 transactions
    #   Weaker chain
    #
    And I have a seed node SEED_B
    And I have a base node NODE_B1 connected to seed SEED_B
    And I have wallet WALLET_B1 connected to seed node SEED_B
    And I have wallet WALLET_B2 connected to seed node SEED_B
    And I have mining node MINER_B1 connected to base node SEED_B and wallet WALLET_B1
    When I wait 5 seconds
    When mining node MINER_B1 mines 7 blocks with min difficulty 1 and max difficulty 100
    Then node SEED_B is at height 7
    Then node NODE_B1 is at height 7
    When I mine 5 blocks on SEED_B
    Then wallet WALLET_B1 detects at least 7 coinbase transactions as Mined_Confirmed
    Then node SEED_B is at height 12
    Then node NODE_B1 is at height 12
    And I multi-send 7 transactions of 1000000 uT from wallet WALLET_B1 to wallet WALLET_B2 at fee 100
    Then wallet WALLET_B1 detects all transactions are at least Broadcast
    When I wait 1 seconds
    #
    # Connect Chain 1 and 2 in stages
    #    New node connects to weaker chain, receives all broadcast (not mined) transactions into mempool
    #    New node connects to stronger chain, then reorgs its complete chain
    #    New node mines blocks; no invalid inputs from the weaker chain should be used in the block template
    #
    And I have a base node NODE_C connected to seed SEED_B
    Then node NODE_C is at height 12
    # Wait for the reorg to filter through
    And I connect node SEED_A to node NODE_C and wait 30 seconds
    Then all nodes are at height 10
    When I mine 6 blocks on NODE_C
    Then all nodes are at height 16

  @critical
  Scenario: Zero-conf transactions
    Given I have 1 seed nodes
    And I have a base node SENDER connected to all seed nodes
    When I mine a block on SENDER with coinbase CB1
    When I mine a block on SENDER with coinbase CB2
    When I mine 4 blocks on SENDER
    When I create a custom fee transaction TX01 spending CB1 to UTX01 with fee 100
    When I create a custom fee transaction TX02 spending UTX01 to UTX02 with fee 100
    When I create a custom fee transaction TX03 spending UTX02 to UTX03 with fee 100
    When I create a custom fee transaction TX11 spending CB2 to UTX11 with fee 100
    When I create a custom fee transaction TX12 spending UTX11 to UTX12 with fee 100
    When I create a custom fee transaction TX13 spending UTX12 to UTX13 with fee 100
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

    @critical
    Scenario: Mempool unconfirmed transactions
      Given I have 1 seed nodes
      And I have a base node BN1 connected to all seed nodes
      When I mine a block on BN1 with coinbase CB1
      When I mine 5 blocks on BN1
      When I create a custom fee transaction TX1 spending CB1 to UTX1 with fee 80
      When I create a custom fee transaction TX2 spending CB1 to UTX1 with fee 80
      When I create a custom fee transaction TX3 spending CB1 to UTX1 with fee 80
      When I create a custom fee transaction TX4 spending CB1 to UTX1 with fee 80
      When I create a custom fee transaction TX5 spending CB1 to UTX1 with fee 80
      When I submit transaction TX1 to BN1
      When I submit transaction TX2 to BN1
      When I submit transaction TX3 to BN1
      When I submit transaction TX4 to BN1
      When I submit transaction TX5 to BN1
      Then I wait until base node BN1 has 5 unconfirmed transactions in its mempool

  @critical
  Scenario: Mempool unconfirmed transaction to mined transaction
    Given I have 1 seed nodes
    And I have a base node BN1 connected to all seed nodes
    When I mine a block on BN1 with coinbase CB1
    When I mine 2 blocks on BN1
    When I create a custom fee transaction TX1 spending CB1 to UTX1 with fee 80
    When I submit transaction TX1 to BN1
    Then I wait until base node BN1 has 1 unconfirmed transactions in its mempool
    When I mine 1 blocks on BN1
    Then I wait until base node BN1 has 0 unconfirmed transactions in its mempool
