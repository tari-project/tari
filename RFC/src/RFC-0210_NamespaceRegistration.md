# RFC-0210-NamespaceRegistration

## Namespace Registration on the Base Layer

![status: raw](C:/Users/pluto/Documents/Code/tari-project/RFC/src/theme/images/status-raw.svg)

**Maintainer(s)**: [Hansie Odendaal](https://github,com/hansieodendaal)

# License

[ The 3-Clause BSD License](https://opensource.org/licenses/BSD-3-Clause).

Copyright <YEAR> <COPYRIGHT HOLDER | The Tari Development Community>

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
following conditions are met:

1. Redistributions of this document must retain the above copyright notice, this list of conditions and the following
   disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following
   disclaimer in the documentation and/or other materials provided with the distribution.
3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products
   derived from this software without specific prior written permission.

THIS DOCUMENT IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS ORrain
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", 
"NOT RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in 
[BCP 14](https://tools.ietf.org/html/bcp14) (covering RFC2119 and RFC8174) when, and only when, they appear in all capitals, as 
shown here.

## Disclaimer

The purpose of this document and its content is for information purposes only and may be subject to change or update
without notice.

This document may include preliminary concepts that may or may not be in the process of being developed by the Tari
community. The release of this document is intended solely for review and discussion by the community regarding the
technological merits of the potential system outlined herein.

## Goals

This document will describe the process for Registered Asset Issuer Name (RAIN) registration on the Tari base layer.

## Related RFCs

- [RFC-0100: Base layer](RFC-0100_BaseLayer.md)

# Description

## Abstract

???

## Notes - To be deleted

- Initial idea:
  - Text string
  - Cost inversely proportional to string length, non-linear, maybe exponential or quadratic 
  - Miners to validate

- Kernel feature
  - new output feature; 
    - RAIN - str[16]
    - Pubkey  - ???
- Features
  - It costs Tari to register a RAIN
  - Must be able to transfer RAIN
- On asset create
  - Create RAIN & sig
- RAIN sell Tx
  - Inputs
    - $ v_{in} ​$ (in Tari coins)
    - RAIN output of seller - $ \Re_{seller} : (str , Pk_s) ​$ 
  - Outputs
    - RAIN output to buyer - $ \Re_{seller} : (str , Pk_b) $ 
    - Change to buyer - $ v_{in} - v_{cost} ​$ 
    - Payment to Seller - $ v_{cost} $ 







