# Copyright 2023 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

Feature: Chat messaging

  Scenario: A message is propagated between nodes
    Given I have a seed node SEED_A
    When I have a chat client CHAT_A connected to seed node SEED_A
    When I have a chat client CHAT_B connected to seed node SEED_A
    When I wait 5 seconds
    When I use CHAT_A to send a message 'Hey there' to CHAT_B
    When I wait 5 seconds
    Then CHAT_B will have 1 message with CHAT_A

  Scenario: A message is sent directly between nodes
    Given I have a seed node SEED_A
    When I have a chat client CHAT_A connected to seed node SEED_A
    When I have a chat client CHAT_B connected to seed node SEED_A
    When I add CHAT_B as a contact to CHAT_A
    When I wait 5 seconds
    When I use CHAT_A to send a message 'Hey there' to CHAT_B
    When I wait 5 seconds
    Then CHAT_B will have 1 message with CHAT_A

  Scenario: Message counts are distinct
    Given I have a seed node SEED_A
    When I have a chat client CHAT_A connected to seed node SEED_A
    When I have a chat client CHAT_B connected to seed node SEED_A
    When I have a chat client CHAT_C connected to seed node SEED_A
    When I wait 5 seconds
    When I use CHAT_A to send a message 'Message 1ab' to CHAT_B
    When I use CHAT_A to send a message 'Message 2ab' to CHAT_B
    When I use CHAT_C to send a message 'Message 1cb' to CHAT_B
    When I use CHAT_A to send a message 'Message 1ac' to CHAT_C
    When I use CHAT_B to send a message 'Message 1bc' to CHAT_C
    When I use CHAT_B to send a message 'Message 2bc' to CHAT_C
    When I wait 5 seconds
    Then CHAT_B will have 2 messages with CHAT_A
    Then CHAT_B will have 3 messages with CHAT_C
    Then CHAT_C will have 1 messages with CHAT_A
    Then CHAT_C will have 3 messages with CHAT_B
    Then CHAT_A will have 2 messages with CHAT_B
    Then CHAT_A will have 1 messages with CHAT_C

