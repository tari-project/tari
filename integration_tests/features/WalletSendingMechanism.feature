@sending_mechanism
Feature: Wallet Sending Mechanism

    Scenario Outline: Wallets transacting via specified routing mechanism only
        Given I have a seed node NODE
        And I have <NumBaseNodes> base nodes connected to all seed nodes
        And I have non-default wallet WALLET_A connected to all seed nodes using <Mechanism>
        And I have <NumWallets> non-default wallets connected to all seed nodes using <Mechanism>
        And I have a merge mining proxy PROXY connected to NODE and WALLET_A
        When I merge mine 20 blocks via PROXY
        Then all nodes are at height 20
        # TODO: This wait is needed to stop base nodes from shutting down
        When I wait 1 seconds
        When I wait for wallet WALLET_A to have more than 100000000 tari
        #When I print the world
        And I multi-send 1000000 tari from wallet WALLET_A to all wallets at fee 100
        Then all wallets detect all transactions are at least pending
        Then all wallets detect all transactions are at least completed
        # TODO: This wait is needed to stop next merge mining task from continuing
        When I wait 1 seconds
        When I merge mine 12 blocks via PROXY
        Then all nodes are at height 32
        Then all wallets detect all transactions as mined
        # TODO: This wait is needed to stop base nodes from shutting down
        When I wait 1 seconds
        Examples:
            | NumBaseNodes | NumWallets | Mechanism                |
            |  5           |  5         | DirectAndStoreAndForward |
            |  5           |  5         | DirectOnly               |
            |  5           |  5         | StoreAndForwardOnly      |
