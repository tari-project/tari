# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@block-template
Feature: BlockTemplate

@critical
Scenario: Verify UTXO and kernel MMR size in header
    Given I have a seed node SEED_A
    When I have 1 base nodes connected to all seed nodes
    Then meddling with block template data from node SEED_A is not allowed

    @critical
    Scenario: Verify gprc can create block with more than 1 coinbase
        Given I have a seed node SEED_A
        When I have 1 base nodes connected to all seed nodes
        Then generate a block BLOCK_02 with 2 coinbases from node SEED_A
        Then generate a block BLOCK_02 with 2 coinbases as a single request from node SEED_A

    @critical
    Scenario: Verify grpc can create full block with maximum number of coinbases
        Given I have 1 seed nodes
        When I have a base node NODE_01 connected to all seed nodes
        When I have wallet WALLET_DEFAULT connected to all seed nodes
        When I have wallet WALLET_01 connected to all seed nodes
        When I have wallet WALLET_02 connected to all seed nodes
        When I have wallet WALLET_03 connected to all seed nodes
        When I have wallet WALLET_04 connected to all seed nodes
        When I have wallet WALLET_05 connected to all seed nodes
        When I have wallet WALLET_06 connected to all seed nodes
        When I have wallet WALLET_07 connected to all seed nodes
        When I have wallet WALLET_08 connected to all seed nodes
        When I have wallet WALLET_09 connected to all seed nodes
        When I have wallet WALLET_10 connected to all seed nodes
        When I have wallet WALLET_11 connected to all seed nodes
        When I have wallet WALLET_12 connected to all seed nodes
        When I have mining node MINER connected to base node NODE_01 and wallet WALLET_DEFAULT
        When mining node MINER mines 1 blocks
        Then all nodes are at height 1

        Then I generate a block BLOCK_02 with 1000 coinbases from node NODE_01 for wallet WALLET_01
        Then all nodes are at height 2
        Then I generate a block BLOCK_03 with 1000 coinbases from node NODE_01 for wallet WALLET_02
        Then all nodes are at height 3
        Then I generate a block BLOCK_04 with 1000 coinbases from node NODE_01 for wallet WALLET_03
        Then all nodes are at height 4
        Then I generate a block BLOCK_05 with 1000 coinbases from node NODE_01 for wallet WALLET_04
        Then all nodes are at height 5
        Then I generate a block BLOCK_06 with 1000 coinbases from node NODE_01 for wallet WALLET_05
        Then all nodes are at height 6
        Then I generate a block BLOCK_07 with 1000 coinbases from node NODE_01 for wallet WALLET_06
        Then all nodes are at height 7
        Then I generate a block BLOCK_08 with 1000 coinbases from node NODE_01 for wallet WALLET_07
        Then all nodes are at height 8
        Then I generate a block BLOCK_09 with 1000 coinbases from node NODE_01 for wallet WALLET_08
        Then all nodes are at height 9
        Then I generate a block BLOCK_10 with 1000 coinbases from node NODE_01 for wallet WALLET_09
        Then all nodes are at height 10
        Then I generate a block BLOCK_11 with 1000 coinbases from node NODE_01 for wallet WALLET_10
        Then all nodes are at height 11
        Then I generate a block BLOCK_12 with 1000 coinbases from node NODE_01 for wallet WALLET_11
        Then all nodes are at height 12
        Then I generate a block BLOCK_13 with 1000 coinbases from node NODE_01 for wallet WALLET_12
        Then all nodes are at height 13

        When mining node MINER mines 1 blocks
        Then all nodes are at height 14
        When mining node MINER mines 1 blocks
        Then all nodes are at height 15
        When mining node MINER mines 1 blocks
        Then all nodes are at height 16
        When mining node MINER mines 1 blocks
        Then all nodes are at height 17

        When I wait for wallet WALLET_01 to have at least 18462050000 uT
        When I wait for wallet WALLET_02 to have at least 18462050000 uT
        When I wait for wallet WALLET_03 to have at least 18462050000 uT
        When I wait for wallet WALLET_04 to have at least 18462050000 uT
        When I wait for wallet WALLET_05 to have at least 18462050000 uT
        When I wait for wallet WALLET_06 to have at least 18462050000 uT
        When I wait for wallet WALLET_07 to have at least 18462050000 uT
        When I wait for wallet WALLET_08 to have at least 18462050000 uT
        When I wait for wallet WALLET_09 to have at least 18462050000 uT
        When I wait for wallet WALLET_10 to have at least 18462050000 uT
        When I wait for wallet WALLET_11 to have at least 18462050000 uT
        When I wait for wallet WALLET_12 to have at least 18462050000 uT

        Then I send a one-sided transaction of 18462000000 uT from wallet WALLET_01 to wallet WALLET_DEFAULT at fee 1
        Then I send a one-sided transaction of 18462000000 uT from wallet WALLET_02 to wallet WALLET_DEFAULT at fee 1
        Then I send a one-sided transaction of 18462000000 uT from wallet WALLET_03 to wallet WALLET_DEFAULT at fee 1
        Then I send a one-sided transaction of 18462000000 uT from wallet WALLET_04 to wallet WALLET_DEFAULT at fee 1
        Then I send a one-sided transaction of 18462000000 uT from wallet WALLET_05 to wallet WALLET_DEFAULT at fee 1
        Then I send a one-sided transaction of 18462000000 uT from wallet WALLET_06 to wallet WALLET_DEFAULT at fee 1
        Then I send a one-sided transaction of 18462000000 uT from wallet WALLET_07 to wallet WALLET_DEFAULT at fee 1
        Then I send a one-sided transaction of 18462000000 uT from wallet WALLET_08 to wallet WALLET_DEFAULT at fee 1
        Then I send a one-sided transaction of 18462000000 uT from wallet WALLET_09 to wallet WALLET_DEFAULT at fee 1
        Then I send a one-sided transaction of 18462000000 uT from wallet WALLET_10 to wallet WALLET_DEFAULT at fee 1
        Then I send a one-sided transaction of 18462000000 uT from wallet WALLET_11 to wallet WALLET_DEFAULT at fee 1
        Then I send a one-sided transaction of 18462000000 uT from wallet WALLET_12 to wallet WALLET_DEFAULT at fee 1

        # Mempool now has 12 transactions, each with 1000 coinbases as inputs, so we can create a big block
        Then I generate a block BLOCK_18 with 1000 coinbases from node NODE_01 for wallet WALLET_DEFAULT
        Then all nodes are at height 18

        Then block BLOCK_18 has serialized size at least 7000000 bytes
