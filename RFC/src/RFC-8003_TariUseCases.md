# RFC-8003/TariUseCases

## Digital Asset Framework

![status: draft](./theme/images/status-draft.svg)

**Maintainer(s)**: [Leland Lee](https://github.com/lelandlee)

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

There will be many types of digital assets that can be issued on Tari. The aim of this Request for Comment (RFC) is to 
help potential asset
issuers identify use cases for Tari that lead to the design of new types of digital assets that may or may not exist in
their ecosystems today.

## Related Requests for Comment
* [RFC-0001: Overview of Tari Network](RFC-0001_overview.md)
* [RFC-0300: Digital Assets Network](RFCD-0300_DAN.md)

## Description

Tari [digital asset]s may exist on the Tari [Digital Assets Network] (DAN), are perceived to have value and can be 
owned, with the associated digital data being classified as intangible, personal property.

### Types

Digital assets may have the following high-level classification scheme:

* Symbolic
	* Insignia
	* Mascots
	* Event-driven or historical, e.g. a rivalry or a highly temporal event
* Artefacts or objects
	* Legendary
	* Rare
	* High demand
	* Artistic
	* Historical
* Utility
	* Tickets
	* In-game items
	* Points
	* Currency
	* Full or fractional representation
	* Bearer instruments/access token
	* Chat stickers
* Persona
	* Personality trait(s)
	* Emotion(s)
	* Statistics
	* Superpower(s)
	* Storyline(s)
	* Relationship(s)
	* Avatar

### Behaviours

Digital asset tokens may influence the following behavioural types:

* Advance purchase
* Experience enhancement
* Sharing
* Loyalty
* Reward for performance
* Tipping
* Donating to a charity
* Collecting
* Trading
* Building/combining

### Attributes

Digital assets have many different properties, which may be one or more of the following:

* Digital assets can be interactive:
	* Easter eggs;
		* media
		* dynamic, e.g. imagine if assets were similar to sounds, visualizations or other kinds of "demos" or "gifs")
	* Game mechanics;
	* Evolutionary.
* Digital assets can be combined to create super assets.
* Digital assets can be attribute(s) of another digital asset, e.g. wheels of a vehicle or a VIP ticket has two drink 
cards.
* Digital assets can have contingencies, e.g. ownership of a digital asset is contingent on ownership of a different 
digital asset. Using this digital asset is contingent on holding it for a particular duration, etc.
* Digital assets can have utility, e.g. be useful.
* Digital assets can be used across platforms, e.g. a digital asset for a game could be used as avatars in a social 
network.
* Digital assets can have history.
* Digital assets can have user-generated tags and/or metadata.

### Interactions

Digital asset owners may have the following interactions with the DAN and/or other people:
* Digital asset owners can attest that they have ownership over their assets at time `t`.
* Digital asset owners may attest ownership to an individual, to a group of friends or to the entire world.

### Rules

Rules are the governance of how digital assets may be used or transferred, as defined by the asset issuer:

* Royalty fees.  Digital asset issuers can set a royalty that charges a fee every time the digital asset is transferred 
between parties. The fee as defined by the issuer can be fixed or dynamic, or follow a complex formula, and value is 
granted to the issuer(s) and/or other entities.
* Contingency. Digital asset ownership/interaction may be contingent/dependent on another asset.
* Timing controls. Digital assets can only be transferred or used at particular times.
* Sharing. Digital assets can be shared with others or even co-owned.
* Privacy. Ownership of a digital asset can be changed from private to public.
* Upgradability/versioning. Digital assets can be upgraded and/or versioned.
* Redeemability. Digital assets can be used once or multiple times.

### Examples

Some examples of how different types of digital assets with different attributes, rules and interactions may be 
manifested are provided here:

#### Crystal Skull of Akador

* Is rare, it is 1 of 5.
* Is legendary:
  * https://indianajones.fandom.com/wiki/Crystal_Skull_of_Akator;
  * press reports that this artefact could be worth $X.
* Drives collectability.
* Drives advance purchase, e.g. if you are one of the first 100,000 people to buy tickets to Indiana Jones World, you 
have a chance of winning this 
  1 of 5 artefact.
* Has superpowers and utility, e.g. if you have this item while visiting Indiana Jones World, you get to skip the line 
three times.
* Is a contingency for another asset, e.g. if you collect this item, two Sankara stones and the Cross of Coronado, you 
  can buy the ark of the covenant:
  - Ark of the covenant is rare. It is 1 of 1.
  - Ark of the covenant is legendary.
  - Ark of the covenant gives you lifetime access to Indiana Jones World.
  - Ark of the covenant has rules; 20% of the resale price goes to Indiana Jones World.

#### AB de Villiers' bat

- Is not rare, it is 1 of 100,000.
- Is legendary - https://www.youtube.com/watch?v=HK6B2da3DPA.
- Drives collectability, it is part of a series of bats from famous batsmen.
- Can be combined with other assets, e.g. be one of the first 10 people to combine six bats to turn this asset into a 
One Day International (ODI) century bat:
  - ODI century bats are rare, they are 1 of 10;
  - ODI century bats are legendary.
- Has no superpowers.
- Has no utility.
- Has no rules.

#### OVO Owl x Supreme

* Is rare, it is 1 of 200.
* Is legendary:
  * https://www.supremenewyork.com;
  * https://us.octobersveryown.com.
* Has a game mechanic:
  * Every time it’s transferred, it may become a golden ticket that grants you access to any Drake show.
  * If it becomes a golden ticket and is transferred, it loses its golden ticket superpower.
* Has utility:
  * Unlocks exclusive media content feat. Drake hosted by OVO SOUND.
  * May become a golden ticket that grants you access to any Drake show.
* Has rules - every time its transferred Supreme and OVO SOUND receive 25% of the transaction value.

[digital asset]: Glossary.md#digital-asset
[Digital Assets Network]:Glossary.md#digital-asset-network
[Validator Nodes]:Glossary.md#validator-node
[asset issuer]:Glossary.md#asset-issuer
