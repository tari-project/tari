# Changelog

All notable changes to this project will be documented in this file. See [standard-version](https://github.com/conventional-changelog/standard-version) for commit guidelines.

## [1.0.0-rc.6a](https://github.com/tari-project/tari/compare/v1.0.0-rc.6...v1.0.0-rc.6a) (2024-03-12)


### Bug Fixes

* downgrade crossterm for windows compatibility ([#6204](https://github.com/tari-project/tari/issues/6204)) ([243243d](https://github.com/tari-project/tari/commit/243243dd7d7030010662f0d514097230d905a4cc))

## [1.0.0-rc.6](https://github.com/tari-project/tari/compare/v1.0.0-rc.5...v1.0.0-rc.6) (2024-03-11)


### ⚠ BREAKING CHANGES

* change split to 50-50 (#6188)
* implement inflating tail emission (#6160)
* changes balance query (#6158)
* change proof of work to be dependant on target difficulty (#6156)

### Features

* change split to 50-50 ([#6188](https://github.com/tari-project/tari/issues/6188)) ([3b7842a](https://github.com/tari-project/tari/commit/3b7842acb12cfea76652b48c400488e436418d0f))
* expose extra_data field to wallet ffi ([#6191](https://github.com/tari-project/tari/issues/6191)) ([2f2b139](https://github.com/tari-project/tari/commit/2f2b1391284f4a6ffcacb7a6d5e880f6c51cc8a3))
* implement inflating tail emission ([#6160](https://github.com/tari-project/tari/issues/6160)) ([63b1f68](https://github.com/tari-project/tari/commit/63b1f6864ef080f9eef9ba9d6a600ab86c8791c5)), closes [#6122](https://github.com/tari-project/tari/issues/6122) [#6131](https://github.com/tari-project/tari/issues/6131)
* lazily evaluate for new random_x template ([#6170](https://github.com/tari-project/tari/issues/6170)) ([d220643](https://github.com/tari-project/tari/commit/d220643b1596955c499bf39df2c58c3052d92724))
* limit transaction size ([#6154](https://github.com/tari-project/tari/issues/6154)) ([abd64d8](https://github.com/tari-project/tari/commit/abd64d8725f7e94b80bbfcbd97c58d9988571087))
* make the make_it_rain submission rate a float ([#6180](https://github.com/tari-project/tari/issues/6180)) ([75d773b](https://github.com/tari-project/tari/commit/75d773bba625bb513c7b7bcef0cd6e9b9dda6c83))
* mining ffi add coinbase add ([#6183](https://github.com/tari-project/tari/issues/6183)) ([820e936](https://github.com/tari-project/tari/commit/820e93676555bc35183470db6bbf3a5fd99eda02))
* multi-network ci ([#6162](https://github.com/tari-project/tari/issues/6162)) ([8990b57](https://github.com/tari-project/tari/commit/8990b575cd4df01c1a3e5e9385e13a9ce3b9ddd4))
* wallet ffi use dns ([#6152](https://github.com/tari-project/tari/issues/6152)) ([464f2c3](https://github.com/tari-project/tari/commit/464f2c3bc8495bf4a08e7292829726e8f9e8c747))
* add import tx method ([#6132](https://github.com/tari-project/tari/issues/6132)) ([f3d9121](https://github.com/tari-project/tari/commit/f3d91212e1e3a1e450b5f8e71ceacf2673cfc8c2))
* allow ffi to see lock height ([#6140](https://github.com/tari-project/tari/issues/6140)) ([48af0b8](https://github.com/tari-project/tari/commit/48af0b8615c80019ab1cf38f995a422cb999459e))
* change CLI  get_block to search orphans ([#6153](https://github.com/tari-project/tari/issues/6153)) ([ae1e379](https://github.com/tari-project/tari/commit/ae1e3796d98e55ceb3642128d659c4e181108b85))
* change proof of work to be dependant on target difficulty ([#6156](https://github.com/tari-project/tari/issues/6156)) ([feb634c](https://github.com/tari-project/tari/commit/feb634cd260a910228e0e9de45c9024b1990683f))
* check chain metadata ([#6146](https://github.com/tari-project/tari/issues/6146)) ([8a16f7b](https://github.com/tari-project/tari/commit/8a16f7ba83fd200618814b2eaf66c88c5b1dfb79))
* turn off node metrics by default ([#6073](https://github.com/tari-project/tari/issues/6073)) ([5ed661c](https://github.com/tari-project/tari/commit/5ed661c840795c3419369e865c8969ef7d49aacb))


### Bug Fixes

* add .h file to mining helper ([#6194](https://github.com/tari-project/tari/issues/6194)) ([237e6b9](https://github.com/tari-project/tari/commit/237e6b963edd3e4a8986ed4f9767a16f36aff05e))
* avoid cloning range proofs during verification ([#6166](https://github.com/tari-project/tari/issues/6166)) ([19a824d](https://github.com/tari-project/tari/commit/19a824dea8971f15a7b263122b20e46286f89857))
* changes balance query ([#6158](https://github.com/tari-project/tari/issues/6158)) ([9ccc615](https://github.com/tari-project/tari/commit/9ccc6153b0fedc1cf40bd547c6987143c23b1649))
* fixed make-it-rain delay ([#6165](https://github.com/tari-project/tari/issues/6165)) ([5c5da46](https://github.com/tari-project/tari/commit/5c5da461690684e90ecc12565d674fbca06b5f53))
* hide unmined coinbase ([#6159](https://github.com/tari-project/tari/issues/6159)) ([2ccde17](https://github.com/tari-project/tari/commit/2ccde173834fbbfc617b87001c7364760b81590e))
* horizon sync ([#6197](https://github.com/tari-project/tari/issues/6197)) ([c96be82](https://github.com/tari-project/tari/commit/c96be82efdbb24f448a5efef3076d0b1819ed07e))
* oms validation ([#6161](https://github.com/tari-project/tari/issues/6161)) ([f3d1219](https://github.com/tari-project/tari/commit/f3d12196530f9bf7c266cba9eff014cba04cecbb))
* remove extra range proof verifications ([#6190](https://github.com/tari-project/tari/issues/6190)) ([57330bf](https://github.com/tari-project/tari/commit/57330bf7e0be7d2d4f325e8009d3b10568f3acad))
* rewind bug causing SMT to be broken ([#6172](https://github.com/tari-project/tari/issues/6172)) ([4cb61a3](https://github.com/tari-project/tari/commit/4cb61a33c60fe18706aae4700e301484abe62471))
* wallet validation during reorgs ([#6173](https://github.com/tari-project/tari/issues/6173)) ([97fc7b3](https://github.com/tari-project/tari/commit/97fc7b382a078ed2178c650214cb9803daeea87f))
* balanced binary merkle tree merged proof ([#6144](https://github.com/tari-project/tari/issues/6144)) ([4d01653](https://github.com/tari-project/tari/commit/4d01653e6780241edfe732761d63d4218a2f742d))
* wallet clear short term output ([#6151](https://github.com/tari-project/tari/issues/6151)) ([ac6997a](https://github.com/tari-project/tari/commit/ac6997af1a1d9828a93064e849df3dcc4ba019ee))


## [1.0.0-rc.5](https://github.com/tari-project/tari/compare/v1.0.0-rc.4...v1.0.0-rc.5) (2024-02-06)


### Bug Fixes

* **comms:** correctly initialize hidden service ([#6124](https://github.com/tari-project/tari/issues/6124)) ([0584782](https://github.com/tari-project/tari/commit/058478255a93e7d50d95c8ac8c196069f76b994b))
* **libtor:** prevent metrics port conflict ([#6125](https://github.com/tari-project/tari/issues/6125)) ([661af51](https://github.com/tari-project/tari/commit/661af5177863f37f0b01c9846dccc7d24f873fc5))


## [1.0.0-rc.4](https://github.com/tari-project/tari/compare/v1.0.0-rc.3...v1.0.0-rc.4) (2024-02-02)


### ⚠ BREAKING CHANGES

* fix horizon sync after smt upgrade (#6006)

### Features

* do validation after adding utxos and txs ([#6114](https://github.com/tari-project/tari/issues/6114)) ([7d886e6](https://github.com/tari-project/tari/commit/7d886e6c85e463a4f7f4dacc5115e625bb1f37f5))
* export transaction ([#6111](https://github.com/tari-project/tari/issues/6111)) ([70d5ad3](https://github.com/tari-project/tari/commit/70d5ad3b4f8a1b8efb83a868102b7c846f2bd50c))
* fix horizon sync after smt upgrade ([#6006](https://github.com/tari-project/tari/issues/6006)) ([b6b80f6](https://github.com/tari-project/tari/commit/b6b80f6ee9b91255815bd2a66f51425c3a628dcf))
* initial horizon sync from prune node ([#6109](https://github.com/tari-project/tari/issues/6109)) ([2987621](https://github.com/tari-project/tari/commit/2987621b2cef6d3b852ed9a1f4215f19b9838e0f))
* smt verification ([#6115](https://github.com/tari-project/tari/issues/6115)) ([78a9348](https://github.com/tari-project/tari/commit/78a93480bc00235cbf221ff977f7d87f8008226a))
* wallet add restart validation to start ([#6113](https://github.com/tari-project/tari/issues/6113)) ([5c236ce](https://github.com/tari-project/tari/commit/5c236ce9928acd3aa212adab716c93f05e8cac9d))


### Bug Fixes

* faster tor startup ([#6092](https://github.com/tari-project/tari/issues/6092)) ([a2872bb](https://github.com/tari-project/tari/commit/a2872bba188c456578ed5b5ad5eb2e37e26a46e6))
* make monero extra data less strict ([#6117](https://github.com/tari-project/tari/issues/6117)) ([38b9113](https://github.com/tari-project/tari/commit/38b9113375bb90d667718f406e796f6a0e021861))

## [1.0.0-rc.3](https://github.com/tari-project/tari/compare/v1.0.0-rc.2...v1.0.0-rc.3) (2024-01-29)


### Features

* add search kernels method to nodejs client ([#6082](https://github.com/tari-project/tari/issues/6082)) ([0190221](https://github.com/tari-project/tari/commit/019022149d94afb3c0ed3f75490dd777d60bad1c))
* prevent runtime error with compact error input ([#6096](https://github.com/tari-project/tari/issues/6096)) ([69421f5](https://github.com/tari-project/tari/commit/69421f5ef97f0ba4c194162bca0b367dc7714ffe))
* update api ([#6101](https://github.com/tari-project/tari/issues/6101)) ([47e73ac](https://github.com/tari-project/tari/commit/47e73ac2b692bbfc924a4329e29597e49f84af0f))
* update codeowners ([#6088](https://github.com/tari-project/tari/issues/6088)) ([58a131d](https://github.com/tari-project/tari/commit/58a131d302fd7295134c708e75a0b788205d287e))


### Bug Fixes

* faster tor startup ([#6092](https://github.com/tari-project/tari/issues/6092)) ([a2872bb](https://github.com/tari-project/tari/commit/a2872bba188c456578ed5b5ad5eb2e37e26a46e6))

## [1.0.0-rc.2](https://github.com/tari-project/tari/compare/v1.0.0-rc.1...v1.0.0-rc.2) (2024-01-18)


### Features

* add tari address as valid string for discovering a peer ([#6075](https://github.com/tari-project/tari/issues/6075)) ([a4c5bc2](https://github.com/tari-project/tari/commit/a4c5bc2c6c08a5d09b58f13ed9acf561e55478fc))
* make all apps non interactive ([#6049](https://github.com/tari-project/tari/issues/6049)) ([bafd7e7](https://github.com/tari-project/tari/commit/bafd7e7baadd0f8b82ca8205ec3f18342d74e92a))
* make libtor on by default for nix builds ([#6060](https://github.com/tari-project/tari/issues/6060)) ([b5e0d06](https://github.com/tari-project/tari/commit/b5e0d0639c540177373b7faa9c2fade64581e46d))


### Bug Fixes

* fix small error in config.toml ([#6052](https://github.com/tari-project/tari/issues/6052)) ([6518a60](https://github.com/tari-project/tari/commit/6518a60dce9a4b8ace6c5cc4b1ee79045e364e0e))
* tms validation correctly updating ([#6079](https://github.com/tari-project/tari/issues/6079)) ([34222a8](https://github.com/tari-project/tari/commit/34222a88bd1746869e67ccde9c2f7529862f3b5d))
* wallet coinbases not validated correctly ([#6074](https://github.com/tari-project/tari/issues/6074)) ([bb66df1](https://github.com/tari-project/tari/commit/bb66df13bcf3d00082e35f7305b1fde72d4ace2a))


## [1.0.0-rc.1](https://github.com/tari-project/tari/compare/v1.0.0-rc.1...v1.0.0-rc.0) (2023-12-14)


### Features

* fix windows installer ([#6043](https://github.com/tari-project/tari/issues/6043)) ([c37a0a8](https://github.com/tari-project/tari/commit/c37a0a89726eec765c9c10d3da0c990d339de9b9))
* side load chat ([#6042](https://github.com/tari-project/tari/issues/6042)) ([d729c45](https://github.com/tari-project/tari/commit/d729c458b17406d9f5dbb8982a9bf5604f39c63c))

### Bug Fixes



## [1.0.0-rc.0](https://github.com/tari-project/tari/compare/v1.0.0-rc0...v0.49.0-rc.0) (2023-12-12)


### ⚠ BREAKING CHANGES

* add paging to utxo stream request (#5302)
* add optional range proof types (#5372)
* hash domain consistency (#5556) ([64443c6f](https://github.com/tari-project/tari/commit/64443c6f428fa84f8ab3e4b86949be6faef35aeb))
* consistent output/kernel versions between sender and receiver (#5553) ([74f9c35f](https://github.com/tari-project/tari/commit/74f9c35f6a34c1cf731274b7febb245734ae7032))
* New Gen block (#5633)
* Validator mr included in mining hash (#5615)
* Monero merkle proof change (#5602)
* Merge mining hash has changed
* remove timestamp from header in proto files (#5667)
* **comms/dht:** limit number of peer claims and addresses for all sources (#5702)
* **comms:** use noise XX handshake pattern for improved privacy (#5696)
* update faucet for genesis block (#5633)
* limit monero hashes and force coinbase to be tx 0 (#5602)
* add validator mr to mining hash (#5615)
* replace utxo MMR with SMT (#5854)
* update key parsing (#5900)
* **proto:** remove proto timestamp wrapper types (#5833)
* **proto:** remove proto bytes for std bytes (#5835)
* upgrade bitflags crate (#5831)
* improve block add where many orphan chain tips existed (#5763)
* lmdb flag set wrong on database (#5916)
* add validator mmr size (#5873)
* completed transaction use bytes for transaction protocol (not hex string) in wallet database (#5906)
* new faucet for esmeralda (#6001)
* dont store entire monero coinbase transaction (#5991)
* ups the min difficulty (#5999)
* network specific domain hashers (#5980)
* add aux chain support for merge mining (#5976)
* disable console wallet grpc (#5988)
* add one-sided coinbase payments (#5967)
* fix opcode signatures (#5966)
* remove mutable mmr (#5954)
* move kernel MMR position to `u64` (#5956)
* standardize gRPC authentication and mitigate DoS (#5936)
* fix difficulty overflow (#5935)
* update status (#6008)

### Features

* add miner timeout config option ([#5331](https://github.com/tari-project/tari/issues/5331)) ([aea14f6](https://github.com/tari-project/tari/commit/aea14f6bf302801c85efa9f304a8f442aaf9a3ff))
* chat ffi ([#5349](https://github.com/tari-project/tari/issues/5349)) ([f7cece2](https://github.com/tari-project/tari/commit/f7cece27c02ae3b668e1ffbd6629828d0432debf))
* chat scaffold ([#5244](https://github.com/tari-project/tari/issues/5244)) ([5b09f8e](https://github.com/tari-project/tari/commit/5b09f8e2b630685d9ff748eae772b9798954f6ff))
* improve message encryption ([#5288](https://github.com/tari-project/tari/issues/5288)) ([7a80716](https://github.com/tari-project/tari/commit/7a80716c71987bae14d83994d7402f96c190242d))
* **p2p:** allow listener bind to differ from the tor forward address ([#5357](https://github.com/tari-project/tari/issues/5357)) ([857fb55](https://github.com/tari-project/tari/commit/857fb55520145ece48b4b5cca0aa5d7fd8f6c69e))* add extended mask recovery ([#5301](https://github.com/tari-project/tari/issues/5301)) ([23d882e](https://github.com/tari-project/tari/commit/23d882eb783f3d94efbfdd928b3d87b2907bf2d7))
* add network name to data path and --network flag to the miners ([#5291](https://github.com/tari-project/tari/issues/5291)) ([1f04beb](https://github.com/tari-project/tari/commit/1f04bebd4f6d14432aab923baeab17d1d6cc39bf))
* add other code template types ([#5242](https://github.com/tari-project/tari/issues/5242)) ([93e5e85](https://github.com/tari-project/tari/commit/93e5e85cbc13be33bea40c7b8289d0ff344df08c))
* add paging to utxo stream request ([#5302](https://github.com/tari-project/tari/issues/5302)) ([3540309](https://github.com/tari-project/tari/commit/3540309e29d450fc8cb48bc714fb780c1c107b81))
* add wallet daemon config ([#5311](https://github.com/tari-project/tari/issues/5311)) ([30419cf](https://github.com/tari-project/tari/commit/30419cfcf198fb923ef431316f2915cbc80f1e3b))
* define different network defaults for bins ([#5307](https://github.com/tari-project/tari/issues/5307)) ([2f5d498](https://github.com/tari-project/tari/commit/2f5d498d2130b5358fbf126c96a917ed98016955))
* feature gates ([#5287](https://github.com/tari-project/tari/issues/5287)) ([72c19dc](https://github.com/tari-project/tari/commit/72c19dc130b0c7652cca422c9c4c2e08e5b8e555))
* fix rpc transaction conversion ([#5304](https://github.com/tari-project/tari/issues/5304)) ([344040a](https://github.com/tari-project/tari/commit/344040ac7322bae5604aa9db48d4194c1b3779fa))
* add metadata signature check ([#5411](https://github.com/tari-project/tari/issues/5411)) ([9c2bf41](https://github.com/tari-project/tari/commit/9c2bf41ec8f649ffac824878256c09598bf52269))
* add optional range proof types ([#5372](https://github.com/tari-project/tari/issues/5372)) ([f24784f](https://github.com/tari-project/tari/commit/f24784f3a2f3f574cd2ac4e2d9fe963078e4c524))
* added burn feature to the console wallet ([#5322](https://github.com/tari-project/tari/issues/5322)) ([45685b9](https://github.com/tari-project/tari/commit/45685b9f3acceba483ec30021e8d4894dbf2861c))
* improved base node monitoring ([#5390](https://github.com/tari-project/tari/issues/5390)) ([c704890](https://github.com/tari-project/tari/commit/c704890ca949bcfcd608e299175694b81cef0165))
* refactor configuration for chat so ffi can create and accept a config file (#5426) ([9d0d8b52](https://github.com/tari-project/tari/commit/9d0d8b5277bd26e79b7fe5506edcaf197ba63eb7), breaks [#](https://github.com/tari-project/tari/issues/), [#](https://github.com/tari-project/tari/issues/), [#](https://github.com/tari-project/tari/issues/))
* ui for template registration in console wallet (#5444) ([701e3c23](https://github.com/tari-project/tari/commit/701e3c2341d1029c2711b81a66952f3bee7d8e42), breaks [#](https://github.com/tari-project/tari/issues/), [#](https://github.com/tari-project/tari/issues/), [#](https://github.com/tari-project/tari/issues/))
* sparse merkle trees (#5457) ([f536d219](https://github.com/tari-project/tari/commit/f536d21929e4eeb11cc185c013eef0b336def216)* proof of work audit part 2 (#5495) ([af32f96f](https://github.com/tari-project/tari/commit/af32f96f36a32235daf7e3b1d9694af7edcf5f8e)
* improve recovery speed (#5489) ([d128f850](https://github.com/tari-project/tari/commit/d128f850356ff18bfd394f6c3bfe78f5bd0607e1))
* add consistent ban reason for sync ([#5729](https://github.com/brianp/tari/issues/5729)) ([9564281](https://github.com/brianp/tari/commit/95642811b9df592eb9bddd9b71d10ee30987e59d))
* add mempool min fee ([#5606](https://github.com/brianp/tari/issues/5606)) ([15c7e8f](https://github.com/brianp/tari/commit/15c7e8f9ca3d656850d6f0041d2f7fc07b4af80b))
* ban peer unexpected response ([#5608](https://github.com/brianp/tari/issues/5608)) ([02494ae](https://github.com/brianp/tari/commit/02494aee0f97469b9deb9c339b4075b14b69ff6f))
* change default script to PushPubKey ([#5653](https://github.com/brianp/tari/issues/5653)) ([f5b89ad](https://github.com/brianp/tari/commit/f5b89add6a04b935b9ae8dda0f694eb826ef6d9a))
* chat ffi status callback ([#5583](https://github.com/brianp/tari/issues/5583)) ([f68b85f](https://github.com/brianp/tari/commit/f68b85f404e524d61d8b6153c13e8b2e6ab2a20b))
* chat message fetching pagination ([#5594](https://github.com/brianp/tari/issues/5594)) ([2024357](https://github.com/brianp/tari/commit/202435742ed78b0eac80efcd19b357df96a6bbb9))
* chat-ffi logging ([#5591](https://github.com/brianp/tari/issues/5591)) ([159959c](https://github.com/brianp/tari/commit/159959cc32c341e111a626729fb1bd9a2851e8a7))
* cleanup errors ([#5655](https://github.com/brianp/tari/issues/5655)) ([c1737b9](https://github.com/brianp/tari/commit/c1737b9d872dbaf858dd46e6350c6febd7f43690))
* fix formatting block ([#5630](https://github.com/brianp/tari/issues/5630)) ([49732f6](https://github.com/brianp/tari/commit/49732f65339f4c120afb49e9edb72eda8d17b737))
* improve block sync error handling ([#5691](https://github.com/brianp/tari/issues/5691)) ([251f796](https://github.com/brianp/tari/commit/251f796dc023459338212a852d50059380399be2))
* new message callback to chat-ffi ([#5592](https://github.com/brianp/tari/issues/5592)) ([bbd543e](https://github.com/brianp/tari/commit/bbd543ee35e4e5fc858d875cf30d6f24fa2e4d96))
* peer sync limiter ([#5445](https://github.com/brianp/tari/issues/5445)) ([548643b](https://github.com/brianp/tari/commit/548643b723a548fea3e56f938a84db652d3ee630))
* remove inherent iterator panic ([#5697](https://github.com/brianp/tari/issues/5697)) ([7f153e5](https://github.com/brianp/tari/commit/7f153e5dd613b3e38586b7f8f536035c6ac98dd8))
* remove orphan validation and only validate on insertion ([#5601](https://github.com/brianp/tari/issues/5601)) ([41244a3](https://github.com/brianp/tari/commit/41244a3ea666f925648aa752c9ac476486702473))
* remove unused wasm_key_manager ([#5622](https://github.com/brianp/tari/issues/5622)) ([508c971](https://github.com/brianp/tari/commit/508c97198617f116bb0ccd69c8e1eba1341b18ac))
* update faucet for genesis block ([#5633](https://github.com/brianp/tari/issues/5633)) ([ffb987a](https://github.com/brianp/tari/commit/ffb987a757f2af721ca5772e28da31035fcf741f))
* update genesis blocks ([#5698](https://github.com/brianp/tari/issues/5698)) ([b9145b3](https://github.com/brianp/tari/commit/b9145b3373319f0c2c25d0e5dd4d393115a4c0bd))
* add (de)serialize to BalancedBinaryMerkleTree ([#5744](https://github.com/tari-project/tari/issues/5744)) ([c53ec06](https://github.com/tari-project/tari/commit/c53ec065b6f7893fe1a5d3a3ccde826fa09e438f))
* add config for grpc server methods ([#5886](https://github.com/tari-project/tari/issues/5886)) ([a3d7cf7](https://github.com/tari-project/tari/commit/a3d7cf771663d2b3c3585796ef502ab00f569ba0))
* add insert function to SMT ([#5776](https://github.com/tari-project/tari/issues/5776)) ([5901b4a](https://github.com/tari-project/tari/commit/5901b4af9fe307cdc379979155961d34dcf8c098))
* add overflow checks to change and fee calculations ([#5834](https://github.com/tari-project/tari/issues/5834)) ([9725fbd](https://github.com/tari-project/tari/commit/9725fbddf1ee7047d2e7698f4ee1975ce22aa605))
* allow multiple initial sync peers ([#5890](https://github.com/tari-project/tari/issues/5890)) ([e1c504a](https://github.com/tari-project/tari/commit/e1c504a3d9b9affafb3221e46831d818cbdcc45a))
* apply obscure_error_if_true consistenlty ([#5892](https://github.com/tari-project/tari/issues/5892)) ([1864203](https://github.com/tari-project/tari/commit/1864203c224611cdcac71adbae83e37161ce0a5c))
* ban bad block-sync peers ([#5871](https://github.com/tari-project/tari/issues/5871)) ([5c2781e](https://github.com/tari-project/tari/commit/5c2781e86be8efacab52c93a0bc2ee662ca56ec8))
* chat ffi verbose logging options ([#5789](https://github.com/tari-project/tari/issues/5789)) ([24b4324](https://github.com/tari-project/tari/commit/24b4324f3d5b4386a3df68952fb834d58fa5217d))
* chatffi simpler callbacks and managed identity and db ([#5681](https://github.com/tari-project/tari/issues/5681)) ([79ab584](https://github.com/tari-project/tari/commit/79ab584100bc6899445fc3789d6e3312a06d21e8))
* **chatffi:** better message metadata parsing ([#5820](https://github.com/tari-project/tari/issues/5820)) ([9a43eab](https://github.com/tari-project/tari/commit/9a43eab2e81aaaa0a5ad53b3dc5d9388b9d43452))
* **chatffi:** get conversationalists ([#5849](https://github.com/tari-project/tari/issues/5849)) ([d9e8e22](https://github.com/tari-project/tari/commit/d9e8e22846cc0974abcfe19ab32b41299c0a500a))
* **chatffi:** message metadata ([#5766](https://github.com/tari-project/tari/issues/5766)) ([a9b730a](https://github.com/tari-project/tari/commit/a9b730aaa2e44dbba7c546b0d78ad0fef4884d29))
* **chatffi:** tor configuration ([#5752](https://github.com/tari-project/tari/issues/5752)) ([1eeb4a9](https://github.com/tari-project/tari/commit/1eeb4a9abbc29ec16593b1c6bec675b928e7b177))
* **chat:** read receipt feature ([#5824](https://github.com/tari-project/tari/issues/5824)) ([d81fe7d](https://github.com/tari-project/tari/commit/d81fe7d39fdc120665b90e18163151bdb938beee))
* cli add list of vns for next epoch ([#5743](https://github.com/tari-project/tari/issues/5743)) ([d2a0c8c](https://github.com/tari-project/tari/commit/d2a0c8cc935bb648460f8095c5f2f7125e642169))
* **comms:** allow multiple messaging protocol instances ([#5748](https://github.com/tari-project/tari/issues/5748)) ([3fba04e](https://github.com/tari-project/tari/commit/3fba04ec862bf405e96e09b5cc38a5d572b77244))
* consistent handling of edge cases for header sync ([#5837](https://github.com/tari-project/tari/issues/5837)) ([3e1ec1f](https://github.com/tari-project/tari/commit/3e1ec1f1fe70b82ed0f7517d91eb9f3f352cbe97))
* enable multiple coinbase utxos ([#5879](https://github.com/tari-project/tari/issues/5879)) ([49e5c9c](https://github.com/tari-project/tari/commit/49e5c9c2fec823f0958a28e5c110cc3f34ba48d6))
* failure of min difficulty should not add block to list of bad blocks ([#5805](https://github.com/tari-project/tari/issues/5805)) ([38dc014](https://github.com/tari-project/tari/commit/38dc014405eb6887210861bd533f2b1dd17f48c2))
* improve block add where many orphan chain tips existed ([#5763](https://github.com/tari-project/tari/issues/5763)) ([19b3f21](https://github.com/tari-project/tari/commit/19b3f217aee6818678ed45082d910f1a2335a9ec))
* make prc errors ban-able for sync ([#5884](https://github.com/tari-project/tari/issues/5884)) ([4ca664e](https://github.com/tari-project/tari/commit/4ca664e5933f2266f594ecccf545d0eec3b18b40))
* prevent possible division by zero in difficulty calculation ([#5828](https://github.com/tari-project/tari/issues/5828)) ([f85a878](https://github.com/tari-project/tari/commit/f85a8785de49dda05b3dc54dfda4f5081424e06f))
* print warning for wallets in direct send only ([#5883](https://github.com/tari-project/tari/issues/5883)) ([6d8686d](https://github.com/tari-project/tari/commit/6d8686dc40ef701fe980698c30347da5b690de07))
* reduce timeouts and increase bans ([#5882](https://github.com/tari-project/tari/issues/5882)) ([df9bc9a](https://github.com/tari-project/tari/commit/df9bc9a912fe6e7c750e34a3dd7bd6796c6d758f))
* replace utxo MMR with SMT ([#5854](https://github.com/tari-project/tari/issues/5854)) ([ca74c29](https://github.com/tari-project/tari/commit/ca74c29db7264413dc3e6542b599db9760993170))
* up the timeout for comms ([#5758](https://github.com/tari-project/tari/issues/5758)) ([1054868](https://github.com/tari-project/tari/commit/1054868248342d0a07077d441151dc48adbfddf3))
* update key parsing ([#5900](https://github.com/tari-project/tari/issues/5900)) ([59d7ceb](https://github.com/tari-project/tari/commit/59d7cebd22cc86ab5d3691aa5dc3d73b37032442))
* update randomx ([#5894](https://github.com/tari-project/tari/issues/5894)) ([e445244](https://github.com/tari-project/tari/commit/e4452440bd9269402f1a5352e9c93cbfa6c72425))
* adaptable min difficulty check ([#5896](https://github.com/tari-project/tari/issues/5896)) ([76f323c](https://github.com/tari-project/tari/commit/76f323c67ee3f46d772b85c410a1c49376348195))
* add robustness to monero block extra field handling ([#5826](https://github.com/tari-project/tari/issues/5826)) ([597b9ef](https://github.com/tari-project/tari/commit/597b9ef7698ef705d550f6d3ecb1c27dbea79636))
* add validator mmr size ([#5873](https://github.com/tari-project/tari/issues/5873)) ([fd51045](https://github.com/tari-project/tari/commit/fd510452c0bf9eefcc4117f378c6434aea7b9fd1))
* completed transaction use bytes for transaction protocol (not hex string) in wallet database ([#5906](https://github.com/tari-project/tari/issues/5906)) ([61256cd](https://github.com/tari-project/tari/commit/61256cde3630f8d81e5648b1f5038ed6e847b9c2))
* add aux chain support for merge mining ([#5976](https://github.com/tari-project/tari/issues/5976)) ([6723dc7](https://github.com/tari-project/tari/commit/6723dc7a88b2c1e40efe51259cb26e12638b9668))
* add constant time comparison for grpc authentication ([#5902](https://github.com/tari-project/tari/issues/5902)) ([2fe44db](https://github.com/tari-project/tari/commit/2fe44db773bbf8ee7c4e306e08973ba25e6af10e))
* add getheaderbyhash method to grpc-js ([#5942](https://github.com/tari-project/tari/issues/5942)) ([ebc4539](https://github.com/tari-project/tari/commit/ebc45398ea7f9eda7f08830cec93f2bf8d4a0e38))
* add one-sided coinbase payments ([#5967](https://github.com/tari-project/tari/issues/5967)) ([89b19f6](https://github.com/tari-project/tari/commit/89b19f6de8f2acf28557ca37feda03af2657cf30))
* bans for bad incoming blocks ([#5934](https://github.com/tari-project/tari/issues/5934)) ([7acc44d](https://github.com/tari-project/tari/commit/7acc44d3dce5d8c9085ae5246a8a0a7487d19516))
* block endless peer stream ([#5951](https://github.com/tari-project/tari/issues/5951)) ([16b325d](https://github.com/tari-project/tari/commit/16b325defc2f42b9b34d3e1fd05a4b6cd6bcf965))
* block wallets from sending if BN connection stale ([#5949](https://github.com/tari-project/tari/issues/5949)) ([18d5f57](https://github.com/tari-project/tari/commit/18d5f57363fb085bfac080a7994cb5ced8c932ab))
* compile out the metrics ([#5944](https://github.com/tari-project/tari/issues/5944)) ([fa2fb27](https://github.com/tari-project/tari/commit/fa2fb27a5834bd56fda62c82a825a7f6d8391fd3))
* create min dust fee setting ([#5947](https://github.com/tari-project/tari/issues/5947)) ([8f5466c](https://github.com/tari-project/tari/commit/8f5466cb1d85518ba80190fa312281321aa721ff))
* disable console wallet grpc ([#5988](https://github.com/tari-project/tari/issues/5988)) ([883de17](https://github.com/tari-project/tari/commit/883de175dadee58c4f49fff9a655cae1a2450b3d))
* dont store entire monero coinbase transaction ([#5991](https://github.com/tari-project/tari/issues/5991)) ([23b10bf](https://github.com/tari-project/tari/commit/23b10bf2d3fdebd296a93eae0aaa5abcd4156de9))
* enable revealed-value proofs ([#5983](https://github.com/tari-project/tari/issues/5983)) ([f3f5879](https://github.com/tari-project/tari/commit/f3f5879903c619a9219c27ce4e77450f4a1b247b))
* fix difficulty overflow ([#5935](https://github.com/tari-project/tari/issues/5935)) ([55bbdf2](https://github.com/tari-project/tari/commit/55bbdf2481bb7522ede5cc3e37ca8cdeb323b4f7))
* grpc over tls ([#5990](https://github.com/tari-project/tari/issues/5990)) ([b80f7e3](https://github.com/tari-project/tari/commit/b80f7e366b14e10b3fb0e9835fb76dd5596d0cf8))
* limit max number of addresses ([#5960](https://github.com/tari-project/tari/issues/5960)) ([40fc940](https://github.com/tari-project/tari/commit/40fc9408161e404a9f4062362fe495de3c2e374f))
* move kernel MMR position to `u64` ([#5956](https://github.com/tari-project/tari/issues/5956)) ([cdd8a31](https://github.com/tari-project/tari/commit/cdd8a3135765c3b5a87027f9a5e0103e737c709a))
* network specific domain hashers ([#5980](https://github.com/tari-project/tari/issues/5980)) ([d7ab283](https://github.com/tari-project/tari/commit/d7ab2838cc08a7c12ccf443697c1560b1ea40b03))
* **node grpc:** add grpc authentication to the node ([#5928](https://github.com/tari-project/tari/issues/5928)) ([3d95e8c](https://github.com/tari-project/tari/commit/3d95e8cb0543f5bdb284f2ea0771e2f03748b71a))
* remove panics from applications ([#5943](https://github.com/tari-project/tari/issues/5943)) ([18c3d0b](https://github.com/tari-project/tari/commit/18c3d0be8123cdc362fdeaed66c45ad17c3e7dfa))
* sender and receiver protocols use bytes (not hex string) in wallet database ([#5950](https://github.com/tari-project/tari/issues/5950)) ([4cbdfec](https://github.com/tari-project/tari/commit/4cbdfec945857c5b7a334962e137d2c8dc4d4c4a))
* warnings for untrusted urls ([#5955](https://github.com/tari-project/tari/issues/5955)) ([e2e278c](https://github.com/tari-project/tari/commit/e2e278c9a4d09f8e0136e9b3ae2f93afc3e9ac4a))
*  hazop findings ([#6020](https://github.com/tari-project/tari/issues/6020)) ([a68d0dd](https://github.com/tari-project/tari/commit/a68d0dd2fb7719ae99bcd2b62980b5f37d66284a))
* add miner input processing ([#6016](https://github.com/tari-project/tari/issues/6016)) ([26f5b60](https://github.com/tari-project/tari/commit/26f5b6044832f737c7019dab0e00d2234aac442f))
* add wallet ffi shutdown tests ([#6007](https://github.com/tari-project/tari/issues/6007)) ([3129ce8](https://github.com/tari-project/tari/commit/3129ce8dd066ea16900ee8add4e608c1890c6545))
* fix hazop findings ([#6017](https://github.com/tari-project/tari/issues/6017)) ([0bc62b4](https://github.com/tari-project/tari/commit/0bc62b4a5b78893a226700226bac01590a543bb8))
* make base node support 1 click mining ([#6019](https://github.com/tari-project/tari/issues/6019)) ([d377269](https://github.com/tari-project/tari/commit/d3772690c36e0dcb6476090fc428e5745298e398))
* update faucets ([#6024](https://github.com/tari-project/tari/issues/6024)) ([394976c](https://github.com/tari-project/tari/commit/394976cc591f9551e1542f2730a8ec299b524229))
* update status ([#6008](https://github.com/tari-project/tari/issues/6008)) ([e19ce15](https://github.com/tari-project/tari/commit/e19ce15549b138d462060997d40147bad39a1871))
* console wallet use dns seeds ([#6034](https://github.com/tari-project/tari/issues/6034)) ([b194954](https://github.com/tari-project/tari/commit/b194954f489bd8ac234993e65463a24808dce8f2))
* update tests and constants ([#6028](https://github.com/tari-project/tari/issues/6028)) ([d558206](https://github.com/tari-project/tari/commit/d558206ea62c12f3258ede8cfcbf9d44f139ccdd))


### Bug Fixes

* add SECURITY.md Vulnerability Disclosure Policy ([#5351](https://github.com/tari-project/tari/issues/5351)) ([72daaf5](https://github.com/tari-project/tari/commit/72daaf5ef614ceb805f690db12c7fefc642d5453))
* added missing log4rs features ([#5356](https://github.com/tari-project/tari/issues/5356)) ([b9031bb](https://github.com/tari-project/tari/commit/b9031bbbece1988c1de180cabbf4e3acfcb50836))
* allow public addresses from command line ([#5303](https://github.com/tari-project/tari/issues/5303)) ([349ac89](https://github.com/tari-project/tari/commit/349ac8957bc513cd4110eaac69550ffa0816862b))
* clippy issues with config ([#5334](https://github.com/tari-project/tari/issues/5334)) ([026f0d5](https://github.com/tari-project/tari/commit/026f0d5e33d524ad302e7edd0c82e108a17800b6))
* default network selection ([#5333](https://github.com/tari-project/tari/issues/5333)) ([cf4b2c8](https://github.com/tari-project/tari/commit/cf4b2c8a4f5849ba51dab61595dfed1a9249c580))
* make the first output optional in the wallet ([#5352](https://github.com/tari-project/tari/issues/5352)) ([bf16140](https://github.com/tari-project/tari/commit/bf16140ecd1ad0ae25f8a9b8cde9c3e4f1d12a02))
* remove wallet panic ([#5338](https://github.com/tari-project/tari/issues/5338)) ([536d16d](https://github.com/tari-project/tari/commit/536d16d2feea283ac1b8f546f479b76465938c4b))
* wallet .h file for lib wallets ([#5330](https://github.com/tari-project/tari/issues/5330)) ([22a3a17](https://github.com/tari-project/tari/commit/22a3a17db6ef8889cb3a73dfe2db081a0691a68c))
* **comms:** only set final forward address if configured to port 0 ([#5406](https://github.com/tari-project/tari/issues/5406)) ([ff7fb6d](https://github.com/tari-project/tari/commit/ff7fb6d6b4ab4f77d108b2d9b7fd010c77e613c7))
* deeplink to rfc spec ([#5342](https://github.com/tari-project/tari/issues/5342)) ([806d3b8](https://github.com/tari-project/tari/commit/806d3b8cc6668f23bb77ca7040833e080c173063))
* don't use in memory datastores for chat client dht in integration tests ([#5399](https://github.com/tari-project/tari/issues/5399)) ([cbdca6f](https://github.com/tari-project/tari/commit/cbdca6fcc8ae61ed2dbfacca9da1a59c78945045))
* fix panic when no public addresses ([#5367](https://github.com/tari-project/tari/issues/5367)) ([49be2a2](https://github.com/tari-project/tari/commit/49be2a27a8aead96c180cb988614e3696c338530))
* loop on mismatched passphrase entry ([#5396](https://github.com/tari-project/tari/issues/5396)) ([ed120b2](https://github.com/tari-project/tari/commit/ed120b277371be7b9bd61c825aa7d61b104d3ac6))
* use domain separation for wallet message signing ([#5400](https://github.com/tari-project/tari/issues/5400)) ([7d71f8b](https://github.com/tari-project/tari/commit/7d71f8bef94fddf1ffa345e6b599cf02ee6ab935))
* use mined at timestamp in fauxconfirmation (#5443) ([f3833c9f](https://github.com/tari-project/tari/commit/f3833c9fc46d77fddaa7a23ef1d53ba9d860182a), breaks [#](https://github.com/tari-project/tari/issues/), [#](https://github.com/tari-project/tari/issues/), [#](https://github.com/tari-project/tari/issues/))
* fix custom wallet startup logic for console wallet (#5429) ([0c1e5765](https://github.com/tari-project/tari/commit/0c1e5765676a9281b45fd66c8846b78ea4c76125), breaks [#](https://github.com/tari-project/tari/issues/), [#](https://github.com/tari-project/tari/issues/), [#](https://github.com/tari-project/tari/issues/))
* **balanced_mp:**  removes some panics, adds some checks and new tests (#5432) ([602f416f](https://github.com/tari-project/tari/commit/602f416f674b5e1835a634f3c8ab123001af600e), breaks [#](https://github.com/tari-project/tari/issues/), [#](https://github.com/tari-project/tari/issues/), [#](https://github.com/tari-project/tari/issues/))
* **comms:**  validate onion3 checksum (#5440) ([0dfdb3a4](https://github.com/tari-project/tari/commit/0dfdb3a4bef51952f0cecf6f6fcb00f6b2bfe302), breaks [#](https://github.com/tari-project/tari/issues/), [#](https://github.com/tari-project/tari/issues/), [#](https://github.com/tari-project/tari/issues/))
* **wallet-ffi:**  don't block on start (#5437) ([27fe8d9d](https://github.com/tari-project/tari/commit/27fe8d9d2fc3ea6468605ef5edea56efdcc8248f), breaks [#](https://github.com/tari-project/tari/issues/), [#](https://github.com/tari-project/tari/issues/), [#](https://github.com/tari-project/tari/issues/))
* **mmr:**  support zero sized balanced merkle proof (#5474) ([ef984823](https://github.com/tari-project/tari/commit/ef98482313c9b9480ac663709162ae62e9c26978)
* **wallet:**  use correct output features for send to self (#5472) ([ce1f0686](https://github.com/tari-project/tari/commit/ce1f0686f56367ff094bf28cfd0388b2ea94a8c9)
*   covenant nit picking (#5506) ([301ca495](https://github.com/tari-project/tari/commit/301ca49513948e84bc972e5d75e16f6882d8fb8b)
*   overflow of target difficulty (#5493) ([822dac60](https://github.com/tari-project/tari/commit/822dac609a4d148227c1bac61d9d81bc1a5925ac)
*   coinbase recovery (#5487) ([48dd157a](https://github.com/tari-project/tari/commit/48dd157a82c4459021a1a02d14f7a3e95e24ebd3))
* **core:**
  *  minor audit improvements (#5486) ([8756e0b3](https://github.com/tari-project/tari/commit/8756e0b3c0030700a2409e7d29c4822f8e75aacb)
  *  remove implicit change in protocol for partial/full signatures (#5488) ([fef701ef](https://github.com/tari-project/tari/commit/fef701efbd07eb769dbe11b5a0cb74c807d7d88c)
  *  compile error in wallet/FFI (#5497) ([49610736](https://github.com/tari-project/tari/commit/49610736b839c1067820ad841d4730ae8032eb2b)
* **core/base_node:**  safe `mmr_position` cast in horizon sync (#5503) ([fb3ac60b](https://github.com/tari-project/tari/commit/fb3ac60b163184f89b2d69b0b9ce3d9b2cfdeeee)
* **core/consensus:**  include `coinbase_extra` max size into coinbase weight calculation (#5501) ([4554cc5f](https://github.com/tari-project/tari/commit/4554cc5f075bf9392c75fedb7576753612b374ee)
* **core/keymanager:**  use tokio rwlock for keymanagers (#5494) ([229aee02](https://github.com/tari-project/tari/commit/229aee029dbb8d401feb74be51caa4f26dd93be1)
* **core/transactions:**  resolve or remove TODOs (#5500) ([4a9f73c7](https://github.com/tari-project/tari/commit/4a9f73c79b98298e61115744b3e467622dd4945b)
* **core/weighting:**  remove optional and define correct rounding for usize::MAX (#5490) ([38c399a2](https://github.com/tari-project/tari/commit/38c399a2e5ee28878e0238e2b8e13c15f658ffbc)
* **mempool:**  remove TODOs and other minor changes (#5498) ([a1f24417](https://github.com/tari-project/tari/commit/a1f244179390d9a4845bce96e3c6a506a59e4b16)
*   mempool should use the correct version of the consensus constant (#5549) ([46ab3ef0](https://github.com/tari-project/tari/commit/46ab3ef07e41b091b869ef59376d0709a24e7437))
*   mempool fetch_highest_priority_txs (#5551) ([f7f749c4](https://github.com/tari-project/tari/commit/f7f749c4c476f489f9e30afb87461780d1996834)
*   remove optional timestamp verification bypass (#5552) ([b5a5bed2](https://github.com/tari-project/tari/commit/b5a5bed2c23c273d3787afa1c845f62badec1a46))
*   update code coverage approach (#5540) ([7a9830ed](https://github.com/tari-project/tari/commit/7a9830edb66b6be3edc40b84ae8a1a9c3f4ef525)
*   use correct TOML field for console wallet network address (#5531) ([70763dde](https://github.com/tari-project/tari/commit/70763dde25c1569013e489a0798540fd66dfa571)
*   llvm-tools installed correctly (#5534) ([4ab4b965](https://github.com/tari-project/tari/commit/4ab4b965e5f0556d508ec071a152deb5ad8ea8cc))
*   push test coverage even if some tests fail (#5533) ([053c748d](https://github.com/tari-project/tari/commit/053c748d3d7aee674bada24609612bde9ba1420e)
* **console-wallet:**  fix possible subtract underflow panic in list (#5535) ([8d5e8e6e](https://github.com/tari-project/tari/commit/8d5e8e6eac45b11867cee6104c207f6559851405)
* **core:**  disable covenants for all networks except igor and localnet (#5505) ([308f5299](https://github.com/tari-project/tari/commit/308f5299007a67df8fb9fe73763809264005e35c)
*   add a not before proof (#5560) ([11f42fb0](https://github.com/tari-project/tari/commit/11f42fb0942da3bd64db8ad203b75c364dbe0926)
*   borsh sized serialization should be fallible   (#5537) ([53058ce2](https://github.com/tari-project/tari/commit/53058ce299cb89f118017ccec5e98a991a7fcbcc)
*   add documentation to covenant crate (#5524) ([442d75b0](https://github.com/tari-project/tari/commit/442d75b09f439e4bc81919fc42eaf43846b2c8ca)
*   covenants audit (#5526) ([dbb59758](https://github.com/tari-project/tari/commit/dbb59758a92cdf4483574dc6e7c719efa94eedfd)
* add validator mr to mining hash ([#5615](https://github.com/brianp/tari/issues/5615)) ([91db6fb](https://github.com/brianp/tari/commit/91db6fb3b9ee1998d186fba3bbb57c970d8e4c5c))
* add-peer also dials the peer ([#5727](https://github.com/brianp/tari/issues/5727)) ([cc8573a](https://github.com/brianp/tari/commit/cc8573ae3ec69d748d3793f02136fd6772983850))
* addition overflow when coinbase + fees is too high ([#5706](https://github.com/brianp/tari/issues/5706)) ([13993f1](https://github.com/brianp/tari/commit/13993f1763eee84f566d6aea83661eb868e47eff))
* adds bans for horizon sync ([#5661](https://github.com/brianp/tari/issues/5661)) ([826473d](https://github.com/brianp/tari/commit/826473d2a96fc6c978e5ccdce38c052919514a37))
* ban peers if they send a bad protobuf message ([#5693](https://github.com/brianp/tari/issues/5693)) ([58cbfe6](https://github.com/brianp/tari/commit/58cbfe677f7328d4c9f9c98b1ada1acb369a47ac))
* better timeout for lagging ([#5705](https://github.com/brianp/tari/issues/5705)) ([5e8a3ec](https://github.com/brianp/tari/commit/5e8a3ecbc9a00cee823260d4a5e33b3e3a60bc9c))
* check bytes remaining on monero blocks ([#5610](https://github.com/brianp/tari/issues/5610)) ([1087fa9](https://github.com/brianp/tari/commit/1087fa9d7846b1bd11431475cc8ca3fd9def8ec6))
* **comms/dht:** limit number of peer claims and addresses for all sources ([#5702](https://github.com/brianp/tari/issues/5702)) ([88ed293](https://github.com/brianp/tari/commit/88ed2935f5094e669470f2c015d055f9c3286941))
* **comms:** check multiple addresses for inbound liveness check ([#5611](https://github.com/brianp/tari/issues/5611)) ([3937ae4](https://github.com/brianp/tari/commit/3937ae422f57f936ad3d2ead8b92ce4fa5adf855))
* **comms:** dont overwrite ban-reason in add_peer ([#5720](https://github.com/brianp/tari/issues/5720)) ([3b9890b](https://github.com/brianp/tari/commit/3b9890ba5857cc8767be77a024d01bf4826e3956))
* **comms:** greatly reduce timeouts for first byte and noise handshake ([#5728](https://github.com/brianp/tari/issues/5728)) ([47a3196](https://github.com/brianp/tari/commit/47a319616dde78c243b4558a51a7d81efc8393e1))
* **comms:** only permit a single inbound messaging substream per peer ([#5731](https://github.com/brianp/tari/issues/5731)) ([c91a35f](https://github.com/brianp/tari/commit/c91a35f82557afd39c9b83f643876630bb4275c5))
* **comms:** timeout and ban for bad behaviour in protocol negotation ([#5679](https://github.com/brianp/tari/issues/5679)) ([d03d0b5](https://github.com/brianp/tari/commit/d03d0b5fc58d4e284b1f6ce4554830fdbbb78efe))
* **comms:** use noise XX handshake pattern for improved privacy ([#5696](https://github.com/brianp/tari/issues/5696)) ([d0ea406](https://github.com/brianp/tari/commit/d0ea406e57b8bbb65196c2e880671da2e51f2b62))
* **core:** always pass the correct timestamp window to header validatior ([#5624](https://github.com/brianp/tari/issues/5624)) ([29700c3](https://github.com/brianp/tari/commit/29700c3d9aa4698742c0c9cd5e313fd3d0727626))
* **dht:** add SAF bans ([#5711](https://github.com/brianp/tari/issues/5711)) ([594e03e](https://github.com/brianp/tari/commit/594e03eada389c1a131d5877f42f8c43b85a9fbe))
* **dht:** limit peer sync and ban on server-caused errors ([#5714](https://github.com/brianp/tari/issues/5714)) ([b3f2dca](https://github.com/brianp/tari/commit/b3f2dcae88740abd1bd4c64f64d89010a13a214b))
* duplicate tari header in monero coinbase ([#5604](https://github.com/brianp/tari/issues/5604)) ([f466840](https://github.com/brianp/tari/commit/f466840a24cd678aac82ae4eaa2661dca2567675))
* error out the stx protocol if the sender sends unsupported data ([#5572](https://github.com/brianp/tari/issues/5572)) ([8a085cd](https://github.com/brianp/tari/commit/8a085cded40b95fb5d3136743a97e50874ee2903))
* handle out of sync errors when returning mempool transactions ([#5701](https://github.com/brianp/tari/issues/5701)) ([b0337cf](https://github.com/brianp/tari/commit/b0337cfaac92939db968231cc368b56836c2cf7e))
* handle target difficulty conversion failure ([#5710](https://github.com/brianp/tari/issues/5710)) ([431c35a](https://github.com/brianp/tari/commit/431c35ac5006d5cd265484e98a224b7f7e75703f))
* header sync ([#5647](https://github.com/brianp/tari/issues/5647)) ([4583eef](https://github.com/brianp/tari/commit/4583eef444f4f71d6d702a9997566dad42a9fce4))
* horizon sync ([#5724](https://github.com/brianp/tari/issues/5724)) ([660a5c1](https://github.com/brianp/tari/commit/660a5c1119f76ce30386860b27ed21316d9ace55))
* **horizon_sync:** check for leftover unpruned outputs ([#5704](https://github.com/brianp/tari/issues/5704)) ([dc5cfce](https://github.com/brianp/tari/commit/dc5cfced6b81b8c7c036db920f7cbbf36d601789))
* **horizon_sync:** check max number of kernels/utxos from peer ([#5703](https://github.com/brianp/tari/issues/5703)) ([5e4f3c2](https://github.com/brianp/tari/commit/5e4f3c20f0de1d0d7c525cdcfbe86e56b9e909f3))
* **horizon_sync:** try sync with next next peer if current one fails ([#5699](https://github.com/brianp/tari/issues/5699)) ([a58ec1f](https://github.com/brianp/tari/commit/a58ec1f40fbc57e147e6fb5c21c6b2b5151150df))
* limit monero hashes and force coinbase to be tx 0 ([#5602](https://github.com/brianp/tari/issues/5602)) ([2af1198](https://github.com/brianp/tari/commit/2af119824e3b21294c4545b18b2fb6a86bb96ea4))
* make sure all needed libs are required for chatffi ([#5659](https://github.com/brianp/tari/issues/5659)) ([241ca67](https://github.com/brianp/tari/commit/241ca673ee5b3503198f3e662383ad0f6387313c))
* memory overflow panic ([#5658](https://github.com/brianp/tari/issues/5658)) ([304e40f](https://github.com/brianp/tari/commit/304e40fb44a3dd9765c10147e1ee85344769c55a))
* miner delay attack ([#5582](https://github.com/brianp/tari/issues/5582)) ([bece2d0](https://github.com/brianp/tari/commit/bece2d0bf82c757808723dba6ec3456bb8e23b2e))
* minor fixes for multiple address support ([#5617](https://github.com/brianp/tari/issues/5617)) ([efa36eb](https://github.com/brianp/tari/commit/efa36eb7dc92905cc085359c35255678136a15b1))
* monero fork attack ([#5603](https://github.com/brianp/tari/issues/5603)) ([9c81b4d](https://github.com/brianp/tari/commit/9c81b4d875aa7794226a97a4a90c9c0b3d6d4585))
* only allow a monero header if it serializes back to the same data ([#5716](https://github.com/brianp/tari/issues/5716)) ([e70c752](https://github.com/brianp/tari/commit/e70c752d6014f0dd9d1a7aeda9a39bbd6dabc21b))
* peer connection to stale nodes ([#5579](https://github.com/brianp/tari/issues/5579)) ([eebda00](https://github.com/brianp/tari/commit/eebda00bd28aae70813c644ff2b63925cc934ced))
* potential u64 overflow panic ([#5688](https://github.com/brianp/tari/issues/5688)) ([f261b79](https://github.com/brianp/tari/commit/f261b7900f879ad991de42073094f8cb4443b8d2))
* prevent access violation when running multiple vms at the same time ([#5734](https://github.com/brianp/tari/issues/5734)) ([18aead2](https://github.com/brianp/tari/commit/18aead232c2da7f6ec4eda152f8ce53e2601a92d))
* remove potential u64 overflow panic ([#5686](https://github.com/brianp/tari/issues/5686)) ([90a8a21](https://github.com/brianp/tari/commit/90a8a21765f2c1a6930775ed4cd95fe8766b02d8))
* remove tari prefix and only allow one mergemining tag ([#5722](https://github.com/brianp/tari/issues/5722)) ([3a7c227](https://github.com/brianp/tari/commit/3a7c227002f8bfacde2ab8081c79bfac435484ce))
* remove timestamp from header in proto files ([#5667](https://github.com/brianp/tari/issues/5667)) ([403b0c6](https://github.com/brianp/tari/commit/403b0c62af9ed2f2eefc48e0feb5025d8c853ecc))
* save dial result on error ([#5717](https://github.com/brianp/tari/issues/5717)) ([c66af69](https://github.com/brianp/tari/commit/c66af69e5ccb31d2fcaf9a8fa29d2e0b5470eeba))
* sorted edge case ([#5590](https://github.com/brianp/tari/issues/5590)) ([f7b2193](https://github.com/brianp/tari/commit/f7b21930c7841e7a8801f4c37d1ee0e8111162bb))
* sparse Merkle tree key querying ([#5566](https://github.com/brianp/tari/issues/5566)) ([623839f](https://github.com/brianp/tari/commit/623839f58116c0828bc5406adbd1dd1b68e7bb3d))
* syncing from prune node ([#5733](https://github.com/brianp/tari/issues/5733)) ([166f469](https://github.com/brianp/tari/commit/166f469cd1122676ec95b88163ee97058cc28fdf))
* **sync:** remove mem::take in all syncs ([#5721](https://github.com/brianp/tari/issues/5721)) ([a48e430](https://github.com/brianp/tari/commit/a48e430b6b5bc21c5998009738be1436e479f7ec))
* **sync:** unify ban logic in all sync processes ([#5713](https://github.com/brianp/tari/issues/5713)) ([4b2b28b](https://github.com/brianp/tari/commit/4b2b28bf2390c400d547cdaa801ff967eb92ac38))
* update peers seed for esme ([#5573](https://github.com/brianp/tari/issues/5573)) ([0f6b750](https://github.com/brianp/tari/commit/0f6b7504bbfc902ffab89f1904dee237270c690b))
* add lock height and kernel features checks on default transactions ([#5836](https://github.com/tari-project/tari/issues/5836)) ([1f87226](https://github.com/tari-project/tari/commit/1f87226722b12750424ab2f4861fe0475a67dfd6))
* ban peer if it sends bad liveness data ([#5844](https://github.com/tari-project/tari/issues/5844)) ([eb40fc4](https://github.com/tari-project/tari/commit/eb40fc44cfc0605545ba9e831c8d27209a4db51f))
* change truncate_from_bits to from_bits ([#5773](https://github.com/tari-project/tari/issues/5773)) ([fb18078](https://github.com/tari-project/tari/commit/fb18078d888b7c65601e8261d66fca366ffff28b))
* chat ffi seed peers ([#5786](https://github.com/tari-project/tari/issues/5786)) ([c04996f](https://github.com/tari-project/tari/commit/c04996f01f3e5627acc376a27e7abcb61d7dda5c))
* **chatffi:** return and read from ptrs ([#5827](https://github.com/tari-project/tari/issues/5827)) ([dd2eddb](https://github.com/tari-project/tari/commit/dd2eddbe9280870485974edd611e224ae585b76a))
* **comms+dht:** mark peers as online inbound connection,join ([#5741](https://github.com/tari-project/tari/issues/5741)) ([e8413ea](https://github.com/tari-project/tari/commit/e8413ea364c0a17785b475ac57d74244b62a7375))
* **diagrams:** missing quotes for messaging diagram ([#5750](https://github.com/tari-project/tari/issues/5750)) ([a8f6eb5](https://github.com/tari-project/tari/commit/a8f6eb5e48e6e823b96919bec87843300311caae))
* **diagrams:** missing quotes for protocol negotiation diagram ([#5751](https://github.com/tari-project/tari/issues/5751)) ([45c20a3](https://github.com/tari-project/tari/commit/45c20a30b849b92e1f6fe402d7e7e657ccf9f663))
* don't ban a peer for sending a banned peer ([#5843](https://github.com/tari-project/tari/issues/5843)) ([12f8a75](https://github.com/tari-project/tari/commit/12f8a75060e1d15fbeac589c568f7ee9e04eb900))
* fix erroneous warning message ([#5846](https://github.com/tari-project/tari/issues/5846)) ([8afcd8b](https://github.com/tari-project/tari/commit/8afcd8b5545a433c92d3a47b4f85b4e89a5408b8))
* get rid of possible 'expect' ([#5794](https://github.com/tari-project/tari/issues/5794)) ([467a8d4](https://github.com/tari-project/tari/commit/467a8d4f4493814f1102d6863fc844896e94a8ec))
* grpc request overflows ([#5812](https://github.com/tari-project/tari/issues/5812)) ([36d72e8](https://github.com/tari-project/tari/commit/36d72e8b2239870550060fc9e0c183131ee3c2fa))
* handle possible underflow in smt ([#5769](https://github.com/tari-project/tari/issues/5769)) ([558e6f2](https://github.com/tari-project/tari/commit/558e6f2bf7d00fb2c7c506b7000237aba928238c))
* listing mode is synced ([#5830](https://github.com/tari-project/tari/issues/5830)) ([ff5a5d8](https://github.com/tari-project/tari/commit/ff5a5d82e3ddbe191bda8b8132590c2afb3282f2))
* mempool panic ([#5814](https://github.com/tari-project/tari/issues/5814)) ([754fb16](https://github.com/tari-project/tari/commit/754fb16e4ae79bb8d712419f0f6bf59efbaf0ce1))
* **p2p:** enable auto join when online ([#5738](https://github.com/tari-project/tari/issues/5738)) ([eb74bbb](https://github.com/tari-project/tari/commit/eb74bbba3746b78c3fd8e0ee5066f1d4d987af3e))
* panic overflow ([#5819](https://github.com/tari-project/tari/issues/5819)) ([af31ba1](https://github.com/tari-project/tari/commit/af31ba1e6deb64a68ec74eac090fdcfc9e8a52ca))
* possible exception in request_context ([#5784](https://github.com/tari-project/tari/issues/5784)) ([6c8e2d3](https://github.com/tari-project/tari/commit/6c8e2d395799757e5a946fe01226f739d0706741))
* potential index out of bounds ([#5775](https://github.com/tari-project/tari/issues/5775)) ([f17ac6b](https://github.com/tari-project/tari/commit/f17ac6b61edfe47dacf091969382c6b17e7bf214))
* potential overflow ([#5759](https://github.com/tari-project/tari/issues/5759)) ([5c93e35](https://github.com/tari-project/tari/commit/5c93e35c785a7a19f8e6c762e3f1df8f8207877e))
* potential overflow ([#5778](https://github.com/tari-project/tari/issues/5778)) ([1d1332d](https://github.com/tari-project/tari/commit/1d1332d21ba0db18e9f3a3c253963fc1735b8193))
* potential sync stuck ([#5760](https://github.com/tari-project/tari/issues/5760)) ([c5ed816](https://github.com/tari-project/tari/commit/c5ed816c80eae43348593e636e4b56da98d8af6b))
* recovery passphrase flow ([#5877](https://github.com/tari-project/tari/issues/5877)) ([4159b76](https://github.com/tari-project/tari/commit/4159b766669e682bb9593c4e7cd3ddb298a56e0b))
* remove peer ([#5757](https://github.com/tari-project/tari/issues/5757)) ([4c48a26](https://github.com/tari-project/tari/commit/4c48a26f20d800b2098c18b723dfb83cb878f0ad))
* remove statement from sparse Merkle tree proofs ([#5768](https://github.com/tari-project/tari/issues/5768)) ([d630d11](https://github.com/tari-project/tari/commit/d630d114f1866f24e729cda0f8cf19f298e7bd50))
* stuck on sync ([#5739](https://github.com/tari-project/tari/issues/5739)) ([33b37a8](https://github.com/tari-project/tari/commit/33b37a8c37f3e1883ef3ebf27a8e18d4dd63fc92))
* unwraps in rpc client ([#5770](https://github.com/tari-project/tari/issues/5770)) ([6f0d20a](https://github.com/tari-project/tari/commit/6f0d20aa30d3dcc23630d3a9650802f8c1ce3a61))
* **proto:** remove proto bytes for std bytes ([#5835](https://github.com/tari-project/tari/issues/5835)) ([491ed83](https://github.com/tari-project/tari/commit/491ed83aaea166a6e60d40e76b8574625b56cf98))
* **proto:** remove proto timestamp wrapper types ([#5833](https://github.com/tari-project/tari/issues/5833)) ([43b994e](https://github.com/tari-project/tari/commit/43b994e62378a9ed241842fc18f01d69231f089f))
* upgrade bitflags crate ([#5831](https://github.com/tari-project/tari/issues/5831)) ([dae7dd9](https://github.com/tari-project/tari/commit/dae7dd9d1f2277b6192dc0ed7bea26b7d2d946ac))
* lmdb flag set wrong on database ([#5916](https://github.com/tari-project/tari/issues/5916)) ([60efd35](https://github.com/tari-project/tari/commit/60efd353973a87b1e0cebc7246649a38b5731051))
* **tariscript:** protect compare and check height from underflows ([#5872](https://github.com/tari-project/tari/issues/5872)) ([aa2ae10](https://github.com/tari-project/tari/commit/aa2ae1066818c1776bd268932fbd3be09f21bf0e))
* display ([#5982](https://github.com/tari-project/tari/issues/5982)) ([8cce48c](https://github.com/tari-project/tari/commit/8cce48cd8bd9b6f780376030918972e993fc1ab7))
* fix opcode signatures ([#5966](https://github.com/tari-project/tari/issues/5966)) ([dc26ca6](https://github.com/tari-project/tari/commit/dc26ca6aeeb4196d0496f2977027ac63a4324043))
* fix the windows installer ([#5938](https://github.com/tari-project/tari/issues/5938)) ([3e65a28](https://github.com/tari-project/tari/commit/3e65a28c5e3729024d70e2b7f55910c8c808495c))
* fix the windows installer auto build ([#5939](https://github.com/tari-project/tari/issues/5939)) ([a138b78](https://github.com/tari-project/tari/commit/a138b7892d4b41a460b8dd8b9466f34e90f65469))
* **shutdown:** is_triggered returns up-to-date value without first polling ([#5997](https://github.com/tari-project/tari/issues/5997)) ([49f2053](https://github.com/tari-project/tari/commit/49f20534ec808427d059cde6892adc5597f33391))
* standardize gRPC authentication and mitigate DoS ([#5936](https://github.com/tari-project/tari/issues/5936)) ([623f127](https://github.com/tari-project/tari/commit/623f12768daf8329731249cf7e4c644e338d9700))
* **tariscript:** multisig ordered signatures and pubkeys ([#5961](https://github.com/tari-project/tari/issues/5961)) ([14e334a](https://github.com/tari-project/tari/commit/14e334aff346aae8a081599488135c905c2c1f84))
* update `ToRistrettoPoint` handling ([#5973](https://github.com/tari-project/tari/issues/5973)) ([12e84f4](https://github.com/tari-project/tari/commit/12e84f42ee1842875f72716833e96d0b84460c78))
* new faucet for esmeralda ([#6001](https://github.com/tari-project/tari/issues/6001)) ([4eccc39](https://github.com/tari-project/tari/commit/4eccc392394b03e974b36538096f640d2b98d25d))
* remove mutable mmr ([#5954](https://github.com/tari-project/tari/issues/5954)) ([0855583](https://github.com/tari-project/tari/commit/0855583c9fb138f7d1633c1829a8cf3f23048c49))
* ups the min difficulty ([#5999](https://github.com/tari-project/tari/issues/5999)) ([fc1e555](https://github.com/tari-project/tari/commit/fc1e555edc56c9d01d7e9cb4d2c7cd0421616034))
* **chat:** chat client possible panics ([#6015](https://github.com/tari-project/tari/issues/6015)) ([cf66c51](https://github.com/tari-project/tari/commit/cf66c51483f4b2744221fb652f3b32340d2ee693))
* chat build ([#6026](https://github.com/tari-project/tari/issues/6026)) ([15793b7](https://github.com/tari-project/tari/commit/15793b7e4dfdcaaad6ec90e357348daf42300eab))
* remove duplicate config settings ([#6029](https://github.com/tari-project/tari/issues/6029)) ([662af28](https://github.com/tari-project/tari/commit/662af28bf811c771cf0fdf9b583c1296a2283188))

## [0.49.0-rc.0](https://github.com/tari-project/tari/compare/v0.48.0-rc.0...v0.49.0-rc.0) (2023-04-12)


### ⚠ BREAKING CHANGES

* **wallet:** use ECDH shard secret for burn mask with claim pubkey (#5238)
* **wallet:** ensure burn shared keys and hashes match dan layer (#5245)
* add claim public key to OutputFeatures (#5239)
* change signature construction to allow better HW support (#5282)
* move key manager service to key_manager (#5284)
* add igor faucet (#5281)
* reset dates for networks (#5283)
* add paging to utxo stream request (#5302)

### Features

* add necessary trait bounds to balanced merkle tree ([#5232](https://github.com/tari-project/tari/issues/5232)) ([3b971a3](https://github.com/tari-project/tari/commit/3b971a3b0e39be774a1a21c477222d95a0e1b242))
* update tari-crypto to v0.16.8 ([#5236](https://github.com/tari-project/tari/issues/5236)) ([c9d355b](https://github.com/tari-project/tari/commit/c9d355baeea2d6087f72df8c2c1645ef2c06ce88))
* **wallet:** use ECDH shard secret for burn mask with claim pubkey ([#5238](https://github.com/tari-project/tari/issues/5238)) ([78838bf](https://github.com/tari-project/tari/commit/78838bfc64839be0ba79d1d668d0c6fb2e72e69e))
* add claim public key to OutputFeatures ([#5239](https://github.com/tari-project/tari/issues/5239)) ([3e7d82c](https://github.com/tari-project/tari/commit/3e7d82c440b162cc5a7e3e97b1fb18acdc6dd681))
* reset esmeralda ([#5247](https://github.com/tari-project/tari/issues/5247)) ([aa2a3ad](https://github.com/tari-project/tari/commit/aa2a3ad5910312642c8652996942993cf6b9df52))
* added FFI function `wallet_get_network_and_version` [#5252](https://github.com/tari-project/tari/issues/5252) ([#5263](https://github.com/tari-project/tari/issues/5263)) ([4b09b59](https://github.com/tari-project/tari/commit/4b09b59ce0cbc7e5c270c4c06a671c2fcff18bfc))
* change signature construction to allow better HW support ([#5282](https://github.com/tari-project/tari/issues/5282)) ([82d2dcb](https://github.com/tari-project/tari/commit/82d2dcb04ced94f05a0801c5cb97bbebc41ca3e0))
* improved passphrase flow ([#5279](https://github.com/tari-project/tari/issues/5279)) ([ac21da6](https://github.com/tari-project/tari/commit/ac21da60abec25db14e7201a5f82e15e4f7f2fe0))
* added auxiliary callback to push base node state changes [#5109](https://github.com/tari-project/tari/issues/5109) ([#5257](https://github.com/tari-project/tari/issues/5257)) ([b7f7d31](https://github.com/tari-project/tari/commit/b7f7d31fb634804ecf2f8ba1c39094163944f584))
* move key manager service to key_manager ([#5284](https://github.com/tari-project/tari/issues/5284)) ([d50ed02](https://github.com/tari-project/tari/commit/d50ed02675dbca9294882e5bbe522b8fda00fb2a))
* reset dates for networks ([#5283](https://github.com/tari-project/tari/issues/5283)) ([d6342a4](https://github.com/tari-project/tari/commit/d6342a4200cb7de469575d67129f9214535cf237))
* add extended mask recovery ([#5301](https://github.com/tari-project/tari/issues/5301)) ([23d882e](https://github.com/tari-project/tari/commit/23d882eb783f3d94efbfdd928b3d87b2907bf2d7))
* add network name to data path and --network flag to the miners ([#5291](https://github.com/tari-project/tari/issues/5291)) ([1f04beb](https://github.com/tari-project/tari/commit/1f04bebd4f6d14432aab923baeab17d1d6cc39bf))
* add other code template types ([#5242](https://github.com/tari-project/tari/issues/5242)) ([93e5e85](https://github.com/tari-project/tari/commit/93e5e85cbc13be33bea40c7b8289d0ff344df08c))
* add paging to utxo stream request ([#5302](https://github.com/tari-project/tari/issues/5302)) ([3540309](https://github.com/tari-project/tari/commit/3540309e29d450fc8cb48bc714fb780c1c107b81))
* add wallet daemon config ([#5311](https://github.com/tari-project/tari/issues/5311)) ([30419cf](https://github.com/tari-project/tari/commit/30419cfcf198fb923ef431316f2915cbc80f1e3b))
* define different network defaults for bins ([#5307](https://github.com/tari-project/tari/issues/5307)) ([2f5d498](https://github.com/tari-project/tari/commit/2f5d498d2130b5358fbf126c96a917ed98016955))
* feature gates ([#5287](https://github.com/tari-project/tari/issues/5287)) ([72c19dc](https://github.com/tari-project/tari/commit/72c19dc130b0c7652cca422c9c4c2e08e5b8e555))
* fix rpc transaction conversion ([#5304](https://github.com/tari-project/tari/issues/5304)) ([344040a](https://github.com/tari-project/tari/commit/344040ac7322bae5604aa9db48d4194c1b3779fa))

### Bug Fixes

* added transaction revalidation to the wallet startup sequence [#5227](https://github.com/tari-project/tari/issues/5227) ([#5246](https://github.com/tari-project/tari/issues/5246)) ([7b4e2d2](https://github.com/tari-project/tari/commit/7b4e2d2cd41c3173c9471ed987a43ae0978afd57))
* immediately fail to compile on 32-bit systems ([#5237](https://github.com/tari-project/tari/issues/5237)) ([76aeed7](https://github.com/tari-project/tari/commit/76aeed79ae0774bfb4cd94f9f27093394808bae1))
* **wallet:** correct change checks in transaction builder ([#5235](https://github.com/tari-project/tari/issues/5235)) ([768a0cf](https://github.com/tari-project/tari/commit/768a0cf310aaf20cc5697eaea32c824f812bc233))
* **wallet:** ensure burn shared keys and hashes match dan layer ([#5245](https://github.com/tari-project/tari/issues/5245)) ([024ce64](https://github.com/tari-project/tari/commit/024ce64843d282981efb366a3a1a5be36c0fb21d))
* windows path format in log4rs files ([#5234](https://github.com/tari-project/tari/issues/5234)) ([acfecfb](https://github.com/tari-project/tari/commit/acfecfb0b52868bdfbee9accb4d03b8a4a59d90b))
* ffi hot fix ([#5251](https://github.com/tari-project/tari/issues/5251)) ([9533e40](https://github.com/tari-project/tari/commit/9533e4017f1229f6de31966a9d5f19ea906117f3))
* reduce warn log to debug in utxo scanner ([#5256](https://github.com/tari-project/tari/issues/5256)) ([3946641](https://github.com/tari-project/tari/commit/394664177dcbd05fdd43d54b3bd9f77bc52ecd88))
* wallet sending local address out to network ([#5258](https://github.com/tari-project/tari/issues/5258)) ([6bfa6f9](https://github.com/tari-project/tari/commit/6bfa6f9fecdd594386ef07169d0e68777b3becd5))
* ensures mutable MMR bitmaps are compressed ([#5278](https://github.com/tari-project/tari/issues/5278)) ([dfddc66](https://github.com/tari-project/tari/commit/dfddc669e3e1271b098c8b271e13f076ca79b039))
* resize transaction tab windows ([#5290](https://github.com/tari-project/tari/issues/5290)) ([bd95a85](https://github.com/tari-project/tari/commit/bd95a853b2eb166a4aa8e32778ed72bb1f8172ad)), closes [#4942](https://github.com/tari-project/tari/issues/4942) [#5289](https://github.com/tari-project/tari/issues/5289) [#12365](https://github.com/tari-project/tari/issues/12365)

## [0.48.0-pre.1](https://github.com/tari-project/tari/compare/v0.48.0-pre.0...v0.48.0-pre.1) (2023-03-08)


### Bug Fixes

* **comms:** dial if connection is not connected ([#5223](https://github.com/tari-project/tari/issues/5223)) ([0a060b6](https://github.com/tari-project/tari/commit/0a060b6827247a5772d04dde477f0494019bad89))
* export error types for balance merkle tree ([#5229](https://github.com/tari-project/tari/issues/5229)) ([9db0501](https://github.com/tari-project/tari/commit/9db0501af3b464f430e889e21dc889ea736886ea))
* fix compile error using decimal-rs 0.1.42 ([#5228](https://github.com/tari-project/tari/issues/5228)) ([6edbb1c](https://github.com/tari-project/tari/commit/6edbb1c8745593e41dd24585c9f8d399a96fff51))

## [0.48.0-pre.0](https://github.com/tari-project/tari/compare/v0.47.0-pre.0...v0.48.0-pre.0) (2023-03-07)


### ⚠ BREAKING CHANGES

* **peer_db:** more accurate peer stats per address (#5142)
* use consensus hashing API for validator node MMR (#5207)
* **consensus:** add balanced binary merkle tree (#5189)

### Features

* add favourite flag to contact ([#5217](https://github.com/tari-project/tari/issues/5217)) ([0371b60](https://github.com/tari-project/tari/commit/0371b608dd7a59664e7c8e1494335709ad21943c))
* add indexer config ([#5210](https://github.com/tari-project/tari/issues/5210)) ([cf95601](https://github.com/tari-project/tari/commit/cf9560192de56ce1be22468b4551c5a60e5d9440))
* add merge proof for balanced binary merkle tree ([#5193](https://github.com/tari-project/tari/issues/5193)) ([8962909](https://github.com/tari-project/tari/commit/8962909127ded86249099bfdd384ac4e8b0db0ee))
* **consensus:** add balanced binary merkle tree ([#5189](https://github.com/tari-project/tari/issues/5189)) ([8d34e8a](https://github.com/tari-project/tari/commit/8d34e8a8eee2ed88ad0ab866a185d10a43300ec1))
* log to base dir ([#5197](https://github.com/tari-project/tari/issues/5197)) ([5147b5c](https://github.com/tari-project/tari/commit/5147b5c81082396dc80605e5a9422eec8b06c1b1))
* **peer_db:** more accurate peer stats per address ([#5142](https://github.com/tari-project/tari/issues/5142)) ([fdad1c6](https://github.com/tari-project/tari/commit/fdad1c6bf7914bbdc0ffc25ef729506196881c35))


### Bug Fixes

* add grpc commitment signature proto type ([#5200](https://github.com/tari-project/tari/issues/5200)) ([d523f1e](https://github.com/tari-project/tari/commit/d523f1e556d0f56c784923600fe48f93e2239520))
* peer seeds for esme/igor ([#5202](https://github.com/tari-project/tari/issues/5202)) ([1bc226c](https://github.com/tari-project/tari/commit/1bc226c85c0810c9ad01dfb6539d8b614cc71fb8))
* remove panics from merged BBMT verification ([#5221](https://github.com/tari-project/tari/issues/5221)) ([a4c5fce](https://github.com/tari-project/tari/commit/a4c5fce5e43153db090465f3623989ed07dfd627))
* source coverage ci failure ([#5209](https://github.com/tari-project/tari/issues/5209)) ([80294a1](https://github.com/tari-project/tari/commit/80294a1a931d248413166966eebb1e297249e506))
* use consensus hashing API for validator node MMR ([#5207](https://github.com/tari-project/tari/issues/5207)) ([de28115](https://github.com/tari-project/tari/commit/de281154ac339cd0e8b0eac59bcf933851dcc5c6))
* wallet reuse existing tor address ([#5092](https://github.com/tari-project/tari/issues/5092)) ([576f44e](https://github.com/tari-project/tari/commit/576f44e48d781e3a61be138549484c4b4a79773e))
* **wallet:** avoids empty addresses in node identity ([#5224](https://github.com/tari-project/tari/issues/5224)) ([1a66312](https://github.com/tari-project/tari/commit/1a66312d13dff7fd627930be88cfebffc4b08074))

## [0.47.0-pre.0](https://github.com/tari-project/tari/compare/v0.46.0...v0.47.0-pre.0) (2023-02-27)


### Features

* next net configuration ([#5204](https://github.com/tari-project/tari/issues/5204)) ([9f267fc](https://github.com/tari-project/tari/commit/9f267fcc4c34c84f4e713be5f20131170dc19664))


### Bug Fixes

* addresses mmr `find_peaks` bug ([#5182](https://github.com/tari-project/tari/issues/5182)) ([ee55e84](https://github.com/tari-project/tari/commit/ee55e843d0fd31b25163e118a3454ef666088c6c))

## [0.46.0](https://github.com/tari-project/tari/compare/v0.45.0...v0.46.0) (2023-02-21)


### ⚠ BREAKING CHANGES

* add key commitment to database main key AEAD  (#5188)

### Features

* add key commitment to database main key AEAD  ([#5188](https://github.com/tari-project/tari/issues/5188)) ([95bc795](https://github.com/tari-project/tari/commit/95bc7956811020957d4cf0a8eef742124d44bcde))
* add more burn details to burn command ([#5169](https://github.com/tari-project/tari/issues/5169)) ([e417e57](https://github.com/tari-project/tari/commit/e417e575beb23cd17a119984829ee7479d39c459))
* print out warning if wallet grpc connections fails ([#5195](https://github.com/tari-project/tari/issues/5195)) ([4e1cb38](https://github.com/tari-project/tari/commit/4e1cb38aeec5cbb61e39920e3d1871699107c06f))


### Bug Fixes

* add missing consensus constants to get_constants grpc ([#5183](https://github.com/tari-project/tari/issues/5183)) ([9900d5d](https://github.com/tari-project/tari/commit/9900d5db3eacf463b479ad242391c9a2e0a38db8))

## [0.45.0](https://github.com/tari-project/tari/compare/v0.44.1...v0.45.0) (2023-02-14)


### ⚠ BREAKING CHANGES

* refactor database encryption (#5154)
* update `Argon2` parameters (#5140)

### Features

* add `node {word} is in state {word}` ([33360cd](https://github.com/tari-project/tari/commit/33360cd1e9c8ad1dec1bd8193ca6cae1b79c81f4))
* add get tari address to wallet ([1b0ed0b](https://github.com/tari-project/tari/commit/1b0ed0b99f8f36d7f04215b0ef846fdb13c095e7))
* add graceful shutdown of base node ([c9797c5](https://github.com/tari-project/tari/commit/c9797c51e996fc043a6e4fd94ae1baebcd39d115))
* add kill signal to cucumber nodes ([4cb21dc](https://github.com/tari-project/tari/commit/4cb21dc9148a32fbefae0017e984c634388f1543))
* add shutdown clone ([ac956c9](https://github.com/tari-project/tari/commit/ac956c90d9ac3f78d7437ee24360c80204870341))
* consolidate stealth payment code ([#5171](https://github.com/tari-project/tari/issues/5171)) ([b7747a2](https://github.com/tari-project/tari/commit/b7747a29c7032278b3ed88e13823d6e4fe7de45e))
* fix miner ([7283eb2](https://github.com/tari-project/tari/commit/7283eb2c61e9e13313e256a1cc5ab191bb4f4b58))
* gracefully shutdown grpc server ([947faf6](https://github.com/tari-project/tari/commit/947faf6559e6c16acdfe342c11c8c1ee99752d36))
* refactor database encryption ([#5154](https://github.com/tari-project/tari/issues/5154)) ([41413fc](https://github.com/tari-project/tari/commit/41413fca3c66bf567777373d2b102c9d7ac0ea57))
* refactor key-related field operations to be atomic ([#5178](https://github.com/tari-project/tari/issues/5178)) ([1ad79c9](https://github.com/tari-project/tari/commit/1ad79c946b3c67a3724f87d15ce55f29966d1e8b))
* remove unused dependencies ([#5144](https://github.com/tari-project/tari/issues/5144)) ([a9d0f37](https://github.com/tari-project/tari/commit/a9d0f3711108ddb27599dc3e91834bb6cd02f821))
* stagenet network ([#5173](https://github.com/tari-project/tari/issues/5173)) ([d2717a1](https://github.com/tari-project/tari/commit/d2717a1147e714f3978aaffb1e5af46986974335))
* update `Argon2` parameters ([#5140](https://github.com/tari-project/tari/issues/5140)) ([4c4a056](https://github.com/tari-project/tari/commit/4c4a056f1f6623f6566b691a96c850ff905c0587))
* wallet FFI cucumber ([795e717](https://github.com/tari-project/tari/commit/795e7178020b41bbda0510563e0ac0c2448eb359))
* wallet password change ([#5175](https://github.com/tari-project/tari/issues/5175)) ([7f13fa5](https://github.com/tari-project/tari/commit/7f13fa5e64144c11b67201ab38bb55bdbb494680))


### Bug Fixes

* couple fixes for cucumber ([ad92e11](https://github.com/tari-project/tari/commit/ad92e1172682e602664ff512f9ce1495a566e473))
* **dht/test:** ban peers who send empty encrypted messages  ([#5130](https://github.com/tari-project/tari/issues/5130)) ([86a9eaf](https://github.com/tari-project/tari/commit/86a9eaf700323a2794d2b71797ebf811ba3679b5))
* do not propagate unsigned encrypted messages ([#5129](https://github.com/tari-project/tari/issues/5129)) ([d4fe7de](https://github.com/tari-project/tari/commit/d4fe7de1088aa986bf00d6ff4c31dd92659b4d95))
* feature flag separation for validation ([#5137](https://github.com/tari-project/tari/issues/5137)) ([0e83463](https://github.com/tari-project/tari/commit/0e83463718001ef14564068f2087fb6dc50b0fa3))
* panic on overflow in release mode ([#5150](https://github.com/tari-project/tari/issues/5150)) ([5f5808b](https://github.com/tari-project/tari/commit/5f5808b309cbf2416541652c7e2a4a923ef46e35))
* potential ban ([#5146](https://github.com/tari-project/tari/issues/5146)) ([9892da6](https://github.com/tari-project/tari/commit/9892da6345468b798b0b669f010322f343fd9f4f))
* **test:** broken address test ([#5134](https://github.com/tari-project/tari/issues/5134)) ([6b125af](https://github.com/tari-project/tari/commit/6b125af57570d48d5864158693f3ab935d23f6a9))
* **wallet-grpc:** return correct available balance and add timelocked_balance ([#5181](https://github.com/tari-project/tari/issues/5181)) ([e001125](https://github.com/tari-project/tari/commit/e0011254ddbf4556a8b0ac2576869615c6549ccc))

### [0.44.1](https://github.com/tari-project/tari/compare/v0.44.0...v0.44.1) (2023-01-19)

## [0.44.0](https://github.com/tari-project/tari/compare/v0.43.3...v0.44.0) (2023-01-18)


### ⚠ BREAKING CHANGES

* prune mode sync (#5124)

### Features

* add tx_id_to export ([#5126](https://github.com/tari-project/tari/issues/5126)) ([7eeeff4](https://github.com/tari-project/tari/commit/7eeeff4bbd5a147bd35e9ae7af75dba1da87383b))
* increase wallet FFI error codes ([#5118](https://github.com/tari-project/tari/issues/5118)) ([d5db596](https://github.com/tari-project/tari/commit/d5db596a2f4522427af7ff380b6e4974152d6ada))
* provide password feedback ([#5111](https://github.com/tari-project/tari/issues/5111)) ([a568e04](https://github.com/tari-project/tari/commit/a568e0464c5da047df316356edb856bff34de4f0))


### Bug Fixes

* add burnt utxos to side chain query ([#5125](https://github.com/tari-project/tari/issues/5125)) ([fb2fa4b](https://github.com/tari-project/tari/commit/fb2fa4b4c7b3a72360926c4d300cd0ce0056dc54))
* automatically set base node fetures on startup, sign only if necessary ([#5108](https://github.com/tari-project/tari/issues/5108)) ([9aa9436](https://github.com/tari-project/tari/commit/9aa9436e945f6db59b34ad9c29a973fdc6515eda))
* **dht:** check for empty body contents in initial msg validation ([#5123](https://github.com/tari-project/tari/issues/5123)) ([48bf2d9](https://github.com/tari-project/tari/commit/48bf2d9302dcc1c8c0953a4576d09dc07577cb3f))
* prune mode sync ([#5124](https://github.com/tari-project/tari/issues/5124)) ([8fa076a](https://github.com/tari-project/tari/commit/8fa076ad0ea5d9c4408b0e863e4f24cfa2a8258a))
* vanity_id example should create id with base node features ([#5107](https://github.com/tari-project/tari/issues/5107)) ([3b21199](https://github.com/tari-project/tari/commit/3b21199dcf4639a7ca5cff727bcb49927b624842))

### [0.43.3](https://github.com/tari-project/tari/compare/v0.43.2...v0.43.3) (2023-01-12)


### Features

* add new igor seeds ([#5106](https://github.com/tari-project/tari/issues/5106)) ([61d1b5e](https://github.com/tari-project/tari/commit/61d1b5e80039c4908b6c0207939c22af2fa3e939))
* add to/from json string for unblinded utxo in wallet ffi ([#5098](https://github.com/tari-project/tari/issues/5098)) ([af25b63](https://github.com/tari-project/tari/commit/af25b63d2909af3cdb025532784bdba118d9f876))


### Bug Fixes

* add const to FixedHash::zero ([#5084](https://github.com/tari-project/tari/issues/5084)) ([2d1bc82](https://github.com/tari-project/tari/commit/2d1bc823274e351b2b413a640bc71aa4d5d6c798))
* console wallet spacing and naming ([#5025](https://github.com/tari-project/tari/issues/5025)) ([e4a6303](https://github.com/tari-project/tari/commit/e4a63033febd01e5b0d4c6dfc9a0b387bb58a5b1))
* functional wallet encryption (issue [#5007](https://github.com/tari-project/tari/issues/5007)) ([#5043](https://github.com/tari-project/tari/issues/5043)) ([7b2311e](https://github.com/tari-project/tari/commit/7b2311e40e2619109dcb4572d9d86d3f4463324e))
* header sync start info ([#5086](https://github.com/tari-project/tari/issues/5086)) ([df53843](https://github.com/tari-project/tari/commit/df53843d4e129fbc1e551f0f1d3560bbc28aed86))
* header sync stuck trying to sync from base node  ([#5080](https://github.com/tari-project/tari/issues/5080)) ([0961f49](https://github.com/tari-project/tari/commit/0961f497ebd9e8478313b88738a2c5bde4608eb3))
* improved encryption key handling ([#5027](https://github.com/tari-project/tari/issues/5027)) ([b2bed79](https://github.com/tari-project/tari/commit/b2bed79a744592b99c0f01a957750f12f787072e))
* update message and signature key types  ([#5064](https://github.com/tari-project/tari/issues/5064)) ([a94189d](https://github.com/tari-project/tari/commit/a94189d3f5500ddc3222aada0bc30c014f2b7e7a))
* use range proof batch splitting  ([#5081](https://github.com/tari-project/tari/issues/5081)) ([70c522b](https://github.com/tari-project/tari/commit/70c522b400d9406855a0b65d78c09e916ccfa274))
* wallet errors ([#5045](https://github.com/tari-project/tari/issues/5045)) ([9b16ffb](https://github.com/tari-project/tari/commit/9b16ffb9925d07f3adeab1f1fd6f4163e493a3c7))

### [0.43.2](https://github.com/tari-project/tari/compare/v0.43.1...v0.43.2) (2022-12-19)


### Features

* add burn-tari to make-it-rain ([#5038](https://github.com/tari-project/tari/issues/5038)) ([62dfd38](https://github.com/tari-project/tari/commit/62dfd383b3bfae40440a60ec0b11a2d71bafd691))


### Bug Fixes

* **ci:** pin ci to ubuntu 20.04 ([#5047](https://github.com/tari-project/tari/issues/5047)) ([1cafba7](https://github.com/tari-project/tari/commit/1cafba73950349b19181f612633040804fe1800d))
* **core:** fix build issues and add ban check listening ([#5041](https://github.com/tari-project/tari/issues/5041)) ([774ab7a](https://github.com/tari-project/tari/commit/774ab7ad4e6033df315efe900b6f3f2b91f059ea))
* **core:** fixes stale chain metadata being sent to listening state ([#5039](https://github.com/tari-project/tari/issues/5039)) ([aaf99b7](https://github.com/tari-project/tari/commit/aaf99b7183b0977076684bd1128ab32322efaa79))
* support arbitrary range proof batching ([#5049](https://github.com/tari-project/tari/issues/5049)) ([3dd10bd](https://github.com/tari-project/tari/commit/3dd10bdfda21d241b53c3cf2bf9c1b96c373cdeb))

### [0.43.1](https://github.com/tari-project/tari/compare/v0.43.0...v0.43.1) (2022-12-12)


### Bug Fixes

* **ci:** workaround - lock linux-x86_64 to ubuntu 20.04 ([#5032](https://github.com/tari-project/tari/issues/5032)) ([dc2cd82](https://github.com/tari-project/tari/commit/dc2cd82c3f1d03a8922f56fde9bfef3c289297c1))

## [0.43.0](https://github.com/tari-project/tari/compare/v0.42.0...v0.43.0) (2022-12-08)


### ⚠ BREAKING CHANGES

* input and output signatures (#4983)

### Features

* add utxo import and export for ffi ([#4999](https://github.com/tari-project/tari/issues/4999)) ([9cda0bb](https://github.com/tari-project/tari/commit/9cda0bb985c995958c6b33bbb41690776129e2ac))
* improve logging of dropped channel ([#5013](https://github.com/tari-project/tari/issues/5013)) ([4650153](https://github.com/tari-project/tari/commit/46501539e2e3cc377a2dc46a8cfed02f24790bcc))
* remove duplicate errors  ([#5009](https://github.com/tari-project/tari/issues/5009)) ([0c9477b](https://github.com/tari-project/tari/commit/0c9477b51f2e50a8323c498e27af440d21b4ea16))
* revalidate invalid utxo ([#5020](https://github.com/tari-project/tari/issues/5020)) ([f418d73](https://github.com/tari-project/tari/commit/f418d73eefbdca10c3adcea8c597df627368cef6))


### Bug Fixes

* **base_layer/core:** fixes incorrect validator node merkle root calculation ([#5005](https://github.com/tari-project/tari/issues/5005)) ([951c0d6](https://github.com/tari-project/tari/commit/951c0d6b247da782082c5f98ad9c43de4110cc66))
* **ci:** wallet ffi build fix ([#4993](https://github.com/tari-project/tari/issues/4993)) ([5145368](https://github.com/tari-project/tari/commit/5145368a66643dba88742252099b1dcf1932928e))
* coinbase extra info is not checked ([#4995](https://github.com/tari-project/tari/issues/4995)) ([af95b45](https://github.com/tari-project/tari/commit/af95b45935c2ea0ac0f66ccbeaeee6a40d408fca))
* currently newly created wallet does not prompt seed words ([#5019](https://github.com/tari-project/tari/issues/5019)) ([#5022](https://github.com/tari-project/tari/issues/5022)) ([96cf8aa](https://github.com/tari-project/tari/commit/96cf8aaaa4cc3a0b209baa49763bbe18893ab2ae))
* improve key handling  ([#4994](https://github.com/tari-project/tari/issues/4994)) ([f069b14](https://github.com/tari-project/tari/commit/f069b14527d269562976413b3c56d0f3725fa707))
* input and output signatures ([#4983](https://github.com/tari-project/tari/issues/4983)) ([a0f1d95](https://github.com/tari-project/tari/commit/a0f1d9588bef9d872f747a59eadffcad7d2effc9))
* remove optionals from wallet set up ([#4984](https://github.com/tari-project/tari/issues/4984)) ([33e6dbf](https://github.com/tari-project/tari/commit/33e6dbfdf3f08b0e5e299396b0beb4fdc0b8970d))
* seed words should be used in wallet recovery (see issue [#4894](https://github.com/tari-project/tari/issues/4894)) ([#5010](https://github.com/tari-project/tari/issues/5010)) ([68a9f76](https://github.com/tari-project/tari/commit/68a9f7617c1b0a7d8ddc8eaaf5d3980f1e71ea2c))
* show mined timestamp for import tx ([#5012](https://github.com/tari-project/tari/issues/5012)) ([49a11d9](https://github.com/tari-project/tari/commit/49a11d9c2c5aff2ab8a17f5671523b9540739ab5))
* update randomx-rs dependency  ([#5011](https://github.com/tari-project/tari/issues/5011)) ([5361dd9](https://github.com/tari-project/tari/commit/5361dd902070403c283b8e6a4f89fe9fff235b53))

## [0.42.0](https://github.com/tari-project/tari/compare/v0.41.0...v0.42.0) (2022-12-02)


### ⚠ BREAKING CHANGES

* **core:** sort validate set by shard key (#4952)
* implement validator node registration as per RFC-0313 (#4928)

### Features

* change log level ffi comms ([#4973](https://github.com/tari-project/tari/issues/4973)) ([554e783](https://github.com/tari-project/tari/commit/554e783100c16e3b740b22e0b2a75c8760a51a06))
* implement validator node registration as per RFC-0313 ([#4928](https://github.com/tari-project/tari/issues/4928)) ([8569f7c](https://github.com/tari-project/tari/commit/8569f7c7108bc700d016239a5272e09ed3d0f593)), closes [#4927](https://github.com/tari-project/tari/issues/4927)
* log app version on startup ([#4970](https://github.com/tari-project/tari/issues/4970)) ([2962028](https://github.com/tari-project/tari/commit/29620287f4ccea6f5ca7ca0b2b71e14ba21b4a4d))
* relax zeroize  ([#4961](https://github.com/tari-project/tari/issues/4961)) ([a6e8991](https://github.com/tari-project/tari/commit/a6e899159db5138ec03b97367d5f8873530b5a22))
* relax zeroize dependencies ([#4971](https://github.com/tari-project/tari/issues/4971)) ([10a19d5](https://github.com/tari-project/tari/commit/10a19d5e790ea7041c8c89e47aa144d3bb14c91a))
* remove extra validation ([#4981](https://github.com/tari-project/tari/issues/4981)) ([3f1ebf6](https://github.com/tari-project/tari/commit/3f1ebf611b62d46148e7933fda7e497514012591))
* reset broken sync ([#4955](https://github.com/tari-project/tari/issues/4955)) ([01e9e7e](https://github.com/tari-project/tari/commit/01e9e7ef10e5392a55a50b82dadb3e3e0c0da529))
* trigger validation on import ([#4962](https://github.com/tari-project/tari/issues/4962)) ([163dce0](https://github.com/tari-project/tari/commit/163dce02ca7d8842f4198b2513f6bdcbb0e0c729))


### Bug Fixes

* **ci:** update libtari_wallet_ffi sha256sums ([#4968](https://github.com/tari-project/tari/issues/4968)) ([5de63d3](https://github.com/tari-project/tari/commit/5de63d35b923a0e78a4da3bdc56ad1b250b4fb47))
* console wallet timestamp display ([#4942](https://github.com/tari-project/tari/issues/4942)) ([baa196f](https://github.com/tari-project/tari/commit/baa196fa5429e488a068ad5036d7ea19873fc3ca))
* **core:** sort validate set by shard key ([#4952](https://github.com/tari-project/tari/issues/4952)) ([349d429](https://github.com/tari-project/tari/commit/349d4292c4fffd102ad83b3fcb49ff208b0d7536))
* hide sensitive data on tari repo (see issue [#4846](https://github.com/tari-project/tari/issues/4846)) ([#4967](https://github.com/tari-project/tari/issues/4967)) ([bcc47e1](https://github.com/tari-project/tari/commit/bcc47e1370d0ca5b61604e2922f899f80b71a72f))
* minimize potential memory leaks of sensitive data on the wallet code ([#4953](https://github.com/tari-project/tari/issues/4953)) ([e364994](https://github.com/tari-project/tari/commit/e364994d30cb5e71b9dd87b485197d023d3121e0))
* node gets banned on reorg ([#4949](https://github.com/tari-project/tari/issues/4949)) ([5bcf6e5](https://github.com/tari-project/tari/commit/5bcf6e5453d451063a1776fa38b4f14aaf07ac88))
* **wallet:** fix wallet_setting keys ([#4976](https://github.com/tari-project/tari/issues/4976)) ([f2cbe6f](https://github.com/tari-project/tari/commit/f2cbe6f75d6cebad441fbf92270213b49349ed1f))
* **wallet:** invalid metadata sig when creating code template utxo ([#4975](https://github.com/tari-project/tari/issues/4975)) ([a8e2e00](https://github.com/tari-project/tari/commit/a8e2e00c09673b0a692f831e20fefd8652ce3572))
* **wallet:** slightly improve error output for failed decryption ([#4972](https://github.com/tari-project/tari/issues/4972)) ([b2370b1](https://github.com/tari-project/tari/commit/b2370b18e86a2e8cc9acf61ed4db22c0148710fb))

## [0.41.0](https://github.com/tari-project/tari/compare/v0.40.2...v0.41.0) (2022-11-25)


### ⚠ BREAKING CHANGES

* update commitment signature  (#4943)

### Features

* add default grpc for localnet ([#4937](https://github.com/tari-project/tari/issues/4937)) ([1e2d227](https://github.com/tari-project/tari/commit/1e2d2274626e368011b58e8c15aa3bb6294f4982))
* **ci:** expose iOS libwallet individually ([#4951](https://github.com/tari-project/tari/issues/4951)) ([e69997c](https://github.com/tari-project/tari/commit/e69997cc27f106e6f89e662218dd72f47ba5a0c8))
* only coinbase output features may have metadata set, and is of limited size; ref [#4908](https://github.com/tari-project/tari/issues/4908) ([#4960](https://github.com/tari-project/tari/issues/4960)) ([22b1330](https://github.com/tari-project/tari/commit/22b13307991698e284d2186ad06db663aedcb3d9))
* replace consensus with borsh ([#4920](https://github.com/tari-project/tari/issues/4920)) ([e669443](https://github.com/tari-project/tari/commit/e669443c9a6ca48a03ccd5d0fff2a1a917901ab9))
* timestamp validation ([#4887](https://github.com/tari-project/tari/issues/4887)) ([4be02b6](https://github.com/tari-project/tari/commit/4be02b66ff2b5eb82f8f061d379d10e7414dc84e))
* update commitment signature  ([#4943](https://github.com/tari-project/tari/issues/4943)) ([00e98f9](https://github.com/tari-project/tari/commit/00e98f9edede034c9135fcc7a87a01a38bdf01b4))


### Bug Fixes

* add hidden types and seed words to key manager ([#4925](https://github.com/tari-project/tari/issues/4925)) ([0bdb568](https://github.com/tari-project/tari/commit/0bdb568fb33643665a151d81db847cf82989a7fe))
* **ci:** update GHA release process ([#4945](https://github.com/tari-project/tari/issues/4945)) ([2af6c94](https://github.com/tari-project/tari/commit/2af6c94417a7cdaab35b91d03f6fba5dbb45961f))
* config cleanup ([#4938](https://github.com/tari-project/tari/issues/4938)) ([68f990f](https://github.com/tari-project/tari/commit/68f990fb568293992bb1bad596f9616b1a34610a))
* deleted_txo_mmr_position_to_height_index  already exists error ([#4924](https://github.com/tari-project/tari/issues/4924)) ([0269f11](https://github.com/tari-project/tari/commit/0269f1105a5210be41a1da50784a2bc7d9f12069))
* remove unused ffi types and methods ([#4948](https://github.com/tari-project/tari/issues/4948)) ([5703d02](https://github.com/tari-project/tari/commit/5703d02d419b1a5f49f4523707f7f689c01eb1b5))
* use same instance of randomx factory for statemachine and validation ([#4947](https://github.com/tari-project/tari/issues/4947)) ([9aed188](https://github.com/tari-project/tari/commit/9aed188ccd7b2caffa10e061ba0d0c3253fb0b16))

### [0.40.2](https://github.com/tari-project/tari/compare/v0.40.1...v0.40.2) (2022-11-18)


### Features

* upgrade tari_crypto sign api ([#4932](https://github.com/tari-project/tari/issues/4932)) ([e2b7ad1](https://github.com/tari-project/tari/commit/e2b7ad186e8ce311576549e25e3ae10770ba0c6b))


### Bug Fixes

* **dht:** use limited ban period for invalid peer ([#4933](https://github.com/tari-project/tari/issues/4933)) ([04a3a8f](https://github.com/tari-project/tari/commit/04a3a8fbb8932b06293abb1fe59c597f1bf3a2a3))

### [0.40.1](https://github.com/tari-project/tari/compare/v0.40.0...v0.40.1) (2022-11-17)


### Bug Fixes

* set wallet start scan height to birthday and not 0 (see issue [#4807](https://github.com/tari-project/tari/issues/4807)) ([#4911](https://github.com/tari-project/tari/issues/4911)) ([797f91a](https://github.com/tari-project/tari/commit/797f91a91578e851b9eefe939294f919c7fec978))

## [0.40.0](https://github.com/tari-project/tari/compare/v0.39.0...v0.40.0) (2022-11-16)


### ⚠ BREAKING CHANGES

* add tari address for wallet to use (#4881)

### Features

* add tari address for wallet to use ([#4881](https://github.com/tari-project/tari/issues/4881)) ([26aacc7](https://github.com/tari-project/tari/commit/26aacc7411866e920d5aa0fa62f5b8ae9e143946))


### Bug Fixes

* **comms:** spawn liveness check after address is final ([#4919](https://github.com/tari-project/tari/issues/4919)) ([f558a11](https://github.com/tari-project/tari/commit/f558a11222a322bac93b8a51b7240442f4a9e9c9))
* remove fs2 dependency from tari_common ([#4921](https://github.com/tari-project/tari/issues/4921)) ([dca7b06](https://github.com/tari-project/tari/commit/dca7b0614c6c27a13417e6108207e9605557551e))
* updates for SafePassword API change ([#4927](https://github.com/tari-project/tari/issues/4927)) ([92d73e4](https://github.com/tari-project/tari/commit/92d73e458319a0bd3d897ebc795e52f0597392b7))

## [0.39.0](https://github.com/tari-project/tari/compare/v0.38.8...v0.39.0) (2022-11-14)


### ⚠ BREAKING CHANGES

* merges feature-dan into development (#4913)
* **wallet:** use KDFs on ECDH shared secrets (#4847)
* **core:** remove unused get_committees call from base node (#4880)
* refactor `CipherSeed`, zeroize, and fix key derivation (#4860)
* impl final tari pow algorithm (#4862)
* **core:** adds utxo and block info to get_template_registrations request (#4789)

### Features

* add block height to input request to get network consensus constants ([#4856](https://github.com/tari-project/tari/issues/4856)) ([23b4313](https://github.com/tari-project/tari/commit/23b43131102fbca030f825c7c8df7ec9f698932f))
* add grpc to get shard key for public key ([#4654](https://github.com/tari-project/tari/issues/4654)) ([0fd3256](https://github.com/tari-project/tari/commit/0fd32569c9bb321fc866681301bbb759888d83ae))
* add missing fields to grpc consensus constants interface ([#4845](https://github.com/tari-project/tari/issues/4845)) ([ce6c22f](https://github.com/tari-project/tari/commit/ce6c22f9eb02a7932afc5b71fd73e34da03791ff))
* add static lifetime to emission amounts calculation ([#4851](https://github.com/tari-project/tari/issues/4851)) ([5b0eb04](https://github.com/tari-project/tari/commit/5b0eb0459c7d29a25339c22a289153d27d57388e))
* add validator node registration ([#4507](https://github.com/tari-project/tari/issues/4507)) ([96a30c1](https://github.com/tari-project/tari/commit/96a30c1662a88e10059da17d114148fe06bf9c43))
* **base_node_grpc_client:** add getActiveValidatorNodes method ([#4719](https://github.com/tari-project/tari/issues/4719)) ([cfa05be](https://github.com/tari-project/tari/commit/cfa05beca87d3ac4687e1794c7d6b6aded5b0671))
* **core:** add template registration sidechain features ([#4470](https://github.com/tari-project/tari/issues/4470)) ([8ee5a05](https://github.com/tari-project/tari/commit/8ee5a05da3bc1de49ac65a6674c60381f72af21f))
* **core:** add validator registration sidechain feature ([#4690](https://github.com/tari-project/tari/issues/4690)) ([0fef174](https://github.com/tari-project/tari/commit/0fef17463faf67ea3a427d4f4a43b1e690acfab7))
* **core:** store and fetch templates from lmdb ([#4726](https://github.com/tari-project/tari/issues/4726)) ([27f77b2](https://github.com/tari-project/tari/commit/27f77b27e67f748631664f7cc94e34065fe48b7c))
* impl final tari pow algorithm ([#4862](https://github.com/tari-project/tari/issues/4862)) ([a580103](https://github.com/tari-project/tari/commit/a58010370afe984d969fd7e54ac7417302e93906)), closes [#4875](https://github.com/tari-project/tari/issues/4875)
* mempool sync wait for node initial sync ([#4897](https://github.com/tari-project/tari/issues/4897)) ([5526721](https://github.com/tari-project/tari/commit/55267216983c110b8bc3b6d59f137f5191bdea92))
* merges feature-dan into development ([#4913](https://github.com/tari-project/tari/issues/4913)) ([539e758](https://github.com/tari-project/tari/commit/539e758245e2a33bf67ac53a1b205202b5ac7dfc))
* remove tracing_subscriber ([#4906](https://github.com/tari-project/tari/issues/4906)) ([956b279](https://github.com/tari-project/tari/commit/956b27954dda1f15f82bff0ba0ba0ee1f0880d2d))


### Bug Fixes

* **base-node:** use less harsh emoji for unreachable node ([#4855](https://github.com/tari-project/tari/issues/4855)) ([2d90e91](https://github.com/tari-project/tari/commit/2d90e91a198d62c887e721e4a60814f21b7bc686))
* **ci:** correct ARM64 builds ([#4876](https://github.com/tari-project/tari/issues/4876)) ([7628692](https://github.com/tari-project/tari/commit/7628692a59e7abf9978fb928d96744ce05421d72))
* **ci:** selectively revert resolver for arm64 builds ([#4871](https://github.com/tari-project/tari/issues/4871)) ([cd88484](https://github.com/tari-project/tari/commit/cd88484d8ef6ac864210ea8e2a5f31a02e86fd7b))
* **ci:** update GHA set-output plus dependabot schedule for GHA ([#4857](https://github.com/tari-project/tari/issues/4857)) ([f978507](https://github.com/tari-project/tari/commit/f978507e795b571add178ec461da4b10864c374c))
* **comms/peer_manager:** fix possible panic in offline calc ([#4877](https://github.com/tari-project/tari/issues/4877)) ([c0d1f58](https://github.com/tari-project/tari/commit/c0d1f585318e8200f155680227712aa22b373fcf))
* computation of vn mmr ([#4772](https://github.com/tari-project/tari/issues/4772)) ([64002e9](https://github.com/tari-project/tari/commit/64002e9c442f7a3b69343d580254e4e93ad69dd4))
* **core/metrics:** set target difficulty as single value ([#4902](https://github.com/tari-project/tari/issues/4902)) ([f625f73](https://github.com/tari-project/tari/commit/f625f7358ff5d4d0b51e77ad8b4e6cf2d0171e6b))
* **core:** add txo version checks to async validator ([#4852](https://github.com/tari-project/tari/issues/4852)) ([2cf51b8](https://github.com/tari-project/tari/commit/2cf51b855a5600653b96b1c0317c54d38fa7c55b))
* **core:** adds utxo and block info to get_template_registrations request ([#4789](https://github.com/tari-project/tari/issues/4789)) ([9e81c7b](https://github.com/tari-project/tari/commit/9e81c7b6257773ddca970982adb89a1e0d548e2b))
* **core:** bring validator node MR inline with other merkle root code ([#4692](https://github.com/tari-project/tari/issues/4692)) ([613b655](https://github.com/tari-project/tari/commit/613b65571540814afee49cdbfee834e5995dc85b))
* **core:** remove unused get_committees call from base node ([#4880](https://github.com/tari-project/tari/issues/4880)) ([392d541](https://github.com/tari-project/tari/commit/392d541285e0766ffaea872063a21f8968715b7c))
* correct value for validator_node_timeout consensus constant in localnet ([#4879](https://github.com/tari-project/tari/issues/4879)) ([bd49bf2](https://github.com/tari-project/tari/commit/bd49bf2dff921d05dc7ed969464d4b8eea0cb2ec))
* delete orphans if they exist ([#4868](https://github.com/tari-project/tari/issues/4868)) ([6ff1c02](https://github.com/tari-project/tari/commit/6ff1c02d3451d856a7c0c979109aaae99dc38ca1))
* **dht:** use new DHKE shared secret type ([#4844](https://github.com/tari-project/tari/issues/4844)) ([234571d](https://github.com/tari-project/tari/commit/234571dc5241bd6122525b02706ca68aae300308))
* fix get shard key ([#4744](https://github.com/tari-project/tari/issues/4744)) ([3a4dd50](https://github.com/tari-project/tari/commit/3a4dd5096559dc7eea2d5d5c90bc64083b766c1a))
* fix validator node registration logic ([#4718](https://github.com/tari-project/tari/issues/4718)) ([72018f4](https://github.com/tari-project/tari/commit/72018f4834b8ee8fe1228c25a6be33189bdd2a3c))
* force wallet sqlite to do checkpoint after db decryption ([#4905](https://github.com/tari-project/tari/issues/4905)) ([55d1334](https://github.com/tari-project/tari/commit/55d133494270fe92ab7cf48d58f18d2a2bdecd17))
* recover mined coinbase ([#4896](https://github.com/tari-project/tari/issues/4896)) ([2028136](https://github.com/tari-project/tari/commit/20281361c58fe3d70acbb654a6cb7e66e3f34e19))
* refactor `CipherSeed`, zeroize, and fix key derivation ([#4860](https://github.com/tari-project/tari/issues/4860)) ([b190c26](https://github.com/tari-project/tari/commit/b190c267222dd883c8f281e09056ee566c8f4684))
* remove tari script serialization fix migration ([#4874](https://github.com/tari-project/tari/issues/4874)) ([44ed0c8](https://github.com/tari-project/tari/commit/44ed0c89e6f37ac08776d5e0e2d30778ac69c5cb))
* remove unused config for validator node ([#4849](https://github.com/tari-project/tari/issues/4849)) ([df5d78e](https://github.com/tari-project/tari/commit/df5d78eff10227834313ca2a90ade0c73e8c08e3))
* **wallet/grpc:** add transaction id and template_address to template_reg response ([#4788](https://github.com/tari-project/tari/issues/4788)) ([4060935](https://github.com/tari-project/tari/commit/4060935ded9c4192c58f5a8ee0b7443ff285f1b1))
* **wallet:** use KDFs on ECDH shared secrets ([#4847](https://github.com/tari-project/tari/issues/4847)) ([3d1a51c](https://github.com/tari-project/tari/commit/3d1a51cb0907ce99a59f42a75abe706169e131d1))

### [0.38.8](https://github.com/tari-project/tari/compare/v0.38.7...v0.38.8) (2022-10-25)


### Features

* add deepsource config ([dceea99](https://github.com/tari-project/tari/commit/dceea99968c803cc8c638df376c4d5cf6966ada9))
* add more detailed error mapping for the ffi ([#4840](https://github.com/tari-project/tari/issues/4840)) ([b27391e](https://github.com/tari-project/tari/commit/b27391ead238f36dd7042a1a6cfde231f7ac8d41))
* add multisig script that returns aggregate of signed public keys ([#4742](https://github.com/tari-project/tari/issues/4742)) ([c004e30](https://github.com/tari-project/tari/commit/c004e30925049865bc84fa5c3ce4cd06b2765882))
* add opcode versions ([#4836](https://github.com/tari-project/tari/issues/4836)) ([c8abe99](https://github.com/tari-project/tari/commit/c8abe998454d9f0ddbc3cfa627979c7d24b8d5ec))
* better FFI feedback from transaction validation ([#4827](https://github.com/tari-project/tari/issues/4827)) ([3c97be4](https://github.com/tari-project/tari/commit/3c97be46ac3bfcde378a38e03c7cfa8cc2436298))
* **comms:** adds periodic socket-level liveness checks ([#4819](https://github.com/tari-project/tari/issues/4819)) ([2bea05f](https://github.com/tari-project/tari/commit/2bea05f89c52edf39d849a4dc9e917d13381e51c))


### Bug Fixes

* **base-node:** use Network::from_str to parse network in cli ([#4838](https://github.com/tari-project/tari/issues/4838)) ([47d279e](https://github.com/tari-project/tari/commit/47d279ed506c815f3db30c4b63c2b7ed7e9283dc))
* **comms/rpc:** measures client-side latency to first message received ([#4817](https://github.com/tari-project/tari/issues/4817)) ([02b8660](https://github.com/tari-project/tari/commit/02b8660f3293abdbef11a27123916044c9682f82))
* **core:** dont request full non-tip block if block is empty ([#4802](https://github.com/tari-project/tari/issues/4802)) ([becff0f](https://github.com/tari-project/tari/commit/becff0fe94714d70bf9bd5f1f214343d24e61cfe))
* **core:** increase sync timeouts ([#4800](https://github.com/tari-project/tari/issues/4800)) ([87dfab5](https://github.com/tari-project/tari/commit/87dfab518402309c1f5eea35027b552afcba06ff))
* **core:** periodically commit large transaction in prune_to_height ([#4805](https://github.com/tari-project/tari/issues/4805)) ([700a007](https://github.com/tari-project/tari/commit/700a0077731f2955cb7686fc72b808f26553c39f))
* **dht:** fix over allocation for encrypted messages ([#4832](https://github.com/tari-project/tari/issues/4832)) ([d29a64c](https://github.com/tari-project/tari/commit/d29a64c975951cd362fb98027870282f244bf218))
* **dht:** zeroize AEAD keys on drop ([#4843](https://github.com/tari-project/tari/issues/4843)) ([9957222](https://github.com/tari-project/tari/commit/9957222e452fe09936550dbe1e4cbc4abbfc4365))
* list-connections ([#4841](https://github.com/tari-project/tari/issues/4841)) ([23b2c9a](https://github.com/tari-project/tari/commit/23b2c9a529f6ce7523de23dd46c5a1ff911abec8))
* remove clear_on_drop dependency ([#4848](https://github.com/tari-project/tari/issues/4848)) ([9edbbce](https://github.com/tari-project/tari/commit/9edbbce9c78c91ba74b4dd74c176c849ed11ee4e))

### [0.38.7](https://github.com/tari-project/tari/compare/v0.38.6...v0.38.7) (2022-10-11)


### Bug Fixes

* **core:** only resize db if migration is required ([#4792](https://github.com/tari-project/tari/issues/4792)) ([4811a57](https://github.com/tari-project/tari/commit/4811a5772665af4e3b9007ccadedfc651e1d232e))
* **miner:** clippy error ([#4793](https://github.com/tari-project/tari/issues/4793)) ([734db22](https://github.com/tari-project/tari/commit/734db22bbdd36b5371aa9c70f4342bb0d3c2f3a4))

### [0.38.6](https://github.com/tari-project/tari/compare/v0.38.5...v0.38.6) (2022-10-11)


### Features

* **base-node:** add client connection count to status line ([#4774](https://github.com/tari-project/tari/issues/4774)) ([8339b1d](https://github.com/tari-project/tari/commit/8339b1de1bace96671d8eba0cf309adb9f78014a))
* move nonce to first in sha hash ([#4778](https://github.com/tari-project/tari/issues/4778)) ([054a314](https://github.com/tari-project/tari/commit/054a314f015ab7a3f1e571f3ee0c7a58ad0ebb5a))
* remove dalek ng ([#4769](https://github.com/tari-project/tari/issues/4769)) ([953b0b7](https://github.com/tari-project/tari/commit/953b0b7cfc371467e7d15e933e79c8d07712f666))


### Bug Fixes

* batch rewind operations ([#4752](https://github.com/tari-project/tari/issues/4752)) ([79d3c47](https://github.com/tari-project/tari/commit/79d3c47a86bc37be0117b33c869f9e04df068384))
* **ci:** fix client path for nodejs  ([#4765](https://github.com/tari-project/tari/issues/4765)) ([c7b5e68](https://github.com/tari-project/tari/commit/c7b5e68b400c79040f2dd92ee1cc779224e463ee))
* **core:** only resize db if migration is required ([#4792](https://github.com/tari-project/tari/issues/4792)) ([4811a57](https://github.com/tari-project/tari/commit/4811a5772665af4e3b9007ccadedfc651e1d232e))
* **dht:** remove some invalid saf failure cases ([#4787](https://github.com/tari-project/tari/issues/4787)) ([86b4d94](https://github.com/tari-project/tari/commit/86b4d9437f87cb31ed922ff7a7dc73e7fe29eb69))
* fix config.toml bug ([#4780](https://github.com/tari-project/tari/issues/4780)) ([f6043c1](https://github.com/tari-project/tari/commit/f6043c1f03f33a34e2612516ffca8a589e319001))
* **miner:** clippy error ([#4793](https://github.com/tari-project/tari/issues/4793)) ([734db22](https://github.com/tari-project/tari/commit/734db22bbdd36b5371aa9c70f4342bb0d3c2f3a4))
* **p2p/liveness:** remove fallible unwrap ([#4784](https://github.com/tari-project/tari/issues/4784)) ([e59be99](https://github.com/tari-project/tari/commit/e59be99401fc4b50f1b4f5a6a16948959e5c56a1))
* **tari-script:** use tari script encoding for execution stack serde de/serialization ([#4791](https://github.com/tari-project/tari/issues/4791)) ([c62f7eb](https://github.com/tari-project/tari/commit/c62f7eb6c5b6b4336c7351bd89cb3a700fde1bb2))

### [0.38.6](https://github.com/tari-project/tari/compare/v0.38.5...v0.38.6) (2022-10-11)


### Features

* **base-node:** add client connection count to status line ([#4774](https://github.com/tari-project/tari/issues/4774)) ([8339b1d](https://github.com/tari-project/tari/commit/8339b1de1bace96671d8eba0cf309adb9f78014a))
* move nonce to first in sha hash ([#4778](https://github.com/tari-project/tari/issues/4778)) ([054a314](https://github.com/tari-project/tari/commit/054a314f015ab7a3f1e571f3ee0c7a58ad0ebb5a))
* remove dalek ng ([#4769](https://github.com/tari-project/tari/issues/4769)) ([953b0b7](https://github.com/tari-project/tari/commit/953b0b7cfc371467e7d15e933e79c8d07712f666))


### Bug Fixes

* batch rewind operations ([#4752](https://github.com/tari-project/tari/issues/4752)) ([79d3c47](https://github.com/tari-project/tari/commit/79d3c47a86bc37be0117b33c869f9e04df068384))
* **ci:** fix client path for nodejs  ([#4765](https://github.com/tari-project/tari/issues/4765)) ([c7b5e68](https://github.com/tari-project/tari/commit/c7b5e68b400c79040f2dd92ee1cc779224e463ee))
* **dht:** remove some invalid saf failure cases ([#4787](https://github.com/tari-project/tari/issues/4787)) ([86b4d94](https://github.com/tari-project/tari/commit/86b4d9437f87cb31ed922ff7a7dc73e7fe29eb69))
* fix config.toml bug ([#4780](https://github.com/tari-project/tari/issues/4780)) ([f6043c1](https://github.com/tari-project/tari/commit/f6043c1f03f33a34e2612516ffca8a589e319001))
* **p2p/liveness:** remove fallible unwrap ([#4784](https://github.com/tari-project/tari/issues/4784)) ([e59be99](https://github.com/tari-project/tari/commit/e59be99401fc4b50f1b4f5a6a16948959e5c56a1))
* **tari-script:** use tari script encoding for execution stack serde de/serialization ([#4791](https://github.com/tari-project/tari/issues/4791)) ([c62f7eb](https://github.com/tari-project/tari/commit/c62f7eb6c5b6b4336c7351bd89cb3a700fde1bb2))

### [0.38.5](https://github.com/tari-project/tari/compare/v0.38.4...v0.38.5) (2022-10-03)


### Features

* add sql transactions to encumbering queries ([#4716](https://github.com/tari-project/tari/issues/4716)) ([a25d216](https://github.com/tari-project/tari/commit/a25d21678e9863bf1d708ca425e9ca0951cda782))
* change priority in mempool to take into account age ([#4737](https://github.com/tari-project/tari/issues/4737)) ([0dad9e8](https://github.com/tari-project/tari/commit/0dad9e805d83a6647bb3bc159869852e58de32c6))
* **clients:** add base node and wallet client crates ([#4722](https://github.com/tari-project/tari/issues/4722)) ([9d06408](https://github.com/tari-project/tari/commit/9d064080bd01a104cda3fae6204f0acd8b56a426))
* **core/sync:** add sync error status ([#4705](https://github.com/tari-project/tari/issues/4705)) ([6178548](https://github.com/tari-project/tari/commit/6178548b89084ea6a2a39dfe0df45bbf1b4c48d3))
* **core/sync:** adds `connecting` sync status ([#4698](https://github.com/tari-project/tari/issues/4698)) ([abde8e8](https://github.com/tari-project/tari/commit/abde8e8706ddb62341647d9e8648acf039ea3f69))
* different default grpc ports for different networks ([#4755](https://github.com/tari-project/tari/issues/4755)) ([933126e](https://github.com/tari-project/tari/commit/933126eb6f99e3842d68809edd1f907be27899db))
* improve bn command mode timeouts ([#4712](https://github.com/tari-project/tari/issues/4712)) ([e7b0b8f](https://github.com/tari-project/tari/commit/e7b0b8f0a3b5b5683b99f2fdf4b67e0345a7ad3d))
* improve the TMS validation process ([#4694](https://github.com/tari-project/tari/issues/4694)) ([030bece](https://github.com/tari-project/tari/commit/030becec8ad1479f394a1bc4b1285b5ee3c9d17b))
* improve txo validation logic ([#4689](https://github.com/tari-project/tari/issues/4689)) ([2b5afcf](https://github.com/tari-project/tari/commit/2b5afcfda7563da75831b9c579a1a415eb716bc5))
* **tariscript:** adds ToRistrettoPoint op-code ([#4749](https://github.com/tari-project/tari/issues/4749)) ([8f872a1](https://github.com/tari-project/tari/commit/8f872a1d5e154cb8f134474da56e917b512e18d5))
* trigger mempool sync on lag ([#4730](https://github.com/tari-project/tari/issues/4730)) ([1e22a03](https://github.com/tari-project/tari/commit/1e22a036bf965f89def9a5ae3514ee4e86886e2b))
* wallet optimize output manager db operations ([#4663](https://github.com/tari-project/tari/issues/4663)) ([25c4d99](https://github.com/tari-project/tari/commit/25c4d99699438526725701dff167e3c608af7ad5))


### Bug Fixes

* add a macos dependency to compile libtor ([#4720](https://github.com/tari-project/tari/issues/4720)) ([b41226c](https://github.com/tari-project/tari/commit/b41226c52c74d5e053e0a366931f558afb78c483))
* **base_node/grpc:** audit of error handling ([#4704](https://github.com/tari-project/tari/issues/4704)) ([595e334](https://github.com/tari-project/tari/commit/595e334ee3a7ab0d885680c6245c88e08c14a5e5))
* **base-node/grpc:** fixes panic if invalid kernel mr is given ([#4693](https://github.com/tari-project/tari/issues/4693)) ([80af7fa](https://github.com/tari-project/tari/commit/80af7fa32a37f51718f7f15113dce3f7d708dd41))
* burned reorg ([#4697](https://github.com/tari-project/tari/issues/4697)) ([08773f4](https://github.com/tari-project/tari/commit/08773f4a4522169e80d06f684e5235b61491d404))
* **ci:** add cargo cache, reduce Ubuntu dependencies and action on pull_request ([#4757](https://github.com/tari-project/tari/issues/4757)) ([33e0dc2](https://github.com/tari-project/tari/commit/33e0dc24222a24e95fbed1d5d66eaa1a340423eb))
* cli wallet cucumber ([#4739](https://github.com/tari-project/tari/issues/4739)) ([62384f9](https://github.com/tari-project/tari/commit/62384f9fc491d9fe87cfc05c659ef6002a00c8e8))
* **clients:** fix tari nodejs client proto paths ([#4743](https://github.com/tari-project/tari/issues/4743)) ([88b75dc](https://github.com/tari-project/tari/commit/88b75dc29b129ee083fa1408a6a65270d0444512))
* **comms/peer_manager:** add migration to remove onionv2 addresses ([#4748](https://github.com/tari-project/tari/issues/4748)) ([a92f205](https://github.com/tari-project/tari/commit/a92f205ff60ea47d1b58da9ec60ee9d2e0249c15))
* **comms/rpc:** increase max frame size limit for rpc requests ([#4724](https://github.com/tari-project/tari/issues/4724)) ([239b64b](https://github.com/tari-project/tari/commit/239b64bc9935b873a646c8b93a8e3011c3a4d483))
* **comms:** fixes edge case where online status event does not get published ([#4756](https://github.com/tari-project/tari/issues/4756)) ([aab729a](https://github.com/tari-project/tari/commit/aab729a139f8fa31cb43eab22abaf16bbad8f3b2))
* **core/mempool:** improve perf of retrieve transactions ([#4710](https://github.com/tari-project/tari/issues/4710)) ([f55762e](https://github.com/tari-project/tari/commit/f55762ea05e54f7711e893f1c7df4d7b670ddabd))
* **core:** broken doctests ([#4763](https://github.com/tari-project/tari/issues/4763)) ([4cbb378](https://github.com/tari-project/tari/commit/4cbb37853281615dd5c8d7009c5ab2b44f7ab0a5))
* **core:** improve logging of dropped reply channels ([#4702](https://github.com/tari-project/tari/issues/4702)) ([9768f02](https://github.com/tari-project/tari/commit/9768f02935f4fe5c84dd5fc2f9058e58251c5ff0))
* **core:** use compact inputs for block propagation ([#4714](https://github.com/tari-project/tari/issues/4714)) ([c659275](https://github.com/tari-project/tari/commit/c65927500c0792f84953274d9036d6c8d1bec72f))
* **dht/encryption:** greatly reduce heap allocations for encrypted messaging ([#4753](https://github.com/tari-project/tari/issues/4753)) ([195df85](https://github.com/tari-project/tari/commit/195df85172a22fe710e6ce082dbe82db5f6c8d19))
* ffi tests ([#4713](https://github.com/tari-project/tari/issues/4713)) ([4551ac3](https://github.com/tari-project/tari/commit/4551ac393f205f83db2e9d0faba54ed236e71400))
* fixes cargo check ([#4729](https://github.com/tari-project/tari/issues/4729)) ([851ba1d](https://github.com/tari-project/tari/commit/851ba1d4d1d6140b14f761d3e3868c9cea90d131))
* mined tx being invalid ([#4735](https://github.com/tari-project/tari/issues/4735)) ([24e396d](https://github.com/tari-project/tari/commit/24e396d9d6073b6b4b7355bb6f2558a73a0560f2))
* refactor incorrect cucumber test on burn funds via cli  ([#4679](https://github.com/tari-project/tari/issues/4679)) ([cd183ef](https://github.com/tari-project/tari/commit/cd183ef44f43cb1400716b10cee6d2e76fd9f81a))
* sync handling and increase reorg speed in mempool ([#4706](https://github.com/tari-project/tari/issues/4706)) ([a3b529a](https://github.com/tari-project/tari/commit/a3b529ad52e9654cecac76667bc748796e5573bf))
* **wallet:** fixes bug in fetch_by_commitment ([#4703](https://github.com/tari-project/tari/issues/4703)) ([97b01c2](https://github.com/tari-project/tari/commit/97b01c2b70f02ded466c866841a5d03ba49fad02))

### [0.38.4](https://github.com/tari-project/tari/compare/v0.38.3...v0.38.4) (2022-09-16)


### Features

* **ci:** add default CI and FFI testing with custom dispatch ([#4672](https://github.com/tari-project/tari/issues/4672)) ([9242051](https://github.com/tari-project/tari/commit/92420516f464146ffdbf16b7a3759012da79cf0d))


### Bug Fixes

* add burn funds command to console wallet (see issue [#4547](https://github.com/tari-project/tari/issues/4547)) ([#4655](https://github.com/tari-project/tari/issues/4655)) ([0242b1d](https://github.com/tari-project/tari/commit/0242b1d435a62433229e3e3752febca84aca7dae))
* **comms:** simplify and remove possibility of deadlock from pipelines and substream close ([#4676](https://github.com/tari-project/tari/issues/4676)) ([f41bcf9](https://github.com/tari-project/tari/commit/f41bcf930ddcfaa622c5f859b8e82331fa8981a8))
* fix potential race condition between add_block and sync ([#4677](https://github.com/tari-project/tari/issues/4677)) ([55f2b9c](https://github.com/tari-project/tari/commit/55f2b9cfe3ed254d0927f4ecc57484320eedca60))
* **p2p:** remove DETACH flag usage ([#4682](https://github.com/tari-project/tari/issues/4682)) ([947f64f](https://github.com/tari-project/tari/commit/947f64fc84e664d927ccc1043d8cff136b0e2449))
* reinsert transactions from failed block ([#4675](https://github.com/tari-project/tari/issues/4675)) ([8030364](https://github.com/tari-project/tari/commit/8030364ec67f3c9102d47fdc8e5bb45ee47cabc3))
* stray clippy error ([#4685](https://github.com/tari-project/tari/issues/4685)) ([a709282](https://github.com/tari-project/tari/commit/a709282c8729807781b45302ee8e87d235ca2d61))
* **wallet:** mark mined_height as null when pending outputs are cancelled ([#4686](https://github.com/tari-project/tari/issues/4686)) ([209ee3d](https://github.com/tari-project/tari/commit/209ee3d27d78c95f37fcdd731b34a846611dd458))

### [0.38.3](https://github.com/tari-project/tari/compare/v0.38.2...v0.38.3) (2022-09-13)


### Bug Fixes

* **ci:** libtor build on Ubuntu ([#4644](https://github.com/tari-project/tari/issues/4644)) ([6f69276](https://github.com/tari-project/tari/commit/6f692766d5cca5e9b393b2a06662c85fc7ca5aff))
* **comms/messaging:** fix possible deadlock in outbound pipeline ([#4657](https://github.com/tari-project/tari/issues/4657)) ([3fcc6a0](https://github.com/tari-project/tari/commit/3fcc6a00c663dfab6ea7a196f56d689eda5990d2))
* **core/sync:** handle deadline timeouts by changing peer ([#4649](https://github.com/tari-project/tari/issues/4649)) ([5ed997c](https://github.com/tari-project/tari/commit/5ed997cdf4ac29daa28f5e2654ace99a65ef0144))
* fee estimate ([#4656](https://github.com/tari-project/tari/issues/4656)) ([d9de2e0](https://github.com/tari-project/tari/commit/d9de2e01c745afb7c876454510191982f1e9af68))
* replace Luhn checksum with DammSum ([#4639](https://github.com/tari-project/tari/issues/4639)) ([c01471a](https://github.com/tari-project/tari/commit/c01471a663eae409d77ba703e40ecd2bb31df173))

### [0.38.2](https://github.com/tari-project/tari/compare/v0.38.1...v0.38.2) (2022-09-08)


### Bug Fixes

* **comms/rpc:** detect early close in all cases ([#4647](https://github.com/tari-project/tari/issues/4647)) ([0125051](https://github.com/tari-project/tari/commit/0125051fe6d80dbf5fe65e91a2e47e9c89a09e5b))
* exclude libtor from windows build ([#4631](https://github.com/tari-project/tari/issues/4631)) ([dffea23](https://github.com/tari-project/tari/commit/dffea2387b7f941eb798548b7eca819738f3e95e))

### [0.38.1](https://github.com/tari-project/tari/compare/v0.38.0...v0.38.1) (2022-09-07)


### Features

* allow user to select specific UTXOs when sending transactions [#4514](https://github.com/tari-project/tari/issues/4514) ([#4523](https://github.com/tari-project/tari/issues/4523)) ([4b40e61](https://github.com/tari-project/tari/commit/4b40e61154e5aa7ee32914ca48540f4f583c1d91))
* attempt to recognize the source of a recovered output ([#4580](https://github.com/tari-project/tari/issues/4580)) ([095196b](https://github.com/tari-project/tari/commit/095196bb684546eba00a9fd2e35c02ddda172437))
* **ci:** merge non-critical & long-running CI into one workflow ([#4614](https://github.com/tari-project/tari/issues/4614)) ([a81228c](https://github.com/tari-project/tari/commit/a81228c4a363035b68c09b49a4435b6fa982f3b7))
* **comms:** update yamux and snow dependencies ([#4600](https://github.com/tari-project/tari/issues/4600)) ([541877a](https://github.com/tari-project/tari/commit/541877a78b85bff9bc540b6e6d465b9bbf41ef7d))
* console and FFI should have setting to not choose outputs that reveal the address [#4403](https://github.com/tari-project/tari/issues/4403) ([#4516](https://github.com/tari-project/tari/issues/4516)) ([17bb64e](https://github.com/tari-project/tari/commit/17bb64e4174549c846aa6f39ad0235cfd4d013f1))
* hide Coinbases that are in the process of being mined ([#4602](https://github.com/tari-project/tari/issues/4602)) ([c6c47fc](https://github.com/tari-project/tari/commit/c6c47fcdc8a12078e2e1210964bdd3977b8a57ca))
* let sql in wal mode provide async db, not app level spawn blocking (transaction service) ([#4597](https://github.com/tari-project/tari/issues/4597)) ([e17c1f9](https://github.com/tari-project/tari/commit/e17c1f9696e3f4aaca73d1f711735bbdc5ffa0ec))
* make sure duplication check happens first in mempool ([#4627](https://github.com/tari-project/tari/issues/4627)) ([23e4894](https://github.com/tari-project/tari/commit/23e4894ddc21f8099a102b22bfb540c6c9dcd13d))
* remove spawn blocking calls from wallet db (wallet storage)([#4591](https://github.com/tari-project/tari/issues/4591)) ([77bb10d](https://github.com/tari-project/tari/commit/77bb10d42e8c004406d0ddd69b65575f0e111cd1))


### Bug Fixes

* add Grpc authentication to merge mining proxy (see issue [#4587](https://github.com/tari-project/tari/issues/4587)) ([#4592](https://github.com/tari-project/tari/issues/4592)) ([004c219](https://github.com/tari-project/tari/commit/004c219643ae42c0c1afcdb835542e53b581bfa3))
* change wallet log target from error to trace (see issue [#4586](https://github.com/tari-project/tari/issues/4586)) ([183fa6e](https://github.com/tari-project/tari/commit/183fa6e22eabb43037605c03236cdc81ce0a7dae))
* cleanup logs ([#4590](https://github.com/tari-project/tari/issues/4590)) ([66c8032](https://github.com/tari-project/tari/commit/66c80327db77a26f8370bc7bd972b8d5abcaf619))
* **comms:** only reap when number of connections exceeds threshold ([#4607](https://github.com/tari-project/tari/issues/4607)) ([415f339](https://github.com/tari-project/tari/commit/415f33989ad55a55a04ca4afc3f4c115a9e930c1))
* **console_wallet:** use cli.non_interactive instead of propmt to show seed words ([#4612](https://github.com/tari-project/tari/issues/4612)) ([8ad67ab](https://github.com/tari-project/tari/commit/8ad67ab5e8626157e475b2d57d4c68ad43df5108))
* **dht:** updates to message padding ([#4594](https://github.com/tari-project/tari/issues/4594)) ([cf4f9bf](https://github.com/tari-project/tari/commit/cf4f9bf1b555755d8be6fd7a3bd401f6bc154fdd))
* ffi wallet file for unknown type name ([#4589](https://github.com/tari-project/tari/issues/4589)) ([5cbf9aa](https://github.com/tari-project/tari/commit/5cbf9aa95a9b03e9e9a95c9b823dd12e43aa30f1))
* **outbound:** reduce messaging protocol error to debug ([#4578](https://github.com/tari-project/tari/issues/4578)) ([99cef05](https://github.com/tari-project/tari/commit/99cef051a341e506420c2a70517122ff68c60dba))
* reduces RPC error log to debug when domain-level RPC service returns an error (fixes [#4579](https://github.com/tari-project/tari/issues/4579)) ([#4611](https://github.com/tari-project/tari/issues/4611)) ([86c030d](https://github.com/tari-project/tari/commit/86c030d7b3adbdf8b65394f6d3dc4ace61ba8c35))
* remove unused dependencies ([#4624](https://github.com/tari-project/tari/issues/4624)) ([058f492](https://github.com/tari-project/tari/commit/058f492e7f61fec68583c3b0d08ffd4de470f27a))
* remove window resize ([#4593](https://github.com/tari-project/tari/issues/4593)) ([896eff9](https://github.com/tari-project/tari/commit/896eff9b8df5b865fa511e3964231c983547e3a0))
* stop race condition in output encumbrance  ([#4613](https://github.com/tari-project/tari/issues/4613)) ([31e130a](https://github.com/tari-project/tari/commit/31e130a821cdba0daaa75da051c8c19237efbff0))
* update cargo versions ([#4622](https://github.com/tari-project/tari/issues/4622)) ([07c1a29](https://github.com/tari-project/tari/commit/07c1a2949e07918a56fd00ba77698037e4212009))
* use dht inbound error for decryption (Fixes [#4596](https://github.com/tari-project/tari/issues/4596))  ([#4601](https://github.com/tari-project/tari/issues/4601)) ([d9ef267](https://github.com/tari-project/tari/commit/d9ef2670df1a2e7c68e3751e0583f77eaf8bdf7c))
* **wallet:** detect base node change during long-running protocols ([#4610](https://github.com/tari-project/tari/issues/4610)) ([2a2a8b6](https://github.com/tari-project/tari/commit/2a2a8b68ee2ff8bf2b4335288fd5fbff0d11ea92))
* **wallet:** use RPC pool connections for non-recovery utxo scanning ([#4598](https://github.com/tari-project/tari/issues/4598)) ([7c9e22c](https://github.com/tari-project/tari/commit/7c9e22cb32ea9d8253dc11b45759a488c7ba1659))

## [0.38.0](https://github.com/tari-project/tari/compare/v0.37.0...v0.38.0) (2022-08-31)


### ⚠ BREAKING CHANGES

* replace AES-GCM with XChaCha20-Poly1305 (#4550)

### Features

* **build:** multiple targeted build types with options for docker builds ([#4540](https://github.com/tari-project/tari/issues/4540)) ([7e7d053](https://github.com/tari-project/tari/commit/7e7d05351e157b8ca6d4d5b5e1e258a6281d6375))
* **comms/rpc:** restrict rpc session per peer [#4497](https://github.com/tari-project/tari/issues/4497) ([#4549](https://github.com/tari-project/tari/issues/4549)) ([080bccf](https://github.com/tari-project/tari/commit/080bccf1a037f5574962704947d29d8f1218d42a))
* **console-wallet:** detect local base node and prompt ([#4557](https://github.com/tari-project/tari/issues/4557)) ([887df88](https://github.com/tari-project/tari/commit/887df88d57fb4566b8383a3e33ad5caee4df762c))
* remove spawn blocking calls from wallet db (contacts service) ([#4575](https://github.com/tari-project/tari/issues/4575)) ([7464581](https://github.com/tari-project/tari/commit/74645813ab836b19d9d722aaa189a2d190eb5c6e))
* remove spawn blocking calls from wallet db (key manager service) ([#4564](https://github.com/tari-project/tari/issues/4564)) ([a5d5133](https://github.com/tari-project/tari/commit/a5d5133943bb11e8509a51aeb7f3d40b67bc065b))
* update tor seed nodes for esmeralda network ([#4572](https://github.com/tari-project/tari/issues/4572)) ([c4cfc12](https://github.com/tari-project/tari/commit/c4cfc128f786be3806f51d699d89465756f97e7b))
* upgrade to tokio 1.20.1 ([#4566](https://github.com/tari-project/tari/issues/4566)) ([777936a](https://github.com/tari-project/tari/commit/777936a0c2783635f77549d3f23520912b87b7bf))


### Bug Fixes

* **cucumber:** handles listHeaders response correctly ([#4551](https://github.com/tari-project/tari/issues/4551)) ([3958dde](https://github.com/tari-project/tari/commit/3958dde8114e4301c33a90073c1a2e3c973e0e5d))
* deserializer for SafePassword ([#4565](https://github.com/tari-project/tari/issues/4565)) ([ee89960](https://github.com/tari-project/tari/commit/ee899606e0b9c9877c89fa35add3dc2fe54be30f))
* ignored consensus tests (see issue [#4559](https://github.com/tari-project/tari/issues/4559)) ([#4571](https://github.com/tari-project/tari/issues/4571)) ([397fe67](https://github.com/tari-project/tari/commit/397fe673b3b47d57422db71523d8012381980e6c))
* potential problem with not updating the OMS database ([#4563](https://github.com/tari-project/tari/issues/4563)) ([c867279](https://github.com/tari-project/tari/commit/c86727969ef3fffc124ab706d44c8845addbf415))
* remove assets and tokens tabs from tari console wallet (see issue [#4543](https://github.com/tari-project/tari/issues/4543)) ([#4556](https://github.com/tari-project/tari/issues/4556)) ([11af787](https://github.com/tari-project/tari/commit/11af7875acfca85d82394d82852729952d638d98))
* removed `seed_words` and `delete_seed_words` commands ([#4567](https://github.com/tari-project/tari/issues/4567)) ([0b2a155](https://github.com/tari-project/tari/commit/0b2a15585e88240c027175a24dd9757cca4218ac))
* replace AES-GCM with XChaCha20-Poly1305 ([#4550](https://github.com/tari-project/tari/issues/4550)) ([85acc2f](https://github.com/tari-project/tari/commit/85acc2f1a06afa4e7b184e4577c2b081691783da))
* resolve tests in output_manager_service_tests.rs (see issue [#4561](https://github.com/tari-project/tari/issues/4561)) ([#4577](https://github.com/tari-project/tari/issues/4577)) ([c69245b](https://github.com/tari-project/tari/commit/c69245bbf5e9f212c07bc1736cedd9351f4d6eef))
* update rest of the crates to tokio 1.20 ([#4576](https://github.com/tari-project/tari/issues/4576)) ([ad24bf7](https://github.com/tari-project/tari/commit/ad24bf71714ffc091c9fce7c1fc224235e3666a9))

## [0.37.0](https://github.com/tari-project/tari/compare/v0.36.0...v0.37.0) (2022-08-25)


### ⚠ BREAKING CHANGES

* reset gen block for new faucets (#4534)
* **core:** replace block and transaction hashes with FixedHash type (#4533)
* change how sha3 difficulty is calculated (#4528)
* header consensus (#4527)
* add domain consensus hashing to all previous uses of default consensus hashing (#4522)

### Features

* add domain consensus hashing to all previous uses of default consensus hashing ([#4522](https://github.com/tari-project/tari/issues/4522)) ([1885509](https://github.com/tari-project/tari/commit/1885509846280d78004ac93fc9dfd0cd5b7aa957))
* change how sha3 difficulty is calculated ([#4528](https://github.com/tari-project/tari/issues/4528)) ([1843998](https://github.com/tari-project/tari/commit/1843998370a515ff93bb570a6a89745ff584b654))
* **console-wallet:** adds basic grpc authentication ([#4532](https://github.com/tari-project/tari/issues/4532)) ([2615c1b](https://github.com/tari-project/tari/commit/2615c1baa58f897f048d6a6f22149726983e22fd))
* make stealth addresses the default for one sided in ffi [#4423](https://github.com/tari-project/tari/issues/4423) ([#4517](https://github.com/tari-project/tari/issues/4517)) ([e89ffac](https://github.com/tari-project/tari/commit/e89ffaca695bf79ceacab2313a89a9dc817f4134))
* merge grpc list headers commands ([#4515](https://github.com/tari-project/tari/issues/4515)) ([18a88d7](https://github.com/tari-project/tari/commit/18a88d72e1f57cf0c4c876085fe6e026aae2e1fd))
* show the banned reason if a peer was banned in the contacts ([#4525](https://github.com/tari-project/tari/issues/4525)) ([f970f81](https://github.com/tari-project/tari/commit/f970f81484d0103c0c294b82a8bb6c6fe8e98df9))


### Bug Fixes

* **base-node:** fix messy output in base node console ([#4509](https://github.com/tari-project/tari/issues/4509)) ([c2dfa23](https://github.com/tari-project/tari/commit/c2dfa23be082ae47b73310f6fb87d2a641f6fc04))
* **build:** remove tari_validator_node from binary builds ([#4518](https://github.com/tari-project/tari/issues/4518)) ([6df9cc8](https://github.com/tari-project/tari/commit/6df9cc8d7939b55951d57234a8016afcbb588592))
* **build:** switch linux-arm64 builds to cross-rs ([#4524](https://github.com/tari-project/tari/issues/4524)) ([69e8d0b](https://github.com/tari-project/tari/commit/69e8d0bb0ddeb9db3e3cb47d9a76dab2b0ff89cb))
* **comms/dht:** fixes invalid peer ban on invalid encrypted msg signature ([#4519](https://github.com/tari-project/tari/issues/4519)) ([7a2c95e](https://github.com/tari-project/tari/commit/7a2c95e9897f57abfbe5410c9e62982d52bff291))
* **comms:** ignore dial cancel event when inbound connection exists ([#4521](https://github.com/tari-project/tari/issues/4521)) ([a7040c5](https://github.com/tari-project/tari/commit/a7040c58c7fc7e507f074c02c1a601fce944469c))
* **core:** fix block consensus encoding and add tests ([#4537](https://github.com/tari-project/tari/issues/4537)) ([0a9d5ef](https://github.com/tari-project/tari/commit/0a9d5ef27b8287a5a2772c8f13344f1a9f9edfa4))
* **core:** replace block and transaction hashes with FixedHash type ([#4533](https://github.com/tari-project/tari/issues/4533)) ([501f550](https://github.com/tari-project/tari/commit/501f550e8ff07d20b4544f9af233d33ba7fb2fc8))
* **cucumber:** update get_online_status function signature in cucumber ([#4536](https://github.com/tari-project/tari/issues/4536)) ([a57eb12](https://github.com/tari-project/tari/commit/a57eb1238a6ef9a483592c7f65a646cb7c217ba9))
* grpc inconsistent serialization of keys (see issue [#4224](https://github.com/tari-project/tari/issues/4224)) ([#4491](https://github.com/tari-project/tari/issues/4491)) ([bdb262c](https://github.com/tari-project/tari/commit/bdb262cc08dc66b4a1297b4cc8dfdecbfd6afe8d))
* header consensus ([#4527](https://github.com/tari-project/tari/issues/4527)) ([3f5fcf2](https://github.com/tari-project/tari/commit/3f5fcf285a0f2dd426de02b19b36978165f3c218))
* re-add user agents to base node and console wallet ([#4520](https://github.com/tari-project/tari/issues/4520)) ([3f60c44](https://github.com/tari-project/tari/commit/3f60c4417e8a3bbd8b63a260a834364e0c41fde1))


* reset gen block for new faucets ([#4534](https://github.com/tari-project/tari/issues/4534)) ([4fe5742](https://github.com/tari-project/tari/commit/4fe5742207e305275fe06b841c102e3ecd67760e))

## [0.36.0](https://github.com/tari-project/tari/compare/v0.35.0...v0.36.0) (2022-08-19)


### ⚠ BREAKING CHANGES

* add hashing API use to base layer (see issue #4394) (#4447)
* change monero consensus encoding an update for hardfork v15 (#4492)
* **core/covenants:** update covenants to support OutputType enum (#4472)
* **core:** restrict output types to only those permitted by consensus (#4467)
* **tariscript:** use varints in tari script (de)serialization (#4460)
* apply hashing api to the mmr (#4445)
* burned commitment mmr calculation and cucumber prune mode (#4453)

### Features

* add hashing API use to base layer (see issue [#4394](https://github.com/tari-project/tari/issues/4394)) ([#4447](https://github.com/tari-project/tari/issues/4447)) ([f9af875](https://github.com/tari-project/tari/commit/f9af875f292b4104c6a9114fce70e06e09012fc5))
* apply hashing api to the mmr ([#4445](https://github.com/tari-project/tari/issues/4445)) ([d6bab2f](https://github.com/tari-project/tari/commit/d6bab2fb276125d9fcc1276f3525ac62a2ccb378))
* **core:** restrict output types to only those permitted by consensus ([#4467](https://github.com/tari-project/tari/issues/4467)) ([a481a06](https://github.com/tari-project/tari/commit/a481a06f9521dcab1703ea061dab703e5d33cdc0))
* move dan layer to new repo ([#4448](https://github.com/tari-project/tari/issues/4448)) ([52bc2be](https://github.com/tari-project/tari/commit/52bc2becec6738d6cfbb9349c42a2dbd46eb6398))
* remove more dan layer code ([#4490](https://github.com/tari-project/tari/issues/4490)) ([073de55](https://github.com/tari-project/tari/commit/073de55c085907bca807381b9b01473afdc656ef))
* remove total_txs and rename total_weight in mempool ([#4474](https://github.com/tari-project/tari/issues/4474)) ([02ed4d4](https://github.com/tari-project/tari/commit/02ed4d447761ed97d60037d0b586714efec0857d))
* **tariscript:** use varints in tari script (de)serialization ([#4460](https://github.com/tari-project/tari/issues/4460)) ([a0403e5](https://github.com/tari-project/tari/commit/a0403e52cf4bff0dca6fe760e03e865f9e4926fa))
* **wallet:** adds --stealth-one-sided flag to make-it-rain ([#4508](https://github.com/tari-project/tari/issues/4508)) ([30dd70e](https://github.com/tari-project/tari/commit/30dd70e0f32a5ed9b28ed67183177b7815d0c2b7))


### Bug Fixes

* **base_node_config:** check_interval is 0 made base node is panicked ([#4495](https://github.com/tari-project/tari/issues/4495)) ([ba4fbf7](https://github.com/tari-project/tari/commit/ba4fbf742d635d51109595516be1011c6ce3ca87)), closes [#4399](https://github.com/tari-project/tari/issues/4399)
* burned commitment mmr calculation and cucumber prune mode ([#4453](https://github.com/tari-project/tari/issues/4453)) ([0c062f3](https://github.com/tari-project/tari/commit/0c062f3f424494c14689c334c77241b716dd04aa))
* change monero consensus encoding an update for hardfork v15 ([#4492](https://github.com/tari-project/tari/issues/4492)) ([2a3af27](https://github.com/tari-project/tari/commit/2a3af27b4fed419a038bac69f5967317d84acc33))
* **ci:** binary builds - remove tari_collectibles and lib deps for linux  ([#4454](https://github.com/tari-project/tari/issues/4454)) ([1f9968e](https://github.com/tari-project/tari/commit/1f9968eb33e18a4d1a54d0ebb1387b72396a2265))
* **core/covenants:** update covenants to support OutputType enum ([#4472](https://github.com/tari-project/tari/issues/4472)) ([e21dfdb](https://github.com/tari-project/tari/commit/e21dfdbdf4e7c29731b2b3d7625c4a879f8a543c))
* fix transaction output hashing ([#4483](https://github.com/tari-project/tari/issues/4483)) ([46d65fc](https://github.com/tari-project/tari/commit/46d65fcff4e5fa00b6eaf0328f77cf5f4646d35c))
* remove old folders from ci ([#4455](https://github.com/tari-project/tari/issues/4455)) ([2c76031](https://github.com/tari-project/tari/commit/2c76031dbee8cdb82772a3516cd5e07a22dd023a))
* remove use of hardcoded value (see issue [#4451](https://github.com/tari-project/tari/issues/4451)) ([#4485](https://github.com/tari-project/tari/issues/4485)) ([37d38d6](https://github.com/tari-project/tari/commit/37d38d6fa409701b01c11dfe8a7009e86bb474ea)), closes [/github.com/tari-project/tari/blob/development/comms/dht/src/network_discovery/on_connect.rs#L94](https://github.com/tari-project//github.com/tari-project/tari/blob/development/comms/dht/src/network_discovery/on_connect.rs/issues/L94)
* wallet always scan interactive payments (see [#4452](https://github.com/tari-project/tari/issues/4452)) ([#4464](https://github.com/tari-project/tari/issues/4464)) ([0c595dd](https://github.com/tari-project/tari/commit/0c595ddd1f118f642299e97052310ce09d490b13))
* wrong ban reason ([#4461](https://github.com/tari-project/tari/issues/4461)) ([4788789](https://github.com/tari-project/tari/commit/4788789a9d0b1ebc39756eabb737429b1411b9a0))

## [0.35.0](https://github.com/tari-project/tari/compare/v0.32.12...v0.35.0) (2022-08-11)


### ⚠ BREAKING CHANGES

* **comms:** use domain hasher for noise DH key derivation (#4432)
* **core:** consensus hashing without extraneous length prefixes for each write (#4420)
* new esmeralda network (#4391)
* fix kernel mutability (#4377)
* **dht:** add message padding for message decryption, to reduce message length leaks (fixes #4140) (#4362)
* improve wallet key derivation by use of proper domain separation (see issue  #4170) (#4316)
* add burned outputs (#4364)
* **base_layer:** new output field for minimum value range proof (#4319)

### Features

* accept 'esme' as network name on cli ([#4401](https://github.com/tari-project/tari/issues/4401)) ([966aa0b](https://github.com/tari-project/tari/commit/966aa0b7ac5ca77247ae50f9c78beba2ea3a6d87))
* add burned outputs ([#4364](https://github.com/tari-project/tari/issues/4364)) ([60f3877](https://github.com/tari-project/tari/commit/60f3877f224d2311cd9e31770f8dbbed536ce742))
* add hashing api to wallet secret keys ([#4424](https://github.com/tari-project/tari/issues/4424)) ([d944574](https://github.com/tari-project/tari/commit/d944574905c3bb63cbcad0dbfa5222e8efe77717))
* add quarantine and backup keys ([#4289](https://github.com/tari-project/tari/issues/4289)) ([ed9f8cb](https://github.com/tari-project/tari/commit/ed9f8cbfd85aad4ca63b0f574c4854c02fee1840))
* add sending to stealth address (command, grpc, gui) ([#4307](https://github.com/tari-project/tari/issues/4307)) ([a897278](https://github.com/tari-project/tari/commit/a897278be21d54b0a529faad38502ed52e428a6d))
* add tari_crypto hashing api support ([#4328](https://github.com/tari-project/tari/issues/4328)) ([dba167b](https://github.com/tari-project/tari/commit/dba167beab6e92071d51a2415b865417b39a3d5a))
* add tari_scanner ([#4280](https://github.com/tari-project/tari/issues/4280)) ([bd672eb](https://github.com/tari-project/tari/commit/bd672eb01b7f2fbc842809f7fcda8323a50bc996))
* **base_layer/core:** add domain hashing wrapper for consensus encoding ([#4381](https://github.com/tari-project/tari/issues/4381)) ([ad11ec5](https://github.com/tari-project/tari/commit/ad11ec50db19bdf3f92c998ad2c806623ba1714c))
* **base_layer:** checkpoint quorum validation ([#4303](https://github.com/tari-project/tari/issues/4303)) ([e1704f4](https://github.com/tari-project/tari/commit/e1704f46a3276dd0bfbc0182352939ca4a3e8828))
* **base_layer:** checkpoint signature validation ([#4297](https://github.com/tari-project/tari/issues/4297)) ([850e78f](https://github.com/tari-project/tari/commit/850e78f4c2589650c19009d89fda2ce75cafc08d))
* **base_layer:** new output field for minimum value range proof ([#4319](https://github.com/tari-project/tari/issues/4319)) ([5cff7a9](https://github.com/tari-project/tari/commit/5cff7a99dc80b2cd6f0cdaff3d580388b9416fa1))
* **base_layer:** remove the initial_reward field for contracts ([#4313](https://github.com/tari-project/tari/issues/4313)) ([a7daf3a](https://github.com/tari-project/tari/commit/a7daf3a9a3902d0e5a9624045254adca247261a2))
* **base-node:** adds `add-peer` command ([#4430](https://github.com/tari-project/tari/issues/4430)) ([b2563a2](https://github.com/tari-project/tari/commit/b2563a27bc491cdf4a08e5fcd63c4a0d9cef1174))
* build arm64 binaries from json matrix ([#4342](https://github.com/tari-project/tari/issues/4342)) ([53397e0](https://github.com/tari-project/tari/commit/53397e07d679a9afbd21ffb97d6309025294b891))
* bump tari_crypto version -> v0.15.4 ([#4382](https://github.com/tari-project/tari/issues/4382)) ([5dd7811](https://github.com/tari-project/tari/commit/5dd781190e1eef272de1423bb53076407d173ad9))
* bump toolchain for GHA from nightly-2021-11-20 to nightly-2022-05-01 ([#4308](https://github.com/tari-project/tari/issues/4308)) ([dbdadcf](https://github.com/tari-project/tari/commit/dbdadcf2e1030464ed098ef9b9d004614164c5cc))
* bump toolchain for GHA from nightly-2021-11-20 to nightly-2022-05-01 for develop ([#4329](https://github.com/tari-project/tari/issues/4329)) ([9797c19](https://github.com/tari-project/tari/commit/9797c1902b83122617dd3dde859d5c63dc71b170))
* **comms:** auto detects configured tor control port auth ([#4428](https://github.com/tari-project/tari/issues/4428)) ([98a7b0c](https://github.com/tari-project/tari/commit/98a7b0c623a71f858e4670e80db609e66c74b96c))
* **dan/engine:** add more engine primitives and add template context ([#4388](https://github.com/tari-project/tari/issues/4388)) ([a481f89](https://github.com/tari-project/tari/commit/a481f896bc6ee6e4114c163b5b15d9bb617c9792))
* **dan/engine:** adds single state storage interface ([#4368](https://github.com/tari-project/tari/issues/4368)) ([954efea](https://github.com/tari-project/tari/commit/954efea1e14f52183020d9467d0917f4997e12c4))
* **dan/wasm:** implement basic wasm module engine calls ([#4350](https://github.com/tari-project/tari/issues/4350)) ([ad89150](https://github.com/tari-project/tari/commit/ad891509d56dbb359713217ab5d02c85fb9d976d))
* **dan:** add WASM template invocation from user instruction ([#4331](https://github.com/tari-project/tari/issues/4331)) ([d265c6f](https://github.com/tari-project/tari/commit/d265c6f61c4f68098129b72f5907108e527868a3))
* **dan:** basic template macro ([#4358](https://github.com/tari-project/tari/issues/4358)) ([594ca0e](https://github.com/tari-project/tari/commit/594ca0e8f0df133b54647ac4f9b3a0ea774ffa2b))
* **dan:** invocation of functions in templates ([#4343](https://github.com/tari-project/tari/issues/4343)) ([3d92eb0](https://github.com/tari-project/tari/commit/3d92eb0901391e3b73e4008a930c37c27bedb913))
* **dan:** template macro handles component state ([#4380](https://github.com/tari-project/tari/issues/4380)) ([696d909](https://github.com/tari-project/tari/commit/696d9098235d1e8df6e7e2f374718001d6dc80c9))
* fix kernel mutability ([#4377](https://github.com/tari-project/tari/issues/4377)) ([d25d726](https://github.com/tari-project/tari/commit/d25d726ed7e04a066135b80b1de9c019d928a75c))
* **mempool:** remove transaction for locally mined blocks that fail validation ([#4306](https://github.com/tari-project/tari/issues/4306)) ([15f41b3](https://github.com/tari-project/tari/commit/15f41b328eadaf2d68ac6f6bab0033905c5372ae))
* multi-threaded vanity id generator ([#4345](https://github.com/tari-project/tari/issues/4345)) ([32569da](https://github.com/tari-project/tari/commit/32569da37eb6bbb10c53633deb7045fbf334eacc))
* new esmeralda network ([#4391](https://github.com/tari-project/tari/issues/4391)) ([622763a](https://github.com/tari-project/tari/commit/622763a5df31f41aab6a213a788a91d860ea58ba))
* proposal acceptance signatures are submitted and validated ([#4288](https://github.com/tari-project/tari/issues/4288)) ([2bf7efe](https://github.com/tari-project/tari/commit/2bf7efe2cbf004f12325a5ce6d11408a317fa275))
* read tor cookies from a file ([#4317](https://github.com/tari-project/tari/issues/4317)) ([c75d224](https://github.com/tari-project/tari/commit/c75d2242ab5de5301c4340a97f96648a056cf208))
* remove hash_digest type ([#4376](https://github.com/tari-project/tari/issues/4376)) ([7b2750a](https://github.com/tari-project/tari/commit/7b2750abe46237594a57865e7f403b74526d6cf3))
* remove hashing domain and update key manager ([#4367](https://github.com/tari-project/tari/issues/4367)) ([e805f1f](https://github.com/tari-project/tari/commit/e805f1f0ba0121747da2e00843979881d750f731))
* remove recovery byte ([#4301](https://github.com/tari-project/tari/issues/4301)) ([a2778f0](https://github.com/tari-project/tari/commit/a2778f0a162e77bca31431561b11ea0b85036fac))
* show tari script properly in tari_explorer ([#4321](https://github.com/tari-project/tari/issues/4321)) ([4a1b50e](https://github.com/tari-project/tari/commit/4a1b50e4cd48b770d3b8dc8c3925ac8df012d69c))
* support for stealth addresses in one-sided transactions ([#4310](https://github.com/tari-project/tari/issues/4310)) ([c73de62](https://github.com/tari-project/tari/commit/c73de62fae2436278f8c0541286c188b5338d28e))
* tari launchpad downstream merge ([#4322](https://github.com/tari-project/tari/issues/4322)) ([222052f](https://github.com/tari-project/tari/commit/222052f71a56b8b68176a6afd41d2c19353351bc))
* **vn:** recognize abandoned state ([#4272](https://github.com/tari-project/tari/issues/4272)) ([e42085a](https://github.com/tari-project/tari/commit/e42085a5f8b14135fdae87953bea848668be0f6b))


### Bug Fixes

* add divide by zero check ([#4287](https://github.com/tari-project/tari/issues/4287)) ([75a8f59](https://github.com/tari-project/tari/commit/75a8f59528adb8c623a2ad8b83cdd0233d14d5be))
* add hashing api on comms repo (see issue [#4393](https://github.com/tari-project/tari/issues/4393)) ([#4429](https://github.com/tari-project/tari/issues/4429)) ([9f32c31](https://github.com/tari-project/tari/commit/9f32c3149b9364e7688fddfb1c35d986289a66ac))
* add more recent zero point for wallet birthday (see issue [#4176](https://github.com/tari-project/tari/issues/4176)) ([#4275](https://github.com/tari-project/tari/issues/4275)) ([815c478](https://github.com/tari-project/tari/commit/815c478985e34384f97a998658fce1d0d462758c))
* addmissing assets for contribution guide ([#4434](https://github.com/tari-project/tari/issues/4434)) ([4c68408](https://github.com/tari-project/tari/commit/4c684087deee524d0f7f376b4e363f91c3569d37))
* address points in issue [#4138](https://github.com/tari-project/tari/issues/4138) and companions ([#4336](https://github.com/tari-project/tari/issues/4336)) ([2ca0672](https://github.com/tari-project/tari/commit/2ca06724f0ab7c63eb0b6caab563372f353f4348)), closes [#4333](https://github.com/tari-project/tari/issues/4333) [#4170](https://github.com/tari-project/tari/issues/4170)
* apply network config overrides from cli or else file ([#4407](https://github.com/tari-project/tari/issues/4407)) ([222ef15](https://github.com/tari-project/tari/commit/222ef15665cf9417dc16c2abd88523f8e97f65e9))
* better coinbase handling (see issue [#4353](https://github.com/tari-project/tari/issues/4353)) ([#4386](https://github.com/tari-project/tari/issues/4386)) ([5581044](https://github.com/tari-project/tari/commit/558104441c02c13de481c743c1e7e19ebaff3620))
* burned output check ([#4374](https://github.com/tari-project/tari/issues/4374)) ([3b30fdb](https://github.com/tari-project/tari/commit/3b30fdb99af14a43eb0e9762861f5fabc2481cca))
* cap unlimited rpc connection ([#4442](https://github.com/tari-project/tari/issues/4442)) ([4f1a4fe](https://github.com/tari-project/tari/commit/4f1a4feb3e3125ae1b2ff1682fb9fa4218f6e4e2))
* clippy ([#4437](https://github.com/tari-project/tari/issues/4437)) ([7078784](https://github.com/tari-project/tari/commit/70787843bebf6b4569c6746c764028f71aeac433))
* **clippy:** allow unused_mut for not(feature = "libtor") ([#4425](https://github.com/tari-project/tari/issues/4425)) ([36ed59a](https://github.com/tari-project/tari/commit/36ed59a1bdfc94dcde3829e289d7ac512b28447e))
* **comms/tor:** re-establish tor control port connection for any failure ([#4446](https://github.com/tari-project/tari/issues/4446)) ([6d9ca81](https://github.com/tari-project/tari/commit/6d9ca815b0284da132eeff4f6d8f2acaf375a6b7))
* **comms:** use domain hasher for noise DH key derivation ([#4432](https://github.com/tari-project/tari/issues/4432)) ([c93182c](https://github.com/tari-project/tari/commit/c93182c8fe7db8f557a80edbc83a59e7d42e4e9f))
* consolidate sql migrations (see issue [#4356](https://github.com/tari-project/tari/issues/4356)) ([#4415](https://github.com/tari-project/tari/issues/4415)) ([91cd76f](https://github.com/tari-project/tari/commit/91cd76f468c902cf9c6de8954f821618e2d86d20))
* **core:** consensus hashing without extraneous length prefixes for each write ([#4420](https://github.com/tari-project/tari/issues/4420)) ([16ddc4e](https://github.com/tari-project/tari/commit/16ddc4e1bfff98fc76bbdccc4d869d734c080186))
* **core:** edge case fix for chunked iterator ([#4315](https://github.com/tari-project/tari/issues/4315)) ([8854ca1](https://github.com/tari-project/tari/commit/8854ca1bbde16cf0e50f56054eef717f8c02f10e))
* **core:** use domain-separated kdf for encrypted value ([#4421](https://github.com/tari-project/tari/issues/4421)) ([c5a0aef](https://github.com/tari-project/tari/commit/c5a0aef0639a5fa06a1aded0493200cb67a2a83d))
* default to esme ([#4398](https://github.com/tari-project/tari/issues/4398)) ([8d9a448](https://github.com/tari-project/tari/commit/8d9a4482bccdcf3668ba9575e725de40f974feec))
* **dht:** add message padding for message decryption, to reduce message length leaks (fixes [#4140](https://github.com/tari-project/tari/issues/4140)) ([#4362](https://github.com/tari-project/tari/issues/4362)) ([b56c63a](https://github.com/tari-project/tari/commit/b56c63a01085d373a60db3b22b52821417c97c75))
* fix flaky port number assignments in cucumber  ([#4305](https://github.com/tari-project/tari/issues/4305)) ([e8d4a00](https://github.com/tari-project/tari/commit/e8d4a00d7a52de42847cc6406322f7ff14519d80))
* fix new block handling of non-tip blocks ([#4431](https://github.com/tari-project/tari/issues/4431)) ([ee757df](https://github.com/tari-project/tari/commit/ee757df369c4ee56f1ed242020f790b3869ff955))
* fix OSX GHA build fix by bumping OSX release ([#4300](https://github.com/tari-project/tari/issues/4300)) ([fe5954d](https://github.com/tari-project/tari/commit/fe5954d43fb3b7626b06f4b9e66c1933270bb2a4))
* force peer identity signature ([#4387](https://github.com/tari-project/tari/issues/4387)) ([c901dbc](https://github.com/tari-project/tari/commit/c901dbca626f163acf715107d512f5aed796a558))
* implement hashing api for dan layer (see issue [#4392](https://github.com/tari-project/tari/issues/4392)) ([#4427](https://github.com/tari-project/tari/issues/4427)) ([f7c5e77](https://github.com/tari-project/tari/commit/f7c5e771dec4fe1c5c816a0d4c71272541d2e7c3))
* improve wallet key derivation by use of proper domain separation (see issue  [#4170](https://github.com/tari-project/tari/issues/4170)) ([#4316](https://github.com/tari-project/tari/issues/4316)) ([7a25028](https://github.com/tari-project/tari/commit/7a2502885eaf6d15fb4ad9f6566d2ddaadbbe1c0))
* **key_manager:** remove trailing '.' from hashing domains, fix WASM tests ([#4378](https://github.com/tari-project/tari/issues/4378)) ([214a986](https://github.com/tari-project/tari/commit/214a986fce22a69c4bac4eae7e689e653a63987c))
* low entropy mac passphrase in cipher seed (see issue [#4182](https://github.com/tari-project/tari/issues/4182))  ([#4296](https://github.com/tari-project/tari/issues/4296)) ([1c5ec0d](https://github.com/tari-project/tari/commit/1c5ec0d9dc6d9f9f1f256332be5b9e9b0b022315))
* possible overflow in difficulty calculation (fixes [#3923](https://github.com/tari-project/tari/issues/3923)) companion ([#4097](https://github.com/tari-project/tari/issues/4097)) ([ddb8453](https://github.com/tari-project/tari/commit/ddb8453b85cca4b6e5ba615a1afb49630f7da5c0))
* prevent code injection ([#4327](https://github.com/tari-project/tari/issues/4327)) ([5391938](https://github.com/tari-project/tari/commit/5391938c41e5d39cfb57993961a98882887e62bd))
* recover all known scripts ([#4397](https://github.com/tari-project/tari/issues/4397)) ([7502fd6](https://github.com/tari-project/tari/commit/7502fd6538eb96bf566527c90d7bec8a158dee1f))
* remove tari_common dep from keymanager ([#4335](https://github.com/tari-project/tari/issues/4335)) ([5e3797f](https://github.com/tari-project/tari/commit/5e3797f310562369601846f04d347003badf4e1d))
* remove winapi compile warning ([#4440](https://github.com/tari-project/tari/issues/4440)) ([7611468](https://github.com/tari-project/tari/commit/761146850ba723dd55f566a614d054eda309453f))
* solve breaking changes introduced by new tari-crypto tag ([#4347](https://github.com/tari-project/tari/issues/4347)) ([3c74064](https://github.com/tari-project/tari/commit/3c74064397ee0821c98c8a93bf6ef343e152be7a))
* transaction validation excess signature check ([#4314](https://github.com/tari-project/tari/issues/4314)) ([f6342a5](https://github.com/tari-project/tari/commit/f6342a5279b707e3860c2d5674de7523ffe38f68))
* use HashError for value encryption parts ([#4302](https://github.com/tari-project/tari/issues/4302)) ([a0da287](https://github.com/tari-project/tari/commit/a0da287b0d3197ab30aefd1d256af8e070d37002))
* use SafePassword struct instead of String for passwords ([#4320](https://github.com/tari-project/tari/issues/4320)) ([a059b99](https://github.com/tari-project/tari/commit/a059b9988ed5fd9228c2110978c9af0f405c19c5))
* **validator-node:** only submit checkpoint if the leader ([#4294](https://github.com/tari-project/tari/issues/4294)) ([fd55107](https://github.com/tari-project/tari/commit/fd55107568fb829d478a66b46d366051a44fb7e6))
* wallet database encryption does not bind to field keys [#4137](https://github.com/tari-project/tari/issues/4137) ([#4340](https://github.com/tari-project/tari/issues/4340)) ([32184b5](https://github.com/tari-project/tari/commit/32184b515bfe428d7da1dbe14c79e8691ea815ae))
* wallet seed_words command is broken (see issue [#4363](https://github.com/tari-project/tari/issues/4363)) ([#4370](https://github.com/tari-project/tari/issues/4370)) ([1cabd70](https://github.com/tari-project/tari/commit/1cabd70f2a945fb15466cb1d2a2ed183fb021b1f))
* **wallet:** implement `check_for_updates` in wallet grpc ([#4359](https://github.com/tari-project/tari/issues/4359)) ([6eae661](https://github.com/tari-project/tari/commit/6eae6615e12287b60f9695fba7773ae76a1fc5e6))
* **wallet:** update seed words for output manager tests ([#4379](https://github.com/tari-project/tari/issues/4379)) ([bfcb95c](https://github.com/tari-project/tari/commit/bfcb95c177a7cc61f122eddf803b637dde19163f))

## [0.34.0](https://github.com/tari-project/tari/compare/v0.32.10...v0.34.0) (2022-07-08)


### ⚠ BREAKING CHANGES

* **core:** include issuer public key in contract id hash (#4239)
* **dan_layer:** generate and add checkpoint signatures (#4261)
* add checkpoint_number to checkpoint with basic base layer validations (#4258)

### Features

* add checkpoint_number to checkpoint with basic base layer validations ([#4258](https://github.com/tari-project/tari/issues/4258)) ([7b76141](https://github.com/tari-project/tari/commit/7b761410cd1dde2c47fd209d4b5e2a77f51aed96))
* add encryption service ([#4225](https://github.com/tari-project/tari/issues/4225)) ([6ce6b89](https://github.com/tari-project/tari/commit/6ce6b893df46d69a4177ef0130f841994e492a09))
* add range proof batch verification to validators ([#4260](https://github.com/tari-project/tari/issues/4260)) ([02d3121](https://github.com/tari-project/tari/commit/02d31212731d4a0643dac1f26afe241b4f5b9204))
* add tari engine for flow and wasm functions ([#4237](https://github.com/tari-project/tari/issues/4237)) ([a997934](https://github.com/tari-project/tari/commit/a99793424815e5b43eb67f7422cb42459636d7af))
* **base_layer:** basic checkpoint validation ([#4293](https://github.com/tari-project/tari/issues/4293)) ([045997a](https://github.com/tari-project/tari/commit/045997a0a141c4391efc98aeabfbe6d6e550367f))
* **comms:** add or_optional trait extension for RpcStatus ([#4246](https://github.com/tari-project/tari/issues/4246)) ([11fddf6](https://github.com/tari-project/tari/commit/11fddf6199af670fb4ccb34a99b89c49a42b336e))
* contract acceptance signatures are submitted and validated ([#4269](https://github.com/tari-project/tari/issues/4269)) ([414be33](https://github.com/tari-project/tari/commit/414be33351781c07358d3850e4e67b750c1fcb8a))
* **core:** validates non-contract utxos have no sidechain features ([#4259](https://github.com/tari-project/tari/issues/4259)) ([a8ba89f](https://github.com/tari-project/tari/commit/a8ba89fe2195232e7e860342617ddf5f6c6244c2))
* **dan_layer/core:** track checkpoint number for each checkpoint submitted ([#4268](https://github.com/tari-project/tari/issues/4268)) ([16e07a0](https://github.com/tari-project/tari/commit/16e07a0b4ab9079f84645d8796a4fc6bb27f0303))
* **dan_layer:** generate and add checkpoint signatures ([#4261](https://github.com/tari-project/tari/issues/4261)) ([0f581ca](https://github.com/tari-project/tari/commit/0f581cafe8bd4f922462757504c772c82d0697c7))
* **wallet:** uses tip height to calc abs acceptance period ([#4271](https://github.com/tari-project/tari/issues/4271)) ([480d55d](https://github.com/tari-project/tari/commit/480d55dade62339dafc457c98681efcb66304beb))


### Bug Fixes

* add saturating sub to prevent potential underflow ([#4286](https://github.com/tari-project/tari/issues/4286)) ([56d184a](https://github.com/tari-project/tari/commit/56d184a7c3c405028e38ef4640804ff3bcb37b1a))
* **base-node:** minor fixups for hex/type parsing and long running commands ([#4281](https://github.com/tari-project/tari/issues/4281)) ([f910cce](https://github.com/tari-project/tari/commit/f910cce13aa6ba3af021253bd922baddd43e885f))
* **core:** include issuer public key in contract id hash ([#4239](https://github.com/tari-project/tari/issues/4239)) ([ef62c00](https://github.com/tari-project/tari/commit/ef62c00b10cdf6dafe9e2b24acecfd2006c48125))
* **dan_layer/core:** include state root in checkpoint signature ([#4285](https://github.com/tari-project/tari/issues/4285)) ([bcaabf0](https://github.com/tari-project/tari/commit/bcaabf04f5cef05d7707293236fb29b1020fa3de))
* **vn:** scan and save contracts without autoaccept ([#4265](https://github.com/tari-project/tari/issues/4265)) ([a137f53](https://github.com/tari-project/tari/commit/a137f53f35db70031155f9c79a04fd11d8e1996f))
* **wallet:** handle not found rpc error in utxo scanning ([#4249](https://github.com/tari-project/tari/issues/4249)) ([bcd14c7](https://github.com/tari-project/tari/commit/bcd14c7dcbfc9c2bd63ec896c80d45785cf04714))

## [0.33.0](https://github.com/tari-project/tari/compare/v0.32.7...v0.33.0) (2022-06-30)


### ⚠ BREAKING CHANGES

* **core:** add contract index to blockchain database (#4184)
* **core:** replace OutputFlags with OutputType (#4174)
* **core:** add side-chain features and constitution to UTXOs (#4134)
* **comms:** commit to public key and nonce in identity sig (#3928)
* **dht:** fixes MAC related key vuln for propagated cleartext msgs (#3907)
* **core:** define OutputFlags for side-chain contracts (#4088)

### Features

* add an encrypted value to the TransactionOutput ([#4148](https://github.com/tari-project/tari/issues/4148)) ([01b600a](https://github.com/tari-project/tari/commit/01b600ae3756b02ad99ffad8c4d16e09e31ffa77))
* add encrypted_value to the UnblindedOutput ([#4142](https://github.com/tari-project/tari/issues/4142)) ([f79d383](https://github.com/tari-project/tari/commit/f79d383533c2e9c4db95d4a13973992c9a8739ef))
* add FeePerGramStats to ffi library ([#4114](https://github.com/tari-project/tari/issues/4114)) ([234d32f](https://github.com/tari-project/tari/commit/234d32f446d5f75c2af78b8e30bc818a628b1dfb))
* add sender to instructions ([#4234](https://github.com/tari-project/tari/issues/4234)) ([6c116ac](https://github.com/tari-project/tari/commit/6c116acae93eff0869cc82fa18b9342624da6914))
* add validator node checkpointing ([#4217](https://github.com/tari-project/tari/issues/4217)) ([8b0add0](https://github.com/tari-project/tari/commit/8b0add0b53011de30253337a6830f3b9c66251b8))
* **base_layer:** basic contract constitution validation ([#4232](https://github.com/tari-project/tari/issues/4232)) ([c2efd5e](https://github.com/tari-project/tari/commit/c2efd5e161176d7c66a6669fef2625bf77d2eb82))
* **base_layer:** basic validations for proposals, proposal acceptances and amendments ([#4238](https://github.com/tari-project/tari/issues/4238)) ([64f8972](https://github.com/tari-project/tari/commit/64f89724896c6ddd7b21efc8c8ff605cbc373f70))
* **base_layer:** validate acceptance window expiration on dan layer ([#4251](https://github.com/tari-project/tari/issues/4251)) ([25e316b](https://github.com/tari-project/tari/commit/25e316b68d270abf917cad660d031a9feb168ae4))
* **base_layer:** validate duplicated acceptances ([#4233](https://github.com/tari-project/tari/issues/4233)) ([3d8a3b2](https://github.com/tari-project/tari/commit/3d8a3b2c09b375b7af59d52f1462a93801beff07))
* **base_layer:** validate that contract definitions are not duplicated ([#4230](https://github.com/tari-project/tari/issues/4230)) ([0a2812c](https://github.com/tari-project/tari/commit/0a2812c165be76fd177f0563c802c3afb43d0215))
* **base_layer:** validation of committee membership in contract acceptances ([#4221](https://github.com/tari-project/tari/issues/4221)) ([641844a](https://github.com/tari-project/tari/commit/641844a749e043ad708debeaebc25b8c4c8adaa6))
* **base-node:** improve contract utxo scanning ([#4208](https://github.com/tari-project/tari/issues/4208)) ([0fcde31](https://github.com/tari-project/tari/commit/0fcde31bdf81e27b92bf3d44dc563c4cf23fd38f))
* change tari explorer block view ([#4226](https://github.com/tari-project/tari/issues/4226)) ([652cba3](https://github.com/tari-project/tari/commit/652cba36a584f208b1782c95057d3839a6317d04))
* **console-wallet:** add contract-definition init command ([#4164](https://github.com/tari-project/tari/issues/4164)) ([8685e2f](https://github.com/tari-project/tari/commit/8685e2fe3b174a00047d049acca998df3c85975c))
* **console-wallet:** generate issuer key for contract init-definition ([#4202](https://github.com/tari-project/tari/issues/4202)) ([7317d6b](https://github.com/tari-project/tari/commit/7317d6ba858dd0b54fb0a39cf5c0c1999042cb7b))
* constitution publishing ([#4150](https://github.com/tari-project/tari/issues/4150)) ([ba83b8f](https://github.com/tari-project/tari/commit/ba83b8f0aaa833dafe2e89c82ab802e35eb190a2))
* contract acceptance publication ([#4151](https://github.com/tari-project/tari/issues/4151)) ([d3d3e91](https://github.com/tari-project/tari/commit/d3d3e91c80b0bc2a6adeb796c2500aa88f1f49cb))
* contract auto acceptance ([#4177](https://github.com/tari-project/tari/issues/4177)) ([87f9969](https://github.com/tari-project/tari/commit/87f996923f138f953198a2f16c11a180cca5134d))
* **core:** add contract acceptance utxo features ([#4145](https://github.com/tari-project/tari/issues/4145)) ([2636cb5](https://github.com/tari-project/tari/commit/2636cb56ddf1b67ed7bf4c4aea8c05f9369b11d0))
* **core:** add contract index to blockchain database ([#4184](https://github.com/tari-project/tari/issues/4184)) ([b7e97f4](https://github.com/tari-project/tari/commit/b7e97f45d3b3b7407058d6bb8da89f6f14f98984))
* **core:** add side-chain features and constitution to UTXOs ([#4134](https://github.com/tari-project/tari/issues/4134)) ([ada3143](https://github.com/tari-project/tari/commit/ada31432ea2e0ac1591153580b0e2b86475b30e7))
* **core:** adds constitution UTXO features ([#4121](https://github.com/tari-project/tari/issues/4121)) ([da5696a](https://github.com/tari-project/tari/commit/da5696a69a7568e744681d5139dbc4fe81031644))
* **core:** define OutputFlags for side-chain contracts ([#4088](https://github.com/tari-project/tari/issues/4088)) ([50993a3](https://github.com/tari-project/tari/commit/50993a3dc0aaf8506ef21a90c45a2a56d801716a))
* **core:** impl consensus encoding for bool ([#4120](https://github.com/tari-project/tari/issues/4120)) ([682aa5d](https://github.com/tari-project/tari/commit/682aa5d0ec108074ffed68aead83a757ee5c9490))
* **core:** new output features for changes in contracts ([#4169](https://github.com/tari-project/tari/issues/4169)) ([41570f6](https://github.com/tari-project/tari/commit/41570f6f159776aaf99a504715ee4af31919f1b7))
* **miner:** friendlier miner output ([#4219](https://github.com/tari-project/tari/issues/4219)) ([4245838](https://github.com/tari-project/tari/commit/42458381105df4ca2b54b3e6510423dc775bde9e))
* publication of contract update proposal acceptances ([#4199](https://github.com/tari-project/tari/issues/4199)) ([e3b2b9b](https://github.com/tari-project/tari/commit/e3b2b9b5bbbc8bced1228832202f4932012f6a6e))
* scan base node for constitutions ([#4144](https://github.com/tari-project/tari/issues/4144)) ([310a2d2](https://github.com/tari-project/tari/commit/310a2d20267f0c0226a76c9ba0b56864569621cb))
* swap dalek bulletproofs for bulletproofs-plus ([#4213](https://github.com/tari-project/tari/issues/4213)) ([46f9bb8](https://github.com/tari-project/tari/commit/46f9bb8359295a2c0432c304ec20c2a4498fa31d))
* use tari_crypto's updated "extended pedersen commitment factory" ([#4206](https://github.com/tari-project/tari/issues/4206)) ([50ce20a](https://github.com/tari-project/tari/commit/50ce20a3b13647a841e4cbfac44837a78a623dcd))
* **validator_node:** add global db ([#4210](https://github.com/tari-project/tari/issues/4210)) ([3965267](https://github.com/tari-project/tari/commit/3965267c53b60e26d8f8effc852107fec4ab3111))
* **validator-node:** add logging ([#4189](https://github.com/tari-project/tari/issues/4189)) ([2ed859f](https://github.com/tari-project/tari/commit/2ed859f22e0436a4e27d6508560c9122746c0e85))
* **validator-node:** allow network to be configured via cli ([#4190](https://github.com/tari-project/tari/issues/4190)) ([6a4c1a4](https://github.com/tari-project/tari/commit/6a4c1a4a3b3014f988d8a1cb31e926bc4d743a68))
* **vn:** record contract states ([#4241](https://github.com/tari-project/tari/issues/4241)) ([92ae4ab](https://github.com/tari-project/tari/commit/92ae4abf2d59e675f2f6c48df053e3273076fbd8))
* wallet selects previous checkpoint for spending ([#4236](https://github.com/tari-project/tari/issues/4236)) ([90a5ec3](https://github.com/tari-project/tari/commit/90a5ec32bd4f746b29f06d70bc9737a9cacf4538))
* **wallet_ffi:** new ffi method to create covenant ([#4115](https://github.com/tari-project/tari/issues/4115)) ([dd65b4b](https://github.com/tari-project/tari/commit/dd65b4bd8b168b9423cd953f5e089b5723dbb747))
* **wallet_ffi:** new ffi method to create output features ([#4109](https://github.com/tari-project/tari/issues/4109)) ([f8fa3ec](https://github.com/tari-project/tari/commit/f8fa3ecb5700e80adf63cc3e61f0b8367217f1bc))
* **wallet:** add help for wallet cli commands ([#4162](https://github.com/tari-project/tari/issues/4162)) ([859b7d3](https://github.com/tari-project/tari/commit/859b7d3022dab60732f5e638b52f8c1237a2a8f4))
* **wallet:** adds contract_id to outputs db ([#4222](https://github.com/tari-project/tari/issues/4222)) ([6f331f8](https://github.com/tari-project/tari/commit/6f331f877b0c41336f73c42440facb32953fa59b))
* **wallet:** new cli commands to initialise proposals and amendments ([#4205](https://github.com/tari-project/tari/issues/4205)) ([40cbd50](https://github.com/tari-project/tari/commit/40cbd50e319e77b037931b8cc33f6b87cf174488))
* **wallet:** new command to publish a contract definition transaction ([#4133](https://github.com/tari-project/tari/issues/4133)) ([b4991a4](https://github.com/tari-project/tari/commit/b4991a471cb3a6db2a54b623c0afc09f71ae3dc4))
* **wallet:** new command to publish a contract update proposal ([#4188](https://github.com/tari-project/tari/issues/4188)) ([0e3bee0](https://github.com/tari-project/tari/commit/0e3bee06a08760b3fb61c2896a52b53a86d7e4a9))
* **wallet:** publish contract amendment ([#4200](https://github.com/tari-project/tari/issues/4200)) ([edcce4a](https://github.com/tari-project/tari/commit/edcce4a816102929284285d0f8cdb04fe7006c76))


### Bug Fixes

* add prettierignore for partials ([#4229](https://github.com/tari-project/tari/issues/4229)) ([923cf07](https://github.com/tari-project/tari/commit/923cf0765581c9e0c471cfff00886015a2e827bb))
* **ci:** sort .license.ignore locally before diff ([#4106](https://github.com/tari-project/tari/issues/4106)) ([8594754](https://github.com/tari-project/tari/commit/859475438219b6ace16e6b2437522788d0c7d737))
* **comms:** commit to public key and nonce in identity sig ([#3928](https://github.com/tari-project/tari/issues/3928)) ([5ac6133](https://github.com/tari-project/tari/commit/5ac6133a8ab0707dfd97cf1647d709256bb9c05b))
* **contract-index:** adds support for ContractAmendment to contract index ([#4214](https://github.com/tari-project/tari/issues/4214)) ([a41d0c9](https://github.com/tari-project/tari/commit/a41d0c92cffa734406dad50820ef1367f24ae133))
* **core:** cleanup duplicate maturity check ([#4181](https://github.com/tari-project/tari/issues/4181)) ([5e55bf2](https://github.com/tari-project/tari/commit/5e55bf22110ac40ffc0dea88d88ba836982591eb))
* **core:** don't allow coinbase transactions in mempool ([#4103](https://github.com/tari-project/tari/issues/4103)) ([46450d5](https://github.com/tari-project/tari/commit/46450d5a475fa8b57107f6806962a4c9a1338ac5))
* **dht:** fixes MAC related key vuln for propagated cleartext msgs ([#3907](https://github.com/tari-project/tari/issues/3907)) ([1e96d45](https://github.com/tari-project/tari/commit/1e96d45535f4af967a761fd71521eb68bbb1b371))
* hash in cucumber ([#4124](https://github.com/tari-project/tari/issues/4124)) ([5d7d55d](https://github.com/tari-project/tari/commit/5d7d55d97f1251619911e4555a925cc03b50c7ed))
* **hotstuff:** fix bug where decide state was listening for wrong message ([#4160](https://github.com/tari-project/tari/issues/4160)) ([fe7b304](https://github.com/tari-project/tari/commit/fe7b304e936dc567ca106c86dfeb7ed403807b04))
* **integration_test:** fix wallet-cli integration tests ([#4132](https://github.com/tari-project/tari/issues/4132)) ([4464064](https://github.com/tari-project/tari/commit/446406491698e97983143036c2ea9dd0ac10b365))
* move peer dbs into sub folders ([#4147](https://github.com/tari-project/tari/issues/4147)) ([2b1a69a](https://github.com/tari-project/tari/commit/2b1a69a9219f29472d6fa26b1b2350be7880b11a))
* **test:** integration test for validator node is broken ([#4192](https://github.com/tari-project/tari/issues/4192)) ([16d6ba5](https://github.com/tari-project/tari/commit/16d6ba5403e4a0e62e676e56c8ab755a69e6e1f0))
* **test:** unifying dan layer integration tests ([#4175](https://github.com/tari-project/tari/issues/4175)) ([f3495ee](https://github.com/tari-project/tari/commit/f3495ee71fbb83edef9b295b42a34b6dfae87acf))
* **validator-node:** return error if contract_id empty for publish_contract_acceptance grpc ([#4191](https://github.com/tari-project/tari/issues/4191)) ([8874114](https://github.com/tari-project/tari/commit/8874114bb25232e62e539e600fe082443a476fec))
* **validator:** set tor_identity base path ([#4187](https://github.com/tari-project/tari/issues/4187)) ([e324b80](https://github.com/tari-project/tari/commit/e324b803f748e862796210226ca31906613bde28))
* **vn scanning:** only scan since last scan and restart accepted contracts ([#4252](https://github.com/tari-project/tari/issues/4252)) ([43b4a53](https://github.com/tari-project/tari/commit/43b4a534609059e4a717ec7d1c69650b7fe2a5d3))
* **wallet:** select only basic utxos when building a transaction ([#4178](https://github.com/tari-project/tari/issues/4178)) ([42269ae](https://github.com/tari-project/tari/commit/42269ae48e9a8eb1ebc479a22813bf2f8cf0c22b))
* **wallet:** use correct type for contract_id in the contract constitution file format ([#4179](https://github.com/tari-project/tari/issues/4179)) ([669a1bd](https://github.com/tari-project/tari/commit/669a1bd45fd68615da037886379e96b89b9f4f76))


* **core:** replace OutputFlags with OutputType ([#4174](https://github.com/tari-project/tari/issues/4174)) ([d779f43](https://github.com/tari-project/tari/commit/d779f4311a0415b3ecd98e806bfbf27fc2486412))

## [0.34.0](https://github.com/tari-project/tari/compare/v0.33.0...v0.34.0) (2022-07-08)


### ⚠ BREAKING CHANGES

* **core:** include issuer public key in contract id hash (#4239)
* **dan_layer:** generate and add checkpoint signatures (#4261)
* add checkpoint_number to checkpoint with basic base layer validations (#4258)

### Features

* add checkpoint_number to checkpoint with basic base layer validations ([#4258](https://github.com/tari-project/tari/issues/4258)) ([7b76141](https://github.com/tari-project/tari/commit/7b761410cd1dde2c47fd209d4b5e2a77f51aed96))
* add encryption service ([#4225](https://github.com/tari-project/tari/issues/4225)) ([6ce6b89](https://github.com/tari-project/tari/commit/6ce6b893df46d69a4177ef0130f841994e492a09))
* add range proof batch verification to validators ([#4260](https://github.com/tari-project/tari/issues/4260)) ([02d3121](https://github.com/tari-project/tari/commit/02d31212731d4a0643dac1f26afe241b4f5b9204))
* add tari engine for flow and wasm functions ([#4237](https://github.com/tari-project/tari/issues/4237)) ([a997934](https://github.com/tari-project/tari/commit/a99793424815e5b43eb67f7422cb42459636d7af))
* **base_layer:** basic checkpoint validation ([#4293](https://github.com/tari-project/tari/issues/4293)) ([045997a](https://github.com/tari-project/tari/commit/045997a0a141c4391efc98aeabfbe6d6e550367f))
* **comms:** add or_optional trait extension for RpcStatus ([#4246](https://github.com/tari-project/tari/issues/4246)) ([11fddf6](https://github.com/tari-project/tari/commit/11fddf6199af670fb4ccb34a99b89c49a42b336e))
* contract acceptance signatures are submitted and validated ([#4269](https://github.com/tari-project/tari/issues/4269)) ([414be33](https://github.com/tari-project/tari/commit/414be33351781c07358d3850e4e67b750c1fcb8a))
* **core:** validates non-contract utxos have no sidechain features ([#4259](https://github.com/tari-project/tari/issues/4259)) ([a8ba89f](https://github.com/tari-project/tari/commit/a8ba89fe2195232e7e860342617ddf5f6c6244c2))
* **dan_layer/core:** track checkpoint number for each checkpoint submitted ([#4268](https://github.com/tari-project/tari/issues/4268)) ([16e07a0](https://github.com/tari-project/tari/commit/16e07a0b4ab9079f84645d8796a4fc6bb27f0303))
* **dan_layer:** generate and add checkpoint signatures ([#4261](https://github.com/tari-project/tari/issues/4261)) ([0f581ca](https://github.com/tari-project/tari/commit/0f581cafe8bd4f922462757504c772c82d0697c7))
* **wallet:** uses tip height to calc abs acceptance period ([#4271](https://github.com/tari-project/tari/issues/4271)) ([480d55d](https://github.com/tari-project/tari/commit/480d55dade62339dafc457c98681efcb66304beb))


### Bug Fixes

* add saturating sub to prevent potential underflow ([#4286](https://github.com/tari-project/tari/issues/4286)) ([56d184a](https://github.com/tari-project/tari/commit/56d184a7c3c405028e38ef4640804ff3bcb37b1a))
* **base-node:** minor fixups for hex/type parsing and long running commands ([#4281](https://github.com/tari-project/tari/issues/4281)) ([f910cce](https://github.com/tari-project/tari/commit/f910cce13aa6ba3af021253bd922baddd43e885f))
* **core:** include issuer public key in contract id hash ([#4239](https://github.com/tari-project/tari/issues/4239)) ([ef62c00](https://github.com/tari-project/tari/commit/ef62c00b10cdf6dafe9e2b24acecfd2006c48125))
* **dan_layer/core:** include state root in checkpoint signature ([#4285](https://github.com/tari-project/tari/issues/4285)) ([bcaabf0](https://github.com/tari-project/tari/commit/bcaabf04f5cef05d7707293236fb29b1020fa3de))
* **vn:** scan and save contracts without autoaccept ([#4265](https://github.com/tari-project/tari/issues/4265)) ([a137f53](https://github.com/tari-project/tari/commit/a137f53f35db70031155f9c79a04fd11d8e1996f))
* **wallet:** handle not found rpc error in utxo scanning ([#4249](https://github.com/tari-project/tari/issues/4249)) ([bcd14c7](https://github.com/tari-project/tari/commit/bcd14c7dcbfc9c2bd63ec896c80d45785cf04714))

## [0.33.0](https://github.com/tari-project/tari/compare/v0.32.5...v0.33.0) (2022-06-30)


### ⚠ BREAKING CHANGES

* **core:** add contract index to blockchain database (#4184)
* **core:** replace OutputFlags with OutputType (#4174)
* **core:** add side-chain features and constitution to UTXOs (#4134)
* **comms:** commit to public key and nonce in identity sig (#3928)
* **dht:** fixes MAC related key vuln for propagated cleartext msgs (#3907)
* **core:** define OutputFlags for side-chain contracts (#4088)

### Features

* add an encrypted value to the TransactionOutput ([#4148](https://github.com/tari-project/tari/issues/4148)) ([01b600a](https://github.com/tari-project/tari/commit/01b600ae3756b02ad99ffad8c4d16e09e31ffa77))
* add encrypted_value to the UnblindedOutput ([#4142](https://github.com/tari-project/tari/issues/4142)) ([f79d383](https://github.com/tari-project/tari/commit/f79d383533c2e9c4db95d4a13973992c9a8739ef))
* add FeePerGramStats to ffi library ([#4114](https://github.com/tari-project/tari/issues/4114)) ([234d32f](https://github.com/tari-project/tari/commit/234d32f446d5f75c2af78b8e30bc818a628b1dfb))
* add sender to instructions ([#4234](https://github.com/tari-project/tari/issues/4234)) ([6c116ac](https://github.com/tari-project/tari/commit/6c116acae93eff0869cc82fa18b9342624da6914))
* add validator node checkpointing ([#4217](https://github.com/tari-project/tari/issues/4217)) ([8b0add0](https://github.com/tari-project/tari/commit/8b0add0b53011de30253337a6830f3b9c66251b8))
* **base_layer:** basic contract constitution validation ([#4232](https://github.com/tari-project/tari/issues/4232)) ([c2efd5e](https://github.com/tari-project/tari/commit/c2efd5e161176d7c66a6669fef2625bf77d2eb82))
* **base_layer:** basic validations for proposals, proposal acceptances and amendments ([#4238](https://github.com/tari-project/tari/issues/4238)) ([64f8972](https://github.com/tari-project/tari/commit/64f89724896c6ddd7b21efc8c8ff605cbc373f70))
* **base_layer:** validate acceptance window expiration on dan layer ([#4251](https://github.com/tari-project/tari/issues/4251)) ([25e316b](https://github.com/tari-project/tari/commit/25e316b68d270abf917cad660d031a9feb168ae4))
* **base_layer:** validate duplicated acceptances ([#4233](https://github.com/tari-project/tari/issues/4233)) ([3d8a3b2](https://github.com/tari-project/tari/commit/3d8a3b2c09b375b7af59d52f1462a93801beff07))
* **base_layer:** validate that contract definitions are not duplicated ([#4230](https://github.com/tari-project/tari/issues/4230)) ([0a2812c](https://github.com/tari-project/tari/commit/0a2812c165be76fd177f0563c802c3afb43d0215))
* **base_layer:** validation of committee membership in contract acceptances ([#4221](https://github.com/tari-project/tari/issues/4221)) ([641844a](https://github.com/tari-project/tari/commit/641844a749e043ad708debeaebc25b8c4c8adaa6))
* **base-node:** improve contract utxo scanning ([#4208](https://github.com/tari-project/tari/issues/4208)) ([0fcde31](https://github.com/tari-project/tari/commit/0fcde31bdf81e27b92bf3d44dc563c4cf23fd38f))
* change tari explorer block view ([#4226](https://github.com/tari-project/tari/issues/4226)) ([652cba3](https://github.com/tari-project/tari/commit/652cba36a584f208b1782c95057d3839a6317d04))
* **ci:** build both x86/arm64 docker images from GHA  ([#4204](https://github.com/tari-project/tari/issues/4204)) ([28a8f8b](https://github.com/tari-project/tari/commit/28a8f8b541f96d2bee4bd7f46cc1625dfeb0d323))
* **console-wallet:** add contract-definition init command ([#4164](https://github.com/tari-project/tari/issues/4164)) ([8685e2f](https://github.com/tari-project/tari/commit/8685e2fe3b174a00047d049acca998df3c85975c))
* **console-wallet:** generate issuer key for contract init-definition ([#4202](https://github.com/tari-project/tari/issues/4202)) ([7317d6b](https://github.com/tari-project/tari/commit/7317d6ba858dd0b54fb0a39cf5c0c1999042cb7b))
* constitution publishing ([#4150](https://github.com/tari-project/tari/issues/4150)) ([ba83b8f](https://github.com/tari-project/tari/commit/ba83b8f0aaa833dafe2e89c82ab802e35eb190a2))
* contract acceptance publication ([#4151](https://github.com/tari-project/tari/issues/4151)) ([d3d3e91](https://github.com/tari-project/tari/commit/d3d3e91c80b0bc2a6adeb796c2500aa88f1f49cb))
* contract auto acceptance ([#4177](https://github.com/tari-project/tari/issues/4177)) ([87f9969](https://github.com/tari-project/tari/commit/87f996923f138f953198a2f16c11a180cca5134d))
* **core:** add contract acceptance utxo features ([#4145](https://github.com/tari-project/tari/issues/4145)) ([2636cb5](https://github.com/tari-project/tari/commit/2636cb56ddf1b67ed7bf4c4aea8c05f9369b11d0))
* **core:** add contract index to blockchain database ([#4184](https://github.com/tari-project/tari/issues/4184)) ([b7e97f4](https://github.com/tari-project/tari/commit/b7e97f45d3b3b7407058d6bb8da89f6f14f98984))
* **core:** add side-chain features and constitution to UTXOs ([#4134](https://github.com/tari-project/tari/issues/4134)) ([ada3143](https://github.com/tari-project/tari/commit/ada31432ea2e0ac1591153580b0e2b86475b30e7))
* **core:** adds constitution UTXO features ([#4121](https://github.com/tari-project/tari/issues/4121)) ([da5696a](https://github.com/tari-project/tari/commit/da5696a69a7568e744681d5139dbc4fe81031644))
* **core:** define OutputFlags for side-chain contracts ([#4088](https://github.com/tari-project/tari/issues/4088)) ([50993a3](https://github.com/tari-project/tari/commit/50993a3dc0aaf8506ef21a90c45a2a56d801716a))
* **core:** impl consensus encoding for bool ([#4120](https://github.com/tari-project/tari/issues/4120)) ([682aa5d](https://github.com/tari-project/tari/commit/682aa5d0ec108074ffed68aead83a757ee5c9490))
* **core:** new output features for changes in contracts ([#4169](https://github.com/tari-project/tari/issues/4169)) ([41570f6](https://github.com/tari-project/tari/commit/41570f6f159776aaf99a504715ee4af31919f1b7))
* **miner:** friendlier miner output ([#4219](https://github.com/tari-project/tari/issues/4219)) ([4245838](https://github.com/tari-project/tari/commit/42458381105df4ca2b54b3e6510423dc775bde9e))
* publication of contract update proposal acceptances ([#4199](https://github.com/tari-project/tari/issues/4199)) ([e3b2b9b](https://github.com/tari-project/tari/commit/e3b2b9b5bbbc8bced1228832202f4932012f6a6e))
* scan base node for constitutions ([#4144](https://github.com/tari-project/tari/issues/4144)) ([310a2d2](https://github.com/tari-project/tari/commit/310a2d20267f0c0226a76c9ba0b56864569621cb))
* swap dalek bulletproofs for bulletproofs-plus ([#4213](https://github.com/tari-project/tari/issues/4213)) ([46f9bb8](https://github.com/tari-project/tari/commit/46f9bb8359295a2c0432c304ec20c2a4498fa31d))
* use tari_crypto's updated "extended pedersen commitment factory" ([#4206](https://github.com/tari-project/tari/issues/4206)) ([50ce20a](https://github.com/tari-project/tari/commit/50ce20a3b13647a841e4cbfac44837a78a623dcd))
* **validator_node:** add global db ([#4210](https://github.com/tari-project/tari/issues/4210)) ([3965267](https://github.com/tari-project/tari/commit/3965267c53b60e26d8f8effc852107fec4ab3111))
* **validator-node:** add logging ([#4189](https://github.com/tari-project/tari/issues/4189)) ([2ed859f](https://github.com/tari-project/tari/commit/2ed859f22e0436a4e27d6508560c9122746c0e85))
* **validator-node:** allow network to be configured via cli ([#4190](https://github.com/tari-project/tari/issues/4190)) ([6a4c1a4](https://github.com/tari-project/tari/commit/6a4c1a4a3b3014f988d8a1cb31e926bc4d743a68))
* **vn:** record contract states ([#4241](https://github.com/tari-project/tari/issues/4241)) ([92ae4ab](https://github.com/tari-project/tari/commit/92ae4abf2d59e675f2f6c48df053e3273076fbd8))
* wallet selects previous checkpoint for spending ([#4236](https://github.com/tari-project/tari/issues/4236)) ([90a5ec3](https://github.com/tari-project/tari/commit/90a5ec32bd4f746b29f06d70bc9737a9cacf4538))
* **wallet_ffi:** new ffi method to create covenant ([#4115](https://github.com/tari-project/tari/issues/4115)) ([dd65b4b](https://github.com/tari-project/tari/commit/dd65b4bd8b168b9423cd953f5e089b5723dbb747))
* **wallet_ffi:** new ffi method to create output features ([#4109](https://github.com/tari-project/tari/issues/4109)) ([f8fa3ec](https://github.com/tari-project/tari/commit/f8fa3ecb5700e80adf63cc3e61f0b8367217f1bc))
* **wallet:** add help for wallet cli commands ([#4162](https://github.com/tari-project/tari/issues/4162)) ([859b7d3](https://github.com/tari-project/tari/commit/859b7d3022dab60732f5e638b52f8c1237a2a8f4))
* **wallet:** adds contract_id to outputs db ([#4222](https://github.com/tari-project/tari/issues/4222)) ([6f331f8](https://github.com/tari-project/tari/commit/6f331f877b0c41336f73c42440facb32953fa59b))
* **wallet:** allow UTXO selection by specific outputs and by token ([#4227](https://github.com/tari-project/tari/issues/4227)) ([f2a7e18](https://github.com/tari-project/tari/commit/f2a7e1846341a69ddea6eb3541467e82e1bf2e47))
* **wallet:** new cli commands to initialise proposals and amendments ([#4205](https://github.com/tari-project/tari/issues/4205)) ([40cbd50](https://github.com/tari-project/tari/commit/40cbd50e319e77b037931b8cc33f6b87cf174488))
* **wallet:** new command to publish a contract definition transaction ([#4133](https://github.com/tari-project/tari/issues/4133)) ([b4991a4](https://github.com/tari-project/tari/commit/b4991a471cb3a6db2a54b623c0afc09f71ae3dc4))
* **wallet:** new command to publish a contract update proposal ([#4188](https://github.com/tari-project/tari/issues/4188)) ([0e3bee0](https://github.com/tari-project/tari/commit/0e3bee06a08760b3fb61c2896a52b53a86d7e4a9))
* **wallet:** publish contract amendment ([#4200](https://github.com/tari-project/tari/issues/4200)) ([edcce4a](https://github.com/tari-project/tari/commit/edcce4a816102929284285d0f8cdb04fe7006c76))


### Bug Fixes

* add prettierignore for partials ([#4229](https://github.com/tari-project/tari/issues/4229)) ([923cf07](https://github.com/tari-project/tari/commit/923cf0765581c9e0c471cfff00886015a2e827bb))
* **ci:** sort .license.ignore locally before diff ([#4106](https://github.com/tari-project/tari/issues/4106)) ([8594754](https://github.com/tari-project/tari/commit/859475438219b6ace16e6b2437522788d0c7d737))
* **comms:** commit to public key and nonce in identity sig ([#3928](https://github.com/tari-project/tari/issues/3928)) ([5ac6133](https://github.com/tari-project/tari/commit/5ac6133a8ab0707dfd97cf1647d709256bb9c05b))
* **contract-index:** adds support for ContractAmendment to contract index ([#4214](https://github.com/tari-project/tari/issues/4214)) ([a41d0c9](https://github.com/tari-project/tari/commit/a41d0c92cffa734406dad50820ef1367f24ae133))
* **core:** cleanup duplicate maturity check ([#4181](https://github.com/tari-project/tari/issues/4181)) ([5e55bf2](https://github.com/tari-project/tari/commit/5e55bf22110ac40ffc0dea88d88ba836982591eb))
* **core:** don't allow coinbase transactions in mempool ([#4103](https://github.com/tari-project/tari/issues/4103)) ([46450d5](https://github.com/tari-project/tari/commit/46450d5a475fa8b57107f6806962a4c9a1338ac5))
* **dht:** fixes MAC related key vuln for propagated cleartext msgs ([#3907](https://github.com/tari-project/tari/issues/3907)) ([1e96d45](https://github.com/tari-project/tari/commit/1e96d45535f4af967a761fd71521eb68bbb1b371))
* hash in cucumber ([#4124](https://github.com/tari-project/tari/issues/4124)) ([5d7d55d](https://github.com/tari-project/tari/commit/5d7d55d97f1251619911e4555a925cc03b50c7ed))
* **hotstuff:** fix bug where decide state was listening for wrong message ([#4160](https://github.com/tari-project/tari/issues/4160)) ([fe7b304](https://github.com/tari-project/tari/commit/fe7b304e936dc567ca106c86dfeb7ed403807b04))
* **integration_test:** fix wallet-cli integration tests ([#4132](https://github.com/tari-project/tari/issues/4132)) ([4464064](https://github.com/tari-project/tari/commit/446406491698e97983143036c2ea9dd0ac10b365))
* move peer dbs into sub folders ([#4147](https://github.com/tari-project/tari/issues/4147)) ([2b1a69a](https://github.com/tari-project/tari/commit/2b1a69a9219f29472d6fa26b1b2350be7880b11a))
* **test:** integration test for validator node is broken ([#4192](https://github.com/tari-project/tari/issues/4192)) ([16d6ba5](https://github.com/tari-project/tari/commit/16d6ba5403e4a0e62e676e56c8ab755a69e6e1f0))
* **test:** unifying dan layer integration tests ([#4175](https://github.com/tari-project/tari/issues/4175)) ([f3495ee](https://github.com/tari-project/tari/commit/f3495ee71fbb83edef9b295b42a34b6dfae87acf))
* **validator-node:** return error if contract_id empty for publish_contract_acceptance grpc ([#4191](https://github.com/tari-project/tari/issues/4191)) ([8874114](https://github.com/tari-project/tari/commit/8874114bb25232e62e539e600fe082443a476fec))
* **validator:** set tor_identity base path ([#4187](https://github.com/tari-project/tari/issues/4187)) ([e324b80](https://github.com/tari-project/tari/commit/e324b803f748e862796210226ca31906613bde28))
* **vn scanning:** only scan since last scan and restart accepted contracts ([#4252](https://github.com/tari-project/tari/issues/4252)) ([43b4a53](https://github.com/tari-project/tari/commit/43b4a534609059e4a717ec7d1c69650b7fe2a5d3))
* **wallet:** select only basic utxos when building a transaction ([#4178](https://github.com/tari-project/tari/issues/4178)) ([42269ae](https://github.com/tari-project/tari/commit/42269ae48e9a8eb1ebc479a22813bf2f8cf0c22b))
* **wallet:** use correct type for contract_id in the contract constitution file format ([#4179](https://github.com/tari-project/tari/issues/4179)) ([669a1bd](https://github.com/tari-project/tari/commit/669a1bd45fd68615da037886379e96b89b9f4f76))


* **core:** replace OutputFlags with OutputType ([#4174](https://github.com/tari-project/tari/issues/4174)) ([d779f43](https://github.com/tari-project/tari/commit/d779f4311a0415b3ecd98e806bfbf27fc2486412))

### [0.32.12](https://github.com/tari-project/tari/compare/v0.32.11...v0.32.12) (2022-07-11)


### Bug Fixes

* cbindgen fix ([#4298](https://github.com/tari-project/tari/issues/4298)) ([2744d46](https://github.com/tari-project/tari/commit/2744d4601f6e3db461515d894314d25364faa59b))

### [0.32.11](https://github.com/tari-project/tari/compare/v0.32.10...v0.32.11) (2022-07-11)


### Bug Fixes

* fixed bug in wallet_coin_join ([#4290](https://github.com/tari-project/tari/issues/4290)) ([2f14c3c](https://github.com/tari-project/tari/commit/2f14c3c1e867eff785b5417a72bacedf17df5022))

### [0.32.10](https://github.com/tari-project/tari/compare/v0.32.9...v0.32.10) (2022-07-07)


### Features

* add mined_timestamp to wallet.db ([#4267](https://github.com/tari-project/tari/issues/4267)) ([c6c9832](https://github.com/tari-project/tari/commit/c6c9832f7ea72b648f4faeebf246677094033a19))
* **wallet_ffi:** added mined_timestamp to TariUtxo ([#4284](https://github.com/tari-project/tari/issues/4284)) ([6e1b3da](https://github.com/tari-project/tari/commit/6e1b3da20e1b0e54cf64ea9ee7cb8f930065f7c3))


### Bug Fixes

* improve GHA docker image builds ([#4257](https://github.com/tari-project/tari/issues/4257)) ([2c01421](https://github.com/tari-project/tari/commit/2c0142121feed0d45ca57a5e19a2fed2aada62bc))
* removed code duplication in TariUtxo conversion ([#4283](https://github.com/tari-project/tari/issues/4283)) ([455d161](https://github.com/tari-project/tari/commit/455d161e42609a072878b9a99de5890753b828cb))

### [0.32.9](https://github.com/tari-project/tari/compare/v0.32.8...v0.32.9) (2022-07-06)


### Features

* **ffi:** added 3 functions ([#4266](https://github.com/tari-project/tari/issues/4266)) ([2be17df](https://github.com/tari-project/tari/commit/2be17dfa337d9175d56e0879b64f0dd4875bbe66))

### [0.32.8](https://github.com/tari-project/tari/compare/v0.32.7...v0.32.8) (2022-07-04)


### Features

* **wallet_ffi:** wallet_coin_split changes ([#4254](https://github.com/tari-project/tari/issues/4254)) ([d367f0b](https://github.com/tari-project/tari/commit/d367f0b56fc86df3662f1d01b5037f63163892a3))

### [0.32.7](https://github.com/tari-project/tari/compare/v0.32.6...v0.32.7) (2022-06-30)


### Features

* **wallet_ffi:** wallet_get_utxos, wallet_coin_join, wallet_coin_split ([#4244](https://github.com/tari-project/tari/issues/4244)) ([88931aa](https://github.com/tari-project/tari/commit/88931aa5eea3f574fda44b37bc4b973dd5a6e125))

### [0.32.6](https://github.com/tari-project/tari/compare/v0.32.5...v0.32.6) (2022-06-29)


### Features

* **ci:** build both x86/arm64 docker images from GHA  ([#4204](https://github.com/tari-project/tari/issues/4204)) ([28a8f8b](https://github.com/tari-project/tari/commit/28a8f8b541f96d2bee4bd7f46cc1625dfeb0d323))
* **wallet_ffi:** add coin join and split ([#4218](https://github.com/tari-project/tari/issues/4218)) ([af6c834](https://github.com/tari-project/tari/commit/af6c834c9e38984135152e664823a02857d09656))
* **wallet:** allow UTXO selection by specific outputs and by token ([#4227](https://github.com/tari-project/tari/issues/4227)) ([f2a7e18](https://github.com/tari-project/tari/commit/f2a7e1846341a69ddea6eb3541467e82e1bf2e47))

### [0.32.5](https://github.com/tari-project/tari/compare/v0.32.4...v0.32.5) (2022-06-20)


### Features

* generate wallet ffi header file automatically ([#4183](https://github.com/tari-project/tari/issues/4183)) ([665f1f7](https://github.com/tari-project/tari/commit/665f1f7cb33b4639a83e7682926c319cdf2ea501))
* **wallet_ffi:** wallet_get_utxos() ([#4209](https://github.com/tari-project/tari/issues/4209)) ([1b30524](https://github.com/tari-project/tari/commit/1b3052491f25efbd03249dd3300a296d7858c0f7))

### [0.32.4](https://github.com/tari-project/tari/compare/v0.32.3...v0.32.4) (2022-06-10)


### Features

* **daily-tests:** use environment variable to set custom seed words ([#4086](https://github.com/tari-project/tari/issues/4086)) ([3773bba](https://github.com/tari-project/tari/commit/3773bba3572360492640036bd40916bc6cea1f6b))
* **wallet-ffi:** adds FFI wallet_get_utxos() and test_wallet_get_utxos() ([#4180](https://github.com/tari-project/tari/issues/4180)) ([9770402](https://github.com/tari-project/tari/commit/9770402718d3a5e81aea1d88350cbc5abc07032b))


### Bug Fixes

* better message for failed software update check ([#4100](https://github.com/tari-project/tari/issues/4100)) ([f33a503](https://github.com/tari-project/tari/commit/f33a503cd5d0f49de9cb0e92b7eba893c39d9217))
* **block-sync:** check coinbase maturity ([#4168](https://github.com/tari-project/tari/issues/4168)) ([38b4af7](https://github.com/tari-project/tari/commit/38b4af7104780373e4ff1eddf1e6b19f479b0ae2))
* **dht:** optimisation, no decrypt if public key dest doesn't match ([#4131](https://github.com/tari-project/tari/issues/4131)) ([987972c](https://github.com/tari-project/tari/commit/987972cffd577b03a8395ce1d5e441c35fb6b109))

### [0.32.3](https://github.com/tari-project/tari/compare/v0.32.1...v0.32.2) (2022-05-23)


### Features

* loosen ffi import utxo requirements ([#4126](https://github.com/tari-project/tari/issues/4126)) ([83a6fd9](https://github.com/tari-project/tari/commit/83a6fd97fbb137d9d8a1eb21466c3dbdcc16aec2))

### [0.32.1](https://github.com/tari-project/tari/compare/v0.32.0...v0.32.1) (2022-05-20)


### Features

* add FeePerGramStats to ffi library ([#4114](https://github.com/tari-project/tari/issues/4114)) ([6f14034](https://github.com/tari-project/tari/commit/6f14034d89356a2b634e89a7e61c63332c206feb))
* add mempool fee per gram stats to transaction service ([#4105](https://github.com/tari-project/tari/issues/4105)) ([34fd58a](https://github.com/tari-project/tari/commit/34fd58a2ae1399b19956828bf71bab7469662977))
* **dht:** add feature to optionally bundle sqlite ([#4104](https://github.com/tari-project/tari/issues/4104)) ([498248e](https://github.com/tari-project/tari/commit/498248e2603fc3105bdb1c1233de522a82cf74a5))
* **wallet_ffi:** new ffi method to create covenant ([#4115](https://github.com/tari-project/tari/issues/4115)) ([121149a](https://github.com/tari-project/tari/commit/121149a72f33fae4853fa6dab746eae8485c161c))
* **wallet_ffi:** new ffi method to create output features (fixed flag size for dibbler) ([#4118](https://github.com/tari-project/tari/issues/4118)) ([cf3a1e6](https://github.com/tari-project/tari/commit/cf3a1e6e16ba3acf65d09e497a93091697130c46))


### Bug Fixes

* **core:** set reorg metrics on header sync rewind ([#4087](https://github.com/tari-project/tari/issues/4087)) ([354bae2](https://github.com/tari-project/tari/commit/354bae285630bc503abd856d27f87a6275f1f6e0))
* possible overflow in difficulty calculation (fixes [#3923](https://github.com/tari-project/tari/issues/3923)) ([#4090](https://github.com/tari-project/tari/issues/4090)) ([e8d1091](https://github.com/tari-project/tari/commit/e8d1091bc3200a304d5527eb47f97a7cf6b561c4))
* single allocation for block header consensus encoding ([#4101](https://github.com/tari-project/tari/issues/4101)) ([aa85738](https://github.com/tari-project/tari/commit/aa857383c5a32be84c269fde1703070e074ef2de))
* tx importing status to correctly reflect the origin ([#4119](https://github.com/tari-project/tari/issues/4119)) ([63a0df6](https://github.com/tari-project/tari/commit/63a0df62a2d4ed5edf910b039443cd697e6c03ab))
* **wallet header:** remove function argument ([#4092](https://github.com/tari-project/tari/issues/4092)) ([cf91e91](https://github.com/tari-project/tari/commit/cf91e915fafd6dd7319d0981b07832ef3cc58d35))
* **wallet:** add default value for wait stage ([#4089](https://github.com/tari-project/tari/issues/4089)) ([810d9ff](https://github.com/tari-project/tari/commit/810d9ff60ccc512419f5d42030ac13d6fe9af405))
* **wallet:** use network specified by cli/envvar in console wallet ([#4096](https://github.com/tari-project/tari/issues/4096)) ([7f2252a](https://github.com/tari-project/tari/commit/7f2252a453c86914e04afa836b0d1597fb453db8))

## [0.32.0](https://github.com/tari-project/tari/compare/v0.31.1...v0.32.0) (2022-05-10)


### ⚠ BREAKING CHANGES

* major config rework. Config files should be recreated  (#4006)

### Features

* allow network to be set by TARI_NETWORK env var ([#4073](https://github.com/tari-project/tari/issues/4073)) ([c27be5c](https://github.com/tari-project/tari/commit/c27be5cdf132c9c4f3a2806c070be1765b41fa61))
* **collectibles:** add list assets command ([#3908](https://github.com/tari-project/tari/issues/3908)) ([5b726a6](https://github.com/tari-project/tari/commit/5b726a6bc47bc2024ae743a772b67c17dee4f988))
* **merge mining proxy:** check achieved Monero difficulty before submitting to Tari basenode ([#4019](https://github.com/tari-project/tari/issues/4019)) ([b09fa76](https://github.com/tari-project/tari/commit/b09fa768e2538c2dfdaaee73cb31353b78c03ba3))
* **p2p:** adds tor.forward_address setting ([#4070](https://github.com/tari-project/tari/issues/4070)) ([8c78717](https://github.com/tari-project/tari/commit/8c7871761e8fb604092f462a4e8e76fa2a103b7a))
* **tari_explorer:** add total hashrate chart ([#4054](https://github.com/tari-project/tari/issues/4054)) ([9e0ec36](https://github.com/tari-project/tari/commit/9e0ec361f5da8b9fff3a95c1a8ec162dfef6601a))


### Bug Fixes

* add Environment cfg source and remove --create-id ([#4018](https://github.com/tari-project/tari/issues/4018)) ([e59e657](https://github.com/tari-project/tari/commit/e59e6577a98e927d6123b704077425e7807d2b60))
* **base-node:** assign correct base dir to tor identity ([#4081](https://github.com/tari-project/tari/issues/4081)) ([1464f8b](https://github.com/tari-project/tari/commit/1464f8b43fe3fd76faebfc556cec82e574e79deb))
* **dht:** saf storage uses constructs correct msg hash ([#4003](https://github.com/tari-project/tari/issues/4003)) ([e1e7669](https://github.com/tari-project/tari/commit/e1e7669f629ad8dd1c5a65106dff4de96d60fdab))
* **dht:** sets file default for DHT db ([#4030](https://github.com/tari-project/tari/issues/4030)) ([5b125e7](https://github.com/tari-project/tari/commit/5b125e702b9f6da79b0a4f8922dc002078470e46))
* **dns-seeds:** use correct cloudflare resolver default ([#4029](https://github.com/tari-project/tari/issues/4029)) ([c95e589](https://github.com/tari-project/tari/commit/c95e58963aa5bdd123d83e373dd0197adfda38e5))
* fix github integration tests ([#4008](https://github.com/tari-project/tari/issues/4008)) ([aa143c6](https://github.com/tari-project/tari/commit/aa143c6ae4b2b5e16924e353aafb1d3f75339228))
* github actions ([#4014](https://github.com/tari-project/tari/issues/4014)) ([a03392e](https://github.com/tari-project/tari/commit/a03392e4d79475a2602a8e3e64a68d34cf6327a5))
* ignore test that timeout in github action ([#4010](https://github.com/tari-project/tari/issues/4010)) ([6c5471e](https://github.com/tari-project/tari/commit/6c5471ed1a05b89bd451ff52007a07958ecd781a))
* **key-manager:** remove floating point math from mnemonic code ([#4064](https://github.com/tari-project/tari/issues/4064)) ([c2d60b3](https://github.com/tari-project/tari/commit/c2d60b363e87a244ade83b29359d88c6a56c377d))
* launchpad build docker images ([#4042](https://github.com/tari-project/tari/issues/4042)) ([50e2812](https://github.com/tari-project/tari/commit/50e281287f3456f50f64b10d0d6c4979f3cd472b))
* **launchpad:** fix config presets ([#4028](https://github.com/tari-project/tari/issues/4028)) ([1b8b274](https://github.com/tari-project/tari/commit/1b8b274786bdf759fca70b8293860c2c41cd5e31))
* long running and non critical github action ([#4009](https://github.com/tari-project/tari/issues/4009)) ([3b8cb8b](https://github.com/tari-project/tari/commit/3b8cb8ba237028fe848b7ee3771f91f729c26cd6))
* makes header consensus encoding infallible ([#4045](https://github.com/tari-project/tari/issues/4045)) ([5ebf129](https://github.com/tari-project/tari/commit/5ebf129119761484a8aac323c5b6d8e32649beac))
* only count base nodes in peers count in base node status ([#4039](https://github.com/tari-project/tari/issues/4039)) ([190d75a](https://github.com/tari-project/tari/commit/190d75ae4f4db65aa0d4622e21daa2cfc85b378d))
* prevent seed peer address from being overwritten unless newer ([#4085](https://github.com/tari-project/tari/issues/4085)) ([59b76c3](https://github.com/tari-project/tari/commit/59b76c32b225d74d59817521c2c8c18bdca926bb))
* support safe non-interactive mode ([#4072](https://github.com/tari-project/tari/issues/4072)) ([b34f79d](https://github.com/tari-project/tari/commit/b34f79d4ff4e73ab2574658eedea082573803330))
* test_output_manager_sqlite_db(_encrypted) ([#4025](https://github.com/tari-project/tari/issues/4025)) ([7a6f980](https://github.com/tari-project/tari/commit/7a6f9809ac59151c4b7c170dc43d46e7a8c68331))
* update daily test configuration ([#4049](https://github.com/tari-project/tari/issues/4049)) ([b15d682](https://github.com/tari-project/tari/commit/b15d682cd6675527e49aefdbbf6f0f0137571d76))
* **wallet:** do not prompt for password if given in config ([#4040](https://github.com/tari-project/tari/issues/4040)) ([fc1aa65](https://github.com/tari-project/tari/commit/fc1aa650e985bce4166ceb226fa2880d48ec5021))
* **wallet:** ensure block hash exists ([#4083](https://github.com/tari-project/tari/issues/4083)) ([a258984](https://github.com/tari-project/tari/commit/a258984154e5e84ffaf9f61b73a59e263650443a))
* weird behaviour of dates in base node banned peers ([#4037](https://github.com/tari-project/tari/issues/4037)) ([7097185](https://github.com/tari-project/tari/commit/7097185c7f52161edd6aa7ddec7f4ab47449795f))



### [0.31.1](https://github.com/tari-project/tari/compare/v0.31.0...v0.31.1) (2022-04-06)


### Bug Fixes

* clippy error when not on unix ([#4000](https://github.com/tari-project/tari/issues/4000)) ([7e38259](https://github.com/tari-project/tari/commit/7e382597aa450e642433c3ec3dce5ac77ec09215))
* fix NodeIdentityHasNoAddress when creating new node identity ([#4002](https://github.com/tari-project/tari/issues/4002)) ([76b526e](https://github.com/tari-project/tari/commit/76b526e22c279d43b3c7d15b12f7c736c4812166))

## [0.31.0](https://github.com/tari-project/tari/compare/v0.30.2...v0.31.0) (2022-04-05)

### Features

* add config setting to resize terminal ([#3920](https://github.com/tari-project/tari/issues/3920)) ([ce697bd](https://github.com/tari-project/tari/commit/ce697bdc68d8d0864b2d8406dd0f79bbd8b9125d))
* add less aggressive txn cancelling ([#3904](https://github.com/tari-project/tari/issues/3904)) ([40bedde](https://github.com/tari-project/tari/commit/40bedde2053a8e98a2c1a97fc5223be105d0e60e))
* check file perms on identity files ([#3958](https://github.com/tari-project/tari/issues/3958)) ([7f381d3](https://github.com/tari-project/tari/commit/7f381d3ada56f14472fd4f8b0644a9c5840b6d50))
* implement qr code for base node ([#3977](https://github.com/tari-project/tari/issues/3977)) ([2f618a2](https://github.com/tari-project/tari/commit/2f618a2560611903e2e87b3d9f93ed6e513dda04))
* remove spawn blocking calls from wallet db (output manager service) ([#3982](https://github.com/tari-project/tari/issues/3982)) ([cbf75ca](https://github.com/tari-project/tari/commit/cbf75ca762a7affc17b8794ea79679b75626f5d1))
* update committee from the committee definions TXs ([#3911](https://github.com/tari-project/tari/issues/3911)) ([0b29c89](https://github.com/tari-project/tari/commit/0b29c89442607089f0d985f07395b76175d76e93))
* **wallet:** send a cancel message to a receiver when it responds to a canceled transaction ([#3976](https://github.com/tari-project/tari/issues/3976)) ([96d24c6](https://github.com/tari-project/tari/commit/96d24c66b534234151974664eb8fcb2e35225386))


### Bug Fixes

* another fix for the recovery daily test ([#3978](https://github.com/tari-project/tari/issues/3978)) ([73365a4](https://github.com/tari-project/tari/commit/73365a466e1e14d697f16f5cd9ff8ca51c310d54))
* estimated hashrate calculation is incorrect ([#3996](https://github.com/tari-project/tari/issues/3996)) ([4587fc0](https://github.com/tari-project/tari/commit/4587fc0e7605359432830ba634f21f18207d3821))
* recovery daily test reporting of uT was rounding incorrectly ([#3992](https://github.com/tari-project/tari/issues/3992)) ([b5797b7](https://github.com/tari-project/tari/commit/b5797b7db05c95a1401bfbc5d8ba78783037bfec))
* separate watch loop for the non-interactive mode ([#3979](https://github.com/tari-project/tari/issues/3979)) ([9bf6503](https://github.com/tari-project/tari/commit/9bf6503e750da7c34c64dc39e5bb1d7e3dda763b))

### [0.30.2](https://github.com/tari-project/tari/compare/v0.30.1...v0.30.2) (2022-03-29)


### Features

* **core:** add tip height metric ([#3964](https://github.com/tari-project/tari/issues/3964)) ([c3f6b11](https://github.com/tari-project/tari/commit/c3f6b11163e52643796296abb8fcd5e053dae13d))


### Bug Fixes

* add output option to the status command ([#3969](https://github.com/tari-project/tari/issues/3969)) ([aabb653](https://github.com/tari-project/tari/commit/aabb6539c8b5920b519b40c85093cd5ad07ae229))
* revert changes to key manager branch seed strings ([#3971](https://github.com/tari-project/tari/issues/3971)) ([db688aa](https://github.com/tari-project/tari/commit/db688aaeaa53b6a9565867c3791d74ee4c571abf))

### [0.30.1](https://github.com/tari-project/tari/compare/v0.30.0...v0.30.1) (2022-03-28)


### Features

* add mmr check to reconstructed block and mempool validation for unique excess signature ([#3930](https://github.com/tari-project/tari/issues/3930)) ([b8f9db5](https://github.com/tari-project/tari/commit/b8f9db50e3bca2e1c4364929cebca1c6a3485956))
* **dht:** monitor and display warning for min ratio for TCPv4 nodes ([#3953](https://github.com/tari-project/tari/issues/3953)) ([c4070ff](https://github.com/tari-project/tari/commit/c4070ffb1b90ac6f6d37394bfa74b8699f366303))
* gracefully handle recovering a duplicate output in LibWallet ([#3903](https://github.com/tari-project/tari/issues/3903)) ([bcd1418](https://github.com/tari-project/tari/commit/bcd1418c542c07894ba3d709312f1166bfc34c1b))
* listen to terminal events in the watch mode ([#3931](https://github.com/tari-project/tari/issues/3931)) ([869abd3](https://github.com/tari-project/tari/commit/869abd3ad1453616c0f1d755611bd9a53ccd8e2f))
* **metrics:** add UTXO set size to base node metrics ([#3932](https://github.com/tari-project/tari/issues/3932)) ([08ecabc](https://github.com/tari-project/tari/commit/08ecabc4559cb9968232b7b4021994e7d88dff31))
* script to produce coverage report for wallet ([#3938](https://github.com/tari-project/tari/issues/3938)) ([48eb86e](https://github.com/tari-project/tari/commit/48eb86effaaa5823c5a6ea4589a3c471784f3c38))


### Bug Fixes

* **base-node:** disable SAF auto requests ([#3919](https://github.com/tari-project/tari/issues/3919)) ([b34503b](https://github.com/tari-project/tari/commit/b34503b3d9b6fb37d69b76649a090f9d85eedca7))
* bug in block timing grpc method ([#3926](https://github.com/tari-project/tari/issues/3926)) ([1c7adc0](https://github.com/tari-project/tari/commit/1c7adc0e71c8e03b192b1eab3010941989d207a2))
* correct main path for wallet rpc client ([#3934](https://github.com/tari-project/tari/issues/3934)) ([b36295c](https://github.com/tari-project/tari/commit/b36295c7c08541f3bed7d38d29bcb95b7c7eeba0))
* fix ffi import external utxo from faucet ([#3956](https://github.com/tari-project/tari/issues/3956)) ([3480323](https://github.com/tari-project/tari/commit/34803238d04298fb9023f4161a3242c0030ed28f))
* fix handling of creating faux transaction for recovered outputs ([#3959](https://github.com/tari-project/tari/issues/3959)) ([c5eb9e5](https://github.com/tari-project/tari/commit/c5eb9e5d2a86af12a69338445fb38c3170612b54))
* fix Tor ID deserialization issue ([#3950](https://github.com/tari-project/tari/issues/3950)) ([c290ab9](https://github.com/tari-project/tari/commit/c290ab974406c5c7d787e2220bcc7d8ea11909a6))
* launch the watch command on start ([#3924](https://github.com/tari-project/tari/issues/3924)) ([7145201](https://github.com/tari-project/tari/commit/71452013493a9ce87ce8ee20621a08ebe7d03391))
* **sync:** adds extra checks for sync stream termination ([#3927](https://github.com/tari-project/tari/issues/3927)) ([dd544cb](https://github.com/tari-project/tari/commit/dd544cb9c8907b05d4aea937f247c087c6484de6))
* **sync:** ban peer if sending invalid prev_header ([#3955](https://github.com/tari-project/tari/issues/3955)) ([384ab0c](https://github.com/tari-project/tari/commit/384ab0ceddd25b5b31722fe639229c0ecf554926))
* **wallet:** ensure that identity sig is stored on startup ([#3951](https://github.com/tari-project/tari/issues/3951)) ([b8d08ed](https://github.com/tari-project/tari/commit/b8d08ed17c23d74b7309867cafd128c136555f82))
* **wallet:** tor identity private key needs to be serialized ([#3946](https://github.com/tari-project/tari/issues/3946)) ([a68614e](https://github.com/tari-project/tari/commit/a68614e55313270b8a22a68a4cc802780030cfca))

## [0.30.0](https://github.com/tari-project/tari/compare/v0.29.0...v0.30.0) (2022-03-16)


### ⚠ BREAKING CHANGES

* change hash to use consensus encoding (#3820)

### Bug Fixes

* aligned tables left ([#3899](https://github.com/tari-project/tari/issues/3899)) ([1279773](https://github.com/tari-project/tari/commit/127977386d49a4305621009cea98f3d39977fb0a))
* **consensus:** check blockchain version within valid range ([#3916](https://github.com/tari-project/tari/issues/3916)) ([faf23f3](https://github.com/tari-project/tari/commit/faf23f3ab8c17bea12c06f55f14702da06b9bcc8))
* change hash to use consensus encoding ([#3820](https://github.com/tari-project/tari/issues/3820)) ([3a2da1d](https://github.com/tari-project/tari/commit/3a2da1d7dbbecb8e3aa4cb3a01496d557b968a59))

## [0.29.0](https://github.com/tari-project/tari/compare/v0.28.1...v0.29.0) (2022-03-14)


### ⚠ BREAKING CHANGES
Existing nodes should delete their databases and resync.

* add recovery byte to output features (#3727)
* add support for specifying custom messages for scanned outputs in libwallet (#3871)
* add committee management utxo (#3835)

### Features

* add committee management utxo ([#3835](https://github.com/tari-project/tari/issues/3835)) ([50fe421](https://github.com/tari-project/tari/commit/50fe421412b795dc69a46b75b65a1fb3834754e4))
* add contacts liveness service to base layer wallet ([#3857](https://github.com/tari-project/tari/issues/3857)) ([0d96ea3](https://github.com/tari-project/tari/commit/0d96ea3ff96b3a1b7561a77e337a51165df820a2))
* add contacts status to tui ([#3868](https://github.com/tari-project/tari/issues/3868)) ([30bf86b](https://github.com/tari-project/tari/commit/30bf86bd89976b8411c64fcbf2993f32fc419272))
* add liveness call to wallet GRPC ([#3854](https://github.com/tari-project/tari/issues/3854)) ([9ab832a](https://github.com/tari-project/tari/commit/9ab832a5d739cbc6822436c8965d18130343064b))
* add logging of cancelled outputs when transaction is rejected ([#3863](https://github.com/tari-project/tari/issues/3863)) ([d28703d](https://github.com/tari-project/tari/commit/d28703dd00addaec00ad151c68e831b15b1fd0fa))
* add recovery byte to output features ([#3727](https://github.com/tari-project/tari/issues/3727)) ([c9985de](https://github.com/tari-project/tari/commit/c9985dea859ba19a823e65258867486e812c2ef7))
* add support for make-it-rain command ([#3830](https://github.com/tari-project/tari/issues/3830)) ([0322402](https://github.com/tari-project/tari/commit/032240242341109e5407e8fdfd8eae77be49ece9))
* add support for specifying custom messages for scanned outputs in libwallet ([#3871](https://github.com/tari-project/tari/issues/3871)) ([0d7f8fc](https://github.com/tari-project/tari/commit/0d7f8fccf33a4beabf16efbc6bdd32383937edb2))
* adds get-mempool-tx command ([#3841](https://github.com/tari-project/tari/issues/3841)) ([a49b1af](https://github.com/tari-project/tari/commit/a49b1af215ef7445111bf64a1301cf08d3e88bf4))
* **base-node:** allow status line interval to be configured ([#3852](https://github.com/tari-project/tari/issues/3852)) ([427463d](https://github.com/tari-project/tari/commit/427463d07275b8375a7e11cb992876b1f986b637))
* **collectibles:** add basic window menu items ([#3847](https://github.com/tari-project/tari/issues/3847)) ([c8ebe5b](https://github.com/tari-project/tari/commit/c8ebe5b4db4c0a6e4dfe10ae3cfc5cd7fbd97835))
* **dht:** convenience function for DHT to discover then connect ([#3840](https://github.com/tari-project/tari/issues/3840)) ([da59c85](https://github.com/tari-project/tari/commit/da59c8540897864e9e7cd2c698288d9d8d186100))
* update committee selection from collectibles ([#3872](https://github.com/tari-project/tari/issues/3872)) ([daf140d](https://github.com/tari-project/tari/commit/daf140d894b86d64b1213e0d6976892013219004))
* update console wallet notifications ([e3e8b3d](https://github.com/tari-project/tari/commit/e3e8b3d284ee6ec4d938503d9278434ed66c0f95))
* update FFI client user agent string ([4a6df68](https://github.com/tari-project/tari/commit/4a6df68081d6d4be1d125c4b3484204058185a2a))
* **validator-node:** committee proposes genesis block w/ instructions ([#3844](https://github.com/tari-project/tari/issues/3844)) ([68a9f76](https://github.com/tari-project/tari/commit/68a9f76c9f23c85b45ce8a7d5aa329d4bbe4d9a5))


### Bug Fixes

* add bound for number of console_wallet notifications ([033db2a](https://github.com/tari-project/tari/commit/033db2a393717f061458d8e281adbfa7155870bf))
* **block-sync:** use avg latency to determine slow sync peer for block sync ([#3912](https://github.com/tari-project/tari/issues/3912)) ([f091c25](https://github.com/tari-project/tari/commit/f091c25bca01c3d2fcc2df3f9108db307d8e9f39))
* **core:** correctly filter pruned sync peers for block sync ([#3902](https://github.com/tari-project/tari/issues/3902)) ([bfdfce6](https://github.com/tari-project/tari/commit/bfdfce6662521be041ebb54c8e645384a074e2ac))
* **dht:** use blocking tasks for db calls ([1832416](https://github.com/tari-project/tari/commit/18324164fd7d6f9cb30a78748372f29a31998d07))
* fix flakey `test_coinbase_abandoned` integration test ([#3866](https://github.com/tari-project/tari/issues/3866)) ([ab52f5e](https://github.com/tari-project/tari/commit/ab52f5e964e9e045063702e378dc186b50d52a9d))
* fix merge mining proxy pool mining ([#3814](https://github.com/tari-project/tari/issues/3814)) ([407160c](https://github.com/tari-project/tari/commit/407160cf68f604ae89cba8b54020a90364621e12))
* improve sha3 pool mining ([#3846](https://github.com/tari-project/tari/issues/3846)) ([be75c74](https://github.com/tari-project/tari/commit/be75c74ed291833cd90ebd5f849929846a10633f))
* remove critical tag from flaky cucumber test ([#3865](https://github.com/tari-project/tari/issues/3865)) ([64b72de](https://github.com/tari-project/tari/commit/64b72de7761fdd4cc1d5ba1f744e845eb69a1496))
* update metadata size calculation to use FixedSet.iter() ([dbbe095](https://github.com/tari-project/tari/commit/dbbe095b461d4a93549d4cf87faf841dabf74ad0))
* update wallet logging config ([7675e75](https://github.com/tari-project/tari/commit/7675e7586be313a90ba214849a1df2bfa3e96d72))
* **validator-node:** fix consensus stall after genesis ([#3855](https://github.com/tari-project/tari/issues/3855)) ([64efeff](https://github.com/tari-project/tari/commit/64efeffc2bcd4cfa320280c7bee093bb7f1c57fe))
* **wallet:** minor wording fix on transactions tab ([#3853](https://github.com/tari-project/tari/issues/3853)) ([fd32bc9](https://github.com/tari-project/tari/commit/fd32bc9251838440d8663d7da112fcb85689838b))

### [0.28.1](https://github.com/tari-project/tari/compare/v0.28.0...v0.28.1) (2022-02-17)


### Features

* add persistence of transaction cancellation reason to wallet db ([#3842](https://github.com/tari-project/tari/issues/3842)) ([31410cd](https://github.com/tari-project/tari/commit/31410cd05c14751136a93ec543c9822fd8221e18))
* **cli:** resize terminal height ([#3838](https://github.com/tari-project/tari/issues/3838)) ([9026152](https://github.com/tari-project/tari/commit/90261526683c940c8aebe224d0d666931d4de11e))
* resize base node terminal on startup ([#3827](https://github.com/tari-project/tari/issues/3827)) ([00bc6e2](https://github.com/tari-project/tari/commit/00bc6e2bb1afbd709d3fc8492d182242e92c7620)), closes [#1728](https://github.com/tari-project/tari/issues/1728)
* **sync:** switch peers when max latency is exceeded ([#3741](https://github.com/tari-project/tari/issues/3741)) ([9e4af94](https://github.com/tari-project/tari/commit/9e4af94221dee0e87aa577a65bfa5d4b7809f558))
* update console wallet tui ([#3837](https://github.com/tari-project/tari/issues/3837)) ([3403db6](https://github.com/tari-project/tari/commit/3403db6320210b2cd4497a87fad87d6d5dc87478))
* **validator-node:** initial state sync implementation (partial) ([#3826](https://github.com/tari-project/tari/issues/3826)) ([ee4b52d](https://github.com/tari-project/tari/commit/ee4b52d97cb41133dbf1ed9dd2f0787fc00375d2))
* **wallet:** add grpc method for setting base node ([#3828](https://github.com/tari-project/tari/issues/3828)) ([8791e93](https://github.com/tari-project/tari/commit/8791e93df52d05bec1a10d80e8c3a9416270d5d9))


### Bug Fixes

* daily test ([#3815](https://github.com/tari-project/tari/issues/3815)) ([815ba8e](https://github.com/tari-project/tari/commit/815ba8ea39fdfde97f3a79dabc4e74bf76ea5363))
* **dan:** include state_root in node hash ([#3836](https://github.com/tari-project/tari/issues/3836)) ([5cda980](https://github.com/tari-project/tari/commit/5cda980d92fb3c4617836611dcfb59e01c58cec6))
* update RFC links and README ([#3675](https://github.com/tari-project/tari/issues/3675)) ([#3839](https://github.com/tari-project/tari/issues/3839)) ([22416a1](https://github.com/tari-project/tari/commit/22416a1c2efd9328f35f11ad89bed3fb845c9f72))
* **wallet:** fix aggressive disconnects in wallet connectivity ([#3807](https://github.com/tari-project/tari/issues/3807)) ([86e0154](https://github.com/tari-project/tari/commit/86e01542e6b92794613bfdb32ca54d28c5e19ed7))

## [0.28.0](https://github.com/tari-project/tari/compare/v0.27.2...v0.28.0) (2022-02-10)


### ⚠ BREAKING CHANGES

* add scanned transaction handling for one-sided payments with callbacks (#3794)
* **wallet_ffi:**  add base node connectivity callback to wallet ffi (#3796)

### Features

* ability to compile on stable rust ([#3759](https://github.com/tari-project/tari/issues/3759)) ([c19db92](https://github.com/tari-project/tari/commit/c19db9257d2f98b2d1a456816f6ef50018bdcbfe))
* add logging and config to collectibles ([#3781](https://github.com/tari-project/tari/issues/3781)) ([96a1e4e](https://github.com/tari-project/tari/commit/96a1e4ec144dc17190f396f94ec25c62fb142ce3))
* add scanned transaction handling for one-sided payments with callbacks ([#3794](https://github.com/tari-project/tari/issues/3794)) ([5453c9e](https://github.com/tari-project/tari/commit/5453c9e05b7d35b7586ff9375ba30ed7ecc7a9dd))
* add specific LibWallet error code for “Fee is greater than amount” ([#3793](https://github.com/tari-project/tari/issues/3793)) ([5aa2a66](https://github.com/tari-project/tari/commit/5aa2a661cdae869a877dda5f3cadc3abb97c374a))
* **base-node:** add base node prometheus metrics ([#3773](https://github.com/tari-project/tari/issues/3773)) ([7502c02](https://github.com/tari-project/tari/commit/7502c020eb5f531c4ebe1a50235ab8493c8f5fd5))
* **base-node:** add number of active sync peers metric ([#3784](https://github.com/tari-project/tari/issues/3784)) ([3495e85](https://github.com/tari-project/tari/commit/3495e85707f3ffba622feeab42e17276181654c2))
* **collectibles:** add delete committee member button ([#3786](https://github.com/tari-project/tari/issues/3786)) ([51f2f91](https://github.com/tari-project/tari/commit/51f2f91e9b2e6289b74cf9148b23335cccea5c40))
* prevent ambiguous output features in transaction protocols ([#3765](https://github.com/tari-project/tari/issues/3765)) ([f5b6ab6](https://github.com/tari-project/tari/commit/f5b6ab629f78497faef62f10b805d1c9a7c242c3))
* re-use scanned range proofs ([#3764](https://github.com/tari-project/tari/issues/3764)) ([ffd502d](https://github.com/tari-project/tari/commit/ffd502d61a709d41723e67c8ec6b2d5004a87edc))
* read asset definitions from base layer ([#3802](https://github.com/tari-project/tari/issues/3802)) ([86de08b](https://github.com/tari-project/tari/commit/86de08baa5e7648f68efcbec150d7b8652437ca9))
* **validator_node:** add get_sidechain_block p2p rpc method ([#3803](https://github.com/tari-project/tari/issues/3803)) ([74df1d0](https://github.com/tari-project/tari/commit/74df1d0705d7acad452564e71d6fea79fc7a8daa))
* **wallet_ffi:**  add base node connectivity callback to wallet ffi ([#3796](https://github.com/tari-project/tari/issues/3796)) ([66ea697](https://github.com/tari-project/tari/commit/66ea697395286ca89b34c77f3d857f1c3f16b421))


### Bug Fixes

* bump flood ban messages config ([#3799](https://github.com/tari-project/tari/issues/3799)) ([bbd0e1e](https://github.com/tari-project/tari/commit/bbd0e1e54e3eded861b004fd2d4aeba41bc6e423))
* coinbase output recovery bug ([#3789](https://github.com/tari-project/tari/issues/3789)) ([beb299e](https://github.com/tari-project/tari/commit/beb299e69ee1af7ec4e46889191051ce49dd1d50))
* **comms:** minor edge-case fix to handle inbound connection while dialing ([#3785](https://github.com/tari-project/tari/issues/3785)) ([2f9603b](https://github.com/tari-project/tari/commit/2f9603b88a8db0064f1783df0b8f18be19a24497))
* **core:** fetch_header_containing_*_mmr functions now take a 0-based mmr position ([#3749](https://github.com/tari-project/tari/issues/3749)) ([f5b72d9](https://github.com/tari-project/tari/commit/f5b72d9dd302eed0b0da612734b128b3078318ae))
* **core:** fix potential panic for sidechain merkle root with incorrect length ([#3788](https://github.com/tari-project/tari/issues/3788)) ([b3cc6f2](https://github.com/tari-project/tari/commit/b3cc6f27359ad33fc1c3fdf49d00478f8e27994f))
* **core:** reduce one block behind waiting period ([#3798](https://github.com/tari-project/tari/issues/3798)) ([cc41f36](https://github.com/tari-project/tari/commit/cc41f36b01a42a6f8d48b02d0ed6fe73c99f061d))
* **ffi:** missing param in header.h ([#3774](https://github.com/tari-project/tari/issues/3774)) ([7645a83](https://github.com/tari-project/tari/commit/7645a832e4c90319ad41e74db93c1ee61daa7b2a))
* **ffi:** mut pointers should be const ([#3775](https://github.com/tari-project/tari/issues/3775)) ([d09ba30](https://github.com/tari-project/tari/commit/d09ba304b07d9681d84f71addbe1c9c70e3c4c67))
* fix rustls and trust-dns-client after version bump ([#3816](https://github.com/tari-project/tari/issues/3816)) ([e6e845c](https://github.com/tari-project/tari/commit/e6e845ceb219842021f5a0b359c00079a7b7eb70))
* improved image handling in collectibles ([#3808](https://github.com/tari-project/tari/issues/3808)) ([4b22252](https://github.com/tari-project/tari/commit/4b2225291eb61955b5ff575f134d68aa47deedfe))
* minor fixes on collectibles ([#3795](https://github.com/tari-project/tari/issues/3795)) ([cfc42dd](https://github.com/tari-project/tari/commit/cfc42ddcc5d6fd96d05922662eea43929b46c81a))
* text explorer show sha-3 correctly + minor fixes ([#3779](https://github.com/tari-project/tari/issues/3779)) ([a5dacf2](https://github.com/tari-project/tari/commit/a5dacf2bcc51ae754d88f9af66cd0632a49b8a1b))

### [0.27.2](https://github.com/tari-project/tari/compare/v0.27.1...v0.27.2) (2022-01-28)


### Bug Fixes

* **ffi:** fix bad access ([#3772](https://github.com/tari-project/tari/issues/3772)) ([2a41f22](https://github.com/tari-project/tari/commit/2a41f22fbbe0981f6520fe8975de2db97b75f383))

## [0.27.0](https://github.com/tari-project/tari/compare/v0.26.0...v0.27.0) (2022-01-28)


### ⚠ BREAKING CHANGES

* **ffi:** Add commitment_signature_create and destroy (#3768)
* **ffi:** add features, metadata_signature and sender_offset_public_key to import_utxo (#3767)

### Features

* **collectibles:** add form validation error when committee not set ([#3750](https://github.com/tari-project/tari/issues/3750)) ([dfdaf4b](https://github.com/tari-project/tari/commit/dfdaf4bbff36d78f51dd222fabc504e60171a627))
* **console-wallet:** shift+tab to go to prev tab ([#3748](https://github.com/tari-project/tari/issues/3748)) ([9725f5f](https://github.com/tari-project/tari/commit/9725f5fb6fd53a4c72d2f4f0f7d23d6f150e79e3))
* **explorer:** better view on mempool ([#3763](https://github.com/tari-project/tari/issues/3763)) ([caa2837](https://github.com/tari-project/tari/commit/caa28374d53b618b335eba014c96fb7f8c7df0dd))
* **ffi:** Add commitment_signature_create and destroy ([#3768](https://github.com/tari-project/tari/issues/3768)) ([2df8193](https://github.com/tari-project/tari/commit/2df8193f0ff32bfbc65cb2962ff89bc2c56f75d1))
* **ffi:** add features, metadata_signature and sender_offset_public_key to import_utxo ([#3767](https://github.com/tari-project/tari/issues/3767)) ([7d8aa69](https://github.com/tari-project/tari/commit/7d8aa69f081a23def2457d1402ea291aabe37bb5))
* show error if IPFS upload fails ([#3746](https://github.com/tari-project/tari/issues/3746)) ([b58cf4c](https://github.com/tari-project/tari/commit/b58cf4c29b903bc4db172a6f93ce67ef2848f7f9))
* update the available balance in console wallet ([#3760](https://github.com/tari-project/tari/issues/3760)) ([d3edfe5](https://github.com/tari-project/tari/commit/d3edfe5daa2ab5f1a54835fcae1427349f8c50c2))


### Bug Fixes

* fix attempting to validate faux transaction ([#3758](https://github.com/tari-project/tari/issues/3758)) ([7de1b23](https://github.com/tari-project/tari/commit/7de1b23fbe8f1a477db126a4d47014647e632a93))
* fix cucumber test for standard recovery ([#3757](https://github.com/tari-project/tari/issues/3757)) ([1d58977](https://github.com/tari-project/tari/commit/1d58977c011f66c80763b7f6369ee9756298080e))
* properly decrypt imported faux tx when reading from db ([#3754](https://github.com/tari-project/tari/issues/3754)) ([997b74b](https://github.com/tari-project/tari/commit/997b74b36221abbcb7f107eca0b78eaccb6aea87))
* use of branch seed in key manager ([#3751](https://github.com/tari-project/tari/issues/3751)) ([ec92919](https://github.com/tari-project/tari/commit/ec92919cad2487307a583362684194d5066c1403))

### [0.26.1](https://github.com/tari-project/tari/compare/v0.26.0...v0.26.1) (2022-01-26)


### Features

* **collectibles:** add form validation error when committee not set ([#3750](https://github.com/tari-project/tari/issues/3750)) ([dfdaf4b](https://github.com/tari-project/tari/commit/dfdaf4bbff36d78f51dd222fabc504e60171a627))
* **console-wallet:** shift+tab to go to prev tab ([#3748](https://github.com/tari-project/tari/issues/3748)) ([9725f5f](https://github.com/tari-project/tari/commit/9725f5fb6fd53a4c72d2f4f0f7d23d6f150e79e3))
* show error if IPFS upload fails ([#3746](https://github.com/tari-project/tari/issues/3746)) ([b58cf4c](https://github.com/tari-project/tari/commit/b58cf4c29b903bc4db172a6f93ce67ef2848f7f9))


### Bug Fixes

* properly decrypt imported faux tx when reading from db ([#3754](https://github.com/tari-project/tari/issues/3754)) ([997b74b](https://github.com/tari-project/tari/commit/997b74b36221abbcb7f107eca0b78eaccb6aea87))
* use of branch seed in key manager ([#3751](https://github.com/tari-project/tari/issues/3751)) ([ec92919](https://github.com/tari-project/tari/commit/ec92919cad2487307a583362684194d5066c1403))

## [0.26.0](https://github.com/tari-project/tari/compare/v0.25.1...v0.26.0) (2022-01-25)


### ⚠ BREAKING CHANGES

* generate new dibbler genesis block (#3742)
* **core:** add missing consensus encoding length byte rangeproof & covenants (#3730)

### Bug Fixes

* **core:** add missing consensus encoding length byte rangeproof & covenants ([#3730](https://github.com/tari-project/tari/issues/3730)) ([d56da1a](https://github.com/tari-project/tari/commit/d56da1aa86e0854c4d51635c45d82135393dfbea))
* ensure that features are set when syncing peers ([#3745](https://github.com/tari-project/tari/issues/3745)) ([8efd2e4](https://github.com/tari-project/tari/commit/8efd2e4960e9c3de2ce3c9ff7e077aae6c4da11a))
* move config to one file ([9f9a46c](https://github.com/tari-project/tari/commit/9f9a46c8dd0b7975764bed4f2fecdcb943d763a6))


### [0.25.1](https://github.com/tari-project/tari/compare/v0.25.0...v0.25.1) (2022-01-24)


### Features

* add icon to mining node ([#3555](https://github.com/tari-project/tari/issues/3555)) ([4551a90](https://github.com/tari-project/tari/commit/4551a9033aff8a22ab3f5cf1c2c4a65534dcb743))
* add sync progress grpc ([#3722](https://github.com/tari-project/tari/issues/3722)) ([d47ad5e](https://github.com/tari-project/tari/commit/d47ad5ec165b23fbf17d791ba63f8a49419075f8))
* validator node shows its public key on startup ([#3734](https://github.com/tari-project/tari/issues/3734)) ([fc7da51](https://github.com/tari-project/tari/commit/fc7da511999d407cf38294dc3e68bb5c99af12a1))
* **validator:** add grpc identity rpc ([#3731](https://github.com/tari-project/tari/issues/3731)) ([6d2a4a4](https://github.com/tari-project/tari/commit/6d2a4a402be8120a66d4dd7b23adfe00bdf9434e))


### Bug Fixes

* prevent key leaking through derive debug impl ([#3735](https://github.com/tari-project/tari/issues/3735)) ([12a90e6](https://github.com/tari-project/tari/commit/12a90e6b8781d1d4de02e1374b2e3277d132d44c))

## [0.25.0](https://github.com/tari-project/tari/compare/v0.24.0...v0.25.0) (2022-01-21)


### ⚠ BREAKING CHANGES

* don't include duplicate unique ID in same block template (#3713)
* reinstate 2 min blocks for dibbler (#3720)

### Features

* add command line arguments support to collectibles ([#3714](https://github.com/tari-project/tari/issues/3714)) ([696f2ef](https://github.com/tari-project/tari/commit/696f2ef9571875701fb72501dc6879a0d7b2d05d))
* **base_node:** base node keeps track of historical reorgs ([#3718](https://github.com/tari-project/tari/issues/3718)) ([79ffdec](https://github.com/tari-project/tari/commit/79ffdec110d73b6256e48fba28a4cc16778e7c75))


### Bug Fixes

* **collectibles:** fix setup and unlock ([#3724](https://github.com/tari-project/tari/issues/3724)) ([f98596d](https://github.com/tari-project/tari/commit/f98596d780cafc35ee8968d4fcb5900d85f94a11))
* **collectibles:** various ui fixes ([#3726](https://github.com/tari-project/tari/issues/3726)) ([1cc51a9](https://github.com/tari-project/tari/commit/1cc51a9494f78e810cf0b393a7ae354f47074616))
* **comms:** ensure signature challenge is constructed consistently ([#3725](https://github.com/tari-project/tari/issues/3725)) ([e5173b7](https://github.com/tari-project/tari/commit/e5173b7425849847d08e4133979622586df5d736))
* don't include duplicate unique ID in same block template ([#3713](https://github.com/tari-project/tari/issues/3713)) ([b501771](https://github.com/tari-project/tari/commit/b5017718a71b51a220c660d34b476d71aa3fc04e))
* put libtor behind optional feature flag ([#3717](https://github.com/tari-project/tari/issues/3717)) ([e850cf0](https://github.com/tari-project/tari/commit/e850cf076b1803afe1fddcbcbe21dfaf92e6b103))
* reinstate 2 min blocks for dibbler ([#3720](https://github.com/tari-project/tari/issues/3720)) ([3c5890d](https://github.com/tari-project/tari/commit/3c5890d231d1d329f93707977a618c6bdf0dcb5d))

## [0.24.0](https://github.com/tari-project/tari/compare/v0.23.0...v0.24.0) (2022-01-18)


### ⚠ BREAKING CHANGES

* fix tx and txo validation callback bug in wallet FFI (#3695)
* compact block propagation (#3704)

### Features

* add versioning to transaction(input,output,kernel) ([#3709](https://github.com/tari-project/tari/issues/3709)) ([08995c7](https://github.com/tari-project/tari/commit/08995c753d24a587993bc2d1264176026c2728c3))
* compact block propagation ([#3704](https://github.com/tari-project/tari/issues/3704)) ([274b7d9](https://github.com/tari-project/tari/commit/274b7d9048d0054688a9ce07bcde35311edf6a36))
* enable optional libtor for macos/linux ([#3703](https://github.com/tari-project/tari/issues/3703)) ([572168f](https://github.com/tari-project/tari/commit/572168f82edfa7fc3521f721392c83281c7239bc))
* update faux transaction status when imported output is validated ([#3684](https://github.com/tari-project/tari/issues/3684)) ([aaeb186](https://github.com/tari-project/tari/commit/aaeb186068c48ab082c693d30b3615bef64d9103))


### Bug Fixes

* fix tx and txo validation callback bug in wallet FFI ([#3695](https://github.com/tari-project/tari/issues/3695)) ([16cfa22](https://github.com/tari-project/tari/commit/16cfa224d95917fe8f0581103d3abe8e1018cb68))

## [0.23.0](https://github.com/tari-project/tari/compare/v0.22.1...v0.23.0) (2022-01-13)


### ⚠ BREAKING CHANGES

* **mempool:** optimisations,excess sig index,fix weight calc (#3691)
* **comms:** add signature to peer identity to allow third party identity updates (#3629)
* provide a compact form of TransactionInput (#3460)
* **comms:** optimise connection establishment (#3658)

### Features

* add 721 template stub ([#3643](https://github.com/tari-project/tari/issues/3643)) ([5c276ee](https://github.com/tari-project/tari/commit/5c276ee600cf4ab4c02720045dfaf2324f74ed9d))
* add chain storage and mempool validation for unique asset ids ([#3416](https://github.com/tari-project/tari/issues/3416)) ([7331b0c](https://github.com/tari-project/tari/commit/7331b0c94bccbc71d292fdd452468d09765ebd50))
* add connect per site into the web extension ([#3626](https://github.com/tari-project/tari/issues/3626)) ([75aa0a3](https://github.com/tari-project/tari/commit/75aa0a36983e4c307846b16b469bcef7b364af16))
* add dibbler testnet and genesis block ([6f355f7](https://github.com/tari-project/tari/commit/6f355f7f9fa546fc8660767c66ef00574a6938a1))
* add file uploading to ipfs for asset create ([#3524](https://github.com/tari-project/tari/issues/3524)) ([b1ee1df](https://github.com/tari-project/tari/commit/b1ee1df0ab4c044b62e48f44a2e4a69c91904a63))
* add onboarding flow for web extension ([#3582](https://github.com/tari-project/tari/issues/3582)) ([0c47bc8](https://github.com/tari-project/tari/commit/0c47bc897a4fa7c0711f505972333ba46bcd34c5))
* add tari collectibles app ([#3480](https://github.com/tari-project/tari/issues/3480)) ([eb46127](https://github.com/tari-project/tari/commit/eb46127b84d3411955c378c8bab080c14127340d))
* add wallet stub to collectibles app ([#3589](https://github.com/tari-project/tari/issues/3589)) ([dfc3b92](https://github.com/tari-project/tari/commit/dfc3b92563e582b337f02f8fdcde64a6f8b9b7f2))
* **collectibles:** load asset registrations from base node on dash ([#3532](https://github.com/tari-project/tari/issues/3532)) ([1fd5d03](https://github.com/tari-project/tari/commit/1fd5d034559b22edbd727513cff304c7b3c14b15))
* **comms:** add signature to peer identity to allow third party identity updates ([#3629](https://github.com/tari-project/tari/issues/3629)) ([c672d48](https://github.com/tari-project/tari/commit/c672d48fd4bc2708cef3e7929a76c55df9f63b01))
* compile key manager to wasm and add javascript interface ([#3565](https://github.com/tari-project/tari/issues/3565)) ([35790d6](https://github.com/tari-project/tari/commit/35790d6c5fa12e59b05a12c776300ca388bf6205))
* covenants implementation ([#3656](https://github.com/tari-project/tari/issues/3656)) ([8dfcf3a](https://github.com/tari-project/tari/commit/8dfcf3a1f5a303ff2a47bb47eb1d8a3bf30a5b23))
* create tari web extension application stub ([#3535](https://github.com/tari-project/tari/issues/3535)) ([4a61c04](https://github.com/tari-project/tari/commit/4a61c04208a9302fe156f59f3cc9512e17444247))
* example to generate vanity node_ids ([#3654](https://github.com/tari-project/tari/issues/3654)) ([40fd1f0](https://github.com/tari-project/tari/commit/40fd1f0cdb456486baaca9baf639bc16d77a7d36))
* provide a compact form of TransactionInput ([#3460](https://github.com/tari-project/tari/issues/3460)) ([834eff9](https://github.com/tari-project/tari/commit/834eff946c7419ca3c4c927cbe7899ff5dcb0a8f))

### Bug Fixes

* allow 0-conf in blockchain db ([#3680](https://github.com/tari-project/tari/issues/3680)) ([246709a](https://github.com/tari-project/tari/commit/246709aba756d6805b8e7705b7f3fbd1188987a3))
* **comms:** improve simultaneous connection handling ([#3697](https://github.com/tari-project/tari/issues/3697)) ([99ba6a3](https://github.com/tari-project/tari/commit/99ba6a379dabb48992a7c5a205544522d007c7ca))
* prefer configured seeds over dns seeds ([#3662](https://github.com/tari-project/tari/issues/3662)) ([0f438aa](https://github.com/tari-project/tari/commit/0f438aaa9cbe64a88b9c1af343fafe4ac161a977))
* remove noise negotiation for debugging on bad wire mode ([#3657](https://github.com/tari-project/tari/issues/3657)) ([eee73f7](https://github.com/tari-project/tari/commit/eee73f7f922dc35a43508dbf37db01874aed9249))

### [0.22.1](https://github.com/tari-project/tari/compare/v0.22.0...v0.22.1) (2022-01-04)


### Features

* add GRPC call to search for utxo via commitment hex ([#3666](https://github.com/tari-project/tari/issues/3666)) ([d006b67](https://github.com/tari-project/tari/commit/d006b6701be3216d5c4dcb67e82520305383e831))
* add search by commitment to explorer ([#3668](https://github.com/tari-project/tari/issues/3668)) ([18f6e29](https://github.com/tari-project/tari/commit/18f6e29ed31e4075f1a102fa138b6b82b088b3d3))
* base_node switching for console_wallet when status is offline ([#3639](https://github.com/tari-project/tari/issues/3639)) ([ca5f0ee](https://github.com/tari-project/tari/commit/ca5f0ee70569fe78c19ddae77078ef2da9bfc142))
* custom_base_node in config ([#3651](https://github.com/tari-project/tari/issues/3651)) ([2c677b8](https://github.com/tari-project/tari/commit/2c677b8ec0b27618335b7488d6a0e29f37478adc))
* improve wallet recovery and scanning handling of reorgs ([#3655](https://github.com/tari-project/tari/issues/3655)) ([fe9033b](https://github.com/tari-project/tari/commit/fe9033b56704602cf22f6d28c03e73de97482f8e))
* tari launchpad ([#3671](https://github.com/tari-project/tari/issues/3671)) ([5dd9e1c](https://github.com/tari-project/tari/commit/5dd9e1ce3664550dbb682a150ac83eb6d143eafa))


### Bug Fixes

* edge cases causing bans during header/block sync ([#3661](https://github.com/tari-project/tari/issues/3661)) ([95af1cf](https://github.com/tari-project/tari/commit/95af1cfa5e56d8fa19f8a5bc1135cef2186b542c))
* end stale outbound queue immediately on disconnect, retry outbound messages ([#3664](https://github.com/tari-project/tari/issues/3664)) ([576a00c](https://github.com/tari-project/tari/commit/576a00caab30f2da5aea9fab2dcc7373a827ec2d))
* return correct index for include_pruned_utxos = false ([#3663](https://github.com/tari-project/tari/issues/3663)) ([80854b2](https://github.com/tari-project/tari/commit/80854b2322bd8a321d3e8f0c838f1b92e1e7a34e))

## [0.22.0](https://github.com/tari-project/tari/compare/v0.21.2...v0.22.0) (2021-12-07)


### ⚠ BREAKING CHANGES

Base node users should delete their node databases and resync

* **consensus:** add tari script byte size limit check to validation (#3640)
* **pruned mode:** prune inputs, allow horizon sync resume and other fixes (#3521)
* sending one-sided transactions in wallet_ffi (#3634)
* multiple monerod addresses in tari merge mining proxy (#3628)
* separate peer seeds to common.network (#3635)
* console wallet grpc_console_wallet_addresss config (#3619)
* add tcp bypass settings for tor in wallet_ffi (#3615)
* expose reason for transaction cancellation for callback in wallet_ffi (#3601)

### Features

* add ban peers metric ([#3605](https://github.com/tari-project/tari/issues/3605)) ([65157b0](https://github.com/tari-project/tari/commit/65157b00237a7cd6b3b68d84f958ed33da3a7297))
* add bulletproof rewind profiling ([#3618](https://github.com/tari-project/tari/issues/3618)) ([5790a9d](https://github.com/tari-project/tari/commit/5790a9d776c954efd0dcc603839c91cba5907c69))
* add page for detailed mempool in explorer ([#3613](https://github.com/tari-project/tari/issues/3613)) ([970f811](https://github.com/tari-project/tari/commit/970f8113c495688aee47a3bce1eaebe067547c52))
* add tcp bypass settings for tor in wallet_ffi ([#3615](https://github.com/tari-project/tari/issues/3615)) ([1003f91](https://github.com/tari-project/tari/commit/1003f918c5f7b46a94cb07620b82afc411917a05))
* bad block list for invalid blocks after sync ([#3637](https://github.com/tari-project/tari/issues/3637)) ([5969723](https://github.com/tari-project/tari/commit/5969723ffd3a3315c3af5d5d235f7ed550ef23a8))
* **consensus:** add tari script byte size limit check to validation ([#3640](https://github.com/tari-project/tari/issues/3640)) ([53a5174](https://github.com/tari-project/tari/commit/53a517438cef23d1ae2960a8d8e46d9ea993a276))
* display network for console wallet ([#3611](https://github.com/tari-project/tari/issues/3611)) ([7432c62](https://github.com/tari-project/tari/commit/7432c628e5f9cc4d0806a1327da92188f450f750))
* expose reason for transaction cancellation for callback in wallet_ffi ([#3601](https://github.com/tari-project/tari/issues/3601)) ([3b3da21](https://github.com/tari-project/tari/commit/3b3da21830fc098b8038b30b1d947e98f9198ede))
* implement dht pooled db connection ([#3596](https://github.com/tari-project/tari/issues/3596)) ([2ac0757](https://github.com/tari-project/tari/commit/2ac07577a4c0d0f2454484f625285a07b2cd0b98))
* improve wallet responsiveness ([#3625](https://github.com/tari-project/tari/issues/3625)) ([73d862f](https://github.com/tari-project/tari/commit/73d862fb4380f606af27101374f94bad53c65dc5))
* language detection for mnemonic seed words ([#3590](https://github.com/tari-project/tari/issues/3590)) ([57f51bc](https://github.com/tari-project/tari/commit/57f51bc8fd23e8380307d210bdc247cf570bc083))
* only trigger UTXO scanning when a new block event is received ([#3620](https://github.com/tari-project/tari/issues/3620)) ([df1be7e](https://github.com/tari-project/tari/commit/df1be7e4249d6e3e0701c837bedbdc2ad6c9ff65))
* prevent banning of connected base node in wallet ([#3642](https://github.com/tari-project/tari/issues/3642)) ([363b254](https://github.com/tari-project/tari/commit/363b254cac7ac19f5547a986b46af53458788a0f))
* removed transaction validation redundant events ([#3630](https://github.com/tari-project/tari/issues/3630)) ([c3dbdc9](https://github.com/tari-project/tari/commit/c3dbdc9726d647ebf1f8fe5a7e50743b12576093))
* sending one-sided transactions in wallet_ffi ([#3634](https://github.com/tari-project/tari/issues/3634)) ([e501aa0](https://github.com/tari-project/tari/commit/e501aa09baf21cba6dbed940fbdf4432399cf2cc))
* standardize output hash for unblinded output, transaction output and transaction input ([#3592](https://github.com/tari-project/tari/issues/3592)) ([2ba437b](https://github.com/tari-project/tari/commit/2ba437b4a7eed2022a7555bb72a8838eb2e608a2))
* track ping failures and disconnect ([#3597](https://github.com/tari-project/tari/issues/3597)) ([91fe921](https://github.com/tari-project/tari/commit/91fe921092991df59f65fbe4f448fba85b42e30b))
* use CipherSeed wallet birthday for recovery start point ([#3602](https://github.com/tari-project/tari/issues/3602)) ([befa621](https://github.com/tari-project/tari/commit/befa6215741c37c3c40f7088cdccb4221750a033))


### Bug Fixes

* allow bullet proof value only rewinding in atomic swaps ([#3586](https://github.com/tari-project/tari/issues/3586)) ([889796a](https://github.com/tari-project/tari/commit/889796a45875d72c4a2bc670b96846d22e359fe1))
* allow bullet proof value only rewinding off one-sided transaction ([#3587](https://github.com/tari-project/tari/issues/3587)) ([f32a38f](https://github.com/tari-project/tari/commit/f32a38f409bb342e0ab507af5336abe60eaca2a8))
* be more permissive of responses for the incorrect request_id ([#3588](https://github.com/tari-project/tari/issues/3588)) ([c0d625c](https://github.com/tari-project/tari/commit/c0d625c1630da1b1b25414ecdc99bd78eccf8bba))
* console wallet grpc_console_wallet_addresss config ([#3619](https://github.com/tari-project/tari/issues/3619)) ([b09acd1](https://github.com/tari-project/tari/commit/b09acd1442ff6d0ff58530df0698eaa0934a2b61))
* get-peer command works with public key again ([#3636](https://github.com/tari-project/tari/issues/3636)) ([2e1500b](https://github.com/tari-project/tari/commit/2e1500b857ab9cc5d08b8d394de09c2400686f5f))
* improve handling of old base nodes and reorgs in wallet recovery ([#3608](https://github.com/tari-project/tari/issues/3608)) ([bb94ea2](https://github.com/tari-project/tari/commit/bb94ea23ad72d7bddac9a105b5f254c91f2a0386))
* minor improvements to available neighbouring peer search ([#3598](https://github.com/tari-project/tari/issues/3598)) ([e59d194](https://github.com/tari-project/tari/commit/e59d194a213370bd316f4330267cf8a72e1adee1))
* multiple monerod addresses in tari merge mining proxy ([#3628](https://github.com/tari-project/tari/issues/3628)) ([ddb9268](https://github.com/tari-project/tari/commit/ddb926872e97070cf49314720509d6f46c2b260c))
* **pruned mode:** prune inputs, allow horizon sync resume and other fixes ([#3521](https://github.com/tari-project/tari/issues/3521)) ([a4341a0](https://github.com/tari-project/tari/commit/a4341a03afedd9df9a29cd09219b2ed9b5cf7a5a))
* remove delay from last request latency call ([eb8b815](https://github.com/tari-project/tari/commit/eb8b8152ecc4cd9ccf49a7fe23fe0e2c77ff2c63))
* remove delay from last request latency call ([#3579](https://github.com/tari-project/tari/issues/3579)) ([c82a8ca](https://github.com/tari-project/tari/commit/c82a8ca3de531d2994031472cff88025e229b884))
* seed word parsing ([#3607](https://github.com/tari-project/tari/issues/3607)) ([fff45db](https://github.com/tari-project/tari/commit/fff45db4bd1a6fa436ad3525e96ae08f26a856e8))
* separate peer seeds to common.network ([#3635](https://github.com/tari-project/tari/issues/3635)) ([326579b](https://github.com/tari-project/tari/commit/326579bf1d933b93f421a5876221a281b0f6e178))
* update daily test start times and seed phrase ([#3584](https://github.com/tari-project/tari/issues/3584)) ([8e271d7](https://github.com/tari-project/tari/commit/8e271d769b7fc540bd78e326f0e0e8155e9de88f))
* use json 5 for tor identity (regression) ([#3624](https://github.com/tari-project/tari/issues/3624)) ([7d49fa4](https://github.com/tari-project/tari/commit/7d49fa4c092f5d7e0a373f3f0c91d9007534e575))

### [0.21.2](https://github.com/tari-project/tari/compare/v0.21.1...v0.21.2) (2021-11-19)

### Features

* add atomic swap refund transaction handling ([#3573](https://github.com/tari-project/tari/issues/3573)) ([337bc6f](https://github.com/tari-project/tari/commit/337bc6f1b11abc0f53cdc3a82a0aa5110e1fe856))
* improve wallet connectivity status for console wallet ([#3577](https://github.com/tari-project/tari/issues/3577)) ([e191e27](https://github.com/tari-project/tari/commit/e191e27ed79aff5cfe4d76effe77473a03eb31f6))

### [0.21.1](https://github.com/tari-project/tari/compare/v0.21.0...v0.21.1) (2021-11-17)


### Features

* add atomic swap htlc sending and claiming ([#3552](https://github.com/tari-project/tari/issues/3552)) ([a185506](https://github.com/tari-project/tari/commit/a1855065e0f2aaabbb6d7e508f71d6d0eaf6acd5))
* add error codes to LibWallet for CipherSeed errors ([#3578](https://github.com/tari-project/tari/issues/3578)) ([2913804](https://github.com/tari-project/tari/commit/291380457ba28c6208d2d1ac97757e8cfa8df85c))
* add support for MultiAddr in RPC config ([#3557](https://github.com/tari-project/tari/issues/3557)) ([9f8e289](https://github.com/tari-project/tari/commit/9f8e2899922c0eec9167ed4f49c5d4c161330221))
* get fee for transactions for stratum transcoder ([#3571](https://github.com/tari-project/tari/issues/3571)) ([ccf1da0](https://github.com/tari-project/tari/commit/ccf1da02dcbf99fe1872deec73f424b4328c70e0))
* implement multiple read single write for sqlite ([#3568](https://github.com/tari-project/tari/issues/3568)) ([8d22164](https://github.com/tari-project/tari/commit/8d22164ca10a4493fe73f66d13edf9ddd57cc6d1))
* implement prometheus metrics for base node ([#3563](https://github.com/tari-project/tari/issues/3563)) ([433bc46](https://github.com/tari-project/tari/commit/433bc46e3d5cd488ec0f29fef6059594cf0cf3e3))
* one-click installer - cli edition ([#3534](https://github.com/tari-project/tari/issues/3534)) ([ec67798](https://github.com/tari-project/tari/commit/ec677987a712c934168040da07f31fc744f66f71))
* trigger time lock balance update when block received ([#3567](https://github.com/tari-project/tari/issues/3567)) ([11b8afa](https://github.com/tari-project/tari/commit/11b8afa31abe7e64ff366f8e83e478b017a86da5))
* **wallet:** import utxo’s as EncumberedToBeReceived rather than Unspent ([#3575](https://github.com/tari-project/tari/issues/3575)) ([c286d40](https://github.com/tari-project/tari/commit/c286d408f5419620a63783c1ae9fe4d9f5cd68d2))


### Bug Fixes

* avoid implicit using of the time crate ([#3562](https://github.com/tari-project/tari/issues/3562)) ([23e8398](https://github.com/tari-project/tari/commit/23e83988cb8fe99babd0a96686602added75011a))
* stop leak of value of recovered output ([#3558](https://github.com/tari-project/tari/issues/3558)) ([e0f2187](https://github.com/tari-project/tari/commit/e0f21876278702aa43096b04aa9e701f0942be67))
* use time crate instead of chrono ([#3527](https://github.com/tari-project/tari/issues/3527)) ([d211031](https://github.com/tari-project/tari/commit/d211031cfa44ad498706db84e8a919b9babaf422))

## [0.21.0](https://github.com/tari-project/tari/compare/v0.13.0...v0.21.0) (2021-11-09)


### ⚠ BREAKING CHANGES

* remove outdated wallet_ffi balance methods (#3528)
* **rpc:** read from substream while streaming to check for interruptions (#3548)

### Features

* add ffi get mnemonic wordlist ([#3538](https://github.com/tari-project/tari/issues/3538)) ([d8e0ced](https://github.com/tari-project/tari/commit/d8e0cedc19ee008a8dd937347d7e2fc5e7fc4c3f))
* optimize transaction validation for wallet ([#3537](https://github.com/tari-project/tari/issues/3537)) ([9064b83](https://github.com/tari-project/tari/commit/9064b830c04000683aecf7b2972ffeabe5d90f08))


### Bug Fixes

* add check for old db encryption and provide warning ([#3549](https://github.com/tari-project/tari/issues/3549)) ([69bbbdf](https://github.com/tari-project/tari/commit/69bbbdfd87fae56d31bcd342fe4dc5c84086402e))
* add decision step between header sync and pruned/archival ([#3546](https://github.com/tari-project/tari/issues/3546)) ([23e868a](https://github.com/tari-project/tari/commit/23e868a8a4d2d8b673e4bd3df9fb9f4d33d191d9))
* check for previously cancelled completed txn before accepting a repeat message ([#3542](https://github.com/tari-project/tari/issues/3542)) ([911b83b](https://github.com/tari-project/tari/commit/911b83b675816cd41b4e40e0f001bca6f7037369))
* prevent race condition between block propagation and sync ([#3536](https://github.com/tari-project/tari/issues/3536)) ([6bbb654](https://github.com/tari-project/tari/commit/6bbb65453ed5d8969e0e659fd855d5183262c6d6))
* remove dns resolver config from cucumber tests, use default ([#3547](https://github.com/tari-project/tari/issues/3547)) ([e17ee64](https://github.com/tari-project/tari/commit/e17ee645add6c3030d1198d8efe96149fffbb7b6))
* **rpc:** read from substream while streaming to check for interruptions ([#3548](https://github.com/tari-project/tari/issues/3548)) ([9194501](https://github.com/tari-project/tari/commit/919450186f70f3c00ade937a76288ce00ef2175c))
* update the seed words used in the Daily tests ([#3545](https://github.com/tari-project/tari/issues/3545)) ([7696840](https://github.com/tari-project/tari/commit/76968400fbb1560d11f3beeecb6d1bb5ba60433b))
* use tcp tls backend for peer seed DNS resolution ([#3544](https://github.com/tari-project/tari/issues/3544)) ([5b38909](https://github.com/tari-project/tari/commit/5b389098aa0aab9dd723213a29aeebe22e4d9bb6))


* remove outdated wallet_ffi balance methods ([#3528](https://github.com/tari-project/tari/issues/3528)) ([413757b](https://github.com/tari-project/tari/commit/413757bcea5474524b18a860a95df255cbe95d33))

## [0.13.0](https://github.com/tari-project/tari/compare/v0.12.0...v0.13.0) (2021-11-04)


### ⚠ BREAKING CHANGES

* implement new CipherSeed and upgrade encryption KDF (#3505)

### Features

* add a Rejected status to TransactionStatus ([#3512](https://github.com/tari-project/tari/issues/3512)) ([c65a01c](https://github.com/tari-project/tari/commit/c65a01c33f20b79f07227daeb91ccbef5b804b18))
* add caching and clippy annotations to CI ([#3518](https://github.com/tari-project/tari/issues/3518)) ([beacb9e](https://github.com/tari-project/tari/commit/beacb9e652fdb70ff7164ebcc5f496759c26a903))
* implement new CipherSeed and upgrade encryption KDF ([#3505](https://github.com/tari-project/tari/issues/3505)) ([ef4f84f](https://github.com/tari-project/tari/commit/ef4f84ff97dd9543669a8b4a37b20d718bd8d18b))


### Bug Fixes

* edge case fix for integer pair iter ([#3508](https://github.com/tari-project/tari/issues/3508)) ([097e3e2](https://github.com/tari-project/tari/commit/097e3e2c5b3b4fee305c0d279177e1231b82bf1c))
* header sync must allow transition to archival/pruned if tip is behind ([#3520](https://github.com/tari-project/tari/issues/3520)) ([e028386](https://github.com/tari-project/tari/commit/e0283867cf7e0c3848dc48b67fee8aa2645ac67c))

## [0.12.0](https://github.com/tari-project/tari/compare/v0.11.0...v0.12.0) (2021-10-29)


### ⚠ BREAKING CHANGES

* **wallet_ffi:** add get_balance callback to wallet ffi (#3475)
* apps should not depend on other app configs (#3469)

### Features

* add decay_params method ([#3454](https://github.com/tari-project/tari/issues/3454)) ([a027f32](https://github.com/tari-project/tari/commit/a027f32edb2910528f65b500f032b143a487dad2))
* add sql query to obtain balance ([#3446](https://github.com/tari-project/tari/issues/3446)) ([e23ceec](https://github.com/tari-project/tari/commit/e23ceecfa441f6739412dafc02e1d5fcc95ff9ab))
* apps should not depend on other app configs ([#3469](https://github.com/tari-project/tari/issues/3469)) ([b33e8b5](https://github.com/tari-project/tari/commit/b33e8b564732a41f4216f38f4c5f92c459a0c623))
* improve logging for tari_mining_node ([#3449](https://github.com/tari-project/tari/issues/3449)) ([db9eb96](https://github.com/tari-project/tari/commit/db9eb9641836f6bf3e878a3065c08661a4c57254))
* optimize get transactions query ([#3496](https://github.com/tari-project/tari/issues/3496)) ([e651a60](https://github.com/tari-project/tari/commit/e651a60f0f18289968fd38dcb382b4d804d0cd2f))
* optimize pending transactions inbound query ([#3500](https://github.com/tari-project/tari/issues/3500)) ([4ea02e7](https://github.com/tari-project/tari/commit/4ea02e7a04d411748f45a8b35b863d2ba2cc3111))
* revalidate all outputs ([#3471](https://github.com/tari-project/tari/issues/3471)) ([9bd4760](https://github.com/tari-project/tari/commit/9bd476099b18cf3d10a11ec789cd1450c5d5f011))
* tx weight takes tariscript and output features into account [igor] ([#3411](https://github.com/tari-project/tari/issues/3411)) ([5bef3fd](https://github.com/tari-project/tari/commit/5bef3fdf6c3771620b5286605faeb83f9b2152e7))
* **wallet_ffi:** add get_balance callback to wallet ffi ([#3475](https://github.com/tari-project/tari/issues/3475)) ([930860d](https://github.com/tari-project/tari/commit/930860dfc0386f9eb8e578501c2c0e0477c4a638))


### Bug Fixes

* add details to UnknownError ([#3429](https://github.com/tari-project/tari/issues/3429)) ([dddc18f](https://github.com/tari-project/tari/commit/dddc18fb4d24b4b8ff0069594a54eb699d560e56))
* add display_currency_decimal method ([#3445](https://github.com/tari-project/tari/issues/3445)) ([1f52ffc](https://github.com/tari-project/tari/commit/1f52ffc0836699872e1f8266bd5821d0d9805eba))
* add sanity checks to prepare_new_block ([#3448](https://github.com/tari-project/tari/issues/3448)) ([76bc1f0](https://github.com/tari-project/tari/commit/76bc1f0177098fdf1e6af5ae44f4972935d5a221))
* ban peer that advertises higher PoW than able to provide ([#3478](https://github.com/tari-project/tari/issues/3478)) ([c04fca5](https://github.com/tari-project/tari/commit/c04fca5a4c027ae7331b4d1637a7d73c4013ef0f))
* check SAF message inflight and check stored_at is in past ([#3444](https://github.com/tari-project/tari/issues/3444)) ([fbf8eb8](https://github.com/tari-project/tari/commit/fbf8eb83353392b977b34bf3d1870ca25320414e))
* correct panic in tracing for comms ([#3499](https://github.com/tari-project/tari/issues/3499)) ([af15fcc](https://github.com/tari-project/tari/commit/af15fcc2bb863cc4d1ba4a4951456c64aba91a1c))
* **dht:** discard encrypted message with no destination ([#3472](https://github.com/tari-project/tari/issues/3472)) ([6ca3424](https://github.com/tari-project/tari/commit/6ca3424de306235088d598f741dcb7952efc73f8))
* ensure that accumulated orphan chain data is committed before header validation ([#3462](https://github.com/tari-project/tari/issues/3462)) ([80f7c78](https://github.com/tari-project/tari/commit/80f7c78b296a48eda3d3e69d266396482679f35a))
* fix config file whitespace issue when auto generated in windows ([#3491](https://github.com/tari-project/tari/issues/3491)) ([996c047](https://github.com/tari-project/tari/commit/996c0476105665bb32a3f2b13417624c808e8e49))
* fix confusing names in get_balance functions ([#3447](https://github.com/tari-project/tari/issues/3447)) ([6cd9228](https://github.com/tari-project/tari/commit/6cd9228d61a4b827c4062267074d40b1908773a6))
* fix flakey rust tests ([#3435](https://github.com/tari-project/tari/issues/3435)) ([7384201](https://github.com/tari-project/tari/commit/73842016db9cfa4cc5abaa3984a5ff3d9f90d7cd))
* fix recovery test reporting message ([#3479](https://github.com/tari-project/tari/issues/3479)) ([335b626](https://github.com/tari-project/tari/commit/335b62604b577ea7ef19b4b9d637993a0732e559))
* improve responsiveness of wallet base node switching ([#3488](https://github.com/tari-project/tari/issues/3488)) ([762cb9a](https://github.com/tari-project/tari/commit/762cb9abe5a20c798d72636c83e6e844fc5629be))
* improve test Wallet should display transactions made ([#3501](https://github.com/tari-project/tari/issues/3501)) ([6b3bac8](https://github.com/tari-project/tari/commit/6b3bac8daaddc01cfbf7208b999ae63d4af67ca9))
* prevent tari_mining_node from being able to start without a valid address for pool mining ([#3440](https://github.com/tari-project/tari/issues/3440)) ([92dee77](https://github.com/tari-project/tari/commit/92dee77feccc79b9c76bfb98d4cba64027ee3799))
* remove consensus breaking change in transaction input ([#3474](https://github.com/tari-project/tari/issues/3474)) ([d1b3523](https://github.com/tari-project/tari/commit/d1b3523d981b7ff797ee205d4e349b5116bbd06f))
* remove is_synced check for transaction validation ([#3459](https://github.com/tari-project/tari/issues/3459)) ([53989f4](https://github.com/tari-project/tari/commit/53989f40b0ab03a9a894f3489452f94efae94bc1))
* remove unbounded vec allocations from base node grpc/p2p messaging ([#3467](https://github.com/tari-project/tari/issues/3467)) ([5d7fb20](https://github.com/tari-project/tari/commit/5d7fb207c9994ea7f0cd6a0d7c05786d8da60792))
* remove unnecessary wallet dependency ([#3438](https://github.com/tari-project/tari/issues/3438)) ([07c2c69](https://github.com/tari-project/tari/commit/07c2c693c7ee155deafd94171a8a346d7c3706ba))
* sha256sum isn't available on all *nix platforms ([#3466](https://github.com/tari-project/tari/issues/3466)) ([6f61582](https://github.com/tari-project/tari/commit/6f6158231eac4b4aefec7499d1a1a551a4350911))
* typo in console wallet ([#3465](https://github.com/tari-project/tari/issues/3465)) ([401aff9](https://github.com/tari-project/tari/commit/401aff9f89f4064dfbe19845730b3530353980b0))
* u64->i64->u64 conversion; chain split height as u64 ([#3442](https://github.com/tari-project/tari/issues/3442)) ([43b2033](https://github.com/tari-project/tari/commit/43b20334151b2872b0969bf3fd56e7c5c2af62bd))
* upgrade rustyline dependencies ([#3476](https://github.com/tari-project/tari/issues/3476)) ([a05ac5e](https://github.com/tari-project/tari/commit/a05ac5e1924987d96e217188bea00a809f5cf57f))
* validate dht header before dedup cache ([#3468](https://github.com/tari-project/tari/issues/3468)) ([81f01d2](https://github.com/tari-project/tari/commit/81f01d228cc425fb859fd07e08dfb34f03e1bd22))

## [0.11.0](https://github.com/tari-project/tari/compare/v0.10.1...v0.11.0) (2021-10-08)


### ⚠ BREAKING CHANGES

* new transaction and output validation protocol (#3421)

### Features

* allow tor proxy to be bypassed for outbound tcp connections ([#3424](https://github.com/tari-project/tari/issues/3424)) ([6a5982e](https://github.com/tari-project/tari/commit/6a5982e2814fe4bd3e4d33f5d4770db2bb28d7e2))
* new transaction and output validation protocol ([#3421](https://github.com/tari-project/tari/issues/3421)) ([6578d1e](https://github.com/tari-project/tari/commit/6578d1eb885578761803b6278123ad154c677d53)), closes [#3191](https://github.com/tari-project/tari/issues/3191) [#3352](https://github.com/tari-project/tari/issues/3352) [#3383](https://github.com/tari-project/tari/issues/3383) [#3391](https://github.com/tari-project/tari/issues/3391) [#3394](https://github.com/tari-project/tari/issues/3394) [#3400](https://github.com/tari-project/tari/issues/3400) [#3417](https://github.com/tari-project/tari/issues/3417)
* update coinbase handling for new tx and output validation ([#3383](https://github.com/tari-project/tari/issues/3383)) ([8b546c9](https://github.com/tari-project/tari/commit/8b546c998c28ca5ca049ed7e446e0507546d02c8))


### Bug Fixes

* allow env override of mining node pool settings ([#3428](https://github.com/tari-project/tari/issues/3428)) ([423dbe1](https://github.com/tari-project/tari/commit/423dbe151344024075ca23731370ae988261eb1f))
* auto update cucumber tests, reduced timeout ([#3418](https://github.com/tari-project/tari/issues/3418)) ([ce17627](https://github.com/tari-project/tari/commit/ce17627ab0a6a6e4cbc3ea8d0e3532256f22d81a))
* don't display an error when there is no message to be read in tari_mining_node ([#3409](https://github.com/tari-project/tari/issues/3409)) ([eb4b560](https://github.com/tari-project/tari/commit/eb4b560bfc18e76ff31c39dcf7b55d42c53f5b0e))
* handle recovering a duplicate output ([#3426](https://github.com/tari-project/tari/issues/3426)) ([f9c9201](https://github.com/tari-project/tari/commit/f9c9201db0886ba2c9643b047f240ceec42bfc29))
* network switching ([#3413](https://github.com/tari-project/tari/issues/3413)) ([9a369a0](https://github.com/tari-project/tari/commit/9a369a024558f217edcd0fd220e788522fea4760))
* only allow one session per peer for block/header sync ([#3402](https://github.com/tari-project/tari/issues/3402)) ([06da165](https://github.com/tari-project/tari/commit/06da165d498bff1983c63eed8fede776841213d0))
* reduce console wallet tui memory usage ([#3389](https://github.com/tari-project/tari/issues/3389)) ([ca1e9fd](https://github.com/tari-project/tari/commit/ca1e9fdfc267e47f15b8434deb2a7953bc7d1c8e))
* update default sha3 pool address in readme and config ([#3405](https://github.com/tari-project/tari/issues/3405)) ([dae656a](https://github.com/tari-project/tari/commit/dae656ac755c1c9a08d670c6e0f258ed725576b9))
* update message on finding a valid share in tari_mining_node ([#3408](https://github.com/tari-project/tari/issues/3408)) ([7c13fde](https://github.com/tari-project/tari/commit/7c13fde7b4d5c6c4dfd0c7d4de5d2858597ac9a9))
* updates to daily recovery test ([#3433](https://github.com/tari-project/tari/issues/3433)) ([8ac27be](https://github.com/tari-project/tari/commit/8ac27bea9a699e4b85a870bfc847bc7e38692790))
* use intermediate u64 to calculate average ([#3432](https://github.com/tari-project/tari/issues/3432)) ([ff6bc38](https://github.com/tari-project/tari/commit/ff6bc38826ea584061f7c575ab5de9a514eb4832))

### [0.10.1](https://github.com/tari-project/tari/compare/v0.10.0...v0.10.1) (2021-10-01)


### Features

* add substream count to list-connections ([#3387](https://github.com/tari-project/tari/issues/3387)) ([965cac2](https://github.com/tari-project/tari/commit/965cac279e604f8035bf356f709e2c2cfb3aaa46))
* display weight, #inputs, #outputs in wallet for txn ([#3393](https://github.com/tari-project/tari/issues/3393)) ([6d57cbd](https://github.com/tari-project/tari/commit/6d57cbda352109f5aa9dbddde53946dce6eb7467))
* get-peer supports partial node id lookup ([#3379](https://github.com/tari-project/tari/issues/3379)) ([e5af5f7](https://github.com/tari-project/tari/commit/e5af5f75512a9822e38a691cc68e96e60db52ea2))
* implement DHT protocol versioning, includes [#3243](https://github.com/tari-project/tari/issues/3243) ([#3377](https://github.com/tari-project/tari/issues/3377)) ([d676bba](https://github.com/tari-project/tari/commit/d676bba552fb08fc3645369e1c676057dc7af760))
* improve console wallet responsiveness ([#3304](https://github.com/tari-project/tari/issues/3304)) ([73017a4](https://github.com/tari-project/tari/commit/73017a4d2eb19a7c79cd2b496270c8d7f9b9182e))
* merge consensus breaking changes in [#3195](https://github.com/tari-project/tari/issues/3195) [#3193](https://github.com/tari-project/tari/issues/3193) with weatherwax compatibility ([#3372](https://github.com/tari-project/tari/issues/3372)) ([79c9c1d](https://github.com/tari-project/tari/commit/79c9c1db303180a8026c92d39190f44ac2bbc80e))


### Bug Fixes

* additional check for cancelled transactions ([#3369](https://github.com/tari-project/tari/issues/3369)) ([ac5f26e](https://github.com/tari-project/tari/commit/ac5f26e0e3dadc688ba04221b03af71d7a52c5c2))
* fix console wallet tick events endless loop edge case at shutdown ([#3380](https://github.com/tari-project/tari/issues/3380)) ([b40a98f](https://github.com/tari-project/tari/commit/b40a98f602818fbff85372882bcca0ee42f8225e))
* fix debouncer delay bug ([#3376](https://github.com/tari-project/tari/issues/3376)) ([4ac2fb2](https://github.com/tari-project/tari/commit/4ac2fb21a32b4a506f5faf371740ddec6297ae49))
* iOS linker error workaround ([#3401](https://github.com/tari-project/tari/issues/3401)) ([58105d3](https://github.com/tari-project/tari/commit/58105d3c895099e2eb9ebb4079073f4985fa3f4a))
* mempool stats reflects unconfirmed pool ([#3398](https://github.com/tari-project/tari/issues/3398)) ([596ea4a](https://github.com/tari-project/tari/commit/596ea4ad354075c63cce6691fbed0ea615aa1a3d))
* pressing b key should not clear existing base node in console wallet ([#3364](https://github.com/tari-project/tari/issues/3364)) ([e594c5f](https://github.com/tari-project/tari/commit/e594c5f0139bf90ea1eaab2b0d389a42b36c481a))
* relative paths for logs is now relative to data path instead of current execution directory ([#3365](https://github.com/tari-project/tari/issues/3365)) ([e164c2b](https://github.com/tari-project/tari/commit/e164c2bfe4fc6d43270242a86beb0c6e90b1470f))
* remove unnecessary range proof verify and fix test temp disk usage  ([#3334](https://github.com/tari-project/tari/issues/3334)) ([eeb62a6](https://github.com/tari-project/tari/commit/eeb62a6b55728e866a00dc8c911a282bc7fc4405))
* resolved feature flags for openssl vendoring ([#3287](https://github.com/tari-project/tari/issues/3287)) ([30343d4](https://github.com/tari-project/tari/commit/30343d44f0260863eb300048e4c7f7bf82ab77bd))
* wallet recovery ([#3366](https://github.com/tari-project/tari/issues/3366)) ([2fde873](https://github.com/tari-project/tari/commit/2fde873b50c6960248dc4151e3476926034f848f))

### [0.10.0](https://github.com/tari-project/tari/compare/v0.9.6...v0.10.0) (2021-09-17)


### Features

* add base installer stubs ([#3281](https://github.com/tari-project/tari/issues/3281)) ([074034b](https://github.com/tari-project/tari/commit/074034bf001689b0778fc474638b19948c24c050)), closes [#3102](https://github.com/tari-project/tari/issues/3102)
* add get-db-stats command ([#3274](https://github.com/tari-project/tari/issues/3274)) ([d785f4f](https://github.com/tari-project/tari/commit/d785f4f3bde0b6de1b85f75f8da2256efef31128))
* add logging of Monero PoW data to debug merge mining ([#3276](https://github.com/tari-project/tari/issues/3276)) ([b0bf982](https://github.com/tari-project/tari/commit/b0bf98253bc7d19741b54ea85605e5f37877b826))
* ping-peer command ([#3295](https://github.com/tari-project/tari/issues/3295)) ([a04a2a6](https://github.com/tari-project/tari/commit/a04a2a613ddef1ebfcc97099828a3046be497413))
* rpc response message chunking ([#3336](https://github.com/tari-project/tari/issues/3336)) ([496ff14](https://github.com/tari-project/tari/commit/496ff1464df68801420c242ae828251deb465b58))
* show status avx2 feature and randomx count and flags ([#3261](https://github.com/tari-project/tari/issues/3261)) ([e2d8d1f](https://github.com/tari-project/tari/commit/e2d8d1f97bfa5425c582ed409dddb6bde539514c))
* update+notifications for console wallet ([#3284](https://github.com/tari-project/tari/issues/3284)) ([faa27fc](https://github.com/tari-project/tari/commit/faa27fc8d868d42e79d368cb0caa181b8d7cd573))


### Bug Fixes

* always grow database when asked to resize ([#3313](https://github.com/tari-project/tari/issues/3313)) ([603bcb3](https://github.com/tari-project/tari/commit/603bcb3034341d0b0ba4969755b1a2f3156e851a))
* ban header sync peer if no headers provided ([#3297](https://github.com/tari-project/tari/issues/3297)) ([570e222](https://github.com/tari-project/tari/commit/570e2223b9443fd681f1c8395405e8aae8180d94))
* block sync validation ([#3236](https://github.com/tari-project/tari/issues/3236)) ([fd081c8](https://github.com/tari-project/tari/commit/fd081c8addf8bcca53f16e3b025ba4401b09d311))
* **ci:** add quotes to pr title ci ([29247c2](https://github.com/tari-project/tari/commit/29247c24fee6a66e8b74e46811432b86e341e8ba)), closes [#3254](https://github.com/tari-project/tari/issues/3254)
* dead_code lint error when base_node_feature is not set ([#3354](https://github.com/tari-project/tari/issues/3354)) ([7fa0572](https://github.com/tari-project/tari/commit/7fa0572f174c3f4a9f62fae6c2f8fe038cc6a7c3))
* dedup sql error when deleting many entries ([#3300](https://github.com/tari-project/tari/issues/3300)) ([7e58845](https://github.com/tari-project/tari/commit/7e588459250906b7e23160fa3574eac1df7a7cec))
* disable P2P transaction negotiation while recovery is in progress ([#3248](https://github.com/tari-project/tari/issues/3248)) ([844e6cf](https://github.com/tari-project/tari/commit/844e6cf747e40ee2f8f950f59ac6f2dc64478bdb))
* disconnected node was never ready ([#3312](https://github.com/tari-project/tari/issues/3312)) ([dfc6fd2](https://github.com/tari-project/tari/commit/dfc6fd28809ca669d4f9a94e44e787752f1d0371))
* fix median timestamp index ([#3349](https://github.com/tari-project/tari/issues/3349)) ([0757e9b](https://github.com/tari-project/tari/commit/0757e9b1814d1476df8d67fea5196e083cad6e42))
* fix regression in cucumber tests for wallet ffi step ([#3356](https://github.com/tari-project/tari/issues/3356)) ([481f3c9](https://github.com/tari-project/tari/commit/481f3c9af55fb2a4f105bf48fb0ddbb56f99ef83))
* handle stream read error case by explicitly closing the substream ([#3321](https://github.com/tari-project/tari/issues/3321)) ([336f4d6](https://github.com/tari-project/tari/commit/336f4d68b2753f64a92b2942651d76ae0f20517d))
* invalid forced sync peer now returns configerror ([#3350](https://github.com/tari-project/tari/issues/3350)) ([8163ef8](https://github.com/tari-project/tari/commit/8163ef8bf622fb1c8b1bbf98ff0f4b8daaa99083))
* prevent immediate run of wallet recovery on cron script ([#3260](https://github.com/tari-project/tari/issues/3260)) ([969b306](https://github.com/tari-project/tari/commit/969b3062488a9306318bd83b1960c665c8de2a6e))
* randomx memory usage ([#3301](https://github.com/tari-project/tari/issues/3301)) ([52e409d](https://github.com/tari-project/tari/commit/52e409d0abfe448eb4130cd1dbeb96fc0b75a9af)), closes [#3104](https://github.com/tari-project/tari/issues/3104) [#3103](https://github.com/tari-project/tari/issues/3103)
* reduce overly-eager connection reaping for slow connections ([#3308](https://github.com/tari-project/tari/issues/3308)) ([9a0c999](https://github.com/tari-project/tari/commit/9a0c999a6308be5c3ffbff78fe22d001b986815d))
* remove explicit panic from rpc handshake on io error ([#3341](https://github.com/tari-project/tari/issues/3341)) ([c2ebfc8](https://github.com/tari-project/tari/commit/c2ebfc8907ee25597b50acacff96f2e470dc2a04))
* remove sqlite from windows installer and scripts ([#3362](https://github.com/tari-project/tari/issues/3362)) ([b2b6912](https://github.com/tari-project/tari/commit/b2b69120966634534e660962f59fd0ea566ca8a5))
* resolved design flaw in wallet_ffi library ([#3285](https://github.com/tari-project/tari/issues/3285)) ([2e6638c](https://github.com/tari-project/tari/commit/2e6638c5612b0a4961b4e30fa6d81e500e96b0e8))
* stop MTP attack ([#3357](https://github.com/tari-project/tari/issues/3357)) ([a82638a](https://github.com/tari-project/tari/commit/a82638a2500ceb04626989957dfe99ca0534c1ca))
* update balance after pending transaction is created ([#3320](https://github.com/tari-project/tari/issues/3320)) ([47bafbf](https://github.com/tari-project/tari/commit/47bafbf13276a6fd535de371ad1ab4a7857c3fa6))
* update block explorer to use local grpc ([#3348](https://github.com/tari-project/tari/issues/3348)) ([fc1e120](https://github.com/tari-project/tari/commit/fc1e1208992243d2c54795e843f51e53ccbdf850))
* update cucumber tests for walletffi.feature ([#3275](https://github.com/tari-project/tari/issues/3275)) ([38191d3](https://github.com/tari-project/tari/commit/38191d3ec627384cde6e7896ace3e260c5260a2f))
* wait couple rounds for no pings to send an event ([#3315](https://github.com/tari-project/tari/issues/3315)) ([2dcc0ea](https://github.com/tari-project/tari/commit/2dcc0ea2b8be69b967c968ce1c5b5b3d5dc60a3d))

<a name="0.9.6"></a>
## 0.9.6 (2021-09-01)


#### Features

*   add ability to bypass rangeproof (#3265) ([055271fc](https://github.com/tari-project/tari/commit/055271fc96e034779b8eb30b9161b1173736c688))
*   allow network to be selected at application start (#3247) ([8a36fb56](https://github.com/tari-project/tari/commit/8a36fb568eef0c4ca72bd108dc388a1ef35ba505))
*   add Igor testnet (#3256) ([0f6d3b1c](https://github.com/tari-project/tari/commit/0f6d3b1c1c600c9436b08e62a57fe22744151bd4))
*   improve basenode switch from listening to lagging mode (#3255) ([9dc335f6](https://github.com/tari-project/tari/commit/9dc335f67b75baa5733bc3bf7f78fc02d9bdfdf9))
*   allow DHT to be configured to repropagate messages for a number of rounds (#3211) ([60f286b3](https://github.com/tari-project/tari/commit/60f286b3e2b16cf4ac02727f6056a4327901f7c6))
*   base_node prompt user to create id if not found (#3245) ([6391941f](https://github.com/tari-project/tari/commit/6391941f83888e8e0ab6b06bfe225bbbba1da7a3))
*   add support for forcing sync from seeds (#3228) ([d1329320](https://github.com/tari-project/tari/commit/d13293208281fd3efbba0279e2a7bf6f64052bae))
* **wallet:**  add tab for error log to the wallet (#3250) ([098f25dc](https://github.com/tari-project/tari/commit/098f25dcd28b6e92157d05bcedb8777e0f085e0d))

#### Bug Fixes

*   make logging less noisy (#3267) ([4798161b](https://github.com/tari-project/tari/commit/4798161b6bd728f3b06718d49e40ae988edc046c))
*   remove cucumber walletffi.js file that got re-included in rebase (#3271) ([77c92565](https://github.com/tari-project/tari/commit/77c92565603665c81d3b241b4f4e212a032d6631))
*   auto update continuously checks auto_update_check_interval is disabled (#3270) ([b3bff31c](https://github.com/tari-project/tari/commit/b3bff31cb5c81a0439f28f30385074c3123a157b))
*   revert mining_node default logging config (#3262) ([edc1a2b9](https://github.com/tari-project/tari/commit/edc1a2b96ceddae3d1c0a54f933ced797b63bed3))
*   off-by-one causing "no further headers to download" bug (#3264) ([3502b397](https://github.com/tari-project/tari/commit/3502b397341d66a938fe94717696f105f939772f))
*   small display bug (#3257) ([d1bb7377](https://github.com/tari-project/tari/commit/d1bb7377afe25f674df4252a47b14e50b931c55f))
*   send transactions to all connected peers (#3239) ([16f779ed](https://github.com/tari-project/tari/commit/16f779edf4a00770de23041d2b55d885d81c7fb6))
*   add periodic connection check to wallet connectivity service (#3237) ([8c7066bc](https://github.com/tari-project/tari/commit/8c7066bc48f5f0b5494626e5f1da42656e92f217))
*   fix base_node_service_config not read (#3251) ([80066887](https://github.com/tari-project/tari/commit/80066887a7dee0a4784911c1c19defa39047b320))
*   daily wallet recovery fixes (#3229) ([6970230d](https://github.com/tari-project/tari/commit/6970230d700a86d0a0a9b5fdd0f46fc712a45aba))
*   remove OpenSSL from Windows runtime (#3242) ([0048c3bc](https://github.com/tari-project/tari/commit/0048c3bcfe509b078634d32e8805e9994a18093a))
*   add status output to logs in non-interactive mode (#3244) ([6b91bb63](https://github.com/tari-project/tari/commit/6b91bb6309d68059278d85bd89518360c45a6364))
*   exit command and free up tokio thread (#3235) ([d924beb6](https://github.com/tari-project/tari/commit/d924beb6a6508bd887fa41d7e7a37d5d0b1ba62a))



<a name="0.9.5"></a>
## 0.9.5 (2021-08-23)


#### Bug Fixes

*   show warnings on console (#3225) ([3291021c](https://github.com/tari-project/tari/commit/3291021c6e63778d4fa14ca6cb10c51681d8a5f5))
*   edge-case fixes for wallet peer switching in console wallet (#3226) ([f577df8e](https://github.com/tari-project/tari/commit/f577df8e9b34c6a823cc555b0fecfa2153ddd7e0))
*   chain error caused by zero-conf transactions and reorgs (#3223) ([f0404273](https://github.com/tari-project/tari/commit/f04042732a78bf3dc98d1aee7bf5b032e398010c))
*   bug in wallet base node peer switching (#3217) ([878c317b](https://github.com/tari-project/tari/commit/878c317be9226da342cef439af2bc0024d1eb77f))
*   division by zero ([8a988e1c](https://github.com/tari-project/tari/commit/8a988e1cd5bd4c49660819494949305963d08173))
*   improve p2p RPC robustness (#3208) ([211dcfdb](https://github.com/tari-project/tari/commit/211dcfdb70eb774f9f2c3cdd080d6db7a24cb46c))
* **wallet:**  add NodeId to console wallet Who Am I tab (#3213) ([706ff5e5](https://github.com/tari-project/tari/commit/706ff5e59185f8088add19ac8654f29cc4ab1145))
* **wallet_ffi:**  fix division by zero during recovery (#3214) ([abd3d849](https://github.com/tari-project/tari/commit/abd3d84965651285c72ecbcca1c401f3e54ad28c))

#### Features

*   add `ping()` to all comms RPC clients (#3227) ([b5b62238](https://github.com/tari-project/tari/commit/b5b62238cf7512abb38803c426369ebbcc8fe540))

#### Breaking Changes

*  base nodes should delete their database and resync


<a name="0.9.4"></a>
## 0.9.4 (2021-08-17)


#### Features

*   add sync rpc client pool to wallet connectivity (#3199) ([305aeda1](https://github.com/tari-project/tari/commit/305aeda139cfc93d35f67926e1d52fae010961c4))
* **wallet:**  add network selection to wallet_ffi (#3178) ([f0f40b20](https://github.com/tari-project/tari/commit/f0f40b20bc2f60fecc26dd9b83bd5820f9212eab))

#### Bug Fixes

*   fix console wallet buffer size bug (#3200) ([b94667fd](https://github.com/tari-project/tari/commit/b94667fddda4299d1ee176b3120a991a5b6903db))
*   ensure peers are added to peer list before recovery starts (#3186) ([5f33414a](https://github.com/tari-project/tari/commit/5f33414a5d39be046f471d5b279da66ecf1e747c))
*   enforce unique commitments in utxo set (#3173) ([23a7d64c](https://github.com/tari-project/tari/commit/23a7d64c550d7689db451c1dcf9e22d723f19f75))
*   cleanup stratum config terminal output in tari_mining_node (#3181) ([6c38f226](https://github.com/tari-project/tari/commit/6c38f2266641f77b39eb1406ca7e26a21ff38151))
* **wallet:**  handle receiver cancelling an inbound transaction that is later received (#3177) ([c79e53cf](https://github.com/tari-project/tari/commit/c79e53cfc20ea404f0d1b160f2686f77d1c52698))


<a name="0.9.3"></a>
## 0.9.3 (2021-08-12)


#### Bug Fixes

*   set robust limits for busy a blockchain (#3150) ([c993780a](https://github.com/tari-project/tari/commit/c993780ad0237feba78857b6e67cfbe6e9f78b1d))
*   update handling of SAF message propagation and deletion (#3164) ([cedb4efc](https://github.com/tari-project/tari/commit/cedb4efcc1b9ef3b01e1425437f84dd62065ac90))
*   improve prune mode to remove panics (#3163) ([05f78132](https://github.com/tari-project/tari/commit/05f7813296797e2583dbb38742084bef91ebbdd4))
*   better method for getting an open port in cucumber tests ([2d9f3a60](https://github.com/tari-project/tari/commit/2d9f3a60342b6af251405ca471ed76e8f25f5b84))
*   fix utxo scan edge case when pc awakes from sleep (#3160) ([5bdc9f39](https://github.com/tari-project/tari/commit/5bdc9f398c9036542a6f9ea385587af237ea96e3))
*   ban peer when merkle roots mismatch ([39ddd337](https://github.com/tari-project/tari/commit/39ddd337cc870932328250417755f2fa6a8201c5))
*   fix search_kernel command (#3157) ([dc99898e](https://github.com/tari-project/tari/commit/dc99898e1faf87c5fa7a26313cdec1623b53d947))
*   introduce cache update cool down to console wallet (#3146) ([5de92526](https://github.com/tari-project/tari/commit/5de92526d3266ff3476088fe91a2779451bd6c39))
*   add timeout to protocol notifications + log improvements (#3143) ([77018464](https://github.com/tari-project/tari/commit/77018464f4304428f8d1b4f0f886825de66af28e))
*   fix GRPC GetTransactionInfo not found response (#3145) ([0e0bfe0f](https://github.com/tari-project/tari/commit/0e0bfe0f31b05d44540a3bfa90e28bfc07ec86a7))
*   fix cucumber transaction builder reliability (#3147) ([d4a7fdd3](https://github.com/tari-project/tari/commit/d4a7fdd3ed4b61b068f9541b24f5fb9ad5bf40b5))
* **wallet:**
  *  fix resize panic (#3149) ([33af0847](https://github.com/tari-project/tari/commit/33af084720d752c5111fbef23ff854eaabe1a7d0))
  *  in wallet block certain keys during popup (#3148) ([84542922](https://github.com/tari-project/tari/commit/84542922f98d46985047d590c237bb63bf35c03b))
  *  correctly deal with new coinbase transactions for the same height (#3151) ([564ef5a2](https://github.com/tari-project/tari/commit/564ef5a26a3056ef855f7f132582beaf2ef0e15a))

#### Features

*   wallet connectivity service (#3159) ([54e8c8e4](https://github.com/tari-project/tari/commit/54e8c8e4020bbd38fd8e563465a4ce5d95408d7a))
*   add a shared p2p rpc client session pool to reduce rpc setup time (#3152) ([778f9512](https://github.com/tari-project/tari/commit/778f951282082e7774f649b043a4e9085fb05bdd))
*   miningcore transcoder (#3003) ([ee9a225c](https://github.com/tari-project/tari/commit/ee9a225c389b43267db34f97aff537b244533844))
* **mining_node:**  mining worker name for tari_mining_node (#3185) ([48a62f98](https://github.com/tari-project/tari/commit/48a62f98db687183759551b8bcd6239021e3c0c3))



<a name="0.9.2"></a>
## 0.9.2 (2021-07-29)


#### Bug Fixes

*   update LibWallet `wallet_import_utxo` method to include valid TariScript (#3139) ([cc6de2ab](https://github.com/tari-project/tari/commit/cc6de2ab7fde419b6bf5358aeed25ea343d0539e))
*   update LibWallet recovery task event handling (#3142) ([0861d726](https://github.com/tari-project/tari/commit/0861d726a1ec8811e8042018116e5a606326f306))
*   improve reliability of get block template protocol in mm proxy (#3141) ([6afde62f](https://github.com/tari-project/tari/commit/6afde62f94be350d58b45945017fef5bc6e16338))
*   replace usage of RangeProof MR with Witness MR (#3129) ([bbfc6878](https://github.com/tari-project/tari/commit/bbfc68783082e59de71ee4fa099f851a6d2f645f))
*   fix prune mode sync (#3138) ([d0d1d614](https://github.com/tari-project/tari/commit/d0d1d614798999e511b48a15aeca0a371612df1d))
*   update transaction and block validator to use full deleted map (#3137) ([4f1509e6](https://github.com/tari-project/tari/commit/4f1509e61b98152369b1eb4e722352119e21dce2))
*   bug that causes non p2p apps to panic on startup (#3131) ([389dd748](https://github.com/tari-project/tari/commit/389dd748371282a6965d7d3dd052f4dbb8962b73))
*   console wallet now recognises wallet.network comms settings (#3121) ([162e98bf](https://github.com/tari-project/tari/commit/162e98bfe21b229f2384404a93853e3eb9823f5b))

#### Features

*   add persistent dedup cache for message hashes (#3130) ([08f2675d](https://github.com/tari-project/tari/commit/08f2675d21ff1e7fc8ad98060b897d4c9254e96e))
* **comms:**
  *  tcp-only p2p protocol listener (#3127) ([6fefd18a](https://github.com/tari-project/tari/commit/6fefd18a57c6c8efa13412291a132c7242e7b1ea))
* **wallet:**  add extra feedback to recovery monitoring callback in Wallet FFI (#3128) ([02836b09](https://github.com/tari-project/tari/commit/02836b099ebcf4261199dcf418cffb2c66bfff5d))

#### Breaking Changes

*   console wallet now recognises wallet.network comms settings (#3121) ([162e98bf](https://github.com/tari-project/tari/commit/162e98bfe21b229f2384404a93853e3eb9823f5b))



<a name="0.9.1"></a>
## 0.9.1 (2021-07-21)


#### Bug Fixes

*   accumulated block data bitmap now contains current stxo indexes (#3109) ([77b1789d](https://github.com/tari-project/tari/commit/77b1789d25b18b2a87432faab308617cd534b160))
*   fix prune mode sync bug introduced in TariScript (#3082) ([b374e7fd](https://github.com/tari-project/tari/commit/b374e7fd23b52cd14754eb320e8dbc120c72983a))
*   accumulated block data bitmap only contains current stxo indexes ([d8440437](https://github.com/tari-project/tari/commit/d84404377b7aa9142818904ab4408843c31081c3))
*   don't log tor control port password (#3110) ([12320ec8](https://github.com/tari-project/tari/commit/12320ec81e3abd3914a86ecfe9344aaa9083917e))
*   reduce UTXO batch size query limit to account for 4MB frame size (#3098) ([c4f5a875](https://github.com/tari-project/tari/commit/c4f5a8757786a3cfff09872784417f4ffa07c968))
*   update transaction status from broadcast if already minedi (#3101) ([32fe3d26](https://github.com/tari-project/tari/commit/32fe3d2651864744a73826386e3d3370e3eb30e4))
*   run wallet on windows terminal if present. (#3091) ([bd017bca](https://github.com/tari-project/tari/commit/bd017bca0f7f69d3c3c85fbc4385eedbfa37b8b4))
*   fallback to default flags if rxcache initialization fails (#3087) ([eace2ffe](https://github.com/tari-project/tari/commit/eace2ffecfcb68d6cb12fe9982e50e914d84340e))
*   update parsing of `num_mining_threads` config field (#3081) ([1f20252b](https://github.com/tari-project/tari/commit/1f20252befc04b2ccf8ea366fd90c6e47edfc7b6))
*   fix bug in wallet FFI header file (#3075) ([a835032d](https://github.com/tari-project/tari/commit/a835032d19e3e8dcca772505f68aead4154a5c1b))
*   update `Tari-common` crate feature flags to exclude git2 from lib_wallet build (#3072) ([a54d87f2](https://github.com/tari-project/tari/commit/a54d87f2c6f8820b4ec131effb84357dfb268fe4))
*   improve transaction receive protocol logic (#3067) ([60de24c9](https://github.com/tari-project/tari/commit/60de24c941418490e5f5ee50629c8e48cfcb2b45))
* **wallet:**
  *  fix UTXO scanning (#3094) ([81422f1c](https://github.com/tari-project/tari/commit/81422f1cce810017907589ff5313be13ac9d6c3f))
  *  clear the console after seeing the seed words. (#3093) ([7b1c29db](https://github.com/tari-project/tari/commit/7b1c29db51a404d35dc260c01cc67142c2048d07))
  *  fix when ESC is pressed while adding contact. (#3092) ([ffd7abfe](https://github.com/tari-project/tari/commit/ffd7abfe2e2309c3ec1a04f20265fefcaa70bef6))

#### Breaking Changes

*   accumulated block data bitmap now contains current stxo indexes (#3109) ([77b1789d](https://github.com/tari-project/tari/commit/77b1789d25b18b2a87432faab308617cd534b160))
*   fix prune mode sync bug introduced in TariScript (#3082) ([b374e7fd](https://github.com/tari-project/tari/commit/b374e7fd23b52cd14754eb320e8dbc120c72983a))
*   accumulated block data bitmap only contains current stxo indexes ([d8440437](https://github.com/tari-project/tari/commit/d84404377b7aa9142818904ab4408843c31081c3))

#### Features

*   add networking grpc calls to wallet and base node (#3100) ([17f37fb6](https://github.com/tari-project/tari/commit/17f37fb6ac47a148e55677c031f2f56a4f6f33d3))
*   add support for `/dns` multiaddrs to dns resolver (#3105) ([6d48dbe8](https://github.com/tari-project/tari/commit/6d48dbe864ed46cd325ecb79b0a339d452adfc33))
*   Add support for `/dns` multiaddrs to dns resolver ([db384c05](https://github.com/tari-project/tari/commit/db384c050ded9390919be299705a7aedcf6d718b))
*   add one-sided txns to make-it-rain (#3084) ([043f27d6](https://github.com/tari-project/tari/commit/043f27d6dc98f9831b96f4b73973fc6330dd4d96))
* **wallet:**  add contact lookup in wallet. (#3096) ([92993d7a](https://github.com/tari-project/tari/commit/92993d7ab59bd1e1d7911f6894f2dca0ef471af2))



<a name="0.9.0"></a>
## 0.9.0 (2021-07-07)


#### Bug Fixes

*   fix missing edge case in header sync (#3060) ([0f0fb856](https://github.com/tari-project/tari/commit/0f0fb856e9369d9c7e172fc59ee64d31dff4637f))
*   remove unstable impl trait from Tari comms (#3056) ([08b019f0](https://github.com/tari-project/tari/commit/08b019f03793f7677b72452e01bead7db89ffa18))
*   fix db update error (#3063) ([b95d558f](https://github.com/tari-project/tari/commit/b95d558f318d045da9e1172cb802555ae3eb5a47))
*   remove unimplemented Blake pow algo variant (#3047) ([347973e3](https://github.com/tari-project/tari/commit/347973e3e8fdd39bb74d978d14ff414c04a39212), breaks [#](https://github.com/tari-project/tari/issues/))
*   fix small issues related to #3020 (#3026) ([da1d7579](https://github.com/tari-project/tari/commit/da1d75790fcb4eb9a71b7822c3ede3d9ba598241))
*   update connectivity manager defaults (#3031) ([229830e5](https://github.com/tari-project/tari/commit/229830e595c6b3c97011547d18885e2c0a3e3f19))
*   check minimum number of headers for calc-timing (#3009) ([b3522027](https://github.com/tari-project/tari/commit/b3522027b824dd8bb50a7183397adba082fdf28e))
*   fix `Unique Constraint` bug when requesting a coinbase output at same height (#3004) ([537db06f](https://github.com/tari-project/tari/commit/537db06f33c49942d42e83fd6838f4fd405028d0))
*   cancel faux transaction when imported UTXO is invalidated (#2984) ([472c3086](https://github.com/tari-project/tari/commit/472c30865cfa5a3cc648bffe22f6ec6e7aa22572))
*   update console wallet on one sided payment import (#2983) ([f45cdc46](https://github.com/tari-project/tari/commit/f45cdc46f8485ea8978dd05edafa26d374c98fdc))
*   fix prune mode (#2952) ([f7dc3a44](https://github.com/tari-project/tari/commit/f7dc3a44d2f57102024605cc6f4c93bb326b292a))
*   fix ChainStorageError after a reorg with new block (#2915) ([7e99ea59](https://github.com/tari-project/tari/commit/7e99ea59ec11f19ba47e62729c3ee8b500d16c2e))
*   improve error messages in tari applications (#2951) ([e04c884e](https://github.com/tari-project/tari/commit/e04c884eb4c7aaf124fa5da5d80ecfc4b00817e1))
*   merge dev, update peer seeds (#2974) ([94ffd185](https://github.com/tari-project/tari/commit/94ffd185ff4ee9ce5575f28ae28e73464342b657))
*   implement cucumber tests for one-sided recovery and scanning (#2955) ([b55d99fe](https://github.com/tari-project/tari/commit/b55d99fe3b08b34485bf1a9429cfad32a3fac84f))
*   update rust nightly toolchain (#2957) ([812a1611](https://github.com/tari-project/tari/commit/812a1611a924b977a79bd5e7fe16eb986649adce))
*   update failing rust tests (#2961) ([ed17fee3](https://github.com/tari-project/tari/commit/ed17fee3e34d3985794af621ba131e066849abec))
* **wallet:**  increment wallet key manager index during recovery (#2973) ([c9fdeb3d](https://github.com/tari-project/tari/commit/c9fdeb3da90a297a75a53ddbea6823f3e6520b8d))

#### Breaking Changes

*   remove unimplemented Blake pow algo variant (#3047) ([347973e3](https://github.com/tari-project/tari/commit/347973e3e8fdd39bb74d978d14ff414c04a39212)
* **ffi:**  `wallet_create` takes seed words for recovery (#2986) ([a2c6b17d](https://github.com/tari-project/tari/commit/a2c6b17de6fd8ac14a5379b0c44d34c1e1e71e2d)

#### Features

*   bundle openssl dependency (#3038) ([7fd5c286](https://github.com/tari-project/tari/commit/7fd5c2865b4093d0c89341ee49062ebf75d5eb5c))
*   bundle sqlite dependency (#3036) ([7bd13411](https://github.com/tari-project/tari/commit/7bd1341159e8879ba9768b2268696f22b575fbe6))
*   add tari script transaction data structures (#3064) ([266b5f1c](https://github.com/tari-project/tari/commit/266b5f1cede2e23603ab1d7eab2e1b5fc577537b))
*   implement metadata comsig on txn output (#3057) ([8ecbb1f2](https://github.com/tari-project/tari/commit/8ecbb1f231da38f2e838c8acc79165b5b0a27136))
*   software auto updates for base node (#3039) ([cf33cdb5](https://github.com/tari-project/tari/commit/cf33cdb5403736f67ea71f958e3ac06413c3f8e7))
*   add zero conf tx (#3043) ([742dd9e6](https://github.com/tari-project/tari/commit/742dd9e6c9fc8c85bb6969e19489a4120d9cc9d1))
*   network separation and protocol versioning implementation (#3030) ([2c9f6999](https://github.com/tari-project/tari/commit/2c9f69991f7cfcbda113a55ceeacdf2c13d90da3))
*   add filtering of abandoned coinbase txs to console wallet (#3032) ([ae15fd9c](https://github.com/tari-project/tari/commit/ae15fd9c6203f8a6fe40be411fe3e4e590270ef7))
*   add input_mr and witness_mr to header (#3041) ([65552cbd](https://github.com/tari-project/tari/commit/65552cbd7b826e76a63ca50e53c41e8986eb9860))
*   Change script_signature type to ComSig (#3016) ([adb4a640](https://github.com/tari-project/tari/commit/adb4a64000f991df06454e86c303728af881241d))
*   update app state when base node is set by command/script mode (#3019) ([4a499564](https://github.com/tari-project/tari/commit/4a499564d59162db25693f194f00eb4bd91f0700))
*   add sender signature to txn output (#3020) ([7901b3ca](https://github.com/tari-project/tari/commit/7901b3ca2a6096e0f9148181b7a07ed16209d168))
*   display local time instead of UTC. Add new wallet commands. (#2994) ([b3760202](https://github.com/tari-project/tari/commit/b3760202992676b8874a155775472820e6a22932))
*   mininal merkle proof for monero pow data (#2996) ([ac062e57](https://github.com/tari-project/tari/commit/ac062e57903d493e09bff0ccee36660f7c088782))
*   modify gamma calculation for TariScript ([c88d789e](https://github.com/tari-project/tari/commit/c88d789e0e8ee2180279debb59f0d53e15db3b66))
*   fix birthday attack vulnerability in tari script offset (#2956) ([5174de0d](https://github.com/tari-project/tari/commit/5174de0d562b3ff444bceebeacbf3917b74dce85))
*   improve LWMA (#2960) ([db303e8c](https://github.com/tari-project/tari/commit/db303e8ca9632c6a6634e52cbfb6a79cd3e43a29))
* **ffi:**  `wallet_create` takes seed words for recovery (#2986) ([a2c6b17d](https://github.com/tari-project/tari/commit/a2c6b17de6fd8ac14a5379b0c44d34c1e1e71e2d))
* **wallet:**
  *  add maturity to transaction detail (#3042) ([9b281cec](https://github.com/tari-project/tari/commit/9b281cec339fea5cad48ca84cb5698302792373f))
  *  ensure recovery will not overwrite existing wallet (#2992) ([70c21294](https://github.com/tari-project/tari/commit/70c21294fa87da8198e8b79f8b49d61bd6bee721))
