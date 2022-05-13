# RFC-0152/EmojiId

## Emoji Id specification

![Status: Outdated](theme/images/status-outofdate.svg)

**Maintainer(s)**:[Cayle Sharrock](https://github.com/CjS77)

# Licence

[ The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2020. The Tari Development Community

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

This document describes the specification for Emoji Ids. Emoji Ids are encoded node ids used for humans to easily verify
peer node addresses.

## Related Requests for Comment

None

## Description

Tari [Communication Node]s are identified on the network via their [Node ID]; which in turn are derived from the node's
public key. Both the node id and public key are simple large integer numbers.

The most common practice for human beings to copy large numbers in cryptocurrency software is to scan a QR code or copy
and paste a value from one application to another. These numbers are typically encoded using hexadecimal or Base58
encoding. The user will then typically scan (parts) of the string by eye to ensure that the value was transferred
correctly.

For Tari, we propose encoding values, the node ID in particular, using emoji. The advantages of this approach are:

* Emoji are more easily identifiable; and if selected carefully, less prone to identification errors (e.g. mistaking an
  O for a 0).
* The alphabet can be considerably larger than hexadecimal (16) or Base58 (58), resulting in shorter character sequences
  in the encoding.

### The specification

#### The emoji character map
An emoji alphabet of 1,024 characters is selected. Each emoji is assigned a unique index from 0 to 1023 inclusive. This
list is the emoji map. For example,

* 😀 => 0
* 😘 => 1
* ...
* 🦊 => 1023

The emoji SHOULD be selected such that

* Similar looking emoji are excluded from the map. e.g. Neither 😁 or 😄 should be included. Similarly the Irish and
  Côte d'Ivoirean flags look very similar, and both should be excluded.
* Modified emoji (skin tones, gender modifiers) are excluded. Only the "base" emoji is considered.

#### Encoding

The essential strategy in the encoding process is to map a sequence of 8-bit values onto a 10-bit alphabet. The general
encoding procedure is as follows:

Given a large integer value, represented as a _byte array_, `S`, in little-endian format (most significant digit last).
Assume the string is addressable, i.e. `S[i]` is the `i`th byte in the array.
* Set `CURSOR` to 0, Set `L` to a multiple of 10 that is `<= len(S)`.
* Set `IDX` to `[]` (an empty array)
* While `CURSOR < L`:
  * Set `L <= S[CURSOR/8 + 1]`, the current low byte; if the index would overflow, set `L` to zero.
  * Set `H <= S[CURSOR/8]`, the current high byte
  * Set `n <= CURSOR % 8`, the position of the cursor in the current high byte
  * Set `i <= ((H as u8) << n) << 2 + (L >> (6 - n))`, where the first shift left (`H as u8 <<n`) is on a one-byte width
    (effectively losing the first n bits) and the second shift left is on a 8-byte width (u64).
  * Push `i` onto `IDX`
  * `CURSOR <= CURSOR + 10`
* Return `IDX`

The emoji string is created by mapping the `IDX` array to the emoji map.

#### Emoji ID definition

The emoji ID is an emoji string of 12 characters. Each character encodes 10 bits according to the bitmap:

```text
+---------------------+------------------+-------------------+
|  Node Id (104 bits) | Version (6 bits) | Checksum (10 bits)|
+---------------------+------------------+-------------------+
```

 The emoji ID is calculated from a 104-bit node id represented as 13 bytes (`B`) as follows:

* Take the current emoji ID version number, `v` and add `v << 2` as an additional byte to `B`. This "right-pads" the
  version in the last byte. This is necessary since we have a 14 byte (112 bit) sequence, which is not divisible by 10.
  This padding sets the last 2 bits, which will be discarded, to zero.
* Encode B into an emoji string with `L` = 11.
* Calculate a 12th emoji using the Luhn mod 1024 checksum algorithm.

#### Decoding

One can extract the node id from an emoji ID as follows:

1. Calculate the checksum of the first 11 emoji using the Luhn mod 1024 algorithm. If it does not match the 12th emoji,
   return with an error. if any emoji character is not in the emoji map, return an error.
2. Extract the version number:
   3. Do a reverse lookup of emoji`[10]` to find its index. Store this u64 value in `I`.
   4. The Version number is `(I && 0x3F) >> 2`. This can be used to set the Emoji map accordingly (and may have to be
      done iteratively, since the version is encoded into the emoji string).
3. Set `CURSOR = 0`.
4. Set `B = []`, and empty byte array
5. While `CURSOR <= 11`:
   4. Set `k <= CURSOR * 2`
   5. Do a reverse lookup of the emoji`[CURSOR]` to find its index. Store this u64 value in `L`.
   6. If `k > 0`, set `H` to the reverse lookup index of emoji`[CURSOR-1]` as u8 (first 2 bits are discarded), else
      `H=0`.
   7. Set `v = ((H as u8)  << (8-k)) + (L >> (2+k))`. Push v onto tho `B`.
   9. Set `CURSOR <= CURSOR + 1`

If the algorithm completes, `B` holds the node ID.

#### Versioning

The current emoji ID version number is 1. If the emoji alphabet changes, the version number MUST be incremented. This
will usually cause incompatible versions of the emoji ID to be detected. However, this is not fail-safe.

The last 6 bits of the 11th emoji encodes the version; this means that the first 4 bits are part of the node ID. On a
reverse mapping, there is a chance that the reverse mapping would offer a valid, but incorrect version number if the new
mapping are not chosen carefully.

##### Example.

In version 1, 😘 => `0b0000_000001` = 1 in the map. Seeing 😘 as the 11th emoji in a string would result in a version
code of 1, which is consistent and expected.

However, in unlucky version 13, if 😘 moves in the map to number 13 (`0b0000_001101`), the version decoding would also
be valid and thus we wouldn't be able to unambiguously identify the version.



[Communication Node]: Glossary.md#communication-node
[Node ID]: Glossary.md#node-id
