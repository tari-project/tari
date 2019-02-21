# RFC-0210/NamespaceRegistration

## Namespace Registration on the Base Layer

![status: raw](./theme/images/status-raw.svg)

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
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS ORRAID
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

This purpose of this document is to describe and specify the process for creating and linking an asset issuer specified domain name with a digital asset on the Digital Assets Network ([DAN]).



## Related RFCs

- [RFC-0100: Base layer](RFC-0100_BaseLayer.md)
- [RFC-0310/AssetTemplates](RFC-0310_AssetTemplates.md)

<br>

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
    - Provide RAIN & sign
    - Miners must verify $ str ​$ has no been used before
  - RAIN sell Tx
    - Inputs
      - [Tari coin]s payment of buyer - $ v_{in} ​$ 
      - RAIN of seller - $ \Re_{seller} : (str , Pk_s) $ 
    - Outputs
      - RAIN to buyer - $ \Re_{buyer} : (str , Pk_b) $ 
      - Tari coins change to buyer - $ v_{in} - v_{cost} ​$ 
      - Tari coins payment to seller - $ v_{cost} ​$ 
    - $ \text{Hash} (\text{prior block hash} \parallel str \parallel Pk) ​$ 
    - $ vG + (k_1 + k_b)H ​$ 
    - $ vG + k_0H + k_b I _e ​$ 
    - $ vG + kH = C ​$ 

