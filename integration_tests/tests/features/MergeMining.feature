# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@merge-mining @base-node
Feature: Merge Mining

  @critical
  Scenario: Merge Mining Functionality Test
    Given I have a seed node NODE
    When I have wallet WALLET connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and WALLET with default config
    When I ask for a block height from proxy PROXY
    Then Proxy response height is valid
    When I ask for a block template from proxy PROXY
    Then Proxy response block template is valid
    When I submit a block through proxy PROXY
    Then Proxy response block submission is valid

  @critical
  Scenario: Simple Merge Mining
    Given I have a seed node NODE
    When I have wallet WALLET connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and WALLET with default config
    When I merge mine 2 blocks via PROXY
    Then all nodes are at height 2
