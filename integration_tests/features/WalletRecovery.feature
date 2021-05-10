@wallet-recovery
Feature: Wallet Recovery

    @critical
    Scenario: Wallet recovery with connected base node staying online
        Given I have a seed node NODE
        And I have 1 base nodes connected to all seed nodes
        And I have wallet WALLET_A connected to all seed nodes
        And I have a merge mining proxy PROXY connected to NODE and WALLET_A with default config
        When I merge mine 10 blocks via PROXY
        When I mine 10 blocks on NODE
        When I wait for wallet WALLET_A to have at least 55000000000 uT
        Then all nodes are at height 20
        When I recover wallet WALLET_A into wallet WALLET_B connected to all seed nodes
        Then wallet WALLET_A and wallet WALLET_B have the same balance
