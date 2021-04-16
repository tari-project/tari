@coinbase_reorg
Feature: Wallet Monitoring

@long-running
Scenario: Wallets monitoring coinbase after a reorg
        #
        # Chain 1:
        #   Collects 10 coinbases into one wallet, send 7 transactions
        #
    Given I have a seed node SEED_A
        # Add multiple base nodes to ensure more robust comms
    And I have a base node NODE_A1 connected to seed SEED_A
    And I have wallet WALLET_A1 connected to seed node SEED_A
    And I have wallet WALLET_A2 connected to seed node SEED_A
    And I have a merge mining proxy PROXY_A connected to SEED_A and WALLET_A1 with default config
    When I merge mine 10 blocks via PROXY_A
    Then all nodes are at height 10
    And I list all coinbase transactions for wallet WALLET_A1
    Then wallet WALLET_A1 has 10 coinbase transactions
    Then wallet WALLET_A1 detects at least 7 coinbase transactions as Mined_Confirmed
        # Use 7 of the 10 coinbase UTXOs in transactions (others require 3 confirmations)
    And I multi-send 7 transactions of 1000000 uT from wallet WALLET_A1 to wallet WALLET_A2 at fee 100
    Then wallet WALLET_A1 detects all transactions are at least Broadcast
        #
        # Chain 2:
        #   Collects 10 coinbases into one wallet, send 7 transactions
        #
    And I have a seed node SEED_B
        # Add multiple base nodes to ensure more robust comms
    And I have a base node NODE_B1 connected to seed SEED_B
    And I have wallet WALLET_B1 connected to seed node SEED_B
    And I have wallet WALLET_B2 connected to seed node SEED_B
    And I have a merge mining proxy PROXY_B connected to SEED_B and WALLET_B1 with default config
    When I merge mine 10 blocks via PROXY_B
    Then all nodes are at height 10
    And I list all coinbase transactions for wallet WALLET_B1
    Then wallet WALLET_B1 has 10 coinbase transactions
    Then wallet WALLET_B1 detects at least 7 coinbase transactions as Mined_Confirmed
        # Use 7 of the 10 coinbase UTXOs in transactions (others require 3 confirmations)
    And I multi-send 7 transactions of 1000000 uT from wallet WALLET_B1 to wallet WALLET_B2 at fee 100
    Then wallet WALLET_B1 detects all transactions are at least Broadcast
        #
        # Connect Chain 1 and 2
        #
        # TODO: This wait is needed to stop next base node task from continuing
    When I wait 1 seconds
    And I have a base node NODE_C connected to all seed nodes
        # Wait for the reorg to filter through
    When I wait 30 seconds
    Then all nodes are at height 10
        # When tip advances past required confirmations, invalid coinbases still being monitored will be cancelled.
    When I mine 6 blocks on NODE_C
    Then all nodes are at height 16
        # Wait for coinbase statuses to change in the wallet
    When I wait 30 seconds
    And I list all coinbase transactions for wallet WALLET_A1
    And I list all coinbase transactions for wallet WALLET_B1
    Then the number of coinbase transactions for wallet WALLET_A1 and wallet WALLET_B1 are 3 less
