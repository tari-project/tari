@propagation
Feature: Mempool

 
  Scenario: Transactions are propagated through a network
    Given I have 10 seed nodes
    And I have a base node SENDER connected to all seed nodes
    And I have 10 base nodes connected to all seed nodes
    When I mine a block on SENDER with coinbase CB1
    When I mine 2 blocks on SENDER
    When I create a transaction TX1 spending CB1 to UTX1
    When I submit transaction TX1 to SENDER
    Then SENDER has TX1 in MEMPOOL state
    Then TX1 is in the MEMPOOL of all nodes


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
