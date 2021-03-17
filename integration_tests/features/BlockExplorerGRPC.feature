@block-explorer
Feature: Block Explorer GRPC

  @critical
  Scenario: As a user I want to get the network difficulties
    Given I have a seed node NODE
    And I have wallet WALLET connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and WALLET with default config
    When I merge mine 2 blocks via PROXY
    Then all nodes are at height 2
    When I request the difficulties of a node NODE
    Then difficulties are available
