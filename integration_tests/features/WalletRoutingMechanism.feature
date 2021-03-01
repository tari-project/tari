@routing_mechanism
Feature: Wallet Routing Mechanism

Scenario Outline: Wallets transacting via specified routing mechanism only
    Given I have a seed node NODE
    And I have <NumBaseNodes> base nodes connected to all seed nodes
    And I have non-default wallet WALLET_A connected to all seed nodes using <Mechanism>
    And I have <NumWallets> non-default wallets connected to all seed nodes using <Mechanism>
    And I have a merge mining proxy PROXY connected to NODE and WALLET_A
        # We need to ensure the coinbase lock heights are gone and we have enough individual UTXOs; mine enough blocks
    When I merge mine 20 blocks via PROXY
    Then all nodes are at height 20
        # TODO: This wait is needed to stop base nodes from shutting down
    When I wait 1 seconds
    When I wait for wallet WALLET_A to have at least 100000000 tari
    #When I print the world
    And I multi-send 1000000 uT from wallet WALLET_A to all wallets at fee 100
    Then all wallets detect all transactions are at least Pending
    Then all wallets detect all transactions are at least Completed
    Then all wallets detect all transactions are at least Broadcast
        # TODO: This wait is needed to stop next merge mining task from continuing
    When I wait 1 seconds
    When I mine 1 blocks on NODE
    Then all nodes are at height 21
    Then all wallets detect all transactions as Mined_Unconfirmed
        # TODO: This wait is needed to stop next merge mining task from continuing
    When I wait 1 seconds
    When I mine 11 blocks on NODE
    Then all nodes are at height 32
    Then all wallets detect all transactions as Mined_Confirmed
        # TODO: This wait is needed to stop base nodes from shutting down
    When I wait 1 seconds
    @long-running
    Examples:
        | NumBaseNodes | NumWallets | Mechanism                |
        |  5           |  5         | DirectAndStoreAndForward |
        |  5           |  5         | DirectOnly               |
        |  5           |  5         | StoreAndForwardOnly      |
