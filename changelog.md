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



