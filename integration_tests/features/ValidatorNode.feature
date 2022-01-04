@dan
Feature: Validator Node
    Scenario: Test committee
        Given I have committee from 4 validator nodes connected
        Then I send instruction successfully with metadata {"issuer" : {"num_clicks" : 1}}
        Then At least 3 out of 4 validator nodes have filled asset data

    @current
    Scenario: Start asset
        Given I have a seed node NODE1
        And I have wallet WALLET1 connected to all seed nodes
        When I mine 9 blocks using wallet WALLET1 on NODE1
        Then I wait for wallet WALLET1 to have at least 1000000 uT
        When I wait 30 seconds
        When I register an NFT asset with committee of 4
        And I mine 3 blocks
        And I create 40 NFTs
        And I mine 3 blocks