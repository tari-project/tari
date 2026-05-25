# Copyright 2026 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

Feature: Transport peer selection

    Scenario Outline: Transport mode controls peer protocol preference
        Then peer selection for transport mode <mode> prefers protocols "<protocols>"

        Examples:
            | mode    | protocols    |
            | tor     | onion        |
            | tcp     | ip4,ip6      |
            | tor_tcp | onion,ip4,ip6 |
            | tcp_tor | ip4,ip6,onion |

    Scenario: Default peer transport prefers TCP then Tor
        Then default peer transport mode is tcp_tor
