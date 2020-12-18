Feature: Reorgs

  @critical
  Scenario: Simple reorg to stronger chain
    Given I have a seed node SA
    And I have a base node B connected to seed SA
    When I stop SA
    And I mine 3 blocks on B
    Given I have a base node C connected to seed SA
    And I mine 15 blocks on C
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



