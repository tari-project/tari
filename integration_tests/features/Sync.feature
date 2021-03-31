Feature: Block Sync

  Scenario Outline: Initial block sync
    Given I have <NumSeeds> seed nodes
    And I have a base node MINER connected to all seed nodes
    When I mine <NumBlocks> blocks on MINER
    Given I have <NumSyncers> base nodes connected to all seed nodes
    # All nodes should sync to tip
    Then all nodes are at height <NumBlocks>
    @critical
    Examples:
      | NumSeeds | NumBlocks | NumSyncers |
      | 1        | 1         | 1          |
      | 1        | 10        | 2          |
      | 1        | 50        | 4          |
      | 8        | 40        | 8          |

  @critical
  Scenario: Simple block sync
    Given I have 1 seed nodes
    And I have a base node MINER connected to all seed nodes
    When I mine 20 blocks on MINER
    Given I have 2 base nodes connected to all seed nodes
    # All nodes should sync to tip
    Then all nodes are at height 20

  Scenario: When a new node joins the network, it should receive all peers.
    Given I have 10 seed nodes
    And I have a base node NODE1 connected to all seed nodes
    Then NODE1 should have 10 peers
    Given I have a base node NODE2 connected to node NODE1
    Then NODE1 should have 11 peers
    Then NODE2 should have 11 peers

  @critical @reorg
  Scenario: Full block sync with small reorg
    Given I have a base node NODE1 connected to all seed nodes
    Given I have a base node NODE2 connected to node NODE1
    When I mine 5 blocks on NODE1
    Then all nodes are at height 5
    Given I stop NODE2
    Then I mine 5 blocks on NODE1
    Given I stop NODE1
    And I start NODE2
    Then I mine 7 blocks on NODE2
    When I start NODE1
    Then all nodes are on the same chain at height 12

  @critical @reorg @long-running
  Scenario: Full block sync with large reorg
    Given I have a base node NODE1 connected to all seed nodes
    Given I have a base node NODE2 connected to node NODE1
    When I mine 5 blocks on NODE1
    Then all nodes are at height 5
    Given I stop NODE2
    Then I mine 1001 blocks on NODE1
    Given I stop NODE1
    And I start NODE2
    Then I mine 1500 blocks on NODE2
    When I start NODE1
    Then all nodes are on the same chain at height 1505

  @critical
  Scenario: Pruned mode
    # TODO: Merge steps into single lines
    Given I have a base node NODE1 connected to all seed nodes
    When I mine a block on NODE1 with coinbase CB1
    When I mine a block on NODE1 with coinbase CB2
    When I mine a block on NODE1 with coinbase CB3
    When I mine a block on NODE1 with coinbase CB4
    When I mine a block on NODE1 with coinbase CB5
    Then all nodes are at height 5
    When I spend outputs CB1 via NODE1
#      When I spend outputs CB2 via NODE1
#      When I spend outputs CB3 via NODE1
    And I mine 3 blocks on NODE1
    Given I have a pruned node PNODE2 connected to node NODE1 with pruning horizon set to 5
    Then all nodes are at height 8
      # Spend txns so that they are pruned when tip moves
#      When I spend outputs CB4 via PNODE2
#      When I spend outputs CB5 via PNODE2
    When I mine 15 blocks on PNODE2
    Then all nodes are at height 23


  @critical @reorg
  Scenario: Pruned mode reorg
    Given I have a base node NODE1 connected to all seed nodes
    When I mine 5 blocks on NODE1
    Then all nodes are at height 5
    Given I have a pruned node PNODE2 connected to node NODE1 with pruning horizon set to 5
    When I mine 4 blocks on NODE1
    Then all nodes are at height 9
    When I mine 5 blocks on PNODE2
    Then all nodes are at height 14
    When I stop PNODE2
    And I mine 3 blocks on NODE1
    And I stop NODE1
    And I start PNODE2
    And I mine 6 blocks on PNODE2
    When I start NODE1
    Then all nodes are at height 20
