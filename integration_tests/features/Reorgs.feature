@reorg
Feature: Reorgs

  @critical
  Scenario: Simple reorg to stronger chain
    Given I have a seed node SA
    And I have a base node B connected to seed SA
    And I have wallet WB connected to base node B
    And I have mining node BM connected to base node B and wallet WB
    When I stop SA
    And Mining node BM mines 3 blocks on B
    Given I have a base node C connected to seed SA
    And I have wallet WC connected to base node C
    And I have mining node CM connected to base node C and wallet WC
    And Mining node CM mines 15 blocks on C
    Then node B is at height 3
    And node C is at height 15
    When I start SA
    Then node B is at height 15
    And node C is at height 15
    And node SA is at height 15

  @critical
  Scenario: Node rolls back reorg on invalid block
    Given I have a seed node SA
    And I have a base node B connected to seed SA
    When I mine 5 blocks on B
    Then node B is at height 5
    When I save the tip on B as BTip1
  # Try a few times to insert an invalid block
    And I mine a block on B at height 3 with an invalid MMR
    And I mine a block on B at height 3 with an invalid MMR
    And I mine a block on B at height 4 with an invalid MMR
    And I mine a block on B at height 4 with an invalid MMR
    Then node B is at tip BTip1

@critical @reorg @ignore
  Scenario: Pruned mode reorg past horizon
    Given I have a base node NODE1 connected to all seed nodes
    When I mine a block on NODE1 with coinbase CB1
    Given I have a base node NODE2 connected to node NODE1
    When I mine but don't submit a block BLOCK2 on NODE2
    When I mine but don't submit a block BLOCK3 on NODE2
    When I mine but don't submit a block BLOCK4 on NODE2
    When I mine but don't submit a block BLOCK5 on NODE2
    When I mine but don't submit a block BLOCK6 on NODE2
    When I mine but don't submit a block BLOCK7 on NODE2
    When I mine but don't submit a block BLOCK8 on NODE2
    When I mine but don't submit a block BLOCK9 on NODE2
    When I mine but don't submit a block BLOCK10 on NODE2
    When I mine but don't submit a block BLOCK11 on NODE2
    When I mine but don't submit a block BLOCK12 on NODE2
    When I mine but don't submit a block BLOCK13 on NODE2
    When I mine but don't submit a block BLOCK14 on NODE2
    When I mine but don't submit a block BLOCK15 on NODE2
    When I mine but don't submit a block BLOCK16 on NODE2
    When I mine but don't submit a block BLOCK17 on NODE2
    When I mine but don't submit a block BLOCK18 on NODE2
    When I mine but don't submit a block BLOCK19 on NODE2
    When I mine but don't submit a block BLOCK20 on NODE2
    And I stop NODE2
    When I mine 3 blocks on NODE1
    When I create a transaction TX1 spending CB1 to UTX1
    When I submit transaction TX1 to NODE1
    Then NODE1 has TX1 in MEMPOOL state
    When I mine 6 blocks on NODE1
    Given I have a pruned node PNODE1 connected to node NODE1 with pruning horizon set to 5
    Then node PNODE1 is at height 10
    When I start NODE2
    When I submit block BLOCK2 to NODE2
    When I submit block BLOCK3 to NODE2
    When I submit block BLOCK4 to NODE2
    When I submit block BLOCK5 to NODE2
    When I submit block BLOCK6 to NODE2
    When I submit block BLOCK7 to NODE2
    When I submit block BLOCK8 to NODE2
    When I submit block BLOCK9 to NODE2
    When I submit block BLOCK10 to NODE2
    When I submit block BLOCK11 to NODE2
    When I submit block BLOCK12 to NODE2
    When I submit block BLOCK13 to NODE2
    When I submit block BLOCK14 to NODE2
    When I submit block BLOCK15 to NODE2
    When I submit block BLOCK16 to NODE2
    When I submit block BLOCK17 to NODE2
    When I submit block BLOCK18 to NODE2
    When I submit block BLOCK19 to NODE2
    When I submit block BLOCK20 to NODE2
    Then all nodes are at height 20
    # Because TX1 should have been re_orged out we should be able to spend CB1 again
    When I create a transaction TX2 spending CB1 to UTX2
    When I submit transaction TX2 to PNODE1
    Then PNODE1 has TX2 in MEMPOOL state
