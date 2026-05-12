# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@xmrig-proxy @base-node
Feature: XMRig Proxy API

  @critical
  Scenario: JSON-RPC getheight endpoint
    Given I have a seed node NODE
    When I call JSON-RPC getheight on proxy of node NODE
    Then XMRig getheight response is valid

  @critical
  Scenario: GET /getheight endpoint
    Given I have a seed node NODE
    When I call GET /getheight on proxy of node NODE
    Then XMRig getheight response is valid

  @critical
  Scenario: JSON-RPC getinfo endpoint
    Given I have a seed node NODE
    When I call JSON-RPC getinfo on proxy of node NODE
    Then XMRig getinfo response is valid

  @critical
  Scenario: GET /getinfo endpoint
    Given I have a seed node NODE
    When I call GET /getinfo on proxy of node NODE
    Then XMRig getinfo response is valid

  @critical
  Scenario: JSON-RPC getheight reflects mined blocks
    Given I have a seed node NODE
    When I mine 3 blocks on NODE
    When I call JSON-RPC getheight on proxy of node NODE
    Then XMRig getheight response height matches node height

  @critical
  Scenario: GET /getheight reflects mined blocks
    Given I have a seed node NODE
    When I mine 3 blocks on NODE
    When I call GET /getheight on proxy of node NODE
    Then XMRig getheight response height matches node height
