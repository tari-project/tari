# Copyright 2023 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

Feature: Chat FFI messaging

  Scenario: A message is propagated between an FFI node and client via 3rd party
    Given I have a seed node SEED_A
    When I have a chat FFI client CHAT_A connected to seed node SEED_A
    When I have a chat client CHAT_B connected to seed node SEED_A
    When I use CHAT_A to send a message 'Hey there' to CHAT_B
    Then CHAT_B will have 1 message with CHAT_A
