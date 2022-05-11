# RFC-0500/PaymentChannels

## Payment channels

![status: deprecated](theme/images/status-deprecated.svg)

**Maintainer(s)**: Cayle Sharrock <CjS77>

# Licence

[ The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2019. The Tari Development Community

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
following conditions are met:

1. Redistributions of this document must retain the above copyright notice, this list of conditions and the following
   disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following
   disclaimer in the documentation and/or other materials provided with the distribution.
3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products
   derived from this software without specific prior written permission.

THIS DOCUMENT IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS", AND ANY EXPRESS OR IMPLIED WARRANTIES,
INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## Language

The keywords "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED",
"NOT RECOMMENDED", "MAY" and "OPTIONAL" in this document are to be interpreted as described in
[BCP 14](https://tools.ietf.org/html/bcp14) (covering RFC2119 and RFC8174) when, and only when, they appear in all capitals, as
shown here.

## Disclaimer

This document and its content are intended for information purposes only and may be subject to change or update
without notice.

This document may include preliminary concepts that may or may not be in the process of being developed by the Tari
community. The release of this document is intended solely for review and discussion by the community regarding the
technological merits of the potential system outlined herein.

## Goals

## Related Requests for Comment

* [RFC-0001: Overview](RFC-0001_overview.md)
* [RFC-0300: The Tari Digital Assets Network](RFCD-0300_DAN.md)

## Description

### Introduction

The base layer is slow. The DAN is fast. However, every DAN instruction that results in a state change has a fee (i.e.
base layer transaction) associated with it.  
To bridge the speed gap between the two layers, a payment channel solution is required.  
This document provides the high-level overview of how this is done.

### The Clacks

The Clacks is a multi-party side-channel off-chain scalability proposal for Tari. The essential idea behind the Clacks
is:

* Users give control of some Tari to a Clacks Committee.
* The committee creates an off-chain UTXO for the user(s). This is called the peg-in transaction.
* Users can transact amongst each other without those transactions touching the base layer. This allows a very high
  throughput of transactions and instant finality. However, the locus of trust move significantly towards the Clacks
  committee. In other words, base layer transactions are slow, but do not require users to trust anyone. Clacks
  transactions are fast, but requires some level of trust in the entity or entities controlling the user's funds.
* Users can send off-chain Tari to any other users, including users that have not made deposits into the Clacks
  committee.
* Users can request a withdrawal for their Tari at any time. At predefined intervals, the Clacks committee will process
  those withdrawal requests and give the Tari UTXO control back to the user.

Users also have a pre-signed, time-locked transaction that returns the user's fund back to them. This refund transaction
can be used in case the Clacks committee stops processing withdrawals and provides insurance for users against having
their funds locked up forever.

The peg-in, peg-out cycle are represented by standard Tari transactions. This means that full nodes verify that no funds
have been created or destroyed over the course of the cycle. They do not check that the "balances" associated with users
are what those users would expect in an honestly run side-chain.

However, if the side-chain is run as a standard mimblewimble process, any third party running a full node and that
access to the opening and closing channel balances, can in fact verify that the committee has operated honestly.
