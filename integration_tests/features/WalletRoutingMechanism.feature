@wallet-routing_mechanism @wallet
Feature: Wallet Routing Mechanism

@flaky
Scenario Outline: Wallets transacting via specified routing mechanism only
    Given I have a seed node NODE
    And I have <NumBaseNodes> base nodes connected to all seed nodes
    And I have non-default wallet WALLET_A connected to all seed nodes using <Mechanism>
    And I have mining node MINER connected to base node NODE and wallet WALLET_A
    And I have <NumWallets> non-default wallets connected to all seed nodes using <Mechanism>
        # We need to ensure the coinbase lock heights are gone and we have enough individual UTXOs; mine enough blocks
    And mining node MINER mines 20 blocks
    Then all nodes are at height 20
        # TODO: This wait is needed to stop base nodes from shutting down
    When I wait 1 seconds
    When I wait for wallet WALLET_A to have at least 100000000 uT
    #When I print the world
    And I multi-send 1000000 uT from wallet WALLET_A to all wallets at fee 100
        # TODO: This wait is needed to stop next merge mining task from continuing
    When I wait 1 seconds
    And mining node MINER mines 1 blocks
    Then all nodes are at height 21
    Then all wallets detect all transactions as Mined_Unconfirmed
        # TODO: This wait is needed to stop next merge mining task from continuing
    When I wait 1 seconds
    And mining node MINER mines 11 blocks
    Then all nodes are at height 32
    Then all wallets detect all transactions as Mined_Confirmed
        # TODO: This wait is needed to stop base nodes from shutting down
    When I wait 1 seconds
    @long-running
    Examples:
        | NumBaseNodes | NumWallets | Mechanism                |
        |  5           |  5         | DirectAndStoreAndForward |
        |  5           |  5         | DirectOnly               |

    @long-running
    Examples:
        | NumBaseNodes | NumWallets | Mechanism                |
        |  5           |  5         | StoreAndForwardOnly      |
