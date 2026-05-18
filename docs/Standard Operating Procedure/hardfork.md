# SOP: Hard Fork for Consensus Changes

This document outlines the Standard Operating Procedure (SOP) for implementing a hard fork that modifies consensus variables.

## Introduction

Tari uses a structure called `ConsensusConstants` to manage consensus-related variables. Each instance of `ConsensusConstants` includes an `effective_from_height` field, which determines the block height from which the new constants will take effect. This height must be chosen carefully to ensure that the majority of the network has upgraded to the new software version before the blockchain reaches this point.

## Procedure

1. Create a new `ConsensusConstants` instance and add it to the vector of constants for the network.
2. Select a future date that provides sufficient time for the network to upgrade.
3. Calculate the `effective_from_height` by estimating the number of blocks between the current height and the chosen date, assuming an average block time of two minutes.