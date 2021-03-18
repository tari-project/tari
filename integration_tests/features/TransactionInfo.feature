@transaction-info
Feature: Transaction Info

@long-running
Scenario: Get Transaction Info
    Given I have a seed node NODE
        # TODO: This test takes an hour if only one base node is used
    And I have 1 base nodes connected to all seed nodes
    And I have wallet WALLET_A connected to all seed nodes
    And I have wallet WALLET_B connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and WALLET_A
        # We need to ensure the coinbase lock heights are gone; mine enough blocks
    When I merge mine 4 blocks via PROXY
    Then all nodes are at height 4
    Then I list all COINBASE transactions for wallet WALLET_A
    When I wait for wallet WALLET_A to have at least 1002000 tari
    And I send 1000000 uT from wallet WALLET_A to wallet WALLET_B at fee 100
    Then wallet WALLET_A detects all transactions are at least Pending
    Then wallet WALLET_B detects all transactions are at least Pending
    Then wallet WALLET_A detects all transactions are at least Completed
    Then wallet WALLET_B detects all transactions are at least Completed
    Then wallet WALLET_A detects all transactions are at least Broadcast
    Then wallet WALLET_B detects all transactions are at least Broadcast
        # TODO: This wait is needed to stop next merge mining task from continuing
    When I wait 1 seconds
    When I mine 1 blocks on NODE
    Then all nodes are at height 5
    Then wallet WALLET_A detects all transactions as Mined_Unconfirmed
    Then wallet WALLET_B detects all transactions as Mined_Unconfirmed
        # TODO: This wait is needed to stop base nodes from shutting down
    When I wait 1 seconds
    When I mine 11 blocks on NODE
    Then all nodes are at height 16
    Then wallet WALLET_A detects all transactions as Mined_Confirmed
    Then wallet WALLET_B detects all transactions as Mined_Confirmed
        # TODO: This wait is needed to stop base nodes from shutting down
    When I wait 1 seconds
