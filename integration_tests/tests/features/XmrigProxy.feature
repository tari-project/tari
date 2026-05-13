# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@xmrig-proxy @base-node
Feature: XMRig Proxy API

  @critical
  Scenario: GET /getinfo reflects mined blocks
    Given I have a seed node NODE
    When I mine 3 blocks on NODE
    When I call GET /getinfo on proxy of node NODE
    Then XMRig getinfo response height matches node height

  @critical
  Scenario: GET /getheight reflects mined blocks
    Given I have a seed node NODE
    When I mine 3 blocks on NODE
    When I call GET /getheight on proxy of node NODE
    Then XMRig getheight response height matches node height

