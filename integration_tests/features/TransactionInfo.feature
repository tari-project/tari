@transaction-info @wallet
Feature: Transaction Info

@long-running
Scenario: Get Transaction Info
    Given I have a seed node NODE
    And I have a SHA3 miner MINER connected to all seed nodes
    And I have wallet WALLET_A connected to all seed nodes
    And I have wallet WALLET_B connected to all seed nodes
    And I have mining node MINER connected to base node NODE and wallet WALLET_A
        # We need to ensure the coinbase lock heights are gone; mine enough blocks
    When mining node MINER mines 4 blocks
    Then all nodes are at height 4
    Then I list all COINBASE transactions for wallet WALLET_A
    When I wait for wallet WALLET_A to have at least 1002000 uT
    And I send 1000000 uT from wallet WALLET_A to wallet WALLET_B at fee 20
    Then wallet WALLET_A detects all transactions are at least Pending
    Then wallet WALLET_B detects all transactions are at least Pending
    Then wallet WALLET_A detects all transactions are at least Completed
    Then wallet WALLET_B detects all transactions are at least Completed
    Then wallet WALLET_A detects all transactions are at least Broadcast
    Then wallet WALLET_B detects all transactions are at least Broadcast
        # TODO: This wait is needed to stop next merge mining task from continuing
    When I wait 1 seconds
    And mining node MINER mines 1 blocks
    Then all nodes are at height 5
    Then wallet WALLET_A detects all transactions as Mined_Unconfirmed
    Then wallet WALLET_B detects all transactions as Mined_Unconfirmed
        # TODO: This wait is needed to stop base nodes from shutting down
    When I wait 1 seconds
    And mining node MINER mines 10 blocks
    Then all nodes are at height 15
    Then wallet WALLET_A detects all transactions as Mined_Confirmed
    Then wallet WALLET_B detects all transactions as Mined_Confirmed
        # TODO: This wait is needed to stop base nodes from shutting down
    When I wait 1 seconds
