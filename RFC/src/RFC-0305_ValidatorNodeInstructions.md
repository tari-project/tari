# RFC-0313/ValidatorNodeInstructions

## Digital Asset Templates

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Stanley Bondi](https://github.com/sdbondi)

# Licence

[The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2022 The Tari Development Community

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
community. The release of this document is intended solely for review and discussion by the community of the
technological merits of the potential system outlined herein.

## Goals

The aim of this Request for Comment (RFC) is to describe a standard instruction format for [validator node committee] 
instructions as well as a procedure for submitting instructions to and between committee members.

The term “instruction” refers to an operation executed by a validator committee on behalf of a user, typically 
resulting in some state change.

## Related Requests for Comment

* [RFC-0312: DAN Specification](RFC-0312_DANHighLevelSpecification.md)

## Overview

An instruction represents a _unit of work_ that is executed by a [validator node committee] for a given contract 
that results in some layer 2 state change. The nature of the state change is determined by the [template]s defined 
within the [contract definition]. Therefore, an instruction data-type must be interpretable by any validator node 
and allow for application/domain-specific data to be represented.

An instruction may be issued by any party that has possession of the requisite credentials to one or more of the 
[validator node committee]. 

This RFC proposes an instruction that represents an authorized call to a contract function. 

### Assumptions

* All instruction messages are sent using the Tari messaging protocol.
* Some proof can be provided that the authenticated user is authorized to perform the requested action. This proof is 
  called a [bearer_token] within this RFC.
  * The [bearer_token] is restricted to the [contract definition], function/s scope and function arguments.
  * The [bearer_token] provides scope granularity.
  * The [bearer_token] is decoupled from the instruction and may be used as many times as permitted.
* The contract or templates provide metadata that can be used for, amongst other things, authorization.
* A function can be uniquely identified by a template id and a function id.

### Instruction format

An instruction is submitted by a user in protobuf format to one or more of the validator node committee members. 

#### Instruction Authentication 
[Instruction Authentication]: #instruction-authentication

The caller MUST provide an authentication signature for the instruction. This proves that the instruction was issued
by the sender and prevents malleability. A [validator node] SHOULD verify this signature before processing the instruction.

```protobuf
message Instruction {
  // Authenticates the caller
  required CallerAuthentication caller_auth = 1;
  // Serialized instruction body
  required bytes body = 2;
}

message CallerAuthentication {
  // A signature (a Schnorr <R, s> tuple) committing to the instruction body.
  // i.e. e = H(<sender_public_key>||<sig_public_nonce>||<serialized body>)
  required Signature signature = 1;
  // The public key of the signer and sender.
  required bytes sender_public_key = 2;
}

// An instruction that is validated and executed by a validator node. 
message InstructionBody {
   // The contract ID that uniquely identifies the contract on which to execute this instruction 
   required bytes contract_id = 1;
   // The stateless proof that the caller is authorized to execute this instruction. 
   required Authorization authorization = 2;
   // The template function to execute.
   required FunctionCall call = 3;
   // A nonce used to disambiguate instructions.
   // It may be desirable to submit the same instruction more than once. This nonce allows
   // a disambiguation between purposefully duplicate instructions.
   required uint64 nonce = 4;
}

message Authorization {
  required BearerToken bearer_token = 1;
}

message BearerToken {
  // ... See RFC on BearerTokens
}

message FunctionCall {
   // An ID that uniquely identifies the template, registered on the base layer.
   required bytes template_id = 1;
   // The function ID defined within a template. (alternative: name string)
   required uint64 function_id = 2;
   // Arguments for the function. The number and type of argument must be valid for the function.
   required Arguments arguments = 3;
}

message Arguments { 
  // Ordered arguments that correspond to the invoked function.
  repeated Argument args = 1;
}

message Argument {
  optional bytes value = 1;
}
```

On receipt of an instruction, the [validator node] MUST authenticate the caller by validating the caller signature.

Once the instruction is authenticated , the validator committee node performs the following actions:
1. Deserialize the instruction body;
2. check that the instruction is well-formed and all required fields are present;
3. check the `contract_id` is managed by the validator and the contract has been initialized;
4. check the `template_id` and `function_id` exist within the contract;
5. MUST verify the [bearer_token] authorizes the execution of the instruction against the [contract] interface;
   * instruction MUST be discarded if the [bearer_token] is revoked;
   * grantee in the [bearer_token] MUST match the public key in the [Instruction Authentication];
   * [bearer_token] MUST be valid for the provided `contract_id`;
   * [bearer_token] MUST be valid for the invoked function scope/s and function arguments.
6. check if the instruction already exists in the mempool, if so, discard the instruction;
    * Instruction uniqueness can be determined by hashing the authenticated message body.
7. store the instruction in the mempool;
8. and lastly, propagate the authentication and instruction messages to other validators in the committee.

### Submission of Instructions to a Validator Node Committee

A client MUST submit the instruction to be executed using the Tari messaging protocol. A validator node MAY
also expose a GRPC interface to be used for instruction submission.

The instruction:
* MUST be sent to one or more nodes participating within the validator node committee;
* MAY be sent directly to one or more members of the contract's validator node committee; 
* MAY be propagated on the Tari network to a member of the contract's validator node committee by transmitting an 
  encrypted message to one or more base nodes;

#### Dissemination of Instructions within a Committee

For consensus and message propagation efficiency, each validator node SHOULD maintain an active peer connection 
to the other validator nodes within the committee. 

* On receipt of a new valid instruction, the validator node SHOULD send the instructions to the majority or all 
  of the validator node committee so that it may be verified and added to their instruction mempool. 
* In some cases (e.g. HotStuff consensus), a validator MAY only send the instruction to the current and subsequent 
  leader or block proposer nodes to reduce message traffic.

[RFC-0301]: RFC-0301_NamespaceRegistration.md
[Schnorr signature]: https://tlu.tarilabs.com/cryptography/introduction-schnorr-signatures
