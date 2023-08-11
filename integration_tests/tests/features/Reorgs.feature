# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@reorg @base-node
Feature: Reorgs

  @critical
  Scenario: Simple reorg to stronger chain
    # Chain 1
    #     Note: Use more than 1 base node to speed up the test
    Given I have a seed node SEED_B
    When I have a base node B connected to seed SEED_B
    When I have wallet WB connected to base node B
    When I have mining node BM connected to base node B and wallet WB
    When mining node BM mines 3 blocks with min difficulty 1 and max difficulty 50
    # Chain 2
    #     Note: Use more than 1 base node to speed up the test
    Given I have a seed node SEED_C
    When I have a base node C connected to seed SEED_C
    When I have wallet WC connected to base node C
    When I have mining node CM connected to base node C and wallet WC
    When mining node CM mines 10 blocks with min difficulty 51 and max difficulty 9999999999
    # Connect chain 1 and 2
    Then node B is at height 3
    Then node C is at height 10
    When I have a base node SA connected to nodes B,C
    Then node SA is at height 10
    Then node B is at height 10
    Then node C is at height 10


  @critical
  Scenario: Simple reorg with burned output
    # Chain 1
    #     Note: Use more than 1 base node to speed up the test
    Given I have a seed node SEED_B
    When I have a base node B connected to seed SEED_B
    When I have wallet WB connected to base node B
    When I have mining node BM connected to base node B and wallet WB
    When mining node BM mines 10 blocks with min difficulty 1 and max difficulty 1

    When I wait for wallet WB to have at least 55000000000 uT
    When I create a burn transaction of 1000000 uT from WB at fee 100
    When mining node BM mines 5 blocks with min difficulty 1 and max difficulty 1

    # Chain 2
    #     Note: Use more than 1 base node to speed up the test
    Given I have a seed node SEED_C
    When I have a base node C connected to seed SEED_C
    When I have wallet WC connected to base node C
    When I have mining node CM connected to base node C and wallet WC
    When mining node CM mines 17 blocks with min difficulty 1 and max difficulty 1

    # Connect chain 1 and 2
    Then node B is at height 15
    Then node C is at height 17
    When I have a base node SA connected to nodes B,C
    Then node SA is at height 17
    Then node B is at height 17
    Then node C is at height 17

  @critical
  Scenario: Node rolls back reorg on invalid block
    Given I have a seed node SA
    When I have a base node B connected to seed SA
    When I mine 5 blocks on B
    Then node B is at height 5
    When I mine but do not submit a block BLOCKA on B
    Then I update block BLOCKA to have an invalid mmr
    When I submit block BLOCKA to B
    Then all nodes are at height 5

  # @reorg @missing-steps
  # Scenario: Pruned mode reorg simple
  #   When I have a base node NODE1 connected to all seed nodes
  #   When I have wallet WALLET1 connected to base node NODE1
  #   When I have mining node MINING1 connected to base node NODE1 and wallet WALLET1
  #   When mining node MINING1 mines 5 blocks with min difficulty 1 and max difficulty 20
  #   Then all nodes are at height 5
  #   When I have a pruned node PNODE2 connected to node NODE1 with pruning horizon set to 5
  #   When I have wallet WALLET2 connected to base node PNODE2
  #   When I have mining node MINING2 connected to base node PNODE2 and wallet WALLET2
  #   When mining node MINING1 mines 4 blocks with min difficulty 1 and max difficulty 20
  #   Then all nodes are at height 9
  #   When mining node MINING2 mines 5 blocks with min difficulty 1 and max difficulty 20
  #   Then all nodes are at height 14
  #   When I stop node PNODE2
  #   When mining node MINING1 mines 3 blocks with min difficulty 1 and max difficulty 20
  #   Then node NODE1 is at height 17
  #   When I stop node NODE1
  #   When I start base node PNODE2
  #   When mining node MINING2 mines 6 blocks with min difficulty 2 and max difficulty 1000000
  #   Then node PNODE2 is at height 20
  #   When I start base node NODE1
  #   Then all nodes are at height 20

  @reorg @flaky @missing-steps
  Scenario: Pruned mode reorg past horizon
    When I have a base node NODE1 connected to all seed nodes
    When I have wallet WALLET1 connected to base node NODE1
    When I have mining node MINING1 connected to base node NODE1 and wallet WALLET1
    When I have a base node NODE2 connected to node NODE1
    When I have wallet WALLET2 connected to base node NODE2
    When I have mining node MINING2 connected to base node NODE2 and wallet WALLET2
    When I mine a block on NODE1 with coinbase CB1
    Then all nodes are at height 1
    When I stop node NODE1
    When mining node MINING2 mines 19 blocks with min difficulty 20 and max difficulty 1000000
    Then node NODE2 is at height 20
    When I stop node NODE2
    When I start base node NODE1
    When mining node MINING1 mines 3 blocks with min difficulty 1 and max difficulty 20
    Then node NODE1 is at height 4
    When I create a transaction TX1 spending CB1 to UTX1
    When I submit transaction TX1 to NODE1
    Then NODE1 has TX1 in MEMPOOL state
    When mining node MINING1 mines 6 blocks with min difficulty 1 and max difficulty 20
    Then node NODE1 is at height 10
    Given I have a pruned node PNODE1 connected to node NODE1 with pruning horizon set to 5
    Then node PNODE1 is at height 10
    When I start base node NODE2
    # Here is where it all goes wrong. the restarted node never syncs
    Then all nodes are at height 20
    # Because TX1 should have been re_orged out we should be able to spend CB1 again
    When I create a transaction TX2 spending CB1 to UTX2
    When I submit transaction TX2 to PNODE1
    Then PNODE1 has TX2 in MEMPOOL state

  @reorg @broken
  Scenario: Zero-conf reorg with spending
    When I have a base node NODE1 connected to all seed nodes
    When I have a base node NODE2 connected to node NODE1
    When I mine 14 blocks on NODE1
    When I mine a block on NODE1 with coinbase CB1
    When I mine 4 blocks on NODE1
    When I create a custom fee transaction TX1 spending CB1 to UTX1 with fee 20
    When I create a custom fee transaction TX11 spending UTX1 to UTX11 with fee 20
    When I submit transaction TX1 to NODE1
    When I submit transaction TX11 to NODE1
    When I mine 1 blocks on NODE1
    Then NODE1 has TX1 in MINED state
    And NODE1 has TX11 in MINED state
    And all nodes are at height 20
    And I stop node NODE1
    And node NODE2 is at height 20
    When I mine a block on NODE2 with coinbase CB2
    When I mine 3 blocks on NODE2
    When I create a custom fee transaction TX2 spending CB2 to UTX2 with fee 20
    When I create a custom fee transaction TX21 spending UTX2 to UTX21 with fee 20
    When I submit transaction TX2 to NODE2
    When I submit transaction TX21 to NODE2
    When I mine 1 blocks on NODE2
    Then node NODE2 is at height 25
    And NODE2 has TX2 in MINED state
    And NODE2 has TX21 in MINED state
    And I stop node NODE2
    When I start base node NODE1
    And node NODE1 is at height 20
    When I mine a block on NODE1 with coinbase CB3
    When I mine 3 blocks on NODE1
    When I create a custom fee transaction TX3 spending CB3 to UTX3 with fee 20
    When I create a custom fee transaction TX31 spending UTX3 to UTX31 with fee 20
    When I submit transaction TX3 to NODE1
    When I submit transaction TX31 to NODE1
    When I mine 1 blocks on NODE1
    Then NODE1 has TX3 in MINED state
    And NODE1 has TX31 in MINED state
    And node NODE1 is at height 25
    When I start base node NODE2
    Then all nodes are on the same chain at height 25

  Scenario Outline: Massive multiple reorg
    #
    # Chain 1a:
    #   Mine X1 blocks
    #
    Given I have a seed node SEED_A1
    # Add multiple base nodes to ensure more robust comms
    When I have a base node NODE_A1 connected to seed SEED_A1
    When I have a base node NODE_A2 connected to seed SEED_A1
    When I mine <X1> blocks with difficulty 1 on SEED_A1
    Then all nodes are on the same chain at height <X1>
    #
    # Chain 1b:
    #   Mine Y1 blocks
    #
    When I have a seed node SEED_A2
    # Add multiple base nodes to ensure more robust comms
    When I have a base node NODE_A3 connected to seed SEED_A2
    When I have a base node NODE_A4 connected to seed SEED_A2
    When I mine <Y1> blocks with difficulty 1 on SEED_A2
    Then node NODE_A3 is at height <Y1>
    Then node NODE_A4 is at height <Y1>
    #
    # Connect Chain 1a and 1b
    #
    When I connect node NODE_A1 to node NODE_A3
    When I connect node NODE_A2 to node NODE_A4
    When I connect node SEED_A1 to node SEED_A2
    Then node SEED_A1 is in state LISTENING
    Then node SEED_A2 is in state LISTENING
    Then all nodes are on the same chain at height <Y1>
    #
    # Chain 2a:
    #   Mine X2 blocks
    #
    Given I have a seed node SEED_B1
    # Add multiple base nodes to ensure more robust comms
    When I have a base node NODE_B1 connected to seed SEED_B1
    When I have a base node NODE_B2 connected to seed SEED_B1
    When I mine <X2> blocks with difficulty 1 on SEED_B1
    Then node NODE_B1 is at height <X2>
    Then node NODE_B2 is at height <X2>
    #
    # Chain 2b:
    #   Mine Y2 blocks (orphan_storage_capacity default set to 10)
    #
    When I have a seed node SEED_B2
    # Add multiple base nodes to ensure more robust comms
    When I have a base node NODE_B3 connected to seed SEED_B2
    When I have a base node NODE_B4 connected to seed SEED_B2
    When I mine <Y2> blocks with difficulty 1 on SEED_B2
    Then node NODE_B3 is at height <Y2>
    Then node NODE_B4 is at height <Y2>
    #
    # Connect Chain 2a and 2b
    #
    When I connect node NODE_B1 to node NODE_B3
    When I connect node NODE_B2 to node NODE_B4
    When I connect node SEED_B1 to node SEED_B2
    Then node SEED_B2 is in state LISTENING
    Then node SEED_B1 is in state LISTENING
    Then node SEED_B2 is at height <Y2>
    Then node NODE_B1 is at height <Y2>
    Then node NODE_B2 is at height <Y2>
    Then node NODE_B3 is at height <Y2>
    Then node NODE_B4 is at height <Y2>
    #
    # Connect Chain 1 and 2
    #
    When I connect node NODE_A1 to node NODE_B1
    When I connect node NODE_A3 to node NODE_B3
    When I connect node SEED_A1 to node SEED_B1
    Then all nodes are on the same chain at height <Y2>

    Examples:
      | X1 | Y1 | X2 | Y2 |
      | 5  | 10 | 15 | 20 |

    @long-running
    Examples:
        | X1     | Y1     | X2    | Y2   |
        | 100    | 125    | 150   | 175  |
        | 1010   | 1110   | 1210  | 1310 |

  @reorg @missing-steps
  Scenario: Full block sync with small reorg
    Given I have a base node NODE1
    When I have wallet WALLET1 connected to base node NODE1
    When I have mining node MINER1 connected to base node NODE1 and wallet WALLET1
    # And I have a base node NODE2 connected to node NODE1
    # When I have wallet WALLET2 connected to base node NODE2
    # And I have mining node MINER2 connected to base node NODE2 and wallet WALLET2
    # And mining node MINER1 mines 5 blocks with min difficulty 1 and max difficulty 10
    # Then all nodes are at height 5
    # Given I stop node NODE2
    # And mining node MINER1 mines 5 blocks with min difficulty 1 and max difficulty 1
    # Then node NODE1 is at height 10
    # Given I stop node NODE1
    # And I start base node NODE2
    # And mining node MINER2 mines 7 blocks with min difficulty 2 and max difficulty 100000
    # Then node NODE2 is at height 12
    # When I start base node NODE1
    # Then all nodes are on the same chain at height 12

  @reorg @long-running @missing-steps
  Scenario: Full block sync with large reorg
    Given I have a base node NODE1
    When I have wallet WALLET1 connected to base node NODE1
    When I have mining node MINER1 connected to base node NODE1 and wallet WALLET1
    # And I have a base node NODE2 connected to node NODE1
    # When I have wallet WALLET2 connected to base node NODE2
    # And I have mining node MINER2 connected to base node NODE2 and wallet WALLET2
    # And mining node MINER1 mines 5 blocks with min difficulty 1 and max difficulty 10
    # Then all nodes are at height 5
    # Given I stop node NODE2
    # And mining node MINER1 mines 1001 blocks with min difficulty 1 and max difficulty 10
    # Then node NODE1 is at height 1006
    # Given I stop node NODE1
    # And I start base node NODE2
    # And mining node MINER2 mines 1500 blocks with min difficulty 11 and max difficulty 100000
    # Then node NODE2 is at height 1505
    # When I start base node NODE1
    # Then all nodes are on the same chain at height 1505
