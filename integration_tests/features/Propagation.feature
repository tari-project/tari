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
