@long-running
Feature: Stress Test

    @long-running
    Scenario Outline: Ramped Stress Test
        Given I have a seed node NODE1
        And I have stress-test wallet WALLET_A connected to the seed node NODE1 with broadcast monitoring timeout <MonitoringTimeout>
        And I have a merge mining proxy PROXY connected to NODE1 and WALLET_A with default config
        # We mine some blocks before starting the other nodes to avoid a spinning sync state when all the nodes are at height 0
        When I merge mine 6 blocks via PROXY
        And I have a seed node NODE2
        And I have <NumNodes> base nodes connected to all seed nodes
        And I have stress-test wallet WALLET_B connected to the seed node NODE2 with broadcast monitoring timeout <MonitoringTimeout>
        # There need to be at least as many mature coinbase UTXOs in the wallet coin splits required for the number of transactions
        When I merge mine <NumCoinsplitsNeeded> blocks via PROXY
        Then all nodes are at current tip height
        When I wait for wallet WALLET_A to have at least 5100000000 tari

        Then I coin split tari in wallet WALLET_A to produce <NumTransactions> UTXOs of 5000 uT each with fee_per_gram 20 uT
        When I merge mine 3 blocks via PROXY
        When I merge mine <NumCoinsplitsNeeded> blocks via PROXY
        Then all nodes are at current tip height
        Then wallet WALLET_A detects all transactions as Mined_Confirmed
        When I send <NumTransactions> transactions of 1111 uT each from wallet WALLET_A to wallet WALLET_B at fee_per_gram 20
        # Mine enough blocks for the first block of transactions to be confirmed.
        When I merge mine 4 blocks via PROXY
        Then all nodes are at current tip height
        # Now wait until all transactions are detected as confirmed in WALLET_A, continue to mine blocks if transactions
        # are not found to be confirmed as sometimes the previous mining occurs faster than transactions are submitted
        # to the mempool
        Then while merge mining via PROXY all transactions in wallet WALLET_A are found to be Mined_Confirmed
        # Then wallet WALLET_B detects all transactions as Mined_Confirmed
        Then while mining via NODE1 all transactions in wallet WALLET_B are found to be Mined_Confirmed
        Examples:
            | NumTransactions   | NumCoinsplitsNeeded   | NumNodes  | MonitoringTimeout |
            | 10                | 1                     | 3         | 10                |
            | 100               | 1                     | 3         | 10                |
            | 1000              | 3                     | 3         | 30                |
            | 10000             | 21                    | 3         | 60                |

    @long-running
    Scenario: Simple Stress Test
        Given I have a seed node NODE1
        And I have stress-test wallet WALLET_A connected to the seed node NODE1 with broadcast monitoring timeout 60
        And I have a merge mining proxy PROXY connected to NODE1 and WALLET_A with default config
        When I merge mine 1 blocks via PROXY
        And I have a seed node NODE2
        And I have 1 base nodes connected to all seed nodes
        And I have stress-test wallet WALLET_B connected to the seed node NODE2 with broadcast monitoring timeout 60
        # We need to ensure the coinbase lock heights are reached; mine enough blocks
        # The following line is how you could mine directly on the node
        When I merge mine 8 blocks via PROXY
        Then all nodes are at current tip height
        When I wait for wallet WALLET_A to have at least 15100000000 tari

        Then I coin split tari in wallet WALLET_A to produce 2000 UTXOs of 5000 uT each with fee_per_gram 20 uT

        # Make sure enough blocks are mined for the coin split transaction to be confirmed
        When I merge mine 8 blocks via PROXY

        Then all nodes are at current tip height
        Then wallet WALLET_A detects all transactions as Mined_Confirmed
        When I send 2000 transactions of 1111 uT each from wallet WALLET_A to wallet WALLET_B at fee_per_gram 20
        # Mine enough blocks for the first block of transactions to be confirmed.
        When I merge mine 4 blocks via PROXY
        Then all nodes are at current tip height
        # Now wait until all transactions are detected as confirmed in WALLET_A, continue to mine blocks if transactions
        # are not found to be confirmed as sometimes the previous mining occurs faster than transactions are submitted
        # to the mempool
        Then while merge mining via PROXY all transactions in wallet WALLET_A are found to be Mined_Confirmed
        # Then wallet WALLET_B detects all transactions as Mined_Confirmed
        Then while mining via NODE1 all transactions in wallet WALLET_B are found to be Mined_Confirmed