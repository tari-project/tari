# RFC-0301/NamespaceRegistration

## Namespace Registration on Base Layer

![status: outdated](../../book/theme/images/status-outofdate.svg)

**Maintainer(s)**: [Hansie Odendaal](https://github.com/hansieodendaal)

# Licence

[The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2019 The Tari Development Community

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

The aim of this Request for Comment (RFC) is to describe and specify the process for creating and linking an [asset issuer] 
specified domain name with a [digital asset] on the Digital Assets Network ([DAN]).



## Related Requests for Comment

- [RFC-0341/AssetRegistration](RFC-0341D_AssetRegistration.md)
- [RFC-0310/AssetTemplates](RFC-0311_AssetTemplates.md)

<br>

# Description



## Background

### Alternative Approaches

In order to easily differentiate [digital asset]s in the DAN, apart from some unique unpronounceable 
character string, a human-readable identifier (domain name) is required. It is perceived that shorter names will have 
higher value due to branding and marketability. The question is, how this can be managed elegantly? It is also 
undesirable if, for example, the real Disney is forced to use the long versioned "*disney.com-goofy-is-my-asset-yes*" 
because some fake Disneys claimed "*goofy*" and "*disney.com-goofy*" and everything in-between.

One method to curb name space squatting is to register names on the [base layer] layer with a domain name registration 
transaction. Let us call such a name a Registered Asset Issuer Name (RAIN). To make registering RAINs difficult enough 
to prevent spamming the network, a certain amount of [Tari coins] must be committed in a burn (permanently destroy) or 
a time-locked pay-to-self type transaction. Lots of management overhead will be associated with such a scheme, even if 
domainless assets are allowed. However, it would be impossible to stop someone 
from registering, say, a "*disney.com*" RAIN if they did not own the real "*disney.com*" Fully Qualified Domain Name (FQDN).

Another approach would be to make use of the public Domain Name System (DNS) and to link the FQDNs that are already 
registered, to the [digital asset]s in the DAN, making use of [OpenAlias](https://openalias.org/) text (TXT) DNS 
records on an FQDN. Let us call this a Registered Asset Issuer Domain (RAID) TXT record. If we hash a public key and 
FQDN pair, it will provide us with a unique RAID_ID (RAID Identification). The RAID_ID will serve a similar purpose in 
the DAN as a Top Level Domain (TLD) in a DNS, as all digital assets belonging to a specific [asset issuer] could then 
be grouped under the FQDN by inference. To make this scheme more elaborate, but potentially unnecessary, all such 
RAID_IDs could also be registered on the [base layer], similar to the RAIN scheme.

If the standard Mimblewimble protocol is followed, a new output feature can be defined to cater for a RAID tuple 
`(RAID_ID, PubKey)` that is linked to a specific Unspent Transaction Output ([UTXO]). The `RAID_ID` could be based on 
a [Base58Check](https://en.bitcoin.it/wiki/Base58Check_encoding) variant applied to `Hash256(PubKey || FQDN)`. If the 
amount of Tari coins associated with the RAID tuple transaction is burned, `(RAID_ID, PubKey)` will forever be present 
on the blockchain and can increase blockchain bloat. On the other hand, if those [Tari coins] are spent back to their 
owner with a specific time lock, it will be possible to spend and prune that UTXO later on. While that UTXO remains 
unspent, the RAID tuple will be valid, but when all or part of it is spent, the RAID tuple will disappear from the 
blockchain. Such a UTXO will thus be "coloured" while unspent, as it will have different properties to a normal UTXO. It 
will also be possible to recreate the original RAID tuple by registering it using the original `Hash256(PubKey || FQDN)`.

Thinking about the make-up of the `RAID_ID`, it is evident that it can easily be calculated on the fly using the public 
key and FQDN, the values of which will always be known. The biggest advantage of having the RAID tuple on the [base 
layer] is that of embedded consensus, where it will be validated (as no duplicates can be allowed) and mined before it 
can be used. However, this comes at the cost of more complex code, a more elaborate asset registration process and 
higher asset registration fees.

### This Request for Comment

This RFC explores the creation and use of RAID TXT records to link asset issuer-specified domain names with 
digital assets on the [DAN], without RAID_IDs being registered on the [base layer].



## Requirements



### OpenAlias TXT DNS Records

An OpenAlias TXT DNS record [[1]] on an FQDN is a single string and starts with "*oa1:\<name\>*" field followed by a 
number of key-value pairs. Standard (optional) key values are: "*recipient_address*"; "*recipient_name*"; 
"*tx_description*"; "*tx_amount*"; "*tx_payment_id*"; "*address_signature*"; and "*checksum*". Additional key values 
may also be defined. Only entities with write access to a specific DNS record will be able to create the required TXT 
DNS record entries.

TXT DNS records are limited to multiple strings of size 255, and as the User Datagram Protocol (UDP) size is 512 bytes, 
a TXT DNS record that exceeds that limit is less optimal [[2], Sections 3.3 and 3.4]. Some hosting sites also place 
limitations on TXT DNS record string lengths and concatenation of multiple strings as per [[2]]. The basic idea of this 
specification is to make the implementation robust and flexible at the same time.

**Req** - Integration with public DNS records MUST be used to ensure valid ownership of an FQDN that needs to be 
linked to a [digital asset] on the DAN. 

**Req** - The total size of the OpenAlias TXT DNS record SHOULD NOT exceed 255 characters.

**Req** - The total size of the OpenAlias TXT DNS record MUST NOT exceed 512 characters.

**Req** - The OpenAlias TXT DNS record implementation MUST make provision to interpret entries that are made up of more 
than one string as defined in [[2]].

**Req** - The OpenAlias TXT DNS record SHOULD adhere to the formatting requirements as specified in [[1]].

**Req** - The OpenAlias TXT DNS record MUST be constructed as follows:

| OpenAlias TXT DNS Record Field | OpenAlias TXT DNS Record Data                                |
| ------------------------------ | ------------------------------------------------------------ |
| oa1:\<name\>                   | "oa1:tari"                                                   |
| pk                             | \<256 bit public key, in hexadecimal format (64 characters), that is converted into a `Base58` encoded string (44 characters)\> |
| raid_id                        | \<`RAID_ID` (*refer to [RAID_ID](#raid_id)*) (15 characters)\> |
| nonce                          | \<256 bit public nonce, in hexadecimal format (64 characters), that is converted into a `Base58` encoded string (44 characters)\> |
| sig                            | \<[Asset issuer]'s 256 bit Schnorr signature for the `RAID_ID` (*refer to [RAID_ID](#raid_id)*), in hexadecimal format (64 characters), that is converted into a `Base58` encoded string (44 characters)\> |
| desc                           | \<Optional RAID description\>; ASCII String; Up to 48 characters for the condensed version (using only one string) and up to 235 characters (when spanning two strings). |
| crc                            | \<CRC-32 checksum of the entire record, up to but excluding the checksum key-value pair (starting at "**oa1:tari**" and ending at the last "**;**" before the checksum key-value pair) in hexadecimal format (eight characters)\> |

&nbsp;&nbsp;&nbsp;&nbsp;**Examples:** Two example OpenAlias TXT DNS records are shown; the first is a condensed version and 
the second spans two strings:

``` text
RAID_ID:
public key    = ca469346d7643336c19155fdf5c6500a5232525ce4eba7e4db757639159e9861
FQDN          = disney.com
 -> id        = RYqMMuSmBZFQkgp
   
base58 encodings:
public key    = ca469346d7643336c19155fdf5c6500a5232525ce4eba7e4db757639159e9861
 -> base58    = EcbmnM6PLosBzpyCcBz1TikpNXRKcucpm73ez6xYfLtg
public nonce  = fc2c5fce596338f43f70dc0ce14659fdfea1ba3e588a7c6fa79957fc70aa1b4b
 -> base58    = 5ctFNnCfBrP99rT1AFmj1WPyMD8uAdNUTESHhLoV3KBZ
signature     = 7dc54ec98da2350b0c8ed0561537517ac6f93a37f08a34482824e7df3514ce0d
 -> base58    = 9TxTorviyTJAaVJ4eY4AQPixwLb6SDL4dieHff6MFUha
   
OpenAlias TXT DNS record (condensed: 212 characters):
IN   TXT = "oa1:tari pk=EcbmnM6PLosBzpyCcBz1TikpNXRKcucpm73ez6xYfLtg;id=RYqMMuSmBZFQkgp;
nonce=5ctFNnCfBrP99rT1AFmj1WPyMD8uAdNUTESHhLoV3KBZ;sig=9TxTorviyTJAaVJ4eY4AQPixwLb6SDL4dieHff6MFUha;
desc=Cartoon characters;crc=176BE80C"

OpenAlias TXT DNS record (spanning two strings: string 1 = 179 characters and string 2 = 250 characters):
IN   TXT = "oa1:tari pk=EcbmnM6PLosBzpyCcBz1TikpNXRKcucpm73ez6xYfLtg; id=RYqMMuSmBZFQkgp; 
nonce=5ctFNnCfBrP99rT1AFmj1WPyMD8uAdNUTESHhLoV3KBZ; sig=9TxTorviyTJAaVJ4eY4AQPixwLb6SDL4dieHff6MFUha; "
"desc=Cartoon characters: Mickey Mouse\; Minnie Mouse\; Goofy\; Donald Duck\; Pluto\; Daisy Duck\; 
Scrooge McDuck\; Launchpad McQuack\; Huey, Dewey and Louie\; Bambi\; Thumper\; Flower\; Faline\; 
Tinker Bell\; Peter Pan and Captain Hook.; crc=54465902"
```



### RAID_ID

Because the `RAID_ID` does not exist as an entity on the base layer or in the [DAN], it cannot be owned or 
transferred, but can only be verified as part of the OpenAlias TXT DNS record [[1]] verification. If an asset creator 
chooses not to link a `RAID_ID` and FQDN, a default network-assigned `RAID_ID` will be used in the digital asset 
registration process.

**Req** - A default `RAID_ID` MUST be used where it will not be linked to an FQDN, e.g. it MAY be derived
from a default input string `"No FQDN"`.

**Req** - An FQDN-linked (non-default) `RAID_ID` MUST be derived from the concatenation `PubKey || <FQDN>`.

**Req** - All concatenations of inputs for any hash algorithm MUST be done without adding any spaces.

**Req** - The hash algorithm MUST be `Blake2b` using a 32 byte digest size, unless otherwise specified.

**Req** - Deriving a `RAID_ID` MUST be calculated as follows:

- Inputs for all hashing algorithms used to calculate the `RAID_ID` MUST be lower-case characters.
- Stage 1 - MUST select the input string to use (either `"No FQDN"` or `PubKey || <FQDN>`).
  - Example: Mimblewimble public key `ca469346d7643336c19155fdf5c6500a5232525ce4eba7e4db757639159e9861` and FQDN 
  `disney.com` are used here, resulting in `ca469346d7643336c19155fdf5c6500a5232525ce4eba7e4db757639159e9861disney.com`.
- Stage 2 - MUST perform `Blake2b` hashing on the result of stage 1 using a 10 byte digest size.
  - Example: In hexadecimal representation `ff517a1387153cc38009` or binary representation`\xffQz\x13\x87\x15<\xc3\x80\t`.
- Stage 3 - MUST concatenate the `RAID_ID` identifier byte, `0x62`, with the result of stage 2.
  - Example: `62ff517a1387153cc38009` `.
- Stage 4 - MUST convert the result of stage 3 from a byte string into a `Base58` encoded string. 
  This will result in a 15 character string starting with `R`.
  - Example: The resulting `RAID_ID` will be `RYqMMuSmBZFQkgp`.

**Req** - A valid `RAID_ID` signature MUST be a 256 bit Schnorr signature defined as `s = PvtNonce + e·PvtKey` with 
the challenge `e` being `e = Blake2b(PubNonce || PubKey || RAID_ID)`.



### Sequence of Events

The sequence of events leading up to digital asset registration is perceived as follows:

1. The [asset issuer] will decide if the default `RAID_ID` or a `RAID_ID` that is linked to an FQDN must be used for 
   asset registration. 
   (_**Note:** A single linked (`RAID_ID`, FQDN) tuple may be associated with multiple digital assets from the same 
   asset issuer._)

2. **Req** - If a default `RAID_ID` is required:

   1. The asset issuer MUST use the default `RAID_ID` (refer to [RAID_ID](#raid_id)).
   2. The asset issuer MUST NOT sign the `RAID_ID`.

3. **Req** - If a linked (`RAID_ID`, FQDN) tuple is required:
   1. The asset issuer MUST create a `RAID_ID`.
   2. The asset issuer MUST sign the `RAID_ID` as specified (refer to [RAID_ID](#raid_id)).
   3. The asset issuer MUST create a valid TXT DNS record (refer to [OpenAlias TXT DNS Records](#openalias-txt-dns-records)).

4. **Req** - [Validator Node]s (VNs) MUST only allow a valid `RAID_ID` to be used in the digital asset registration 
   process.

5. **Req** - VNs MUST verify the OpenAlias TXT DNS record if a linked (`RAID_ID`, FQDN) tuple is used:
   1. Verify that all fields have been completed as per the specification (refer to 
       [OpenAlias TXT DNS Records](#openalias-txt-dns-records)).
   2. Verify that the `RAID_ID` can be calculated from information provided in the TXT DNS record and the FQDN of the 
   public DNS record it is in.
   3. Verify that the asset issuer's `RAID_ID` signature is valid.
   4. Verify the checksum.



### Confidentiality and Security

To prevent client lookups from leaking, OpenAlias recommends making use of [DNSCrypt](https://dnscrypt.info/), 
resolution via DNSCrypt-compatible resolvers that support Domain Name System Security Extensions (DNSSEC) and without 
DNS requests being logged.

**Req** - [Token Wallet]s (TWs) and VNs SHOULD implement the following confidentiality and security measures when 
dealing with OpenAlias TXT DNS records:

- All queries SHOULD make use of the DNSCrypt protocol.
- Resolution SHOULD be forced via DNSCrypt-compatible resolvers that:
  - support DNSSEC;
  - do not log DNS requests.
- The DNSSEC trust chain validity:
  - SHOULD be verified;
  - SHOULD be reported back to the [asset issuer].



## References

[[1]] Crate Openalias [online]. Available: <https://docs.rs/openalias/0.2.0/openalias/index.html>. 
Date accessed: 2019-03-05.

[1]: https://docs.rs/openalias/0.2.0/openalias/index.html "Crate openalias"

[[2]] RFC 7208: Sender Policy Framework (SPF) for Authorizing Use of Domains in Email, Version 1 [online]. 
Available: <https://tools.ietf.org/html/rfc7208>. Date accessed: 2019-03-06.

[2]: https://tools.ietf.org/html/rfc7208 "RFC 7208"

[DAN]: Glossary.md#digital-asset-network

[UTXO]: Glossary.md#unspent-transaction-outputs

[base layer]: Glossary.md#base-layer

[Tari coins]: Glossary.md#tari-coin

[Validator Node]: Glossary.md#validator-node

[Token Wallet]: Glossary.md#token-wallet

[digital asset]: Glossary.md#digital-asset

[asset issuer]: Glossary.md#asset-issuer


