# Copyright 2022 The Taiji Project
# SPDX-License-Identifier: BSD-3-Clause

@block-explorer
Feature: Block Explorer GRPC

  Scenario: As a user I want to get the network difficulties
    Given I have a seed node NODE
    When I have wallet WALLET connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and WALLET with default config
    When I merge mine 2 blocks via PROXY
    Then all nodes are at height 2
    When I request the difficulties of a node NODE
    # Then difficulties are available
