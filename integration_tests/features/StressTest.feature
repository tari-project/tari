Feature: Stress Test

@long-running
Scenario: Stress Test
    Given I have a seed node NODE1
    And I have a seed node NODE2
    And I have 1 base nodes connected to all seed nodes
    And I have wallet WALLET_A connected to seed node NODE1
    And I have wallet WALLET_B connected to seed node NODE2
    And I have a merge mining proxy PROXY connected to NODE1 and WALLET_A
    # We need to ensure the coinbase lock heights are reached; mine enough blocks
    # The following line is how you could mine directly on the node
    When I merge mine 8 blocks via PROXY

    Then all nodes are at height 8
    When I wait for wallet WALLET_A to have at least 15100000000 tari
    # A coin split can produce at most 499 outputs
    And I coin split tari in wallet WALLET_A to produce 499 UTXOs of 7000 uT each with fee_per_gram 20 uT
    And I coin split tari in wallet WALLET_A to produce 499 UTXOs of 7000 uT each with fee_per_gram 20 uT

    # Make sure enough blocks are mined for the coin split transaction to be confirmed
    When I merge mine 6 blocks via PROXY

    Then all nodes are at height 14
    Then wallet WALLET_A detects all transactions as Mined_Confirmed
    When I wait 1 seconds
    When I send 400 transactions of 1111 uT each from wallet WALLET_A to wallet WALLET_B at fee_per_gram 20
    # The following line really taxes SEED NODE A
    # When I send 1400 transactions of 1111 uT each from wallet WALLET_A to wallet WALLET_B at fee_per_gram 20

    # Mine enough blocks for the first block of transactions to be confirmed.
    When I merge mine 4 blocks via PROXY
    Then all nodes are at height 18
    # Now wait until all transactions are detected as confirmed in WALLET_A, continue to mine blocks if transactions
    # are not found to be confirmed as sometimes the previous mining occurs faster than transactions are submitted
    # to the mempool
    Then while merge mining via NODE1 all transactions in wallet WALLET_A are found to be Mined_Confirmed
    # Then wallet WALLET_B detects all transactions as Mined_Confirmed
    Then while mining via NODE1 all transactions in wallet WALLET_B are found to be Mined_Confirmed