# RFC-0365/ConstitutionChanges

## Constitution Changes

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Miguel Naveira](https://github.com/mrnaveira)

# Licence

[ The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2022. The Tari Development Community

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
[BCP 14](https://tools.ietf.org/html/bcp14) (covering RFC2119 and RFC8174) when, and only when, they appear in all capitals, as shown here.

## Disclaimer

This document and its content are intended for information purposes only and may be subject to change or update
without notice.

This document may include preliminary concepts that may or may not be in the process of being developed by the Tari
community. The release of this document is intended solely for review and discussion by the community regarding the
technological merits of the potential system outlined herein.

## Goals
The aim of this document is to describe the mechanisms of contract constitution changes on the Tari Digital Asset network (DAN).

## Related Requests for Comment
* [RFC-0312: High level Digital Asset Network Specification](RFC-0312_DANHighLevelSpecification.md)

## Description
After the contract definition transaction, the asset issuer publishes the contract constitution transaction. That transaction (UTXO) defines how a contract is managed, including, among others:
1. Validator Node Committee (VNC) composition. This includes the rules over how members are added or removed. It may be that the VNC has autonomy over these changes, or that the asset issuer must approve changes, or some other authorization mechanism.
2. Side-chain medatada record: the consensus algorithm to be used and the checkpoint quorum requirements.
3. Checkpoint parameters record: minimum checkpoint frequency, comitee change rules (e.g. asset issuer must sign, or a quorum of VNC members, or a whitelist of keys).

Note that changes (how and when) in both side-chain metadata record and checkpoint parameters record MAY be specified in the contract constitution UTXO, inside a`RequirementsForConstitutionChange` record. If omitted, the checkpoint parameters and side-chain metadata records are immutable via covenant.

Then, during the contract execution, we support the change of any of those three parameters. The constitution changes MUST happen at checkpoints.

This document describes how those constituion changes are implemented.

### Who can propose constitution changes


### Stages of constitution changes
Each stage requrires one checkpoint

#### Proposal
#### Validation
#### Acceptance
#### Activation

### Use case example
main use case is the committee changing