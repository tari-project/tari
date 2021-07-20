@base-node-connectivity
Feature: Base Node Connectivity

    Scenario: Basic connectivity between 2 nodes
        Given I have a seed node SEED_A
        And I have a base node NODE_A connected to all seed nodes
        When I wait for NODE_A to connect to SEED_A
        Then SEED_A is connected to NODE_A

    Scenario: Basic connectivity between nodes and wallet
        Given I have a seed node SEED_A
        And I have wallet WALLET_A connected to all seed nodes
        Then I wait for WALLET_A to connect to SEED_A
        Then I wait for WALLET_A to have 1 node connections
        Then I wait for WALLET_A to have ONLINE connectivity
        Then SEED_A is connected to WALLET_A
