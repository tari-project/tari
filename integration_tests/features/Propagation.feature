@propagation
Feature: Block Propagation

  Scenario Outline: Blocks are propagated through the network
    Given I have <NumSeeds> seed nodes
    And I have <NumNonSeeds> base nodes connected to all seed nodes
    And I have a SHA3 miner MINER connected to all seed nodes
    And mining node MINER mines <NumBlocks> blocks
    Then all nodes are at height <NumBlocks>
    @critical
    Examples:
      | NumSeeds | NumNonSeeds | NumBlocks |
      | 1        | 1           | 5         |

    Examples:
      | NumSeeds | NumNonSeeds | NumBlocks |
      | 1        | 10          | 5         |
      | 4        | 10          | 5         |

    @long-running
    Examples:
      | NumSeeds | NumNonSeeds | NumBlocks |
      | 8        | 40          | 10        |

  @critical
  Scenario: Simple propagation
    Given I have 2 seed nodes
    And I have 4 base nodes connected to all seed nodes
    And I have a SHA3 miner MINER connected to all seed nodes
    And mining node MINER mines 5 blocks
    Then node MINER is at height 5
    Then all nodes are at height 5

  Scenario: Duplicate block is rejected
    Given I have 1 seed nodes
    And I have a base node MINER connected to all seed nodes
    When I mine but do not submit a block BLOCKA on MINER
    When I submit block BLOCKA to MINER
    Then all nodes are at height 1
    When I submit block BLOCKA to MINER
    Then I receive an error containing 'Block exists'
    And all nodes are at height 1
    # Check that the base node continues to accept blocks
    When I mine 1 blocks on MINER
    Then all nodes are at height 2

  Scenario: Submit orphan
    Given I have 1 seed nodes
    And I have a base node MINER connected to all seed nodes
    When I mine but don't submit a block BLOCKA on MINER
    And I update the parent of block BLOCKA to be an orphan
    When I submit block BLOCKA to MINER
    Then I receive an error containing 'Orphan block'
    Then all nodes are at height 1
    # Do it twice to be sure
    When I submit block BLOCKA to MINER
    Then I receive an error containing 'Orphan block'
    And all nodes are at height 1

  @non-sync-propagation @long-running
  Scenario: Nodes should never switch to block sync but stay synced via propagation
    Given I have 1 seed nodes
    Given I have a SHA3 miner MINER connected to all seed nodes
    And I have a lagging delayed node LAG1 connected to node MINER with blocks_behind_before_considered_lagging 10000
    Given I have a lagging delayed node LAG2 connected to node MINER with blocks_behind_before_considered_lagging 10000
    # Wait for node to so start and get into listening mode
    When I wait 100 seconds
    When mining node MINER mines 5 blocks
    Then all nodes are at height 5
    Given mining node MINER mines 15 blocks
    Then all nodes are at height 20

  @long-running
  Scenario: Node should lag for while before syncing
    Given I have 1 seed nodes
    Given I have a SHA3 miner MINER connected to all seed nodes
    And I have a lagging delayed node LAG1 connected to node MINER with blocks_behind_before_considered_lagging 6
    Given mining node MINER mines 1 blocks
    When I wait 100 seconds
    When I stop LAG1
    When I wait 10 seconds
    And mining node MINER mines 5 blocks
    When I wait 100 seconds
    When I start LAG1
    # Wait for node to so start and get into listening mode
    When I wait 100 seconds
    Then node MINER is at height 6
    #node was shutdown, so it never received the propagation messages
    Then node LAG1 is at height 1
    Given mining node MINER mines 1 blocks
    Then node MINER is at height 7
    When I wait 20 seconds
    Then all nodes are at height 7