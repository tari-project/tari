@auto_update
Feature: AutoUpdate

    # Not sure why this takes so long on CI
    @long-running @broken
    Scenario: Auto update finds a new update on base node
        Given I have a node NODE_A with auto update enabled
        Then NODE_A has a new software update

    @broken
    Scenario: Auto update ignores update with invalid signature on base node
        Given I have a node NODE_A with auto update configured with a bad signature
        And I wait 10 seconds
        Then NODE_A does not have a new software update
