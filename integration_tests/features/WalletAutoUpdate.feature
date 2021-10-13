@auto_update
Feature: AutoUpdate

    @broken
    Scenario: Auto update finds a new update on wallet
        Given I have a wallet WALLET with auto update enabled
        Then WALLET has a new software update

    @broken
    Scenario: Auto update ignores update with invalid signature on wallet
        Given I have a wallet WALLET with auto update configured with a bad signature
        Then WALLET does not have a new software update
