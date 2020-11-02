Feature: Reorgs

Scenario: Simple reorg to stronger chain
  Given I have a seed node SA
  And I have a base node B connected to SA
  And I have a base node C connected to SA
  When I stop SA
  And I mine 3 blocks on B
  And I mine 5 blocks on C
  Then node B is at height 3
  And node C is at height 5
  When I start SA
  Then node B is at height 5
  And node C is at height 5
  And node SA is at height 5

Scenario: Node rolls back reorg on invalid block
  Given I have a seed node SA
  And I have a base node B connected to SA
  When I mine 5 blocks on B
  Then node B is at height 5
  When I save the tip on B as BTip1
  When I mine a block on B based on height 3
  And I mine a block on B based on height 4
  Then node B is at tip BTip1
