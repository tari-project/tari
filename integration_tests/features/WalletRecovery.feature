@wallet-recovery @wallet
Feature: Wallet Recovery

    @critical
    Scenario: Wallet recovery with connected base node staying online
        Given I have a seed node NODE
        And I have 1 base nodes connected to all seed nodes
        And I have wallet WALLET_A connected to all seed nodes
        And I have a merge mining proxy PROXY connected to NODE and WALLET_A with default config
        When I merge mine 10 blocks via PROXY
        When I mine 5 blocks on NODE
        When I wait for wallet WALLET_A to have at least 55000000000 uT
        Then all nodes are at height 15
        When I recover wallet WALLET_A into wallet WALLET_B connected to all seed nodes
        Then wallet WALLET_A and wallet WALLET_B have the same balance
        And I have wallet WALLET_C connected to all seed nodes
        And I send 100000 uT from wallet WALLET_B to wallet WALLET_C at fee 100
        Then wallet WALLET_B detects all transactions are at least Broadcast
        When I mine 5 blocks on NODE
        Then all nodes are at height 20
        Then I wait for wallet WALLET_C to have at least 100000 uT

    Scenario Outline: Multiple Wallet recovery from seed node
        Given I have a seed node NODE
        And I have wallet WALLET_A connected to all seed nodes
        And I have a merge mining proxy PROXY connected to NODE and WALLET_A with default config
        When I merge mine 15 blocks via PROXY
        When I wait for wallet WALLET_A to have at least 55000000000 uT
        Then all nodes are at height 15
        When I recover wallet WALLET_A into <NumWallets> wallets connected to all seed nodes
        When I wait for <NumWallets> wallets to have at least 55000000000 uT
        Then Wallet WALLET_A and <NumWallets> wallets have the same balance
        @critical
        Examples:
            | NumWallets |
            | 1        |
            | 2        |
            | 5        |
            | 10        |

    # fails often on circle CI
    @critical @flaky
    Scenario: Recover one-sided payments
        Given I have a seed node NODE
        And I have 1 base nodes connected to all seed nodes
        And I have wallet WALLET_A connected to all seed nodes
        And I have a merge mining proxy PROXY connected to NODE and WALLET_A with default config
        When I merge mine 10 blocks via PROXY
        Then all nodes are at height 10
        And I have wallet WALLET_B connected to all seed nodes
        And I stop wallet WALLET_B
        # Send 2 one-sided payments to WALLET_B so it can spend them in two cases
        Then I send a one-sided transaction of 1000000 uT from WALLET_A to WALLET_B at fee 100
        Then I send a one-sided transaction of 1000000 uT from WALLET_A to WALLET_B at fee 100
        When I merge mine 5 blocks via PROXY
        Then all nodes are at height 15
        When I recover wallet WALLET_B into wallet WALLET_C connected to all seed nodes
        Then I wait for wallet WALLET_C to have at least 2000000 uT
        # Send one of the recovered outputs back to Wallet A as a one-sided transactions
        Then I send a one-sided transaction of 900000 uT from WALLET_C to WALLET_A at fee 100
        When I merge mine 5 blocks via PROXY
        Then all nodes are at height 20
        Then I wait for wallet WALLET_C to have less than 1100000 uT
        # Send the remaining recovered UTXO to self in standard MW transaction
        Then I send 1000000 uT from wallet WALLET_C to wallet WALLET_C at fee 100
        Then I wait for wallet WALLET_C to have less than 100000 uT
        When I merge mine 5 blocks via PROXY
        Then all nodes are at height 25
        Then I wait for wallet WALLET_C to have at least 1000000 uT
