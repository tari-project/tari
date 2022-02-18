# Changelog

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



