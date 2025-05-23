# Changelog

All notable changes to this project will be documented in this file. See [standard-version](https://github.com/conventional-changelog/standard-version) for commit guidelines.
## [3.0.0](https://github.com/tari-project/tari/compare/v2.1.1...v3.0.0) (2025-05-20)


### ⚠ BREAKING CHANGES

* allow nextnet to mine randomxT (#7070)
* make tari randomx pow compatible with xmrig (#7069)
* add second tari only randomx mining (#7057)
* vm calc height (#7082)

### Features

* add GetBlockHeightTransactions grpc method ([#7081](https://github.com/tari-project/tari/issues/7081)) ([d8fa8f3](https://github.com/tari-project/tari/commit/d8fa8f3d9ae11a8a2c3189a5022db816a8d68b1e))
* add second tari only randomx mining ([#7057](https://github.com/tari-project/tari/issues/7057)) ([f593638](https://github.com/tari-project/tari/commit/f5936380e9ba4e7e76b83e365c65eb02fbfcb730))
* add spendable supply grpc query ([#7055](https://github.com/tari-project/tari/issues/7055)) ([8901bcb](https://github.com/tari-project/tari/commit/8901bcbbbbfc50c392ba3c11462c9e2f350f37af))
* allow nextnet to mine randomxT ([#7070](https://github.com/tari-project/tari/issues/7070)) ([3152af2](https://github.com/tari-project/tari/commit/3152af2cdba9c52fe2c746a736c1d0ea5c67c02b))
* improve listening error propagation ([#7050](https://github.com/tari-project/tari/issues/7050)) ([894d70a](https://github.com/tari-project/tari/commit/894d70a71a6c37067d73cc2a5c944a0d1ab35a15))
* make tari randomx pow compatible with xmrig ([#7069](https://github.com/tari-project/tari/issues/7069)) ([e82e5ff](https://github.com/tari-project/tari/commit/e82e5ffa6948fa6b3efe0d56900f5ae61b503a0c))
* print out errors better ([#7053](https://github.com/tari-project/tari/issues/7053)) ([5127a3d](https://github.com/tari-project/tari/commit/5127a3db295ccf67a81261af0b8f4fb8b1c5b8f9))
* expose user payment id ([#7077](https://github.com/tari-project/tari/issues/7077)) ([e7bb008](https://github.com/tari-project/tari/commit/e7bb008e7f0e35778c8fc5712cf99bfc32ddf33f))
* import transactions via grpc ([#7078](https://github.com/tari-project/tari/issues/7078)) ([12db85d](https://github.com/tari-project/tari/commit/12db85da0541a1d8bc0301001aac3fc515e14247))
* vm calc height ([#7082](https://github.com/tari-project/tari/issues/7082)) ([c7bec97](https://github.com/tari-project/tari/commit/c7bec97f3d5e217f03db6969898c5bfa9110e7cd))


### Bug Fixes

* duplicate tx when importing completed tx ([#7064](https://github.com/tari-project/tari/issues/7064)) ([0c9d7f6](https://github.com/tari-project/tari/commit/0c9d7f6797c499c6ed2304b2b342f704b7d1ac86))
* only wait for 5 seconds in waiting state ([51ada84](https://github.com/tari-project/tari/commit/51ada84068284858ceca5c3f062920e2266958d2))
* transaction error display ([#7065](https://github.com/tari-project/tari/issues/7065)) ([9279f2a](https://github.com/tari-project/tari/commit/9279f2a2c04f95a8f6f771785dbc724052f36cb3))
* wallet sender details from sent transaction ([#7066](https://github.com/tari-project/tari/issues/7066)) ([8f38071](https://github.com/tari-project/tari/commit/8f38071d62b9120aee429177f121eb159da3fa3d))
* base node panic ([#7074](https://github.com/tari-project/tari/issues/7074)) ([c64b79a](https://github.com/tari-project/tari/commit/c64b79aec0b694f22573607f6304e559fdf39b34))

### [2.1.1(https://github.com/tari-project/tari/compare/v2.1.0...v2.1.1) (2025-05-09)

### Bug Fixes

* Revert connection pool cycling

## [2.1.0](https://github.com/tari-project/tari/compare/v2.0.1...v2.1.0) (2025-05-09)


### Features

* add block hash to grpc method ([#7025](https://github.com/tari-project/tari/issues/7025)) ([161bdf7](https://github.com/tari-project/tari/commit/161bdf7247835821a67439c4da34316264317ef8))
* add block height to query ([#7033](https://github.com/tari-project/tari/issues/7033)) ([50c2839](https://github.com/tari-project/tari/commit/50c2839029319c0cd2ff6388df83c617f2550ac5))
* add connection pool cycling ([#7011](https://github.com/tari-project/tari/issues/7011)) ([0f758cf](https://github.com/tari-project/tari/commit/0f758cf54a5e2974771b82c8df4e261de61123ee))
* add view key to ffi ([#7041](https://github.com/tari-project/tari/issues/7041)) ([d2cdb90](https://github.com/tari-project/tari/commit/d2cdb9019d56920325ca5ed3b884c451c1d13688))
* overrride coinbase payment_id if included in wallet payment address ([#7038](https://github.com/tari-project/tari/issues/7038)) ([3c6683a](https://github.com/tari-project/tari/commit/3c6683a3e76e61e8e69d6a801c952442d15f6e70))


### Bug Fixes

* config file comment ([#7034](https://github.com/tari-project/tari/issues/7034)) ([0c83469](https://github.com/tari-project/tari/commit/0c83469fd3cd6a30fee2b134ad26a7f56233850d))
* implement jmt ([#7036](https://github.com/tari-project/tari/issues/7036)) ([073eb44](https://github.com/tari-project/tari/commit/073eb4498e7cbd45434e198ddcccd5348d5d2193))
* vet ([b5da6e8](https://github.com/tari-project/tari/commit/b5da6e87d9d32532b76f2c25221c540db4f51ef3))


### [2.0.1](///compare/v2.0.0...v2.0.1) (2025-05-06)

* first github repo release

## [2.0.0] (2025-05-06)

* first release