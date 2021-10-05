# Changelog

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