- Discussion 2 & 3

  - Make use of text records of a Fully Qualified Domain Name (FQDN) in the Domain Name System (DNS), as developed by [OpenAlias](https://openalias.org/) 

  - As DNS records are write only by their owners, fraudulent activity should be minimized. Maintenance of the DNS records will always be the responsibility of the owner.

  - A RAID UTXO consists of tuple `(RAID_ID, PubKey)`

  - Sequence of events for RAID registration and linking to a digital asset

    -  Register RAID on the [base layer]

      - Asset issuer create `RAID_ID = Hash(PubKey || FQDN)` 
      - Asset issuer create a RAID registration Tx and post it to the [base layer]
      - Miners validate Tx as per RAID registration validation rules

    - Create FQDN text record entry

      - The DNS owner (whom may be the asset issuer) create the OpenAlias type text record:

        `"oa1:tari_RAID` 

        `recipient_address=<RAID_ID>;` 

        `recipient_name=<FQDN>;` 

        `tx_description=<optional description>;` 

        `address_signature=<Asset issuer signature (containing the FQDN and RAID_ID)>;`

        `checksum=<CRC-32 cheksum of the entire record up to but excluding the checksum key-value pair>;"`

    - Create asset on DAN and link the RAID_ID

      - Asset issuer create asset from template, supplying the FQDN and `RAID_ID`
      - Asset issuer post asset create Tx to VNs
      - VNs verify valid `RAID_ID` and valid FQDN text record
      - VNs create asset in DAN
      - VNs register asset proof on the [base layer]
      - VNs update RAID to FQDN hash table



## Background

In order to easily differentiate different digital assets in the DAN, apart from some unique unpronounceable character string, a human readable identifier (domain name) is required. It is perceived that shorter names will have higher value due to branding and marketability, and the question is how this can be managed elegantly. It is also undesirable if for example the real Disney is forced to use the long versioned "*disney.com-goofy-is-my-asset-yes*" because some fake Disneys claimed "*goofy*" and "*disney.com-goofy*" and everything in between.

One method to curb name space squatting is to register names on the [base layer] layer with a domain name registration transaction. Let us call such a name a Registered Asset Issuer Name (RAIN). To make registering RAINs difficult enough to prevent spamming the network, a certain amount of [Tari coin]s must be committed in a burn (permanently destroy) or a time locked pay to self type transaction. Lots of management overhead will be associated with such a scheme, even if domain-less assets are allowed. Although name space squatting will be curbed, it would be impossible to stop someone from registering say a "*disney.com*" RAIN if they do not own the real "*disney.com*" Fully Qualified Domain Name (FQDN).

Another approach would be to make use of the public Domain Name System (DNS) and to link the FQDNs, that are already registered, to the digital assets in the DAN, making use of [OpenAlias](https://openalias.org/) text (TXT) DNS records on a FQDN. Let us call this a Registered Asset Issuer Domain (RAID) TXT record. If we hash a public key and FQDN pair, it will provide us with a unique RAID_ID (RAID Identification). The RAID_ID will serve a similar purpose in the DAN as a Top Level Domain (TLD) in a DNS, as all digital assets belonging to a specific asset issuer could then be grouped under the FQDN by inference. To make this scheme more elaborate, but potentially unnecessary, all such RAIN_IDs could be also be registered on the [base layer] layer, similar to the RAIN scheme.

If the standard Mimblewimble protocol is to be followed, a new output feature can be defined to cater for a RAID tuple `(RAID_ID, PubKey)` that is linked to a specific Unspent Transaction Output ([UTXO]). The `RAID_ID` could be based on a [Base58Check](https://en.bitcoin.it/wiki/Base58Check_encoding) variant applied to `Hash(PubKey || FQDN)`. If the amount of Tari coin associated with the RAID tuple transaction is burned, `(RAID_ID, PubKey)` will forever be present on the blockchain and can assist with blockchain bloat. On the other hand, if that [Tari coin] is spent back to its owner with a specific time lock, it will be possible to spend and prune that UTXO later on. While that UTXO remains unspent, the RAID tuple will be valid, but when all or part of it is spent, the RAID tuple will disappear from the blockchain. Such a UTXO will thus be "colored" while unspent as it will have different properties to a normal UTXO. It will also be possible to recreate the original RAID tuple by registering it using the original `Hash(PubKey || FQDN)`.

Thinking about the makeup of the `RAID_ID` it is evident that it can easily be calculated on the fly using the public key and FQDN, both of which values will always be known. The biggest advantage having the RAID tuple on the [base layer] is that of embedded consensus, where it will be validated (as no duplicates can be allowed) and mined before it can be used. However, this comes at the cost of more complex code, a more elaborate asset registration process and higher asset registration fees.

This document explores the creation and use of RAID TXT records to link asset issuer specified domain names with digital assets on the [DAN], without RAID_IDs being registered on the [base layer].



## Requirements



### OpenAlias TXT DNS Records

An [OpenAlias](https://openalias.org/) TXT DNS record on a FQDN starts with "*oa1:\<name\>*" followed by a number of key-value pairs. Standard (optional) key-values are: "*recipient_address*"; "*recipient_name*"; "*tx_description*"; "*tx_amount*"; "*tx_payment_id*"; "*address_signature*" and "*checksum*". Only entities with write access to a specific DNS record will be able to create the required TXT DNS record entries.

**Req** - Integration with public DNS records MUST be used to ensure valid ownership of a FQDN. 

**Req** - The [OpenAlias](https://openalias.org/) TXT DNS record that links a `RAIN_ID` and FQDN must be constructed as follows:

| OpenAlias TXT DNS Record Field | OpenAlias TXT DNS Record Data                                |
| ------------------------------ | ------------------------------------------------------------ |
| oa1:\<name\>                   | "oa1:tari_RAID"                                              |
| recipient_address              | \<`RAID_ID`\>                                                |
| recipient_name                 | \<FQDN\>                                                     |
| tx_description                 | \<Optional description\>                                     |
| address_signature              | \<Asset issuer signature (containing the FQDN and RAID_ID)\> |
| checksum"                      | \<CRC-32 cheksum of the entire record up to but excluding the checksum key-value pair\> |



Ownership of a `RAID_ID` MUST be confirmed by the `PubKey` used to create it.

A `RAID_ID` cannot be transferred.

Duplicate `RAID_ID`s cannot be allowed on the [DAN]. Validator Nodes' (VN) validation rules MUST ensure no duplicate `RAID_ID`s are created with asset registration.







[DAN]: Glossary.md#digital-asset-network

[UTXO]: Glossary.md#unspent-transaction-outputs

[base layer]: Glossary.md#base-layer

[Tari coin]: Glossary.md#tari-coin


