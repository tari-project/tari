Feature: Transaction Info

  Scenario: Get Transaction Info
    Given I have a seed node NODE
    And I have 1 base nodes connected to all seed nodes
    And I have wallet WALLET_A connected to all seed nodes
    And I have wallet WALLET_B connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and WALLET_A
    When I merge mine 2 blocks via PROXY
    Then all nodes are at height 2
    When I wait 10 seconds
    And I send 1000000 tari from WALLET_A to one wallet WALLET_B at fee 100
    When I wait 10 seconds
    Then Transaction status of last result from WALLET_A to WALLET_B is known to both wallets
    When I merge mine 2 blocks via PROXY
    Then all nodes are at height 4
    When I wait 10 seconds
    Then Transaction status of last result from WALLET_A to WALLET_B is known to both wallets
