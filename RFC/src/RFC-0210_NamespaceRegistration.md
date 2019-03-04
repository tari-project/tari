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



## Background

In order to easily differentiate different digital assets in the DAN, apart from some unique unpronounceable character string, a human readable identifier (domain name) is required. It is perceived that shorter names will have higher value due to branding and marketability, and the question is how this can be managed elegantly. It is also undesirable if for example the real Disney is forced to use the long versioned "*disney.com-goofy-is-my-asset-yes*" because some fake Disneys claimed "*goofy*" and "*disney.com-goofy*" and everything in between.

One method to curb name space squatting is to register names on the [base layer] layer with a domain name registration transaction. Let us call such a name a Registered Asset Issuer Name (RAIN). To make registering RAINs difficult enough to prevent spamming the network, a certain amount of [Tari coin]s must be committed in a burn (permanently destroy) or a time locked pay to self type transaction. Lots of management overhead will be associated with such a scheme, even if domain-less assets are allowed. Although name space squatting will be curbed, it would be impossible to stop someone from registering say a "*disney.com*" RAIN if they do not own the real "*disney.com*" Fully Qualified Domain Name (FQDN).

Another approach would be to make use of the public Domain Name System (DNS) and to link the FQDNs, that are already registered, to the digital assets in the DAN, making use of [OpenAlias](https://openalias.org/) text (TXT) DNS records on a FQDN. Let us call this a Registered Asset Issuer Domain (RAID) TXT record. If we hash a public key and FQDN pair, it will provide us with a unique RAID_ID (RAID Identification). The RAID_ID will serve a similar purpose in the DAN as a Top Level Domain (TLD) in a DNS, as all digital assets belonging to a specific asset issuer could then be grouped under the FQDN by inference. To make this scheme more elaborate, but potentially unnecessary, all such RAIN_IDs could be also be registered on the [base layer] layer, similar to the RAIN scheme.

If the standard Mimblewimble protocol is to be followed, a new output feature can be defined to cater for a RAID tuple `(RAID_ID, PubKey)` that is linked to a specific Unspent Transaction Output ([UTXO]). The `RAID_ID` could be based on a [Base58Check](https://en.bitcoin.it/wiki/Base58Check_encoding) variant applied to `Hash256(PubKey || FQDN)`. If the amount of Tari coin associated with the RAID tuple transaction is burned, `(RAID_ID, PubKey)` will forever be present on the blockchain and can assist with blockchain bloat. On the other hand, if that [Tari coin] is spent back to its owner with a specific time lock, it will be possible to spend and prune that UTXO later on. While that UTXO remains unspent, the RAID tuple will be valid, but when all or part of it is spent, the RAID tuple will disappear from the blockchain. Such a UTXO will thus be "colored" while unspent as it will have different properties to a normal UTXO. It will also be possible to recreate the original RAID tuple by registering it using the original `Hash256(PubKey || FQDN)`.

Thinking about the makeup of the `RAID_ID` it is evident that it can easily be calculated on the fly using the public key and FQDN, both of which values will always be known. The biggest advantage having the RAID tuple on the [base layer] is that of embedded consensus, where it will be validated (as no duplicates can be allowed) and mined before it can be used. However, this comes at the cost of more complex code, a more elaborate asset registration process and higher asset registration fees.

This document explores the creation and use of RAID TXT records to link asset issuer specified domain names with digital assets on the [DAN], without RAID_IDs being registered on the [base layer].



## Requirements



### OpenAlias TXT DNS Records

An [OpenAlias](https://openalias.org/) TXT DNS record on a FQDN starts with "*oa1:\<name\>*" field followed by a number of key-value pairs. Standard (optional) key-values are: "*recipient_address*"; "*recipient_name*"; "*tx_description*"; "*tx_amount*"; "*tx_payment_id*"; "*address_signature*" and "*checksum*". Only entities with write access to a specific DNS record will be able to create the required TXT DNS record entries.

**Req** - Integration with public DNS records MUST be used to ensure valid ownership of a FQDN. 

**Req** - The [OpenAlias](https://openalias.org/) TXT DNS record that links a `RAIN_ID` and FQDN must be constructed as follows:

| OpenAlias TXT DNS Record Field | OpenAlias TXT DNS Record Data                                |
| ------------------------------ | ------------------------------------------------------------ |
| oa1:\<name\>                   | "oa1:tari_RAID"                                              |
| recipient_address              | \<`RAID_ID`\>                                                |
| recipient_name                 | \<FQDN\>                                                     |
| tx_description                 | \<Optional description\>                                     |
| address_signature              | \<Asset issuer signature (containing the FQDN and RAID_ID)\> |
| checksum                       | \<CRC-32 checksum of the entire record up to but excluding the checksum key-value pair\> |



### The `RAID_ID`

Because the `RAID_ID` does not exist as an entity on the base layer or in the [DAN], it cannot be owned or transferred, but only be verified as part of the [OpenAlias](https://openalias.org/) TXT DNS record verification. If an asset creator chooses not to link a `RAID_ID` and FQDN, a default network assigned `RAID_ID` will be used in the digital asset registration process.

**Req** - A FQDN linked (non-default) `RAID_ID` MUST be calculated as follows: `RAID_ID = Base58Check(Hash256(PubKey || FQDN))`.

**Req** - Validator Nodes (VN) MUST verify the [OpenAlias](https://openalias.org/) TXT DNS record to ensure the `RAID_ID` and FQDN are correctly linked:

- The TXT DNS record MUST start with "oa1:tari_RAID".
- The "*recipient_address*" field MUST contain a valid `RAID_ID`.
- The "*recipient_name*" field MUST correspond to the FQDN the TXT DNS record is in.
- The "*address_signature*" field MUST contain a valid asset issuer Schnorr signature: `S = R + e x PubKey` with the challenge `e = RAID_ID`.
- The "*checksum*" field MUST contain a valid CRC-32 checksum.

**Req** - VNs MUST assign a default `RAID_ID` where it will not be linked to a FQDN.

**Req** - VNs MUST only allow a valid `RAID_ID` to be used in the digital asset registration process.



### Sequence of Events

The sequence of events for digital asset registration are perceived as follows:

1. The asset issuer will decide if a default `RAID_ID` or one that is linked to a FQDN must be used for asset registration.
2. The asset issuer will create a valid TXT DNS record (see [The `RAID_ID`](???)).
3. 





[DAN]: Glossary.md#digital-asset-network

[UTXO]: Glossary.md#unspent-transaction-outputs

[base layer]: Glossary.md#base-layer

[Tari coin]: Glossary.md#tari-coin


