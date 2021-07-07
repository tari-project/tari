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



