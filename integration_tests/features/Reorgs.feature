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
    And I stop NODE1
    When I mine 19 blocks on NODE2
    And node NODE2 is at height 20
    And I stop NODE2
    And I start NODE1
    When I mine 3 blocks on NODE1
    When I create a transaction TX1 spending CB1 to UTX1
    When I submit transaction TX1 to NODE1
    Then NODE1 has TX1 in MEMPOOL state
    When I mine 6 blocks on NODE1
    Given I have a pruned node PNODE1 connected to node NODE1 with pruning horizon set to 5
    Then node PNODE1 is at height 10
    When I start NODE2
    Then all nodes are at height 20
    # Because TX1 should have been re_orged out we should be able to spend CB1 again
    When I create a transaction TX2 spending CB1 to UTX2
    When I submit transaction TX2 to PNODE1
    Then PNODE1 has TX2 in MEMPOOL state

Scenario Outline: Massive multiple reorg
        #
        # Chain 1a:
        #   Mine X1 blocks (orphan_storage_capacity default set to 10)
        #
    Given I have a seed node SEED_A1
        # Add multiple base nodes to ensure more robust comms
    And I have a base node NODE_A1 connected to seed SEED_A1
    And I have a base node NODE_A2 connected to seed SEED_A1
    When I mine <X1> blocks on SEED_A1
    Then node NODE_A1 is at height <X1>
    Then node NODE_A2 is at height <X1>
        #
        # Chain 1b:
        #   Mine Y1 blocks (orphan_storage_capacity default set to 10)
        #
    And I have a seed node SEED_A2
        # Add multiple base nodes to ensure more robust comms
    And I have a base node NODE_A3 connected to seed SEED_A2
    And I have a base node NODE_A4 connected to seed SEED_A2
    When I mine <Y1> blocks on SEED_A2
    Then node NODE_A3 is at height <Y1>
    Then node NODE_A4 is at height <Y1>
        #
        # Connect Chain 1a and 1b
        #
    And I connect node NODE_A1 to node NODE_A3 and wait 1 seconds
    And I connect node NODE_A2 to node NODE_A4 and wait 1 seconds
    And I connect node SEED_A1 to node SEED_A2 and wait <SYNC_TIME> seconds
    Then node SEED_A1 is at the same height as node SEED_A2
    When I mine 10 blocks on SEED_A1
    Then node SEED_A2 is at the same height as node SEED_A1
    Then node NODE_A1 is at the same height as node SEED_A1
    Then node NODE_A2 is at the same height as node SEED_A1
    Then node NODE_A3 is at the same height as node SEED_A1
    Then node NODE_A4 is at the same height as node SEED_A1
        #
        # Chain 2a:
        #   Mine X2 blocks (orphan_storage_capacity default set to 10)
        #
    Given I have a seed node SEED_B1
        # Add multiple base nodes to ensure more robust comms
    And I have a base node NODE_B1 connected to seed SEED_B1
    And I have a base node NODE_B2 connected to seed SEED_B1
    When I mine <X2> blocks on SEED_B1
    Then node NODE_B1 is at height <X2>
    Then node NODE_B2 is at height <X2>
        #
        # Chain 2b:
        #   Mine Y2 blocks (orphan_storage_capacity default set to 10)
        #
    And I have a seed node SEED_B2
        # Add multiple base nodes to ensure more robust comms
    And I have a base node NODE_B3 connected to seed SEED_B2
    And I have a base node NODE_B4 connected to seed SEED_B2
    When I mine <Y2> blocks on SEED_B2
    Then node NODE_B3 is at height <Y2>
    Then node NODE_B4 is at height <Y2>
        #
        # Connect Chain 2a and 2b
        #
    And I connect node NODE_B1 to node NODE_B3 and wait 1 seconds
    And I connect node NODE_B2 to node NODE_B4 and wait 1 seconds
    And I connect node SEED_B1 to node SEED_B2 and wait <SYNC_TIME> seconds
    Then node SEED_B1 is at the same height as node SEED_B2
    When I mine 10 blocks on SEED_B1
    Then node SEED_B2 is at the same height as node SEED_B1
    Then node NODE_B1 is at the same height as node SEED_B1
    Then node NODE_B2 is at the same height as node SEED_B1
    Then node NODE_B3 is at the same height as node SEED_B1
    Then node NODE_B4 is at the same height as node SEED_B1
        #
        # Connect Chain 1 and 2
        #
    And I connect node NODE_A1 to node NODE_B1 and wait 1 seconds
    And I connect node NODE_A3 to node NODE_B3 and wait 1 seconds
    And I connect node SEED_A1 to node SEED_B1 and wait <SYNC_TIME> seconds
    Then node SEED_A1 is at the same height as node SEED_B1
    When I mine 10 blocks on SEED_A1
    Then all nodes are at the same height as node SEED_A1
    @critical
    Examples:
        | X1     | Y1     | X2    | Y2   | SYNC_TIME |
        | 5      | 10     | 15    | 20   | 20        |

    @long-running
    Examples:
        | X1     | Y1     | X2    | Y2   | SYNC_TIME |
        | 100    | 125    | 150   | 175  | 30        |

    @long-running @to-be-fixed-currently-failing
    Examples:
        | X1     | Y1     | X2    | Y2   | SYNC_TIME |
        | 500    | 550    | 600   | 650  | 60        |

Scenario Outline: Massive reorg simple case
        #
        # Chain 1a:
        #   Mine X1 blocks (orphan_storage_capacity default set to 10)
        #
    Given I have a seed node SEED_A1
        # Add multiple base nodes to ensure more robust comms
    And I have a base node NODE_A1 connected to seed SEED_A1
    And I have a base node NODE_A2 connected to seed SEED_A1
    When I mine <X1> blocks on SEED_A1
    Then node NODE_A1 is at height <X1>
    Then node NODE_A2 is at height <X1>
        #
        # Chain 1b:
        #   Mine Y1 blocks (orphan_storage_capacity default set to 10)
        #
    And I have a seed node SEED_A2
        # Add multiple base nodes to ensure more robust comms
    And I have a base node NODE_A3 connected to seed SEED_A2
    And I have a base node NODE_A4 connected to seed SEED_A2
    When I mine <Y1> blocks on SEED_A2
    Then node NODE_A3 is at height <Y1>
    Then node NODE_A4 is at height <Y1>
        #
        # Connect Chain 1a and 1b
        #
#    And I connect node NODE_A1 to node NODE_A3 and wait 1 seconds
#    And I connect node NODE_A2 to node NODE_A4 and wait 1 seconds
        # Note: If the above two lines are included in the test, the 500+ case sometimes
        #       passes as well.
    And I connect node SEED_A1 to node SEED_A2 and wait <SYNC_TIME> seconds
    Then node SEED_A1 is at the same height as node SEED_A2
    When I mine 10 blocks on SEED_A1
    Then all nodes are at the same height as node SEED_A1
    @critical
    Examples:
        | X1     | Y1     | SYNC_TIME |
        | 5      | 10     | 20        |

    @long-running
    Examples:
        | X1     | Y1     | SYNC_TIME |
        | 100    | 125    | 30        |

    @long-running @to-be-fixed-currently-failing
    Examples:
        | X1     | Y1     | SYNC_TIME |
        | 500    | 550    | 60        |
