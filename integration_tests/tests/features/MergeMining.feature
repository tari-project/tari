# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@merge-mining @base-node
Feature: Merge Mining

  @broken
  Scenario: Merge Mining Functionality Test Without Submitting To Origin
    Given I have a seed node NODE
    When I have wallet WALLET connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and WALLET with origin submission disabled
    When I wait 2 seconds
    When I ask for a block height from proxy PROXY
    Then Proxy response height is valid
    When I ask for a block template from proxy PROXY
    Then Proxy response block template is valid
    When I submit a block through proxy PROXY
    Then Proxy response block submission is valid without submitting to origin

  @broken
  Scenario: Merge Mining Functionality Test With Submitting To Origin
    Given I have a seed node NODE
    When I have wallet WALLET connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and WALLET with origin submission enabled
    When I ask for a block height from proxy PROXY
    Then Proxy response height is valid
    When I ask for a block template from proxy PROXY
    Then Proxy response block template is valid
    When I submit a block through proxy PROXY
    Then Proxy response block submission is valid with submitting to origin
    When I ask for the last block header from proxy PROXY
    Then Proxy response for last block header is valid
    When I ask for a block header by hash using last block header from proxy PROXY
    Then Proxy response for block header by hash is valid

  # BROKEN: get_block_template returns error 500
  @critical @broken
  Scenario: Simple Merge Mining
    Given I have a seed node NODE
    When I have wallet WALLET connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and WALLET with default config
    When I merge mine 2 blocks via PROXY
    Then all nodes are at height 2

