Feature: Wallet Base Node Switch

    Scenario: As a user I want to change base node for a wallet
        Given I have a base node Node1 connected to all seed nodes
        And I have a base node Node2 connected to all seed nodes
        And I have wallet Wallet connected to base node Node1
        When I stop wallet Wallet
        And change base node of Wallet to Node2
        Then I wait for Wallet to connect to Node2
