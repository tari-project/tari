All notable changes to this project will be documented in this file. See [standard-version](https://github.com/conventional-changelog/standard-version) for commit guidelines.

# Changelog
## [5.5.0](https://github.com/tari-project/tari/compare/v5.4.0...v5.5.0) (2026-07-16)

### ⚠ BREAKING CHANGES

* enforce unique burn commtiments (#7910)

### Features

*  better logs ([#7924](https://github.com/tari-project/tari/issues/7924)) ([2c394a9](https://github.com/tari-project/tari/commit/2c394a9ded48067cd4383e345a877a7cfb272b97))
*  better resize management ([#7906](https://github.com/tari-project/tari/issues/7906)) ([45425cb](https://github.com/tari-project/tari/commit/45425cb325afeba28bfecb44e945d3c68f6b9d39))
* add mined_in_epoch to burn claim proof file ([#7919](https://github.com/tari-project/tari/issues/7919)) ([dd1cf1d](https://github.com/tari-project/tari/commit/dd1cf1d018a3876e949bf9c3b423ac3bdffd694d))
* enforce unique burn commtiments ([#7910](https://github.com/tari-project/tari/issues/7910)) ([8c10d94](https://github.com/tari-project/tari/commit/8c10d94184a2b8376c99a7c6c85dc1959b86ee8a))
* improve node pool management ([#7921](https://github.com/tari-project/tari/issues/7921)) ([9542ed2](https://github.com/tari-project/tari/commit/9542ed274e31dd759cdd65b084403e3cef6b7423))
* improve peer sync ([#7903](https://github.com/tari-project/tari/issues/7903)) ([1dbd6e0](https://github.com/tari-project/tari/commit/1dbd6e06e1ba3ad127daf5ad0e716901a4cf9208))
* improve template calls ([#7900](https://github.com/tari-project/tari/issues/7900)) ([a1845cc](https://github.com/tari-project/tari/commit/a1845ccb524a58a8399aa9534d152daa624c203d))
* **installer:** add unified Minotari Ledger installer ([#7864](https://github.com/tari-project/tari/issues/7864)) ([bad5a5f](https://github.com/tari-project/tari/commit/bad5a5fb3f077cf03d81d51362916b7bb4b46c2f)), closes [#7795](https://github.com/tari-project/tari/issues/7795)
* make taripulse optional ([#7897](https://github.com/tari-project/tari/issues/7897)) ([4223d34](https://github.com/tari-project/tari/commit/4223d34c2221a8c804fe971297696ac1c25935ac))
* use uv and not python directly ([#7922](https://github.com/tari-project/tari/issues/7922)) ([1c714ec](https://github.com/tari-project/tari/commit/1c714ecc5b374b70cec64e3576ac172e1168189a))


### Bug Fixes

* coinbase status ([#7911](https://github.com/tari-project/tari/issues/7911)) ([1739585](https://github.com/tari-project/tari/commit/173958528be1722e06d71ab936ca54da0c858dd4))
* **deps:** update crossbeam-epoch advisory ([#7918](https://github.com/tari-project/tari/issues/7918)) ([1c7c5ac](https://github.com/tari-project/tari/commit/1c7c5acba21112b735ed69f290cca75dc5433cb9))
* harden utxo scanner ([#7898](https://github.com/tari-project/tari/issues/7898)) ([4062b24](https://github.com/tari-project/tari/commit/4062b241fc24dabdb46965b79fe22a99d01dd285))
* prune  deleted_txo_hash_to_header_index ([#7894](https://github.com/tari-project/tari/issues/7894)) ([8871066](https://github.com/tari-project/tari/commit/88710661114317c71079ab56adacc676fef31cd6))
* reduce listening state log spam ([#7905](https://github.com/tari-project/tari/issues/7905)) ([7c1104f](https://github.com/tari-project/tari/commit/7c1104f2e215a40faa6ed7b343859f4a3cf3ea46))
## [5.4.0](https://github.com/tari-project/tari/compare/v5.3.1...v5.4.0) (2026-06-30)


### Features

* harden prune mode ([#7902](https://github.com/tari-project/tari/issues/7902)) ([152e281](https://github.com/tari-project/tari/commit/152e2811e7d2b8df10aaa13afa9b00ad08d75c83))
* change wallet output selection ([#7889](https://github.com/tari-project/tari/issues/7889)) ([401093a](https://github.com/tari-project/tari/commit/401093ac031b0ad7d7bab8d350a7f6631f50b050))
* harden block add ([#7888](https://github.com/tari-project/tari/issues/7888)) ([9125a95](https://github.com/tari-project/tari/commit/9125a9500dc4ae3a1b512fc575a658db894d40f5))
* bundle install scripts ([#7879](https://github.com/tari-project/tari/issues/7879)) ([fe12274](https://github.com/tari-project/tari/commit/fe1227415e7a8c40ba71b30ee8fd69ea09bd2d46))
* update reorg logic ([#7881](https://github.com/tari-project/tari/issues/7881)) ([9d41c68](https://github.com/tari-project/tari/commit/9d41c68c988bf6d481249f7c061b3893a4b0c538))
* add ledger scripts to assets ([#7878](https://github.com/tari-project/tari/issues/7878)) ([5822b36](https://github.com/tari-project/tari/commit/5822b36c81deb10fb540be1d54e34b901e4b0701))
* make lmdb compaction disk space non fatal ([#7874](https://github.com/tari-project/tari/issues/7874)) ([1b13660](https://github.com/tari-project/tari/commit/1b1366088a4d8dec6376269e5bb4c7d190b74923))
* add synced gRPC healthcheck ([#7867](https://github.com/tari-project/tari/issues/7867)) ([561f415](https://github.com/tari-project/tari/commit/561f41531bd04c52413b9394154a503f20282881))
* change connection handling ([#7865](https://github.com/tari-project/tari/issues/7865)) ([8b3e358](https://github.com/tari-project/tari/commit/8b3e3582c00337dc034a2f199b0b3f57f1b4dbd1))
* improve migration flow of jmt upgrade ([#7862](https://github.com/tari-project/tari/issues/7862)) ([0c12351](https://github.com/tari-project/tari/commit/0c123516654a0feb63901d7ef3e149d94cbb88df))
* add peer transport preference modes ([#7851](https://github.com/tari-project/tari/issues/7851)) ([0c3c87d](https://github.com/tari-project/tari/commit/0c3c87df4ce14938be56fc3cbd056298484a5600)), closes [#7830](https://github.com/tari-project/tari/issues/7830)
* **wallet:** migrate legacy output key-ids to current format on startup (closes [#7829](https://github.com/tari-project/tari/issues/7829)) ([#7859](https://github.com/tari-project/tari/issues/7859)) ([9e9c832](https://github.com/tari-project/tari/commit/9e9c8323f0d4bddb2631ff789b4e4c00004ae6fb))
* **wallet:** stealth-address claim key for L1->L2 burns ([#7861](https://github.com/tari-project/tari/issues/7861)) ([d17fac8](https://github.com/tari-project/tari/commit/d17fac8e588c98820dff1250c4e00752545a6aed)), closes [tari-project/tari-ootle#1890](https://github.com/tari-project/tari-ootle/issues/1890)
* **sidechain:** carry next-epoch hash in EndEpoch command ([#7856](https://github.com/tari-project/tari/issues/7856)) ([941dd85](https://github.com/tari-project/tari/commit/941dd8592993629bbb1d2cc9f0dca51689a4bfef))
* better wallet feedback ([#7845](https://github.com/tari-project/tari/issues/7845)) ([88b893b](https://github.com/tari-project/tari/commit/88b893b437cdef7658b18d3b69e4af4a40b5bbee))
* **xmrig-proxy:** add getinfo and getheight methods to the node's integrated xmrig_proxy ([#7827](https://github.com/tari-project/tari/issues/7827)) ([7ae1410](https://github.com/tari-project/tari/commit/7ae14100bcc3844f14d1e6a5b10cd221f0bed1ad))


### Bug Fixes

* update wallet utxo selection ([#7895](https://github.com/tari-project/tari/issues/7895)) ([6c2937c](https://github.com/tari-project/tari/commit/6c2937cb78365a20a5243fcd469bfd7d7b351caf))
* diff check ([#7890](https://github.com/tari-project/tari/issues/7890)) ([0741da5](https://github.com/tari-project/tari/commit/0741da50efbbd830f2e724ec246ff720f8b064c8))
* ledger ([#7877](https://github.com/tari-project/tari/issues/7877)) ([0fd9331](https://github.com/tari-project/tari/commit/0fd933111c831711f9117db85d4d376ba77b207f))
* **base_node:** reject duplicate validator node registrations within a block ([#7870](https://github.com/tari-project/tari/issues/7870)) ([d584707](https://github.com/tari-project/tari/commit/d584707bf6aa98e495d7e5f9e4e1c93bf15cebbc))
* potential cache issues ([#7623](https://github.com/tari-project/tari/issues/7623)) ([7cab6cb](https://github.com/tari-project/tari/commit/7cab6cb9c18ab164147275dac1cde31513fa42e0))
* remove unnecessary clone from [#7868](https://github.com/tari-project/tari/issues/7868) ([#7869](https://github.com/tari-project/tari/issues/7869)) ([e2cf3b5](https://github.com/tari-project/tari/commit/e2cf3b50ace10954f9c031870f1b19d110119a97))
* **wallet:** bind target sidechain into burn claim ownership proof ([#7868](https://github.com/tari-project/tari/issues/7868)) ([00ab044](https://github.com/tari-project/tari/commit/00ab044565ee994782a53947f9c7ea4008437afc)), closes [tari-project/tari-ootle#445](https://github.com/tari-project/tari-ootle/issues/445)
* **common:** make StaticApplicationInfo work for crates.io consumers ([#7860](https://github.com/tari-project/tari/issues/7860)) ([3241ca9](https://github.com/tari-project/tari/commit/3241ca916bf0b6529ca72ca27e5eb748aa1b29af))
* improve pool management ([#7857](https://github.com/tari-project/tari/issues/7857)) ([53f34f2](https://github.com/tari-project/tari/commit/53f34f25fb68f8fb42a63dc64eeb43f52d27dc1f))
* builds ([#7841](https://github.com/tari-project/tari/issues/7841)) ([ca68990](https://github.com/tari-project/tari/commit/ca689909ebd8cdb32e7a0c44944bc9a6875f5ccf))
* increase buffer ([#7844](https://github.com/tari-project/tari/issues/7844)) ([cbce477](https://github.com/tari-project/tari/commit/cbce477b37155b7352eed9a1d063108bc97e9ca4))
* jmt data usage ([#7824](https://github.com/tari-project/tari/issues/7824)) ([2ffe7da](https://github.com/tari-project/tari/commit/2ffe7dadcb319037482edf2daa77d6c3098fcd28))

## [5.2.1](https://github.com/tari-project/tari/compare/v5.2.1...v5.3.0) (2026-04-28)


### ⚠ BREAKING CHANGES

* **sidechain:** include `epoch_hash` in sidechain block header ([#7767](https://github.com/tari-project/tari/issues/7767))
* set epoch length to 10 for all networks ([#7725](https://github.com/tari-project/tari/issues/7725))


### Features

#### Wallet

* add branch and bound as UTXO selection option ([#7651](https://github.com/tari-project/tari/issues/7651)) ([4e4cec3](https://github.com/tari-project/tari/commit/4e4cec34f5c035d116c088a0ed400c33763d94a8))
* wire branch and bound into the console wallet ([#7671](https://github.com/tari-project/tari/issues/7671)) ([5af622e](https://github.com/tari-project/tari/commit/5af622ef382d1deffd9b9c6dfdc99ecd2e1526f4))
* offline signer support ([#7663](https://github.com/tari-project/tari/issues/7663)) ([d8c76a0](https://github.com/tari-project/tari/commit/d8c76a017b82b7b7ce3efb6e72d5f2b0bef77fe9))
* payref tracking ([#7734](https://github.com/tari-project/tari/issues/7734)) ([f247880](https://github.com/tari-project/tari/commit/f247880339a520bd68c623cc1f67a27e989a5218))
* sparse block header storage for wallet scanner ([#7744](https://github.com/tari-project/tari/issues/7744)) ([317a59a](https://github.com/tari-project/tari/commit/317a59a9f96fce4a70ffcb6004a645241c783f14))
* improved wallet debugging tools ([#7755](https://github.com/tari-project/tari/issues/7755)) ([af60157](https://github.com/tari-project/tari/commit/af601575efe7a197a46dceb82c6f9e302249f610))
* improved transaction feedback ([#7754](https://github.com/tari-project/tari/issues/7754)) ([24011fd](https://github.com/tari-project/tari/commit/24011fd50210f3234529729c75d511f92a290867))
* `export-audit` CLI command for wallet transaction CSV export ([#7700](https://github.com/tari-project/tari/issues/7700)) ([98aebeb](https://github.com/tari-project/tari/commit/98aebebf1ef2c99608230196134cb2ac3512e577))
* remove libtor from console wallet ([#7653](https://github.com/tari-project/tari/issues/7653)) ([d7da75c](https://github.com/tari-project/tari/commit/d7da75c620aa4a9795b59bea49ed8b1ac14c0ea7))
* install scripts for Ledger ([#7694](https://github.com/tari-project/tari/issues/7694)) ([9235d76](https://github.com/tari-project/tari/commit/9235d76099dbe816216876b3d7622a5ffbdc6e76))
*  feat: add api to change birthday ([#7782](https://github.com/tari-project/tari/pull/7782))


#### Base node

* background database pruning for large prune operations ([#7739](https://github.com/tari-project/tari/issues/7739)) ([0e5f9ca](https://github.com/tari-project/tari/commit/0e5f9ca0d29adf1b13dc15361d91a950065e1288))
* new node pool management logic ([#7728](https://github.com/tari-project/tari/issues/7728)) ([f70500d](https://github.com/tari-project/tari/commit/f70500d01447256fc45777531641478fbfc596a7))
* network silence mode ([#7696](https://github.com/tari-project/tari/issues/7696)) ([8354e8a](https://github.com/tari-project/tari/commit/8354e8acb53724588c71dfc6e440830c05829b40))
* wait-for-shutdown support ([#7666](https://github.com/tari-project/tari/issues/7666)) ([e8e9eaf](https://github.com/tari-project/tari/commit/e8e9eaf828cd445d9cd863d57d9efa8ae79af11a))
* make readiness gRPC a config option ([#7678](https://github.com/tari-project/tari/issues/7678)) ([6f8f361](https://github.com/tari-project/tari/commit/6f8f3617e8e8f36526356aed354c215c72ec9f64))
* show `-p` overrides in `print-env` and improve unknown-field config errors ([#7701](https://github.com/tari-project/tari/issues/7701)) ([c99231a](https://github.com/tari-project/tari/commit/c99231aec78d831918e512e0ce4701da297db3f7))
* add metric to track mempool double spends ([#7699](https://github.com/tari-project/tari/issues/7699)) ([ba3b103](https://github.com/tari-project/tari/commit/ba3b1037379683e8956e43af6b9070904fb60c27))
* add new reorg metrics ([#7697](https://github.com/tari-project/tari/issues/7697)) ([bfb007b](https://github.com/tari-project/tari/commit/bfb007bc9d0d9e00e18952ebd8a563a02f06d6d1))

#### APIs

* `exclude_inputs` query parameter on the `sync_utxos_by_block` endpoint ([#7723](https://github.com/tari-project/tari/issues/7723)) ([363b9fe](https://github.com/tari-project/tari/commit/363b9fedf775683d45430c1df6f0849df335ebee))
* updated API for deleted block info ([#7735](https://github.com/tari-project/tari/issues/7735)) ([5eeca3a](https://github.com/tari-project/tari/commit/5eeca3a4ca1ec440a6547fa130d95d681f8f9b20))

#### Mining

* allow XMRig to request a block template from a Tari node ([#7714](https://github.com/tari-project/tari/issues/7714)) ([3b6f685](https://github.com/tari-project/tari/commit/3b6f685840a97d47d7bf8d098560449eefa38b17))
* convert merge mining cucumber tests to RxT ([#7747](https://github.com/tari-project/tari/issues/7747)) ([f75de46](https://github.com/tari-project/tari/commit/f75de46d4908748e2a479773d3bb5c9a559bafeb))


### Bug Fixes

#### Sync and networking

* better sync ([#7774](https://github.com/tari-project/tari/issues/7774)) ([11dc8d2](https://github.com/tari-project/tari/commit/11dc8d2d976e0078f324a4240b20b187818f5849))
* seed peer connections kept open ([#7687](https://github.com/tari-project/tari/issues/7687)) ([01b107f](https://github.com/tari-project/tari/commit/01b107fe75fa1f47ac6b92bb1e6a12811df4c2da))
* fix sync peer swapping by in ([#7781](https://github.com/tari-project/tari/pull/7781))

#### Wallet

* branch-and-bound edge case ([#7768](https://github.com/tari-project/tari/issues/7768)) ([566193d](https://github.com/tari-project/tari/commit/566193d466973dc041a89249defa278670312cd9))
* miscellaneous edge cases ([#7769](https://github.com/tari-project/tari/issues/7769)) ([b32d517](https://github.com/tari-project/tari/commit/b32d51745b1542a7691d4661634dac83a286b65e))
* legacy transaction status ([#7756](https://github.com/tari-project/tari/issues/7756)) ([35768f1](https://github.com/tari-project/tari/commit/35768f19aa4c4a6f25a0419b592eeb507657592a))
* burn claim flow ([#7658](https://github.com/tari-project/tari/issues/7658)) ([f42e14d](https://github.com/tari-project/tari/commit/f42e14ddac360db0bda56eff43e6c7e00167fb10))
* save complete burn proof to file ([#7726](https://github.com/tari-project/tari/issues/7726)) ([5a278cb](https://github.com/tari-project/tari/commit/5a278cb8834acc10ebe714c39f012249ea2e966c))
* correct displayed transaction fee ([#7659](https://github.com/tari-project/tari/issues/7659)) ([6453d3e](https://github.com/tari-project/tari/commit/6453d3eaf6895373605601b65844268dae4d0198))
* user-pays-fee and replace-by-fee behaviour ([#7662](https://github.com/tari-project/tari/issues/7662)) ([b95e35f](https://github.com/tari-project/tari/commit/b95e35f88096cefa1662ad95bf24366f7856906a))
* fee-per-gram stat call for HTTP calls ([#7667](https://github.com/tari-project/tari/issues/7667)) ([68ae120](https://github.com/tari-project/tari/commit/68ae1205801cfc784924fcab4ae1f2f93463a037))
* kernel Merkle proof fetching ([#7665](https://github.com/tari-project/tari/issues/7665)) ([20c4672](https://github.com/tari-project/tari/commit/20c4672f1efaf44c5394171fdc8b67af1964e59f))
* remove blocking base node call in `TransactionServiceRequest::FetchUnspentOutputs` ([#7724](https://github.com/tari-project/tari/issues/7724)) ([b44dee8](https://github.com/tari-project/tari/commit/b44dee8395a8d15c2a447d82039de016209595aa))
* `import-paper-wallet` when `base-dir` is absolute ([#7720](https://github.com/tari-project/tari/issues/7720)) ([394e22d](https://github.com/tari-project/tari/commit/394e22d222fc5e03e8e6b714dfc6ecec95ec2520))
* wallet scanning edge case ([#7657](https://github.com/tari-project/tari/issues/7657)) ([c70542d](https://github.com/tari-project/tari/commit/c70542dea56367ce7ed56dd5542039f7f3689f13))
* wallet handling of duplicate blocks ([#7656](https://github.com/tari-project/tari/issues/7656)) ([3f0bea1](https://githu


## [4.10.0](https://github.com/tari-project/tari/compare/v4.9.1...v4.10.0) (2025-07-18)


### ⚠ BREAKING CHANGES

* only scan in 100 block sections (#7344)
* remove wallet ffi transport (#7347)
* add birthday offset to wallet create (#7345)
* add initial validation flag to wallet state (#7341)

### Features

* add birthday offset to wallet create ([#7345](https://github.com/tari-project/tari/issues/7345)) ([68c996e](https://github.com/tari-project/tari/commit/68c996e66c50022e4f05c35c11f909c77c834b17))
* add initial validation flag to wallet state ([#7341](https://github.com/tari-project/tari/issues/7341)) ([323e308](https://github.com/tari-project/tari/commit/323e3080548e28961296b03050130a00b3aae8eb))
* add output hash of inputs to scanning stream ([#7334](https://github.com/tari-project/tari/issues/7334)) ([3ceac84](https://github.com/tari-project/tari/commit/3ceac84bab363db69b98fd6563423f1d55e80ae0))
* only scan in 10 block sections ([#7344](https://github.com/tari-project/tari/issues/7344)) ([d2e6df9](https://github.com/tari-project/tari/commit/d2e6df99d4b2330fe2131b2cea723cf43bbfa853))
* remove wallet ffi transport ([#7347](https://github.com/tari-project/tari/issues/7347)) ([422af03](https://github.com/tari-project/tari/commit/422af038d6cc5ebec1ce8fa3a10b06ecdf7be70c))


### Bug Fixes

* coinbase detection ([#7337](https://github.com/tari-project/tari/issues/7337)) ([9eb66d4](https://github.com/tari-project/tari/commit/9eb66d40705cfd5024644abc84a8f581a316b185))
* ffi callbacks ([#7340](https://github.com/tari-project/tari/issues/7340)) ([c83ff6a](https://github.com/tari-project/tari/commit/c83ff6acd3eb1fe77e10f3ee495cf6405be91376))

### [4.9.1](https://github.com/tari-project/tari/compare/v4.9.0...v4.9.1) (2025-07-16)


### Features

* add transport dial timeout ([#7312](https://github.com/tari-project/tari/issues/7312)) ([0ab5252](https://github.com/tari-project/tari/commit/0ab52522c85fd35b5ccaf17f9a99611d340c77f3))
* enable caching of http requests ([#7325](https://github.com/tari-project/tari/issues/7325)) ([db97351](https://github.com/tari-project/tari/commit/db973514ddd55475e8091e1e14abaec65b500b04))
* replace by fee and user pay for fee commands ([#7284](https://github.com/tari-project/tari/issues/7284)) ([a877048](https://github.com/tari-project/tari/commit/a877048936562b305429d477a8deda0369cb85c6))
* update search-utxo with payref info ([#7319](https://github.com/tari-project/tari/issues/7319)) ([24ae263](https://github.com/tari-project/tari/commit/24ae263755ca60929d0df13150754e0d9b05e3bf))


### Bug Fixes

* blocking main tokio thread when reading last latency ([#7320](https://github.com/tari-project/tari/issues/7320)) ([4b8745e](https://github.com/tari-project/tari/commit/4b8745ed5a671227281b2ef0dff7dab4feef22b5))
* http wallet json_rpc route size limit ([#7324](https://github.com/tari-project/tari/issues/7324)) ([5dcaccd](https://github.com/tari-project/tari/commit/5dcaccdea535a06f89412cc69194844e44b11ff1))


## [4.9.0](https://github.com/tari-project/tari/compare/v4.8.0...v4.9.0) (2025-07-14)


### ⚠ BREAKING CHANGES

* full http wallet refactor (#7215)

### Features

* add concurrency when contacting seed peers while performing seed strap ([#7294](https://github.com/tari-project/tari/issues/7294)) ([453ebb6](https://github.com/tari-project/tari/commit/453ebb691c9b64877c1325d049e628cb6517f11d))
* add minotari_utils ([#7157](https://github.com/tari-project/tari/issues/7157)) ([1ffeef7](https://github.com/tari-project/tari/commit/1ffeef769c41c5f73cdc8b7ce0ca8a5cea3d4f72))
* message signing exposed via gRPC ([#7299](https://github.com/tari-project/tari/issues/7299)) ([2493ee3](https://github.com/tari-project/tari/commit/2493ee36ff7b3577901ff81d0e52d66e1369edb9))
* modify soft disconnect criteria ([#7307](https://github.com/tari-project/tari/issues/7307)) ([35b5db7](https://github.com/tari-project/tari/commit/35b5db767c893f860a8ebc661526227c8299b9b1))
* full http wallet refactor ([#7215](https://github.com/tari-project/tari/issues/7215)) ([482a70e](https://github.com/tari-project/tari/commit/482a70e41cf06d7c2e09014a90f9f39510c0d807))


### Bug Fixes

* dont start second utxo scanner for recovery ([#7298](https://github.com/tari-project/tari/issues/7298)) ([32dbe08](https://github.com/tari-project/tari/commit/32dbe082b2c23d50b48886f89ec03508d1a5385e))
* freebsd build process failure ([#7302](https://github.com/tari-project/tari/issues/7302)) ([e3891f1](https://github.com/tari-project/tari/commit/e3891f1c1b5226b8f9b8d81991031f55b5aef21d))
* increase http server limit ([#7314](https://github.com/tari-project/tari/issues/7314)) ([433942a](https://github.com/tari-project/tari/commit/433942ae40b22adfe5b7381bc2debf2020495479))
* scanned height tracking ([#7301](https://github.com/tari-project/tari/issues/7301)) ([e0cc004](https://github.com/tari-project/tari/commit/e0cc004acc24ad58706bf5ad89b6f2b1460e3374))
* seed peers being disconnected while seedstrap is in progress ([#7303](https://github.com/tari-project/tari/issues/7303)) ([ea52f7f](https://github.com/tari-project/tari/commit/ea52f7f97309e6206e09d9eecee460f07de030aa))
* view wallet scan height ([#7313](https://github.com/tari-project/tari/issues/7313)) ([adbfcef](https://github.com/tari-project/tari/commit/adbfcef2858ad7e1d21001ecf7ae75331ef3c3a2))
* wallet sync command ([#7305](https://github.com/tari-project/tari/issues/7305)) ([081969a](https://github.com/tari-project/tari/commit/081969a339cb2b79695f094eae38baf6fc320910))


## [4.8.0](https://github.com/tari-project/tari/compare/v4.7.0...v4.8.0) (2025-07-07)


### ⚠ BREAKING CHANGES

* expand gRPC readiness status to contain current processed block info (#7262)
* payref migration and indexes, add grpc query via output hash (#7266)
* improve grpc token supply (#7261)

### Features

* add payref background task ([#7280](https://github.com/tari-project/tari/issues/7280)) ([a2b8a93](https://github.com/tari-project/tari/commit/a2b8a93d256759169db183e254e930051cd382bb))
* auto zero value coinbase reward calculation ([#7259](https://github.com/tari-project/tari/issues/7259)) ([607729a](https://github.com/tari-project/tari/commit/607729a6b7d5791f023803c2724b3311aa4d98c7))
* expand gRPC readiness status to contain current processed block info ([#7262](https://github.com/tari-project/tari/issues/7262)) ([ee9f76d](https://github.com/tari-project/tari/commit/ee9f76da2cf5d7b6cd6d14492e5267429b7dc137))
* improve connection stats ([#7285](https://github.com/tari-project/tari/issues/7285)) ([bf3cc16](https://github.com/tari-project/tari/commit/bf3cc164c99c9e32fc42ac31d3dae91919260aa5))
* improve grpc token supply ([#7261](https://github.com/tari-project/tari/issues/7261)) ([b072a6f](https://github.com/tari-project/tari/commit/b072a6f6c13ff489248355fc01e034a20043f128))
* new ffi method to get payment_id from tari address ([#7282](https://github.com/tari-project/tari/issues/7282)) ([37fd3e4](https://github.com/tari-project/tari/commit/37fd3e48d7f31a1180912c04ee6277e9f664f474))


### Bug Fixes

* correctly validate coinbase transactions for recovered wallets ([#7278](https://github.com/tari-project/tari/issues/7278)) ([3d5a043](https://github.com/tari-project/tari/commit/3d5a0439d1cfb43e39355d34ecdf5af90c50fc14))
* payref migration and indexes, add grpc query via output hash ([#7266](https://github.com/tari-project/tari/issues/7266)) ([3ceea6e](https://github.com/tari-project/tari/commit/3ceea6e738a2e027ed83c7992c8d40f9c8a2b825))


### [4.7.0](https://github.com/tari-project/tari/compare/v4.6.2...v4.7.0) (2025-06-26)


### Features

* offline signing ([#7122](https://github.com/tari-project/tari/issues/7122)) ([86539c8](https://github.com/tari-project/tari/commit/86539c858cd452a3194267f97c34f2a2324d9659))


### Bug Fixes

* get_all_completed_transactions limit issues ([#7267](https://github.com/tari-project/tari/issues/7267)) ([da3f82d](https://github.com/tari-project/tari/commit/da3f82db3108357adb34bcacaf240be9bf9a8bbd))
* ledger builds ([#7260](https://github.com/tari-project/tari/issues/7260)) ([d3676ef](https://github.com/tari-project/tari/commit/d3676ef8a921c50968bc57bbfded8c77e072c565))

### [4.6.2](https://github.com/tari-project/tari/compare/v4.6.1...v4.6.2) (2025-06-24)


### Bug Fixes

* remove long timeout in interactive_tx till tx is persisted into db ([#7252](https://github.com/tari-project/tari/issues/7252)) ([3a78aba](https://github.com/tari-project/tari/commit/3a78aba2a5ed8c764525687ede683cc726ac880a))

### [4.6.1](https://github.com/tari-project/tari/compare/v4.6.0...v4.6.1) (2025-06-23)


### Features

* readiness status during initialization ([#7240](https://github.com/tari-project/tari/issues/7240)) ([078cad8](https://github.com/tari-project/tari/commit/078cad82efab14e79df411fa00c350b909402bda))


### Bug Fixes

* database cannot resize on jmt write ([#7244](https://github.com/tari-project/tari/issues/7244)) ([1df5cfe](https://github.com/tari-project/tari/commit/1df5cfeb91472cac482169b2e18605b540242845))
* minotari_merge_mining_proxy returns Tari block hash even if submit_to_origin is disabled ([#7242](https://github.com/tari-project/tari/issues/7242)) ([d21f99c](https://github.com/tari-project/tari/commit/d21f99ce982b534b976f08c0808bd59c6d3aff54))
* 
## [4.6.0](https://github.com/tari-project/tari/compare/v4.5.0...v4.6.0) (2025-06-20)


### Features

* add gprc methods to get fees ([#7235](https://github.com/tari-project/tari/issues/7235)) ([83969f3](https://github.com/tari-project/tari/commit/83969f3a46f92fe6cab59b0ae035a34dc8a46853))
* limit txs searches ([#7236](https://github.com/tari-project/tari/issues/7236)) ([6c6f47f](https://github.com/tari-project/tari/commit/6c6f47f2d2da250e077c8c88722ad6148b166e00))


### Bug Fixes

* grpc interactive transaction transfer ([#7234](https://github.com/tari-project/tari/issues/7234)) ([15471bc](https://github.com/tari-project/tari/commit/15471bc981be9f3c5493e747aaf32f2600ce665d))
* imported transaction directions ([#7233](https://github.com/tari-project/tari/issues/7233)) ([5de7d7d](https://github.com/tari-project/tari/commit/5de7d7dcfc4c28677a673938097f14989db55b48))
* peer dialling ([#7218](https://github.com/tari-project/tari/issues/7218)) ([5a2b934](https://github.com/tari-project/tari/commit/5a2b934cd5c886dd495edc296adff4c4bd6476d2))


## [4.5.0](https://github.com/tari-project/tari/compare/v4.4.1...v4.5.0) (2025-06-18)


### ⚠ BREAKING CHANGES

* ensure payref persists during recovery (#7225)

### Features

* add payref to grpc outputs ([#7216](https://github.com/tari-project/tari/issues/7216)) ([0e322e1](https://github.com/tari-project/tari/commit/0e322e1f160811a676f64e784ede7983abcddcca))
* ensure payref persists during recovery ([#7225](https://github.com/tari-project/tari/issues/7225)) ([2737a14](https://github.com/tari-project/tari/commit/2737a1404753cb416400a76e122f4839a7625dda))
* integrated address support for Ledger ([#7198](https://github.com/tari-project/tari/issues/7198)) ([7ab0cd5](https://github.com/tari-project/tari/commit/7ab0cd5f2e440a4d42b5385d544f85253c805339))


### Bug Fixes

* fix scanner service when connectivity offline ([#7223](https://github.com/tari-project/tari/issues/7223)) ([e0ab8d1](https://github.com/tari-project/tari/commit/e0ab8d15df31520e1723d0000f555470d745a333))


### [4.1.1](https://github.com/tari-project/tari/compare/v4.4.0...v4.1.1) (2025-06-12)


### ⚠ BREAKING CHANGES

* update grpc supply query (#7137)

### Features

* improve wallet balance checks from external clients ([#7207](https://github.com/tari-project/tari/issues/7207)) ([58c3e41](https://github.com/tari-project/tari/commit/58c3e41f7b6cb71406a65a063a1f79f8ca50f94b))
* update grpc supply query ([#7137](https://github.com/tari-project/tari/issues/7137)) ([4ce3977](https://github.com/tari-project/tari/commit/4ce39778950560b70af4ff67db8695a8f76a5d19))


### Bug Fixes

* add filtering flag back ([#7208](https://github.com/tari-project/tari/issues/7208)) ([5c1923f](https://github.com/tari-project/tari/commit/5c1923fff0eee745d18aa87833597791cbf8de1f))
* migration can now correctly resume after stopping ([#7210](https://github.com/tari-project/tari/issues/7210)) ([d268f2b](https://github.com/tari-project/tari/commit/d268f2b7c98e510d400a9195f8ef6b51bc0945be))
* only revalidated rejected transactions on startup ([#7209](https://github.com/tari-project/tari/issues/7209)) ([65af015](https://github.com/tari-project/tari/commit/65af015d4cccea527e73452c2c973223ac2aad1e))

## [4.4.0](https://github.com/tari-project/tari/compare/v4.3.1...v4.4.0) (2025-06-11)


### Features

* full PayRef implementation ([#7154](https://github.com/tari-project/tari/issues/7154)) ([ea038a4](https://github.com/tari-project/tari/commit/ea038a426ef85096ef9eeccc1a2ef7caf4e2277a))
* improve peer partial match resiliency ([#7166](https://github.com/tari-project/tari/issues/7166)) ([375f28d](https://github.com/tari-project/tari/commit/375f28d3842d9e4889523c83c303971e707529e7))
* update base node proto to search bytes ([#7201](https://github.com/tari-project/tari/issues/7201)) ([af1203a](https://github.com/tari-project/tari/commit/af1203a1419f52e089d8bf4ac243ba7487ca7047))


### Bug Fixes

* **network-discovery:** add back idle event handling ([#7194](https://github.com/tari-project/tari/issues/7194)) ([1412179](https://github.com/tari-project/tari/commit/1412179c477415d2041eadf9a3955134654bfcfd))
* payment_id deserialize ([#7187](https://github.com/tari-project/tari/issues/7187)) ([a049549](https://github.com/tari-project/tari/commit/a049549ac4234bf419f836e90339f6c0546b35be))
* reduce threshold for flood ban ([#7171](https://github.com/tari-project/tari/issues/7171)) ([0d958de](https://github.com/tari-project/tari/commit/0d958dea1eb8cbc6f3832c37e6f10caf54429fef))
* remove code for deleting stale peers ([#7184](https://github.com/tari-project/tari/issues/7184)) ([3b28a61](https://github.com/tari-project/tari/commit/3b28a61bd14f3c623344e87cca2e224f2c56783d))
* transaction manager service unmined lookup ([#7192](https://github.com/tari-project/tari/issues/7192)) ([73af2d9](https://github.com/tari-project/tari/commit/73af2d91b9ebac413ef8b1f2a91ae2ddc21dd66b))
* wallet ffi database name mismatch for mobile wallet ([#7191](https://github.com/tari-project/tari/issues/7191)) ([ed31974](https://github.com/tari-project/tari/commit/ed31974e6c911ae9e08f82359c3a2310adce2dd2))

### [4.3.1](https://github.com/tari-project/tari/compare/v4.3.0...v4.3.1) (2025-06-03)


### Bug Fixes

* fixed the wallet ffi ([#7174](https://github.com/tari-project/tari/issues/7174)) ([7bd4ff7](https://github.com/tari-project/tari/commit/7bd4ff7bfebe14fca0e239f052d39c8ff6a874b0))
* unban peers when their ban expires ([#7177](https://github.com/tari-project/tari/issues/7177)) ([4965ff0](https://github.com/tari-project/tari/commit/4965ff0558f8f7d8b87530209a97dba03b950684))

## [4.3.0](https://github.com/tari-project/tari/compare/v4.2.0...v4.3.0-) (2025-06-03)


### Features

* disable default dht discovery forwarding ([#7128](https://github.com/tari-project/tari/issues/7128)) ([b6894ff](https://github.com/tari-project/tari/commit/b6894ff3900a75f7fb7f074a54f9fad2de171ba4))
* get_all_completed_transactions bitflag status filtering ([#7161](https://github.com/tari-project/tari/issues/7161)) ([7248e18](https://github.com/tari-project/tari/commit/7248e18a3fe9b24c2acfb62afd6b98995d3a02b8))


### Bug Fixes

* don't ban peers for invalid peer data ([#7170](https://github.com/tari-project/tari/issues/7170)) ([7049ab0](https://github.com/tari-project/tari/commit/7049ab0389857258052537de720ab827247bb836))
* the statemachine ([#7169](https://github.com/tari-project/tari/issues/7169)) ([ca6a03e](https://github.com/tari-project/tari/commit/ca6a03e6fca35e2575a6df4a6780eaacce0bc374))

## [4.2.0](https://github.com/tari-project/tari/compare/v4.1.0...v4.2.0) (2025-06-03)


### ⚠ BREAKING CHANGES

* update target time (#7141)

### Bug Fixes

* add migration code ([#7153](https://github.com/tari-project/tari/issues/7153)) ([41add9f](https://github.com/tari-project/tari/commit/41add9fe30e1c0d80e93197944a86446bcb966b7))
* update target time ([#7141](https://github.com/tari-project/tari/issues/7141)) ([f29829f](https://github.com/tari-project/tari/commit/f29829fbace9cfcd7371bbfc20c6419a7a46a28d))

## [4.1.0](https://github.com/tari-project/tari/compare/v4.0.0...v4.1.0) (2025-05-30)


### ⚠ BREAKING CHANGES

* remove the ability to send completely raw bytes via grpc (#7117)

### Features

* add base node HTTP wallet service ([#7061](https://github.com/tari-project/tari/issues/7061)) ([1382008](https://github.com/tari-project/tari/commit/1382008771037e11e3c9c8bdeb71f5aa198e9e21))
* add sqlite peer_db ([#6963](https://github.com/tari-project/tari/issues/6963)) ([0f1b0dc](https://github.com/tari-project/tari/commit/0f1b0dc386462529512676d177c6316ece8bd20e))
* get all completed txs with pagination ([#7113](https://github.com/tari-project/tari/issues/7113)) ([d292cec](https://github.com/tari-project/tari/commit/d292cecd4c95870947831129f9b476f73bf8ea59))
* new bootstrap process ([#7121](https://github.com/tari-project/tari/issues/7121)) ([e5a0854](https://github.com/tari-project/tari/commit/e5a08540b30084d566a1cfb99336797f50a65b3e))
* remove the ability to send completely raw bytes via grpc ([#7117](https://github.com/tari-project/tari/issues/7117)) ([1e7ac28](https://github.com/tari-project/tari/commit/1e7ac281ad7a20f45c6852f131ea44db246f4f72))


### Bug Fixes

* add hardcoded esme seeds for dns fallback ([#7120](https://github.com/tari-project/tari/issues/7120)) ([0fd2442](https://github.com/tari-project/tari/commit/0fd2442d59d79e5ef4321508e74822edb8674be5))
* exclude coinbases from fee calc ([#7112](https://github.com/tari-project/tari/issues/7112)) ([d35a8c8](https://github.com/tari-project/tari/commit/d35a8c88c1dce2a05b8cb0d51d44f7309541f5f3))
* ffi tari address from emoji ([#7114](https://github.com/tari-project/tari/issues/7114)) ([8c97103](https://github.com/tari-project/tari/commit/8c971034327058cd37c5b0bc60185f3e890b5f05))
* mismatched tms db state ([#7131](https://github.com/tari-project/tari/issues/7131)) ([9e9b8b6](https://github.com/tari-project/tari/commit/9e9b8b620870eecb1063cebfa7e5057ed7092d2b))
* peer retention and connections ([#7123](https://github.com/tari-project/tari/issues/7123)) ([7867d12](https://github.com/tari-project/tari/commit/7867d12dcb4044f9069a5ef5051ad7c0b5c96d21))

## [4.0.0](https://github.com/tari-project/tari/compare/v3.0.2...v4.0.0) (2025-05-26)

### Features

* change consensus to be 33%,33%,33% pow ([9e121b0](https://github.com/tari-project/tari/commit/9e121b021d0c3149db45ab72428978a279d23240))

### [3.0.2](https://github.com/tari-project/tari/compare/v3.0.1...v3.0.2) (2025-05-23)


### Bug Fixes

* return min results ([#7098](https://github.com/tari-project/tari/issues/7098)) ([d5ec945](https://github.com/tari-project/tari/commit/d5ec9459f884ecada424589b1552dfd5422f8aa5))

### [3.0.1](https://github.com/tari-project/tari/compare/v3.0.0...v3.0.1) (2025-05-23)


### ⚠ BREAKING CHANGES

* sync (#7088)

### Bug Fixes
[
* sync ([#7088](https://github.com/tari-project/tari/issues/7088)) ([b268384](https://github.com/tari-project/tari/commit/b2683849b4d6f631b5b7efb3b66cd75b7526c119))]()


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