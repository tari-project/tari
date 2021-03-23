@merge-mining
  Feature: Merge Mining

    @critical
    Scenario: Merge Mining Functionality Test Without Submitting To Origin
      Given I have a seed node NODE
      And I have wallet WALLET connected to all seed nodes
      And I have a merge mining proxy PROXY connected to NODE and WALLET with origin submission disabled
      When I ask for a block height from proxy PROXY
      Then Proxy response height is valid
      When I ask for a a block template from proxy PROXY
      Then Proxy response block template is valid
      When I submit a block through proxy PROXY
      Then Proxy response block submission is valid without submitting to origin

    @critical
    Scenario: Merge Mining Functionality Test With Submitting To Origin
      Given I have a seed node NODE
      And I have wallet WALLET connected to all seed nodes
      And I have a merge mining proxy PROXY connected to NODE and WALLET with origin submission enabled
      When I ask for a block height from proxy PROXY
      Then Proxy response height is valid
      When I ask for a a block template from proxy PROXY
      Then Proxy response block template is valid
      When I submit a block through proxy PROXY
      Then Proxy response block submission is valid with submitting to origin

    @critical
    Scenario: Simple Merge Mining
      Given I have a seed node NODE
      And I have wallet WALLET connected to all seed nodes
      And I have a merge mining proxy PROXY connected to NODE and WALLET with default config
      When I merge mine 2 blocks via PROXY
      Then all nodes are at height 2

