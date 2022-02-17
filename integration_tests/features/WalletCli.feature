@wallet-cli
Feature: Wallet CLI

    Scenario: As a user I want to change base node for a wallet via command line
        Given I have a base node NODE1 connected to all seed nodes
        And I have a base node NODE2 connected to all seed nodes
        And I have wallet WALLET connected to base node NODE1
        Then I change base node of WALLET to NODE2 via command line

    Scenario: As a user I want to set and clear custom base node for a wallet via command line
        Given I have a base node NODE1
        And I have a base node NODE2
        And I have wallet WALLET connected to base node NODE1
        Then I set custom base node of WALLET to NODE2 via command line
        And I clear custom base node of wallet WALLET via command line

    Scenario: As a user I want to change password via command line
        Given I have a seed node SEED
        Given I have wallet WALLET connected to all seed nodes
        When I stop wallet WALLET
        And I change the password of wallet WALLET to changedpwd via command line
        Then the password of wallet WALLET is not kensentme
        Then the password of wallet WALLET is changedpwd

    Scenario: As a user I want to get balance via command line
        Given I have a base node BASE
        And I have wallet WALLET connected to base node BASE
        And I have mining node MINE connected to base node BASE and wallet WALLET
        And mining node MINE mines 5 blocks
        Then I wait for wallet WALLET to have at least 1000000 uT
        And I stop wallet WALLET
        Then I get balance of wallet WALLET is at least 1000000 uT via command line

    @long-running
    Scenario: As a user I want to send tari via command line
        Given I have a seed node SEED
        And I have a base node BASE connected to seed SEED
        And I have wallet SENDER connected to base node BASE
        And I have wallet RECEIVER connected to base node BASE
        And I have mining node MINE connected to base node BASE and wallet SENDER
        And mining node MINE mines 5 blocks
        Then I wait for wallet SENDER to have at least 1100000 uT
        # TODO: Remove this wait when the wallet CLI commands involving transactions will only commence with a valid
        # TODO: base node connection.
        And I wait 30 seconds
        And I stop wallet SENDER
        And I send 1000000 uT from SENDER to RECEIVER via command line
        Then wallet SENDER has at least 1 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
        Then wallet RECEIVER has at least 1 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
        And mining node MINE mines 5 blocks
        Then I wait for wallet RECEIVER to have at least 1000000 uT

    @long-running
    Scenario: As a user I want to send one-sided via command line
        Given I have a seed node SEED
        And I have a base node BASE connected to seed SEED
        And I have wallet SENDER connected to base node BASE
        And I have wallet RECEIVER connected to base node BASE
        And I have mining node MINE connected to base node BASE and wallet SENDER
        And mining node MINE mines 5 blocks
        Then I wait for wallet SENDER to have at least 1100000 uT
        # TODO: Remove this wait when the wallet CLI commands involving transactions will only commence with a valid
        # TODO: base node connection.
        And I wait 30 seconds
        And I stop wallet SENDER
        And I send one-sided 1000000 uT from SENDER to RECEIVER via command line
        Then wallet SENDER has at least 1 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
        And mining node MINE mines 5 blocks
        Then I wait for wallet RECEIVER to have at least 1000000 uT

    @long-running
    Scenario: As a user I want to make-it-rain via command line
        Given I have a seed node SEED
        And I have a base node BASE connected to seed SEED
        And I have wallet SENDER connected to base node BASE
        And I have wallet RECEIVER connected to base node BASE
        And I have mining node MINE connected to base node BASE and wallet SENDER
        And mining node MINE mines 15 blocks
        Then wallets SENDER should have EXACTLY 12 spendable coinbase outputs
        # TODO: Remove this wait when the wallet CLI commands involving transactions will only commence with a valid
        # TODO: base node connection.
        And I wait 30 seconds
        And I stop wallet SENDER
        And I make it rain from wallet SENDER 1 tx per sec 10 sec 8000 uT 100 increment to RECEIVER via command line
        Then wallet SENDER has at least 10 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
        Then wallet RECEIVER has at least 10 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
        And mining node MINE mines 5 blocks
        Then I wait for wallet RECEIVER to have at least 84500 uT

    @long-running
    Scenario: As a user I want to coin-split via command line
        Given I have a seed node SEED
        And I have a base node BASE connected to seed SEED
        And I have wallet WALLET connected to base node BASE
        And I have mining node MINE connected to base node BASE and wallet WALLET
        And mining node MINE mines 4 blocks
        Then I wait for wallet WALLET to have at least 1100000 uT
        # TODO: Remove this wait when the wallet CLI commands involving transactions will only commence with a valid
        # TODO: base node connection.
        And I wait 30 seconds
        And I stop wallet WALLET
        And I do coin split on wallet WALLET to 10000 uT 10 coins via command line
        Then wallet WALLET has at least 1 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
        And mining node MINE mines 5 blocks
        Then wallet WALLET has at least 1 transactions that are all TRANSACTION_STATUS_MINED_CONFIRMED and not cancelled
        And I stop wallet WALLET
        Then I get count of utxos of wallet WALLET and it's at least 10 via command line

    Scenario: As a user I want to count utxos via command line
        Given I have a base node BASE
        And I have wallet WALLET connected to base node BASE
        And I have mining node MINE connected to base node BASE and wallet WALLET
        And mining node MINE mines 4 blocks
        Then I wait for wallet WALLET to have at least 1000000 uT
        And I stop wallet WALLET
        Then I get count of utxos of wallet WALLET and it's at least 1 via command line

    Scenario: As a user I want to export utxos via command line
        Given I have a base node BASE
        And I have wallet WALLET connected to base node BASE
        And I have mining node MINE connected to base node BASE and wallet WALLET
        And mining node MINE mines 4 blocks
        Then I wait for wallet WALLET to have at least 1000000 uT
        And I export the utxos of wallet WALLET via command line

    @flaky
    Scenario: As a user I want to discover-peer via command line
        Given I have a seed node SEED
        And I have wallet WALLET connected to seed node SEED
        And I have a base node BASE1 connected to seed SEED
        And I have a base node BASE2 connected to seed SEED
        And I discover peer BASE2 on wallet WALLET via command line
        Then WALLET is connected to BASE2

    Scenario: As a user I want to run whois via command line
        Given I have a base node BASE
        And I have wallet WALLET connected to base node BASE
        Then I run whois BASE on wallet WALLET via command line

    Scenario: As a user I want to set sidechain committee via command line
        Given I have a base node BASE
        And I have wallet WALLET connected to base node BASE
        And I have mining node MINE connected to base node BASE and wallet WALLET
        And mining node MINE mines 4 blocks
        Then I wait for wallet WALLET to have at least 1000000 uT
        And I register asset ONE on wallet WALLET via command line
        And I create committee checkpoint for asset on wallet WALLET via command line
        And mining node MINE mines 1 blocks
        Then WALLET is connected to BASE