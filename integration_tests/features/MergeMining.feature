@merge-mining
  Feature: Merge Mining

    @critical
    Scenario: Simple Merge Mining
      Given I have a seed node NODE
      And I have wallet WALLET connected to all seed nodes
      And I have a merge mining proxy PROXY connected to NODE and WALLET
      When I merge mine 2 blocks via PROXY
      Then all nodes are at height 2
