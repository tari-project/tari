# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@sync @base-node
Feature: Block Sync

  Scenario Outline: Initial block sync
    Given I have <NumSeeds> seed nodes
    And I have a base node MINER connected to all seed nodes
    When I mine <NumBlocks> blocks on MINER
    Given I have <NumSyncers> base nodes connected to all seed nodes
    # All nodes should sync to tip
    Then all nodes are at height <NumBlocks>


    Examples:
      | NumSeeds | NumBlocks | NumSyncers |
      | 1        | 1         | 1          |

    @long-running
    Examples:
      | NumSeeds | NumBlocks | NumSyncers |
      | 1        | 10        | 2          |
      | 1        | 50        | 4          |
      | 8        | 40        | 8          |


  @critical
  Scenario: Simple block sync
    Given I have 1 seed nodes
    Given I have a SHA3 miner MINER connected to all seed nodes
    Given mining node MINER mines 20 blocks
    Given I have 2 base nodes connected to all seed nodes
    # All nodes should sync to tip
    Then all nodes are at height 20

  @critical @pruned @broken
  Scenario: Pruned mode simple sync
    Given I have 1 seed nodes
    Given I have a SHA3 miner NODE1 connected to all seed nodes
    When I mine a block on NODE1 with coinbase CB1
    And I mine 4 blocks on NODE1
    When I spend outputs CB1 via NODE1
    Given mining node NODE1 mines 15 blocks
    Given I have a pruned node PNODE1 connected to node NODE1 with pruning horizon set to 5
    Then all nodes are at height 20

  @critical
  Scenario: When a new node joins the network, it should receive all peers
    Given I have 10 seed nodes
    And I have a base node NODE1 connected to all seed nodes
    # additional peer seeds are being included from config.toml [common]
    Then NODE1 should have at least 10 peers
    Given I have a base node NODE2 connected to node NODE1
    Then NODE1 should have at least 11 peers
    Then NODE2 should have at least 11 peers


  Scenario: Pruned mode sync test
    Given I have a seed node SEED
    Given I have a base node NODE1 connected to all seed nodes
    When I mine a block on NODE1 with coinbase CB1
    And I mine 4 blocks on NODE1
    Then all nodes are at height 5
    When I spend outputs CB1 via NODE1
    And I mine 3 blocks on NODE1
    Given I have a pruned node PNODE2 connected to node NODE1 with pruning horizon set to 5
    Then all nodes are at height 8
    When I mine 15 blocks on PNODE2
    Then all nodes are at height 23

  @long-running @flaky
  Scenario: Node should not sync from pruned node
    Given I have a base node NODE1 connected to all seed nodes
    And I have wallet WALLET1 connected to base node NODE1
    And I have mining node MINING1 connected to base node NODE1 and wallet WALLET1
    And I have a pruned node PNODE1 connected to node NODE1 with pruning horizon set to 6
    When mining node MINING1 mines 40 blocks with min difficulty 20 and max difficulty 9999999999
    Then all nodes are at height 40
    When I stop node NODE1
    Given I have a pruned node PNODE2 connected to node PNODE1 with pruning horizon set to 5
    Given I have a base node NODE2
    And I have wallet WALLET2 connected to base node NODE2
    And I have mining node MINING2 connected to base node NODE2 and wallet WALLET2
    When mining node MINING2 mines 5 blocks with min difficulty 1 and max difficulty 2
    And I connect node NODE2 to node PNODE1
    And I connect node NODE2 to node PNODE2
    Then node PNODE2 is at height 40
    Then node NODE2 is at height 5
    When I start base node NODE1
    # We need for node to boot up and supply node 2 with blocks
    And I connect node NODE2 to node NODE1
    # NODE2 may initially try to sync from PNODE1 and PNODE2, then eventually try to sync from NODE1; mining blocks
    # on NODE1 will make this test less flaky and force NODE2 to sync from NODE1 much quicker
    When I mine 10 blocks on NODE1
    Then all nodes are at height 50

  Scenario Outline: Syncing node while also mining before tip sync
    Given I have a seed node SEED
    And I have wallet WALLET1 connected to seed node SEED
    And I have wallet WALLET2 connected to seed node SEED
    And I have mining node MINER connected to base node SEED and wallet WALLET1
    And I have a base node SYNCER connected to all seed nodes
    And I have mine-before-tip mining node MINER2 connected to base node SYNCER and wallet WALLET2
    And I stop node SYNCER
    When mining node MINER mines <X1> blocks with min difficulty 1 and max difficulty 9999999999
    Then node SEED is at height <X1>
    When I start base node SYNCER
    # Try to mine much faster than block sync, but still producing a lower accumulated difficulty
    And mining node MINER2 mines <Y1> blocks with min difficulty 1 and max difficulty 2
    Then node SYNCER is at the same height as node SEED
    @critical
    Examples:
      | X1  | Y1 |
      | 101 | 10 |

    @long-running
    Examples:
      | X1   | Y1 |
      | 501  | 50 |
      | 999  | 50 |
      | 1000 | 50 |
      | 1001 | 50 |

  Scenario: Pruned mode network only
    Given I have a base node NODE1 connected to all seed nodes
    Given I have a pruned node PNODE1 connected to node NODE1 with pruning horizon set to 5
    Given I have a pruned node PNODE2 connected to node PNODE1 with pruning horizon set to 5
    When I mine a block on PNODE1 with coinbase CB1
    When I mine 2 blocks on PNODE1
    When I create a transaction TX1 spending CB1 to UTX1
    When I submit transaction TX1 to PNODE1
    When I mine 1 blocks on PNODE1
    Then TX1 is in the MINED of all nodes
    When I stop node NODE1
    When I mine 16 blocks on PNODE1
    Then node PNODE2 is at height 20
    Given I have a pruned node PNODE3 connected to node PNODE1 with pruning horizon set to 5
    Then node PNODE3 is at height 20

  Scenario Outline: Force sync many nodes against one peer
    Given I have a base node BASE
    And I have a SHA3 miner MINER connected to node BASE
    And mining node MINER mines <BLOCKS> blocks
    And I have <NODES> base nodes with pruning horizon <PRUNE_HORIZON> force syncing on node BASE
    Then all nodes are at height <BLOCKS>

    Examples:
      | NODES | BLOCKS | PRUNE_HORIZON |
      | 5     | 10     | 0             |

    @long-running
    Examples:
      | NODES | BLOCKS | PRUNE_HORIZON |
      | 5     | 100    | 0             |
      | 10    | 100    | 0             |
      | 20    | 100    | 0             |
      | 5     | 999    | 0             |
      | 10    | 1000   | 0             |
      | 20    | 1001   | 0             |
      | 5     | 999    | 100           |
      | 10    | 1000   | 100           |
      | 20    | 1001   | 100           |
