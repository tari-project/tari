# RFC-1000/TariUseCases

## A Digital Asset Framework

![status: draft](./theme/images/status-draft.svg)

**Maintainer(s)**: [Leland Lee](https://github.com/lelandlee)

# License

[ The 3-Clause BSD License](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2019 The Tari Development Community

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
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
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
There will be many types of digital assets that can be issued on Tari. This document is intended to help potential asset
issuers identify use cases for Tari that lead to the design of new types of digital assets that may or may not exist in
their ecosystems today.

## Related RFCs
* [RFC-0001: An overview of the Tari network](RFC-0001_overview.md)
* [RFC-0300: The Digital Assets Network](RFC-0300_DAN.md)

## Description

Tari [digital asset]s may exist on the Tari [Digital Assets Network] (DAN), are perceived to have value and can be 
owned, with the associated digital data being classified as intangible, personal property.

### Types
A potential high level taxonomy of different types of digital assets.

* Symbolic
	* Insignia
	* Mascots
	* Event-driven or historical (eg. a rivalry or a highly temporal event)
* Artifacts or Objects
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
	* Bearer instruments / access token
	* Chat stickers
* Persona
	* Personality trait(s)
	* Emotion(s)
	* Statistics
	* Superpower(s)
	* Storyline(s)
	* Relationship(s)
	* Avatar

### Behaviors
Digital asset tokens may influence the following behavioral types:

* Advance purchase
* Experience enhancement
* Sharing
* Loyalty
* Reward for performance
* Tipping
* Donating to a charity
* Collecting
* Trading
* Building / combining

### Attributes
Digital assets have many different properties, which may be one or more of the following:

* Digital assets can be interactive:
	* Easter eggs
		* Media
		* Dynamic (eg. Imagine if assets were similar to sounds, visualisations or other kinds of "demos" or "gifs")
	* Game mechanics
	* Evolutionary
* Digital assets can be combined to create super assets.
* Digital assets can be attribute(s) of another digital asset (e.g. wheels of a vehicle or a VIP ticket has two drink cards).
* Digital assets can have contingencies (e.g. ownership of a digital asset is contingent on ownership of a different digital asset, using this digital asset is contingent on holding it for a particular duration, etc)
* Digital assets can have utility (e.g. be useful).
* Digital assets can be used across platforms (e.g. a digital asset for a game could be used as avatars in a social network).
* Digital assets can have history.
* Digital assets can have user-generated tags and/or metadata.

### Interactions

Digital asset owners may have the following interactions with the DAN and/or other people:
* Digital asset owners can attest that they have ownership over there assets at time `t`.
* Digital assets owners may attest ownership to an individual, to a group of friends or to the entire world.

### Rules
Rules are the governance of how digital assets may be used or transferred, as defined by the asset issuer:

* Royalty Fees -  Digital asset issuers can set a royalty that charges a fee every time the digital asset is transferred between parties. The fee as defined by the issuer can be fixed or dynamic or follow a complex formula and value is granted to the issuer(s) and/or other entities.
* Contingency - Digital asset ownership/interaction may be contingent/dependent on another asset.
* Timing Controls - Digital assets can only be transferred or used at particular times.
* Sharing - Digital assets can be shared to others or even co-owned.
* Privacy - Ownership of a digital asset can be changed from private to public.
* Upgradability/Versioning - Digital assets can be upgraded and/or versioned.
* Redeemability - Digital assets can be used once or multiple times.

### Examples
Some examples of how different types of digital assets with different attributes, rules and interactions may be 
manifested are shown below.

#### Crystal Skull of Akador

* Is rare
	* Is 1 of 5
* Is legendary
	* https://indianajones.fandom.com/wiki/Crystal_Skull_of_Akator
	* Press reports that this artifact could be worth $X
* Drives collectibility
* Drives advance purchase
	* If you are one of the first 100,000 people to buy tickets to Indiana Jones World you have a chance of winning this 1 of 5 artifact.
* Has superpowers and utility
	* If you have this item while visiting Indiana Jones World, you get to skip the line three times.
* Is a contingency for another asset
	* If you collect this item, two Sankara stones, and the Cross of Coronado, you can buy the ark of the covenant:
		* Ark of the covenant is rare. It is 1 of 1.
		* Ark of the covenant is legendary.
		* Ark of the covenant gives you lifetime access to Indiana Jones World.
		* Ark of the covenant has rules; 20% of the resale price goes to Indiana Jones World.

#### AB de Villiers' bat

* Isn’t Rare
	* Is 1 of 100,000
* Is Legendary
	* https://www.youtube.com/watch?v=HK6B2da3DPA
* Drives collectibility
	* Is part of a series of bats from famous batsmen.
* Can be combined with other assets
	* Be one of the first 10 people to combine six bats to turn this asset into a One Day International (ODI) century bat:
		* ODI century bats are rare, they are 1 of 10.
		* ODI century bats are legendary.
* Has no superpowers
* Has no utility
* Has no rules

#### OVO Owl x Supreme

* Is Rare
	* Is 1 of 200
* Is Legendary
	* https://www.supremenewyork.com
	* https://us.octobersveryown.com
* Has a game mechanic
	* Every time it’s transferred, it may become a golden ticket that grants you access to any Drake show.
	* If its become a golden ticket and is transferred, it loses its golden ticket superpower.
* Has utility
	* Unlocks exclusive media content feat. Drake hosted by OVO SOUND.
	* May become a golden ticket that grants you access to any Drake show.
* Has rules
	* Every time its transferred Supreme and OVO SOUND receive 25% of the transaction value.

[digital asset]: Glossary.md#digital-asset
[Digital Assets Network]:Glossary.md#digital-asset-network
[Validator Nodes]:Glossary.md#validator-node
[asset issuer]:Glossary.md#asset-issuer
