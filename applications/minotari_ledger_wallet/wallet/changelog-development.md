# Changelog

All notable changes to this project will be documented in this file. See [standard-version](https://github.com/conventional-changelog/standard-version) for commit guidelines.

## Unreleased


### ⚠ Security

* **Update the Ledger application, not only the wallet.** Ledger applications from before this change let a compromised
  host recover the wallet's root spend key without any prompt on the device. Updating the wallet software alone does
  not protect a device: the fix is in the Ledger application. Until the application is updated, do not connect the
  device to a host you do not trust.
* **A `OneSidedSenderOffset` key can no longer sign an arbitrary output without review.** `GetRawSchnorrSignature`
  now refuses `OneSidedSenderOffset` keys with `BadBranchKey`. Before this, a compromised host could have one sign any
  output's metadata signature through the raw instruction, with nothing on the screen. A `OneSidedSenderOffset` key
  now signs a metadata signature only through `GetOneSidedMetadataSignature`, which shows the amount and receiver for
  review. (Its script signature, script Schnorr signature and Diffie-Hellman instructions still accept it; those use
  other hash domains and cannot make a metadata signature.)

  **The device reads what it signs.** `GetOneSidedMetadataSignature` (now instruction `0x15`, chunked) carries the
  output's raw version, features, covenant, encrypted data and minimum value promise, and the device hashes them
  into the metadata signature itself, instead of trusting a 32 byte hash from the host. It also builds the script
  itself, as it always has. A host that lies about any of these fields gets a signature that verifies on no output
  carrying the fields it publishes, because consensus recomputes the hash from the output's own fields.

  The device signs without a review only an output that is change in every respect it can see: to this wallet's own
  address - its view key and spend key both this device's own, for the account the host names - with default
  features (a standard output, maturity 0, no coinbase data, no sidechain feature, a bullet proof range proof), an
  empty covenant and no minimum value promise. Change, coin splits and joins and payments to self are silent.
  Everything else is reviewed, and the review shows what is not default: the output type, a maturity, a sidechain
  feature (with the validator node's key for a registration or exit), a minimum value promise, a revealed value range
  proof. An address carrying this wallet's spend key with any other view key is reviewed too: the host derives the
  output's mask and encrypted data from the view key, so such an output would be locked to this wallet yet invisible
  to its scanner. The device refuses a covenant, a burn and a coinbase outright (`OutputNotSignable`).

  This closes "change" that is really a burn claimable on L2 by a key the host chooses, a freeze behind a huge
  maturity or a covenant, and an offline payload recipient at the wallet's own address carrying a maturity: all of
  them are now either reviewed, with the reason shown, or refused.

  What this does not close, each tracked separately:
  - It does not by itself stop a compromised host from spending without a prompt: the script offset the device hands
    the host, together with an output under a host held sender offset key, remains a residual.
  - The encrypted data is hashed but not verified: it is data only the receiver reads, and a lie there only affects
    the receiver's recovery of the output.
  - The device never sees the fee. An offline payload's author can set a fee up to the total recipient amount -
    recipients at this wallet's own address count towards that cap. A summary of an offline payload's fee on the
    Ledger console wallet is a follow-up.
  - `PreMine` sender offset keys are not refused. A host can mint them on demand on any wallet, through
    `GetScriptOffset` with a `PreMine` script key, and they still sign raw challenges with no review. Pre-mine sender
    offset signing - the aggregated step 3 raw, step 4 through the legacy nonce instruction - is unchanged and belongs
    with the separate pre-mine issue.


### ⚠ Upgrade notes

* **Upgrade the wallet and the Ledger application together.** Wallets from `5.7.0-pre.6` up to the release that
  ships this change still connect to this application, but they label pre-mine sender offset keys as
  `OneSidedSenderOffset`, so pre-mine spends and backup pre-mine spends fail against it. Do not run a pre-mine spend
  or a backup pre-mine spend from an older wallet against this application. The release that ships this change
  raises `MIN_LEDGER_APP_VERSION` so that wallets refuse older applications.
* **Redo pre-mine step 2.** Step 2 session files written with an older application name keys this application no
  longer signs with (a `OneSidedSenderOffset` sender offset key, and nonce indexes that older applications derived
  modulo `2^32`). Start those sessions again from step 2.
* **Upgrade the wallet and the Ledger application together - neither works with the other's older version.**
  `GetOneSidedMetadataSignature` moved from instruction `0x11`, one APDU with an opaque hash, to `0x15`, chunked with
  the raw fields; `0x11` is retired. An older wallet on this application is refused every metadata signature
  (`InsNotSupported`), and its change is refused on `GetRawSchnorrSignature` (`BadBranchKey`); this wallet on an older
  application is refused every metadata signature too.
* **Not supported on a Ledger wallet:**
  - Burns, plain and L2-bound: refused by the transaction service with `NotSupported` before any input is selected. In
    the key manager a plain burn's script hits `LedgerSenderOffsetNeedsRecipient`, and an L2-bound burn's host held
    sender offset key hits `LedgerHostHeldSenderOffset`.
  - HTLC (atomic swap) sends: refused by the transaction service with `NotSupported` before any input is selected.
  - Multisig deposit and withdraw, which remain software wallet flows, both hit `LedgerSenderOffsetNeedsRecipient`:
    the deposit because its script is not the standard stealth script, the withdraw because its output, although it is
    the standard stealth script to the recipient, is signed with no recipient address.
  - Aggregated (multi-party) sender metadata signatures by a `OneSidedSenderOffset` key:
    `LedgerSenderOffsetRawSignature`.
  - Offline signing payloads that carry pre-built (custom) outputs: `LedgerSenderOffsetNeedsRecipient`, refused before
    the device is asked to review anything.

  HTLC claims and refunds, and the pre-mine ceremony, are not affected by these refusals. The coinbase's host held
  sender offset key still signs in software.
* **Outputs that are not plain change are reviewed.** Validator node registration and exit, and any output carrying a
  maturity, a minimum value promise or a revealed value range proof, now show a review even when they go to the
  wallet's own address; the review names what is not default. Change, coin splits and joins and payments to self with
  default features stay silent.
* **Withdraw multisig funds before upgrading.** A Ledger wallet that is a party to a multisig deposit made with an
  earlier wallet and application should withdraw those funds - or have a software co-signer withdraw them - before
  upgrading. The new wallet refuses the Ledger side of a multisig withdraw with `LedgerSenderOffsetNeedsRecipient`.
* Key indexes are now derived from all 64 bits. Every index below `2^32` keeps its key, and no key an ordinary
  wallet re-derives after a transaction is built is above it, so ordinary sends and spends are unaffected.

## [5.2.0-pre.6](https://github.com/tari-project/tari/compare/v5.2.0-pre.5...v5.2.0-pre.6) (2025-11-28)


### Features

* add more detail to grpc payment reference information ([#7605](https://github.com/tari-project/tari/issues/7605)) ([e884b69](https://github.com/tari-project/tari/commit/e884b69f6b95d4dbe4c2a25089b517441f271eef))
* batch offline signer ([#7600](https://github.com/tari-project/tari/issues/7600)) ([0483ab2](https://github.com/tari-project/tari/commit/0483ab20dfff523839b67348503328bcd8de5d14))
* harden wallet scan resiliency ([#7593](https://github.com/tari-project/tari/issues/7593)) ([9f61b37](https://github.com/tari-project/tari/commit/9f61b37fe45a2d5e94f05697aabf0267f8ef06f8))
* improve base node check-db command ([#7576](https://github.com/tari-project/tari/issues/7576)) ([bca05d0](https://github.com/tari-project/tari/commit/bca05d09967bb673d45dedb2c6a26c82866cda9a))
* improve wallet validation performance ([#7596](https://github.com/tari-project/tari/issues/7596)) ([4016dc2](https://github.com/tari-project/tari/commit/4016dc2a694add6343920238e97ec8acdb565874))


### Bug Fixes

* cache age for old blocks ([#7611](https://github.com/tari-project/tari/issues/7611)) ([61b4b3d](https://github.com/tari-project/tari/commit/61b4b3d4eb764fd93e07138fb9b1982876a7fec8))
* http server request size ([#7609](https://github.com/tari-project/tari/issues/7609)) ([d52a6ca](https://github.com/tari-project/tari/commit/d52a6ca257507e1884498f4040eb8629ea4f5dfb))
* key manager ([#7597](https://github.com/tari-project/tari/issues/7597)) ([4560898](https://github.com/tari-project/tari/commit/4560898eb89d8bdf31279866823adf80f5f71f30))
* ledger tx flow ([#7612](https://github.com/tari-project/tari/issues/7612)) ([da8d3ed](https://github.com/tari-project/tari/commit/da8d3ed3b3db5dfd27f433ba603e658007cbb9c0))
* scanning page count ([#7606](https://github.com/tari-project/tari/issues/7606)) ([f5ad6d3](https://github.com/tari-project/tari/commit/f5ad6d3af7ec9326c1264199e0e3e3a351b27dae))
* store completed transactions with corresponding sent_output_hash ([#7595](https://github.com/tari-project/tari/issues/7595)) ([362c2f5](https://github.com/tari-project/tari/commit/362c2f51daea5cd99a142066335aa416c4982110))
* tokio panic in output manager service txo validation task ([#7594](https://github.com/tari-project/tari/issues/7594)) ([37a6d5c](https://github.com/tari-project/tari/commit/37a6d5cf6470f7d4481ac85fb59c9298f64d03fa))

## [5.2.0-pre.6](https://github.com/tari-project/tari/compare/v5.2.0-pre.5...v5.2.0-pre.6) (2025-11-28)


### Features

* add more detail to grpc payment reference information ([#7605](https://github.com/tari-project/tari/issues/7605)) ([e884b69](https://github.com/tari-project/tari/commit/e884b69f6b95d4dbe4c2a25089b517441f271eef))
* batch offline signer ([#7600](https://github.com/tari-project/tari/issues/7600)) ([0483ab2](https://github.com/tari-project/tari/commit/0483ab20dfff523839b67348503328bcd8de5d14))
* harden wallet scan resiliency ([#7593](https://github.com/tari-project/tari/issues/7593)) ([9f61b37](https://github.com/tari-project/tari/commit/9f61b37fe45a2d5e94f05697aabf0267f8ef06f8))
* improve base node check-db command ([#7576](https://github.com/tari-project/tari/issues/7576)) ([bca05d0](https://github.com/tari-project/tari/commit/bca05d09967bb673d45dedb2c6a26c82866cda9a))
* improve wallet validation performance ([#7596](https://github.com/tari-project/tari/issues/7596)) ([4016dc2](https://github.com/tari-project/tari/commit/4016dc2a694add6343920238e97ec8acdb565874))


### Bug Fixes

* cache age for old blocks ([#7611](https://github.com/tari-project/tari/issues/7611)) ([61b4b3d](https://github.com/tari-project/tari/commit/61b4b3d4eb764fd93e07138fb9b1982876a7fec8))
* http server request size ([#7609](https://github.com/tari-project/tari/issues/7609)) ([d52a6ca](https://github.com/tari-project/tari/commit/d52a6ca257507e1884498f4040eb8629ea4f5dfb))
* key manager ([#7597](https://github.com/tari-project/tari/issues/7597)) ([4560898](https://github.com/tari-project/tari/commit/4560898eb89d8bdf31279866823adf80f5f71f30))
* ledger tx flow ([#7612](https://github.com/tari-project/tari/issues/7612)) ([da8d3ed](https://github.com/tari-project/tari/commit/da8d3ed3b3db5dfd27f433ba603e658007cbb9c0))
* scanning page count ([#7606](https://github.com/tari-project/tari/issues/7606)) ([f5ad6d3](https://github.com/tari-project/tari/commit/f5ad6d3af7ec9326c1264199e0e3e3a351b27dae))
* store completed transactions with corresponding sent_output_hash ([#7595](https://github.com/tari-project/tari/issues/7595)) ([362c2f5](https://github.com/tari-project/tari/commit/362c2f51daea5cd99a142066335aa416c4982110))
* tokio panic in output manager service txo validation task ([#7594](https://github.com/tari-project/tari/issues/7594)) ([37a6d5c](https://github.com/tari-project/tari/commit/37a6d5cf6470f7d4481ac85fb59c9298f64d03fa))
