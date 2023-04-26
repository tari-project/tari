# Copyright 2023 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@chat-ffi @critical
Feature: Chat FFI messaging

  Scenario: A message is propagated between an FFI node and native client via 3rd party
    Given I have a seed node SEED_A
    When I have a chat FFI client CHAT_A connected to seed node SEED_A
    When I have a chat client CHAT_B connected to seed node SEED_A
    When I use CHAT_A to send a message 'Hey there' to CHAT_B
    Then CHAT_B will have 1 message with CHAT_A

  Scenario: A message is sent directly between two FFI clients
    Given I have a seed node SEED_A
    When I have a chat FFI client CHAT_A connected to seed node SEED_A
    When I have a chat FFI client CHAT_B connected to seed node SEED_A
    When CHAT_A adds CHAT_B as a contact
    When I stop node SEED_A
    When CHAT_A waits for contact CHAT_B to be online
    When I use CHAT_A to send a message 'Hey there' to CHAT_B
    Then CHAT_B will have 1 message with CHAT_A
