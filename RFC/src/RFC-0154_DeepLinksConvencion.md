# RFC-0154/DeepLinksConvention

## Deep links structure convention.

![status: raw](theme/images/status-draft.svg)

**Maintainer(s)**: [Adrian Truszczyński](https://github.com/TruszczynskiA)

# Licence

[ The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

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
community. The release of this document is intended solely for review and discussion by the community regarding the
technological merits of the potential system outlined herein.

## Goals

The aim of this Request for Comment (RFC) is to specify the deep links structure used in the Tari Aurora project.
The primary motivation is to create a simple, human-readable, and scalable way to structure deep links used by the Tari Aurora clients.

## Description

Deep links are the URIs with hierarchical components sequence. We can use this sequence to pass and handle data in a standardized and predictable way. To do that, we need to pass three components to the target client: the scheme, command, and data.

### Scheme
The scheme is used to address the client, which will handle the command and data components. In the Tari Aurora project, we're using the `tari` scheme to open the wallet app and execute the command.

### Command
The command is a path string used to pass information about the action that should be performed by the client. The handler uses this command to determine how to deserialize the data, before passing it to the command function.
To support multiple networks the first path component should be the name of corresponding network. Before proceeding with the command, the client should first check that the active account is pointed to the correct network, or show an error if there is a mismatch. 

For simple actions, the command can be defined as a single phrase. For more complex actions, the command should be defined as a multi-path where the second path component should be the name of the action group.

#### Examples:

Simple Actions:
```ignore
mainnet/user_profile
testnet/login
```
Complex Actions:
```ignore
mainnet/payments/send
testnet/payments/request
```

### Data
The data component is an optional string of key-value pairs used by the parser/decoder to deserialize the data, which the client passes to the command function. The data component should be formatted in the same way as a URL query. The sub-component should have a `?` prefix, the key-value pairs should be separated by `&`, and every key should be separated from the value by `=` character.

### The Structure
Combining all three components, they will form a deep link with a structure presented below:
```ignore
{scheme}://{command}?{data}
```
#### Examples:
```ignore
tari://mainnet/profile
tari://mainnet/profile/username
tari://testnet/payments/send?amount=1.23&pubKey=01234556789abcde
```

### Deeplinks in use:

* `/transactions/send`
   
The data contains transaction information used in the send tokens process.

| Value Name | Value Type | Note                         |
| ---------- | :--------: | ---------------------------- |
| publicKey  | String     | Receiver's public key        |
| amount     | UInt64?    | The amount in micro Tari     |
| note       | String?    | Note passed with transaction |

* `/base_nodes/add`

The data contains a custom base node configuration. This deep link adds a new base node configuration to the pool and switches to the added base node. 

| Value Name | Value Type | Note                                                        |
| ---------- | :--------: | ----------------------------------------------------------- |
| name       | String     | The name of the base node                                   |
| peer       | String     | Base node's public link and onion address combined together |  