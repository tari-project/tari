Feature: Wallet Password Change

    Scenario: As a user I want to change password
        Given I have wallet Wallet connected to all seed nodes
        When I stop wallet Wallet
        And I change the password of wallet Wallet to changedpwd
        Then the password of wallet Wallet is not kensentme
        Then the password of wallet Wallet is changedpwd