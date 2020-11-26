@propagation
Feature: Block Propagation

  Scenario Outline: Blocks are propagated through the network
    Given I have <NumSeeds> seed nodes
    And I have <NumNonSeeds> base nodes connected to all seed nodes
    And I have a base node MINER connected to all seed nodes
    When I mine <NumBlocks> blocks on MINER
    Then all nodes are at height <NumBlocks>
    Examples:
      | NumSeeds | NumNonSeeds | NumBlocks |
      | 1        | 1           | 5         |
      | 1        | 10          | 5         |
      | 4        | 10          | 5         |
      | 8        | 40          | 10        |

  @critical
  Scenario: Simple propagation
    Given I have 2 seed nodes
    And I have 4 base nodes connected to all seed nodes
    And I have a base node MINER connected to all seed nodes
    When I mine 5 blocks on MINER
    Then all nodes are at height 5

  Scenario: Duplicate block is rejected
    Given I have 1 seed nodes
    And I have a base node MINER connected to all seed nodes
    When I mine but don't submit a block BLOCKA on MINER
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
    And I update block BLOCKA's parent to be an orphan
    When I submit block BLOCKA to MINER
    Then I receive an error containing 'Orphan block'
    Then all nodes are at height 1
    # Do it twice to be sure
    When I submit block BLOCKA to MINER
    Then I receive an error containing 'Orphan block'
    And all nodes are at height 1

