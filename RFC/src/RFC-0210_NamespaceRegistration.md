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

- Discussion 1

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
    - Create RAIN & sign
    - Miners must verify $ str ​$ has no been used before
  - RAIN sell Tx
    - Inputs
      - Tari coins payment of buyer - $ v_{in} $ 
      - RAIN of seller - $ \Re_{seller} : (str , Pk_s) ​$ 
    - Outputs
      - RAIN to buyer - $ \Re_{buyer} : (str , Pk_b) ​$ 
      - Tari coins change to buyer - $ v_{in} - v_{cost} ​$ 
      - Tari coins payment to seller - $ v_{cost} ​$ 
    - $ \text{Hash} (\text{prior block hash} \parallel str \parallel Pk) ​$ 
    - $ vG + (k_1 + k_b)H ​$ 
    - $ vG + k_0H + k_b I _e ​$ 
    - $ vG + kH = C $ 

- Discussion 2

  - Make use of TXT records of a Fully Qualified Domain Name (FQDN) in the Domain Name System (DNS), as developed by [OpenAlias](https://openalias.org/) 

  - As DNS records are write only by their owners fraudulent activity should be minimized. Maintenance of the DNS records will always be the responsibility of the owner.

  - RAIN registration UTXO consists of tuple `(RAIN_ID, PubKey)`

  - Sequence of events for RAIN registration

    - Asset issuer creates a RAIN registration Tx

      - Create `RAIN_ID = Hash(prior block hash || ??? || PubKey)` ​
      - Post Tx to base layer
      - Miners validate Tx as per RAIN registration validation rules

    - DNS owner (whom may be the asset issuer) create FQDN TXT record entry

      `"oa1:tari_rain` 

      `recipient_address=<RAIN_ID>;` 

      `recipient_name=<FQDN>;` 

      `address_signature=<signature (containing the FQDN and RAIN_ID)>;`

      `checksum=<CRC-32 cheksum of the entire record up to but excluding the checksum key-value pair>;"`

    - Asset issuer create the asset Tx

      - Create asset from template
      - Supply FQDN and `RAIN_ID`
      - Post asset create Tx to VNs

    - VNs 

      - Verify `RAIN_ID` on base layer
      - Verify FQDN TXT record
      - Create asset in DAN
      - Register asset on base layer
      - Update RAIN to FQDN hash table



## Background

[OpenAlias](https://openalias.org/) FQDN TXT records composition

- A FQDN TXT record starts with `oa1:<name>` followed by a number of key-value pairs. Standard (optional) key-value are:
  - `recipient_address`
  - `recipient_name`
  - `tx_description`
  - `tx_amount`
  - `tx_payment_id`
  - `address_signature`
  - `checksum`

A RAIN UTXO consists of tuple `(RAIN_ID, PubKey)`. It is identified by a kernel feature flag and is a non-private non-fungible digital asset on the base layer. Ownership is confirmed by the `PubKey` value in the RAIN UTXO tuple. It costs Tari coins to register a RAIN UTXO and it can be transferred. A valid RAIN UTXO tuple is required to register a digital asset on the [Digital Asset Network] (DAN). Miners will ensure that no duplicate `RAIN_ID`s are created with registration.







[Digital Asset Network]: Glossary.md#digital-asset-network








