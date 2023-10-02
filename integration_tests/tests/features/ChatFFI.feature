# Copyright 2023 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@chat-ffi @critical
Feature: Chat FFI messaging

  Scenario: A message is propagated between clients via 3rd party
    Given I have a seed node SEED_A
    When I have a chat FFI client CHAT_A connected to seed node SEED_A
    When I have a chat FFI client CHAT_B connected to seed node SEED_A
    When I use CHAT_A to send a message 'Hey there' to CHAT_B
    Then CHAT_B will have 1 message with CHAT_A

  Scenario: Callback for new message received
    Given I have a seed node SEED_A
    When I have a chat FFI client CHAT_A connected to seed node SEED_A
    When I have a chat FFI client CHAT_B connected to seed node SEED_A
    When I use CHAT_A to send a message 'Hey there' to CHAT_B
    Then there will be a MessageReceived callback of at least 1
    Then CHAT_B will have 1 message with CHAT_A

  # Also flaky on CI. Seems liveness has issues on CI
  @broken
  Scenario: Callback for status change is received
    Given I have a seed node SEED_A
    When I have a chat FFI client CHAT_A connected to seed node SEED_A
    When I have a chat FFI client CHAT_B connected to seed node SEED_A
    When CHAT_A adds CHAT_B as a contact
    When CHAT_A waits for contact CHAT_B to be online
    Then there will be a contact status update callback of at least 1

  #This is flaky, passes on local run time, but fails CI
  @broken
  Scenario: A message is sent directly between two FFI clients
    Given I have a seed node SEED_A
    When I have a chat FFI client CHAT_A connected to seed node SEED_A
    When I have a chat FFI client CHAT_B connected to seed node SEED_A
    When CHAT_A adds CHAT_B as a contact
    When CHAT_B adds CHAT_A as a contact
    When CHAT_A waits for contact CHAT_B to be online
    When I use CHAT_A to send a message 'Hey there' to CHAT_B
    Then CHAT_B will have 1 message with CHAT_A

  Scenario: Chat shuts down without any errors
    Given I have a seed node SEED_A
    When I have a chat FFI client CHAT_A connected to seed node SEED_A
    Then I can shutdown CHAT_A without a problem

  Scenario: Reply to message
    Given I have a seed node SEED_A
    When I have a chat FFI client CHAT_A connected to seed node SEED_A
    When I have a chat FFI client CHAT_B connected to seed node SEED_A
    When I use CHAT_A to send a message 'Hey there' to CHAT_B
    When I use CHAT_B to send a reply saying 'oh hai' to CHAT_A's message 'Hey there'
    Then CHAT_B will have 2 messages with CHAT_A
    Then CHAT_A will have 2 messages with CHAT_B
    Then CHAT_A will have a replied to message from CHAT_B with 'oh hai'

  Scenario: A message receives a delivery receipt via FFI
    Given I have a seed node SEED_A
    When I have a chat FFI client CHAT_A connected to seed node SEED_A
    When I have a chat FFI client CHAT_B connected to seed node SEED_A
    When I use CHAT_A to send a message 'Hey there' to CHAT_B
    When CHAT_B will have 1 message with CHAT_A
    Then CHAT_A and CHAT_B will have a message 'Hey there' with matching delivery timestamps

  Scenario: A message receives a read receipt via FFI
    Given I have a seed node SEED_A
    When I have a chat FFI client CHAT_A connected to seed node SEED_A
    When I have a chat FFI client CHAT_B connected to seed node SEED_A
    When I use CHAT_A to send a message 'Hey there' to CHAT_B
    When CHAT_B will have 1 message with CHAT_A
    When CHAT_B sends a read receipt to CHAT_A for message 'Hey there'
    Then CHAT_A and CHAT_B will have a message 'Hey there' with matching read timestamps