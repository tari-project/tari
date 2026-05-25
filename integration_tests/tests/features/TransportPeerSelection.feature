# Copyright 2026 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

Feature: Transport peer selection

    @critical @transport-selection
    Scenario Outline: Transport mode filters peers and dial addresses
        Given transport peer selection fixtures are available
        Then transport mode <mode> selects peers in order <peer_order>
        Then transport mode <mode> dials addresses in order <dial_order>

        Examples:
            | mode    | peer_order | dial_order |
            | tor     | onion      | onion      |
            | tcp     | tcp        | tcp        |
            | tor_tcp | onion,tcp  | onion,tcp  |
            | tcp_tor | tcp,onion  | tcp,onion  |
