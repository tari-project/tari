# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@stress-test
Feature: Stress Test

    Scenario Outline: Ramped Stress Test
        Given I have a seed node NODE1
        # And I have stress-test wallet WALLET_A connected to the seed node NODE1 with broadcast monitoring timeout <MonitoringTimeout>
        # And I have mining node MINER connected to base node NODE1 and wallet WALLET_A
        # # We mine some blocks before starting the other nodes to avoid a spinning sync state when all the nodes are at height 0
        When I have a seed node NODE2
        # And I have <NumNodes> base nodes connected to all seed nodes
        # And I have stress-test wallet WALLET_B connected to the seed node NODE2 with broadcast monitoring timeout <MonitoringTimeout>
        # When mining node MINER mines 6 blocks
        # # There need to be at least as many mature coinbase UTXOs in the wallet coin splits required for the number of transactions
        # When mining node MINER mines <NumCoinsplitsNeeded> blocks
        # Then all nodes are on the same chain tip
        # When I wait for wallet WALLET_A to have at least 5100000000 uT

        # Then I coin split tari in wallet WALLET_A to produce <NumTransactions> UTXOs of 5000 uT each with fee_per_gram 20 uT
        # When mining node MINER mines 3 blocks
        # When mining node MINER mines <NumCoinsplitsNeeded> blocks
        # Then all nodes are on the same chain tip
        # Then wallet WALLET_A detects all transactions as Mined_or_OneSidedConfirmed
        # When I send <NumTransactions> transactions of 1111 uT each from wallet WALLET_A to wallet WALLET_B at fee_per_gram 4
        # # Mine enough blocks for the first block of transactions to be confirmed.
        # When mining node MINER mines 4 blocks
        # Then all nodes are on the same chain tip
        # # Now wait until all transactions are detected as confirmed in WALLET_A, continue to mine blocks if transactions
        # # are not found to be confirmed as sometimes the previous mining occurs faster than transactions are submitted
        # # to the mempool
        # Then while mining via SHA3 miner MINER all transactions in wallet WALLET_A are found to be Mined_or_OneSidedConfirmed
        # # Then wallet WALLET_B detects all transactions as Mined_or_OneSidedConfirmed
        # Then while mining via node NODE1 all transactions in wallet WALLET_B are found to be Mined_or_OneSidedConfirmed

        # @flaky
        # Examples:
        #     | NumTransactions | NumCoinsplitsNeeded | NumNodes | MonitoringTimeout |
        #     | 10              | 1                   | 3        | 10                |

        # @long-running
        # Examples:
        #     | NumTransactions | NumCoinsplitsNeeded | NumNodes | MonitoringTimeout |
        #     | 100             | 1                   | 3        | 10                |
        #     | 1000            | 3                   | 3        | 60                |

        # @long-running
        # Examples:
        #     | NumTransactions | NumCoinsplitsNeeded | NumNodes | MonitoringTimeout |
        #     | 10000           | 21                  | 3        | 600               |

    @long-running
    Scenario: Simple Stress Test
        Given I have a seed node NODE1
        # And I have stress-test wallet WALLET_A connected to the seed node NODE1 with broadcast monitoring timeout 60
        # And I have mining node MINER connected to base node NODE1 and wallet WALLET_A
        # When mining node MINER mines 1 blocks
        When I have a seed node NODE2
        # And I have 1 base nodes connected to all seed nodes
        # And I have stress-test wallet WALLET_B connected to the seed node NODE2 with broadcast monitoring timeout 60
        # # We need to ensure the coinbase lock heights are reached; mine enough blocks
        # # The following line is how you could mine directly on the node
        # When mining node MINER mines 8 blocks
        # Then all nodes are on the same chain tip
        # When I wait for wallet WALLET_A to have at least 15100000000 uT

        # Then I coin split tari in wallet WALLET_A to produce 2000 UTXOs of 5000 uT each with fee_per_gram 4 uT

        # # Make sure enough blocks are mined for the coin split transaction to be confirmed
        # When mining node MINER mines 8 blocks

        # Then all nodes are on the same chain tip
        # Then wallet WALLET_A detects all transactions as Mined_or_OneSidedConfirmed
        # When I send 2000 transactions of 1111 uT each from wallet WALLET_A to wallet WALLET_B at fee_per_gram 4
        # # Mine enough blocks for the first block of transactions to be confirmed.
        # When mining node MINER mines 4 blocks
        # Then all nodes are on the same chain tip
        # # Now wait until all transactions are detected as confirmed in WALLET_A, continue to mine blocks if transactions
        # # are not found to be confirmed as sometimes the previous mining occurs faster than transactions are submitted
        # # to the mempool
        # Then while mining via SHA3 miner MINER all transactions in wallet WALLET_A are found to be Mined_or_OneSidedConfirmed
        # # Then wallet WALLET_B detects all transactions as Mined_or_OneSidedConfirmed
        # Then while mining via node NODE1 all transactions in wallet WALLET_B are found to be Mined_or_OneSidedConfirmed
