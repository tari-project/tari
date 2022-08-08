# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

  # This file covers all of the GRPC functions for the base node
@base_node_grpc @base_node
Feature: Base Node GRPC
  Scenario: Base node lists headers
    Given I have 1 seed nodes
    And I have a base node BN1 connected to all seed nodes
    When I mine 5 blocks on BN1
    Then node BN1 lists headers 1 to 5 with correct heights

    @current
  Scenario: Base node get header by hash
    Given I have 1 seed nodes
    And I have base node BN1 connected to all seed nodes
    When I mine 5 blocks on BN1
    And I get the header by height 2 on BN1
    And I save the hash of the last header to HASH1
    And I get the header by hash HASH1 on BN1
    Then header is returned with height 2

  Scenario: Base node lists blocks
    Given I have 1 seed nodes
    And I have a base node N1 connected to all seed nodes
    When I mine 5 blocks on N1
    Then node N1 lists blocks for heights 1 to 5


  Scenario: Base node GRPC - get header by height
    Given I have 1 seed nodes
    And I have a base node BN1 connected to all seed nodes
    When I mine 5 blocks on BN1
    When I get header by height 1 on BN1
    Then header is returned with height 1
