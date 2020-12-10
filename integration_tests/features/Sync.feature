Feature: Block Sync

  Scenario Outline: Initial block sync
    Given I have <NumSeeds> seed nodes
    And I have a base node MINER connected to all seed nodes
    When I mine <NumBlocks> blocks on MINER
    Given I have <NumSyncers> base nodes connected to all seed nodes
    # All nodes should sync to tip
    Then all nodes are at height <NumBlocks>
    Examples:
      | NumSeeds |  NumBlocks | NumSyncers |
      | 1        | 1           | 1         |
      | 1        | 10          | 2         |
      | 1        | 50          | 4          |
      | 8        | 40          | 8         |

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
