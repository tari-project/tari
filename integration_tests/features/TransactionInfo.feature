@transaction-info
    Feature: Transaction Info

 Get Transaction Info
        Given I have a seed node NODE
        # TODO: This test takes an hour if only one base node is used
        And I have 1 base nodes connected to all seed nodes
        And I have wallet WALLET_A connected to all seed nodes
        And I have wallet WALLET_B connected to all seed nodes
        And I have a merge mining proxy PROXY connected to NODE and WALLET_A
        When I merge mine 2 blocks via PROXY
        Then all nodes are at height 2
        # We need to ensure the coinbase lock heights are gone
        When I mine 2 blocks on NODE
        When I wait for wallet WALLET_A to have more than 1002000 tari
        And I send 1000000 tari from wallet WALLET_A to wallet WALLET_B at fee 100
        Then wallet WALLET_A detects all transactions are at least pending
        Then wallet WALLET_B detects all transactions are at least pending
        Then wallet WALLET_A detects all transactions are at least completed
        Then wallet WALLET_B detects all transactions are at least completed
        # TODO: This wait is needed to stop next merge mining task from continuing
        When I wait 1 seconds
        When I merge mine 12 blocks via PROXY
        Then all nodes are at height 16
        Then wallet WALLET_A detects all transactions as mined
        Then wallet WALLET_B detects all transactions as mined
        # TODO: This wait is needed to stop base nodes from shutting down
        When I wait 1 seconds

