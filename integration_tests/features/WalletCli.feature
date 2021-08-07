Feature: Wallet CLI

    Scenario: As a user I want to change base node for a wallet via command line
        Given I have a base node NODE1 connected to all seed nodes
        And I have a base node NODE2 connected to all seed nodes
        And I have wallet WALLET connected to base node NODE1
        Then I change base node of WALLET to NODE2

    Scenario: As a user I want to set and clear custom base node for a wallet via command line
        Given I have a base node NODE1
        And I have a base node NODE2
        And I have wallet WALLET connected to base node NODE1
        Then I set custom base node of WALLET to NODE2
        And I clear custom base node of wallet WALLET

    Scenario: As a user I want to change password via command line
        Given I have wallet WALLET connected to all seed nodes
        When I stop wallet WALLET
        And I change the password of wallet WALLET to changedpwd
        Then the password of wallet WALLET is not kensentme
        Then the password of wallet WALLET is changedpwd

    Scenario: As a user I want to get balance via command line
        Given I have a base node BASE
        And I have wallet WALLET connected to base node BASE
        And I have mining node MINE connected to base node BASE and wallet WALLET
        And mining node MINE mines 5 blocks
        Then I wait for wallet WALLET to have at least 1000000 uT
        And I stop wallet WALLET
        Then I get balance via command line of wallet WALLET is at least 1000000 uT

    Scenario: As a user I want to send tari via command line
        Given I have a base node BASE
        And I have wallet SENDER connected to base node BASE
        And I have wallet RECEIVER connected to base node BASE
        And I have mining node MINE connected to base node BASE and wallet SENDER
        And mining node MINE mines 5 blocks
        Then I wait for wallet SENDER to have at least 1000000 uT
        And I wait 5 seconds
        And I stop wallet SENDER
        And I send 1000000 uT from SENDER to RECEIVER via command line
        And mining node MINE mines 5 blocks
        Then I wait for wallet RECEIVER to have at least 1000000 uT

    Scenario: As a user I want to send one-sided via command line
        Given I have a base node BASE
        And I have wallet SENDER connected to base node BASE
        And I have wallet RECEIVER connected to base node BASE
        And I have mining node MINE connected to base node BASE and wallet SENDER
        And mining node MINE mines 5 blocks
        Then I wait for wallet SENDER to have at least 1000000 uT
        And I wait 5 seconds
        And I stop wallet SENDER
        And I send one-sided 1000000 uT from SENDER to RECEIVER via command line
        And mining node MINE mines 5 blocks
        Then I wait for wallet RECEIVER to have at least 1000000 uT

    Scenario: As a user I want to make-it-rain via command line
        Given I have a base node BASE
        And I have wallet SENDER connected to base node BASE
        And I have wallet RECEIVER connected to base node BASE
        And I have mining node MINE connected to base node BASE and wallet SENDER
        And mining node MINE mines 15 blocks
        Then I wait for wallet SENDER to have at least 1000000 uT
        And I wait 5 seconds
        And I stop wallet SENDER
        And I make it rain from wallet SENDER 1 tx / sec 10 sec 8000 uT 100 increment to RECEIVER
        And I wait 15 seconds
        And mining node MINE mines 5 blocks
        Then I wait for wallet RECEIVER to have at least 84500 uT

    Scenario: As a user I want to coin-split via command line
        Given I have a base node BASE
        And I have wallet WALLET connected to base node BASE
        And I have mining node MINE connected to base node BASE and wallet WALLET
        And mining node MINE mines 4 blocks
        Then I wait for wallet WALLET to have at least 1000000 uT
        And I wait 5 seconds
        And I stop wallet WALLET
        And I do coin split on wallet WALLET to 10000 uT 10 coins
        And I wait 5 seconds
        And mining node MINE mines 5 blocks
        And I wait 5 seconds
        And I stop wallet WALLET
        Then I get count of utxos of wallet WALLET via command line and it's at least 10

    Scenario: As a user I want to count utxos via command line
        Given I have a base node BASE
        And I have wallet WALLET connected to base node BASE
        And I have mining node MINE connected to base node BASE and wallet WALLET
        And mining node MINE mines 4 blocks
        Then I wait for wallet WALLET to have at least 1000000 uT
        And I stop wallet WALLET
        Then I get count of utxos of wallet WALLET via command line and it's at least 1

    Scenario: As a user I want to export utxos via command line
        Given I have a base node BASE
        And I have wallet WALLET connected to base node BASE
        And I have mining node MINE connected to base node BASE and wallet WALLET
        And mining node MINE mines 4 blocks
        Then I wait for wallet WALLET to have at least 1000000 uT
        And I export the utxos of wallet WALLET via command line

    Scenario: As a user I want to discover-peer via command line
        Given I have a seed node SEED
        And I have a base node BASE1 connected to seed SEED
        And I have a base node BASE2 connected to seed SEED
        And I have wallet WALLET connected to base node BASE1
        And I discover peer BASE2 on wallet WALLET via command line
        Then WALLET is connected to BASE2

    Scenario: As a user I want to run whois via command line
        Given I have a base node BASE
        And I have wallet WALLET connected to base node BASE
        Then I run whois BASE on wallet WALLET
