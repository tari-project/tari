@propagation @base-node
Feature: Block Propagation

  Scenario Outline: Blocks are propagated through the network
    Given I have <NumSeeds> seed nodes
    And I have <NumNonSeeds> base nodes connected to all seed nodes
    And I have a SHA3 miner MINER connected to all seed nodes
    And mining node MINER mines <NumBlocks> blocks
    Then all nodes are at height <NumBlocks>

    Examples:
      | NumSeeds | NumNonSeeds | NumBlocks |
      | 1        | 1           | 5         |

    @long-running
    Examples:
      | NumSeeds | NumNonSeeds | NumBlocks |
      | 1        | 10          | 5         |
      | 4        | 10          | 5         |
      | 8        | 40          | 10        |

  @critical
  Scenario: Simple propagation
    Given I have 2 seed nodes
    And I have 4 base nodes connected to all seed nodes
    And I have a SHA3 miner MINER connected to all seed nodes
    And mining node MINER mines 5 blocks
    Then node MINER is at height 5
    Then all nodes are at height 5

  @critical
  Scenario: Duplicate block is rejected
    Given I have 1 seed nodes
    And I have a base node MINER connected to all seed nodes
    When I mine but do not submit a block BLOCKA on MINER
    When I submit block BLOCKA to MINER
    Then all nodes are at height 1
    When I submit block BLOCKA to MINER
    # TODO: this step is not implemented.
    Then I receive an error containing 'Block exists'
    And all nodes are at height 1
    # Check that the base node continues to accept blocks
    When I mine 1 blocks on MINER
    Then all nodes are at height 2

  Scenario: Submit orphan
    Given I have 1 seed nodes
    And I have a base node MINER connected to all seed nodes
    When I mine but do not submit a block BLOCKA on MINER
    # TODO: Step is missing, so I commented it out
    # And I update the parent of block BLOCKA to be an orphan
    When I submit block BLOCKA to MINER
    Then I receive an error containing 'Orphan block'
    Then all nodes are at height 1
    # Do it twice to be sure
    When I submit block BLOCKA to MINER
    Then I receive an error containing 'Orphan block'
    And all nodes are at height 1

  @non-sync-propagation
  Scenario: Nodes should never switch to block sync but stay synced via propagation
    Given I have 1 seed nodes
    Given I have a SHA3 miner MINER connected to all seed nodes
    And I have a lagging delayed node LAG1 connected to node MINER with blocks_behind_before_considered_lagging 10000
    Given I have a lagging delayed node LAG2 connected to node MINER with blocks_behind_before_considered_lagging 10000
    # Wait for node to so start and get into listening mode
    Then node LAG1 has reached initial sync
    Then node LAG2 has reached initial sync
    When mining node MINER mines 5 blocks
    Then all nodes are at height 5
    Given mining node MINER mines 15 blocks
    Then all nodes are at height 20

  Scenario: Node should lag for while before syncing
    Given I have 1 seed nodes
    Given I have a SHA3 miner MINER connected to all seed nodes
    And I have a lagging delayed node LAG1 connected to node MINER with blocks_behind_before_considered_lagging 6
    Given mining node MINER mines 1 blocks
    Then all nodes are at height 1
    When I stop node LAG1
    And mining node MINER mines 5 blocks
    Then node MINER is at height 6
    When I start base node LAG1
    # Wait for node to so start and get into listening mode
    Then node LAG1 has reached initial sync
    #node was shutdown, so it never received the propagation messages
    Then node LAG1 is at height 1
    Given mining node MINER mines 1 blocks
    Then node MINER is at height 7
    Then all nodes are at height 7

  @critical
  Scenario: Pruned node should prune outputs
    Given I have 1 seed nodes
    And I have a base node SENDER connected to all seed nodes
    Given I have a pruned node PNODE1 connected to node SENDER with pruning horizon set to 5
    When I mine a block on SENDER with coinbase CB1
    When I mine 2 blocks on SENDER
    When I create a transaction TX1 spending CB1 to UTX1
    When I submit transaction TX1 to SENDER
    When I mine 1 blocks on SENDER
    Then TX1 is in the MINED of all nodes
    When I mine 17 blocks on SENDER
    Then all nodes are on the same chain at height 21
    Then node PNODE1 has a pruned height of 15
