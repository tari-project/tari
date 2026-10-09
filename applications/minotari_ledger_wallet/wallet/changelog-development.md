# Changelog

All notable changes to this project will be documented in this file. See [standard-version](https://github.com/conventional-changelog/standard-version) for commit guidelines.

## Unreleased


### ⚠ Security

* **Update the Ledger application, not only the wallet.** Ledger applications from before this change let a compromised
  host recover the wallet's root spend key without any prompt on the device. Updating the wallet software alone does
  not protect a device: the fix is in the Ledger application. Until the application is updated, do not connect the
  device to a host you do not trust.
* **Every output a sender offset key signs is reviewed on the device.** `GetRawSchnorrSignature` now refuses a
  `OneSidedSenderOffset` key with `BadBranchKey`, so a sender offset key signs only through
  `GetOneSidedMetadataSignature`, which shows the amount and receiver for review. Before this, a compromised host
  could sign an output's metadata through the raw instruction and spend the wallet's funds with no prompt. Change to
  the wallet's own address is auto-approved: when the receiver's spend key is the device's own, it signs without a
  review.


### ⚠ Upgrade notes

* **Upgrade the wallet and the Ledger application together.** Wallets from `5.7.0-pre.6` up to the release that
  ships this change still connect to this application, but they label pre-mine sender offset keys as
  `OneSidedSenderOffset`, so pre-mine spends and backup pre-mine spends fail against it. Do not run a pre-mine spend
  or a backup pre-mine spend from an older wallet against this application. The release that ships this change
  raises `MIN_LEDGER_APP_VERSION` so that wallets refuse older applications.
* **Redo pre-mine step 2.** Step 2 session files written with an older application name keys this application no
  longer signs with (a `OneSidedSenderOffset` sender offset key, and nonce indexes that older applications derived
  modulo `2^32`). Start those sessions again from step 2.
* **Sends with change need the wallet and the application from this change together.** Older wallets sign change
  through `GetRawSchnorrSignature`, which this application refuses, so their sends with change fail. A wallet from this
  change against an older application still works, but the older application shows change for review.
* **Not supported on a Ledger wallet:** outputs with no recipient address or a script other than the standard stealth
  script (burns, HTLCs), and aggregated (multi-party) sender metadata signatures. Multisig deposit and withdraw remain
  software wallet flows.
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
