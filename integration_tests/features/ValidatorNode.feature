Feature: Validator Node
    Scenario: Test committee
        Given I have committee from 4 validator nodes connected
        Then I send instruction successfully with metadata {"issuer" : {"num_clicks" : 1}}
        Then At least 3 out of 4 validator nodes have filled asset data