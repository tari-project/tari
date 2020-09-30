// Copyright 2020. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use tari_core::{
    blocks::Block,
    consensus::{ConsensusManagerBuilder, Network},
    proof_of_work::Difficulty,
    tari_utilities::message_format::MessageFormat,
    transactions::types::CryptoFactories,
    validation::{block_validators::StatelessBlockValidator, Validation},
};

/// Commit df95cee73812689bbae77bfb547c1d73a49635d4 introduced a bug in Windows builds that resulted in Block 9182
/// failing validation tests. This test checks to see that this issue has been resolved.
#[test]
fn test_block_9182() {
    let _ = env_logger::try_init();
    let block_str = r#"
    {"header":{"version":1,"height":9182,"prev_hash":"ab924a4a44b6a06b2b94cf9c0c4b7a21e16c2aa62632ef8e2b92fe7a11ebf53e","timestamp":1589169791,"output_mr":"cffacafd9d84e3d72a8fb61ea337a94915c71b2cdf4b2ef07710aba787d7025f","range_proof_mr":"369cda1bf51a58c602ae2f907208e65105535e6631f2b59e9733d0269abafd0c","kernel_mr":"741148c4dbf26bee92b17b31cd3ea6d3e823a9beafe0074b6c9b97ffffb6de9f","total_kernel_offset":"0000000000000000000000000000000000000000000000000000000000000000","nonce":3082584105675462448,"pow":{"accumulated_monero_difficulty":1,"accumulated_blake_difficulty":40552612639391,"target_difficulty":53193415,"pow_algo":"Blake","pow_data":[]}},"body":{"sorted":true,"inputs":[],"outputs":[{"features":{"flags":{"bits":1},"maturity":9242},"commitment":"d2a13cd789918860abe8c98ecf834109c446146a61d72469d306bac7fdb5576c","proof":"0cd0b8766d45ab9b9469d956977542c95c2d60718460863dc10e07e3273c8919f6edbbc0b6c2aedf6fdf914b7b77805043747287654e4452226291eb69d4b076b0fe4ca418baf598e743ceb3f4a2904d6825cbf0e0fec00bafe7ee71b11b313852371a049fab8702f72361fe6f0012d9803241822c7359035ef48ee5afb74623cb5180060394c55e9067b5b1f9aaca5ed542f3dbfc502ea86012716d49888202ef7e7f9077029df32775d57f2d8560373437215a5571fb65a58b40608731ee02a107ae8aa261ddad5968d4aa388b44dc1cad8112829917c80efb04f3d0369606f0aae3fa83b72dced0d10ffbd69c4329c4994a4b1bae2486064d14be98ccd37030821ed9617a0476c875a0d8a0801c43f13f2991f6975562b014790280deec4b40adc8f747a36d87d90ce96c6523d5210df4117b10b41202187a223eca2a2014221891e237f30245928479ff177ece7b7c5d963c6fac11718797335622dd60649283d79040af2c07be2e067a369d2bb58c1073a8c79fd41d2f76ddb258785c67ea3c27ae1385754ccd3fa301a5c391cb55ec5f39c3e4f74cd7c153bd62f70e02fa32fd3117dc81f317a46bd11484db8e73c5c60f49e6c9ea060067a03f46d93bfc69bbdbb5004b8458fe5c7021857cbff7345555b8d070ce391209f0d503f77562eae4acf1b69eba0bd3e8e5d956e9c0ebff0e2388592a0c264756b242505324108d6463c4830354ddc850c74a348ce72c907382e7187d6aa60928acd1ece53f46bbc2fabca4f7eadacfa4d1c36b94486c8d67ab43ba60a5f25b6bfc0dc219217401979be5975f5269b8d6ba582955c35c84006999ceeaba9fed0f94f6402b0046fad7cb660265c6d2f7af80f623ea3a27c135aae2e160bcdd43f64d0db3c20053161cb7c523d1f85e86abbe16d4fd0982c88eb9107109082f063b6d9c98a30a"}],"kernels":[{"features":{"bits":1},"fee":0,"lock_height":0,"meta_info":null,"linked_kernel":null,"excess":"48874beaee641cf62c4a0c2f85c90542af781feffb9579fa2e848a05f6f9f86b","excess_sig":{"public_nonce":"3071e26b38fe420f0cbebe26bafdb03636f827c8b1ab6b8c3186a99b928dd848","signature":"cc9185ed7835cd23183d0b514169da1d118e7942402c669c42fa11f4e63e9004"}}]}}
    "#;
    let block = Block::from_json(block_str).unwrap();
    let rules = ConsensusManagerBuilder::new(Network::Rincewind).build();
    let factories = CryptoFactories::default();
    println!("{}", block);
    let validator = StatelessBlockValidator::new(rules, factories);
    assert!(validator.validate(&block).is_ok());
    assert!(block.header.achieved_difficulty().unwrap() > Difficulty::from(53193415));
}

/// Commit df95cee73812689bbae77bfb547c1d73a49635d4 introduced a bug in Windows builds that resulted in Block 9430
/// failing validation tests. This test checks to see that this issue has been resolved.
#[test]
fn test_block_9430() {
    let _ = env_logger::try_init();
    let block_str = r#"
    {"header":{"version":1,"height":9430,"prev_hash":"adb95b8df024fa7416936f793e547e9a2594548a86634d64ae73f46e17f0f0fc","timestamp":1589198340,"output_mr":"20c2bbe511c726b8ee93cb1ad668ed4616d041de76163d47dbeaf92530a5ae02","range_proof_mr":"6e776c9e74b10653497578d949d5c936e7dcfbfee61490991eb16a95530d38c4","kernel_mr":"a798a98c3a0b011c64e0c64a87df7cfc5035aec52911de29ffbc25b383f893de","total_kernel_offset":"0000000000000000000000000000000000000000000000000000000000000000","nonce":12558657073957944672,"pow":{"accumulated_monero_difficulty":1,"accumulated_blake_difficulty":40737626009318,"target_difficulty":72373773,"pow_algo":"Blake","pow_data":[]}},"body":{"sorted":true,"inputs":[],"outputs":[{"features":{"flags":{"bits":1},"maturity":9490},"commitment":"be25030d7c3f94be52ceba78043296e86e09b51c506ba7aa373c1ab8894c2369","proof":"4c38b294e4d5e8b6f377806f4552b3e73cfbc25c19d174f300a1f7a4c5bed60672338c76887966c2c35ad9829da1e1acd33148e159f2dc1cb999884503a71a7e12842b1115c8ecd7ea8f964e6e9576e317394f08667849dbb35385c5c0df63446852c1dd2d12efff3294e31c4abbbf3b4a3f9b7edc6a4579263a63afd244293d8b4b3dc458bf12e0c637d9678ba0e4f3fa5bca54455448c05a277197b7fba007bc7cc446e5842ef8ce3077edf236b7cafc3c3533a604a46c0ac3b00968abd006939651763d3396b4ee610d5dd75fd525a98ec8d6e5249bb37d7d69d9a605800fbaa619c07961aed2fe44035ed7eebde292b4d77a23566226618752e710f1737eb6657361305e3fe887debeca3c8beb80f04a5cb7ead57728a43d1935511a0c3ad6dc385bba0b20c636102605f2dfafa5716b7bcdbfc24134e5581d9c3cca52746a7b975c8c05ea43fb77c20f0947372200ab5b70ee0a886dcfc08419a33d492bea8bd0d0343c1540cab014719a8f6be63535b5cc5e6e679837e5b0dab67e6d555a8b26f2a38a1e2d73936a09d3ec1ee1522e3229bcc86efc70cbd5ee00ebb06c00c67b1a4cade2426aa3279c7596ec2a918415690084831f03b9ca5551895f3c14aef638625235eeda34c625b8978d47f2b60e1a1108a21e8f931f866de514288ecc6c7a738740c5dd1c277a0c8bc7f3379f890e0e1469dd62082852138b3177e8c3a14fd9acc3ba10b1efebe2a57b9e699c22aa16869db8314635255a71dd381408af5665f678939949941cb4542ca734bf0e2facb075f0e9c911b607c16f3dd0666c5196464e9e484dd5f7052231bb7c93fe6627986a70ffbd360c7a31493993aa622d0bf7db06cb5653dfde72a48e63b293167c43f21426a958468854dc0817be7d271338b252fb6c6dd478fdba4f928ec01a0ee25eeb4837d7f862830307"}],"kernels":[{"features":{"bits":1},"fee":0,"lock_height":0,"meta_info":null,"linked_kernel":null,"excess":"bac85dc24639f472c703e7639812400ee1b1d10a08932151a5a661f83d65ef22","excess_sig":{"public_nonce":"acd0417afe2dcc611a9a05db9308153ddd618abf5ec1647fc627099852dbb701","signature":"b271f7c439f28825da43cc76623620eb75d0c92d74c8202977b1e8c9be87c500"}}]}}
    "#;
    let block = Block::from_json(block_str).unwrap();
    let rules = ConsensusManagerBuilder::new(Network::Rincewind).build();
    let factories = CryptoFactories::default();
    println!("{}", block);
    let validator = StatelessBlockValidator::new(rules, factories);
    assert!(validator.validate(&block).is_ok());
    assert!(block.header.achieved_difficulty().unwrap() > Difficulty::from(53193415));
}

/// Commit df95cee73812689bbae77bfb547c1d73a49635d4 introduced a bug in Windows builds that resulted in Block 10856
/// failing validation tests. This test checks to see that this issue has been resolved.
#[test]
fn test_block_10856() {
    let _ = env_logger::try_init();
    let block_str = r#"
    {"header":{"version":1,"height":10856,"prev_hash":"f51026f9ee73772f3d35028947e5938c878e0de476e6756569b1c4b93072ee77","timestamp":1589370699,"output_mr":"27aaa81b85550d44f75a464caabc9451e859ba3f732ac29417fcca80d4621e07","range_proof_mr":"17c69e41cb222136624478eaa3e732164cd39e03f5a2f462b3d9b50277e39f15","kernel_mr":"1f5e00dff70440fefbd4029a042ee0a5b439c4a851411a57ac84924412373004","total_kernel_offset":"0000000000000000000000000000000000000000000000000000000000000000","nonce":10171711902839592350,"pow":{"accumulated_monero_difficulty":1,"accumulated_blake_difficulty":42121484020544,"target_difficulty":87321441,"pow_algo":"Blake","pow_data":[]}},"body":{"sorted":true,"inputs":[],"outputs":[{"features":{"flags":{"bits":1},"maturity":10916},"commitment":"7c4f17ffed87a67aac9005859ae48cef9c187cf2a4b33df253cb3e0f02ff5c43","proof":"fcb92dcf552616869bb334cfb1cac4828efb4eaa4a52e5cdd7757c2cf81fe14d18cc2927ee1c072223cd38760c07528af4ccde9ad55829351f1b067711eda54122ad366eaa33ac065ac9734a5013e8355721179b462522600b2f46d18b8ce96c4e2f0a12e4585cc2c89af1ba1b058aba5743c30357dd13e15eb7e6f27e25697109154a0b3ec321dab4a6d2eb66861108866d020938d891478bbd7d65f96c1a0cfa31e48c974e174b181d49013e2620cf1d2538ed680e0cb22c7b70fecf68710c47978c156f1852457c8869dbc104c242ad6c4d21a76c7c2d17aa9dd9f9b48f043485da8f6d2e41bd8d407bdbb213f5e5242e2c4b2df3992661c86c4fa5bda1282286ff0dd4236c65dae074e75ae6cff94c53577ef157b096e15f53c7bd3bbf744c1a617d46c9686e3dc8f0effd829c089e237d7b2a73690b1f0eba941cd33b4caa36e0144319b1a51ea91bc60f6d5099e48111b6897b8a6a42de93217bc0e94e708b0437a2ece17923b0f08df9fdca3c1eecaf75df3defc9e1a847220c80c8597c02ef3c0712ce6162d6e49cf67a81bda76b9877ba31e6fe913ac188452c704da074dad7a2b4a2bcbef6abf05b381754ee2519bd9342c6b3915736a5a5ddcd677ee5555413e93f7ed10ce35d0edb47287b8232a803f11dee1dbda68548adf86288c407bc6eb3aa3407ebb2d176c908bf84305f54d19eaa8f943a09f153a5d258a6f9d9c476c797b9cbc695e2482aef1a950ac22378e88aaeca92389febc48109b87ba3e41bdc9f475bc5c891d4ef30784bcf38cea39c1ebf8f7d18326e4f7b415870a13857814d96ba111c9bf1b80e77ee42b32e035b4e8a70c5b314b960f26f90cebc03754895dd4dc0e0e38b8d8ecdb957e557344ccd2402fecf4378055e08b43f848b3f07ca205ee2c7f0bcc08f018cb5e7f58213f028ed97ab98dd60cb01"}],"kernels":[{"features":{"bits":1},"fee":0,"lock_height":0,"meta_info":null,"linked_kernel":null,"excess":"42c85a5910cbf3a7aa9f3e0463523d2680a6896b30b569e9f4d95592ac8c4661","excess_sig":{"public_nonce":"b234797842e9018199475bd6e965565e70fa62f9f9b10bba852f9a0fb309b64b","signature":"528429bff34f46a960434c3a50da7409c76f225142be79a72fcf93306d8d7c0a"}}]}}
    "#;
    let block = Block::from_json(block_str).unwrap();
    let rules = ConsensusManagerBuilder::new(Network::Rincewind).build();
    let factories = CryptoFactories::default();
    println!("{}", block);
    let validator = StatelessBlockValidator::new(rules, factories);
    assert!(validator.validate(&block).is_ok());
    assert!(block.header.achieved_difficulty().unwrap() > Difficulty::from(53193415));
}

/// Commit df95cee73812689bbae77bfb547c1d73a49635d4 introduced a bug in Windows builds that resulted in Block 11708
/// failing validation tests. This test checks to see that this issue has been resolved.
#[test]
fn test_block_11708() {
    let _ = env_logger::try_init();
    let block_str = r#"
    {"header":{"version":1,"height":11708,"prev_hash":"b0cefac87af2978469d816f9b17857bf98d1229913412b7deb05e58219b44a38","timestamp":1589484682,"output_mr":"01ddf5ea2c7c2e0b4f59d1afe9f6b49df65d1a48fb21c7f7bd18e7eee224bd73","range_proof_mr":"97568ec5753e0f59cea8a98b5156e7ea5f5477f86b4ab12175d98692d8c39ae3","kernel_mr":"c7ca0f317bd55d72659871dde925e36881317b4bdad81102c4074f2049dfcfee","total_kernel_offset":"0000000000000000000000000000000000000000000000000000000000000000","nonce":14528220554652063826,"pow":{"accumulated_monero_difficulty":1,"accumulated_blake_difficulty":43814444759341,"target_difficulty":144466012,"pow_algo":"Blake","pow_data":[]}},"body":{"sorted":true,"inputs":[],"outputs":[{"features":{"flags":{"bits":1},"maturity":11768},"commitment":"5eae3b3a881b4f9189fb03a43f7b4a26340b9687ecd0b174cd8d3366f053be7e","proof":"9ac939e5da4bb12cdce31fdd8b075427dc94b9e6c91f527371646d00001ba95c8e717ea865c55b6cd21f6be82b018f17339bb9e16560f06db0eed4c69c2e1d5d929881660f1d3b7ae78a52a04324174e56290b8f617a1347dc226fb469ab8f5dd0ef20ca2670917a7d243bd4c50d6bdd181b67a101c8235844eb93d3ddb935336b9457c7ce83574cf9a5041fbc32af2a87b46a7b7dc1b159c9ee147ec2840e0394bc4b1e1c2cc5ff2961f0d73da1e057aa7d5dbe89210193c50fa3c801c23c0a601f792931f173805429cf48194e08493116237cf9757aaa5460f16710695503e8439c1d61780613ea520c321d1c36a7fe5e9dd24ed7f63d862429e4260faa7646ae930291dbd1f461a84809f5e144aeca66633e3cc971f2eebf46378834ee349a65e304e8a2039f6e49313d47e5be608b377d9502505f8ed0853b1b9dbddd4c96ceff19323fb6a654cf559367aaa9bfef2449be5ae33baffc2c68e65e12c9543e09e9a708a6029e3616a8f2ce7e2dcc3f43d4308f082013e91119fd9a9f8848985336c72fa2b75e868b5040c3b3c47be58e78e842f2aacca4a15c28450a4c4820f3d1a8d86f28a741f2ff26156978d37b56dc3bd7220e397ab7fd12d5a5d756609cda1e2b942229d441787d40b8a488081d27d9de45f1ca28f5bebf76b1435bfa9b33d727f9810097e2abd59956c537ff1fbb1eab693043b156f851c7af206c083abf0a6f6407666ae6a6f2dad3c220bd2e5fd0eda86af3a7a1106b36d5270ea4880a5331ae2c9613a2543a77589f3c09aef055c58fbe0033dbf4339977d818ac340ee78b9f856907c69fb3d633dd396dd31b3fab60551846ce19a4b4cb4c6b5c3969d68ef098be66ba8190991eac61ae29c05069c6f7549f47db33ec379a034d7f1b6d204dc520d8ddd76f665df70109741a62568e33bc7996732236172a0d"}],"kernels":[{"features":{"bits":1},"fee":0,"lock_height":0,"meta_info":null,"linked_kernel":null,"excess":"5811e5d3a6a5ceb500161c8b24335b1c621fa23c601d9abeda2b85151a5f3568","excess_sig":{"public_nonce":"4a4d729c05657fec05b5e3aa2633f739cd65b657ae87a930510c6c44a4b30529","signature":"ec0f50bf5ee0a9bc02e9bfee46ad95db712faf0f658d81298ef2f684f1cdcd03"}}]}}
    "#;
    let block = Block::from_json(block_str).unwrap();
    let rules = ConsensusManagerBuilder::new(Network::Rincewind).build();
    let factories = CryptoFactories::default();
    println!("{}", block);
    let validator = StatelessBlockValidator::new(rules, factories);
    assert!(validator.validate(&block).is_ok());
    assert!(block.header.achieved_difficulty().unwrap() > Difficulty::from(53193415));
}

/// Commit df95cee73812689bbae77bfb547c1d73a49635d4 introduced a bug in Windows builds that resulted in Block 30335
/// failing validation tests. This test checks to see that this issue has been resolved.
#[test]
fn test_block_30335() {
    let _ = env_logger::try_init();
    let block_str = r#"
    {"header":{"version":1,"height":30335,"prev_hash":"441cdc102cedd0721afab5fa9be80003964d6aff38d5663809929e8464ca1039","timestamp":1591765406,"output_mr":"30e062cd20d34103471789ad4413710487b38d81a0f33968f8e099534ab550b5","range_proof_mr":"6fdf72734d0ce7e4f07430702d238073f72cd42f46ebce63cd4b466cbe97b79f","kernel_mr":"ccbd6d03fd15cc91b9d4c5e81bf708587e872dc57b929da0e8bf76906e399d9f","total_kernel_offset":"0000000000000000000000000000000000000000000000000000000000000000","nonce":16201801948487771403,"pow":{"accumulated_monero_difficulty":1,"accumulated_blake_difficulty":163057447959847,"target_difficulty":151763817,"pow_algo":"Blake","pow_data":[]}},"body":{"sorted":true,"inputs":[],"outputs":[{"features":{"flags":{"bits":1},"maturity":30395},"commitment":"b81d9d624667f6546409125eacefd753a79fb6dc746862ba4d35fca6e35b0c05","proof":"9a57fa21e2474fa78b8a98b7a9bea3fe7f9ddcc9d95c2bade6bc2e232b8027093c52bb86b84e830def58006758934cd215464fb1613ee65fdc0430c52303d65c16d752c88d03833333ed66590ff7b898f167550c745ed69da7bcf4089d5fff2bb23b1cd0bc1595191642de8e57bf5f76943dac8cbc9226cbe28a88e599b1d20593804d3650fd6191d42860ca258964bf87199ff93639c67974e5cd9a613d700cf76bbb78fcb79ae56cb93acb6d6e80212a5943f9f62fedcb9b546ff698fdcf0293a56c9633b21e5e7ecb5ad5e767ff15844bbfad61f452fe11b477e5a9e4540db6c9df0fade1704c702c05f2981730124b1d628e103dc4b295e9f0a64a9c75595290e3684fdae3d4dd787e95133a46636f7000e7abbc5017c505527e0857b80c40395a2011ec7e6f3018a570d9eefa4811e53132961bb655e69f85221677311b2867b4f38c37ef37fc7ef5fff4bc066e7b00f0850f211314cc101d87cdfa136978b2c07012d986854e927221f1ab424463abbf2a2f670133915d8162d49ade4a1e252bf4cc8b60dba8765bc0d80d61c5f0173479d56d7d5a4bfa03e3adde302236ceb82f44d054847b0ce2986bc7703f462ac246a03a7757d91de90a0e454e160215eac5d428ea2804817cebd6c583213dc7a396049e66e35c7dab1e158f2731a444f26452d45dad7b237b16f7bfd38ba4a9c7ba904f33cd5872cdf5370acb2c22ffcc3107fdd0517864b695c59ad4469c90b83e8c8f50fcdb7b5e5c920cf3474829534d29dfb0a4a0f2bf78ad88e1167c24ae7f118e3ea1276028536544283c50ead9cb812692aa54612ac49630198f49c131d2b0f250978fddd5ee8f45e01c72520695e85512fadad3b74127ef9a947bfb876a92284e7bb6d04c0907e18c0ec9cbd3ae5079807a22f9f6b8f66e838393e12806a1c303f41a272dca10618a0a"}],"kernels":[{"features":{"bits":1},"fee":0,"lock_height":0,"meta_info":null,"linked_kernel":null,"excess":"72a7a9c0452160fad7a59d36a68ea9531b268aec4f5c1afe451665065361462b","excess_sig":{"public_nonce":"aaad737064d9fe82bf1ecf1e2d7fce7beb8319ee31b54d2846f0d2f0fe51107f","signature":"73107d96dc164df3f12552e64359a0219137c89ba454745925cf945f11f5cd00"}}]}}
    "#;
    let block = Block::from_json(block_str).unwrap();
    let rules = ConsensusManagerBuilder::new(Network::Rincewind).build();
    let factories = CryptoFactories::default();
    println!("{}", block);
    let validator = StatelessBlockValidator::new(rules, factories);
    assert!(validator.validate(&block).is_ok());
    assert!(block.header.achieved_difficulty().unwrap() > Difficulty::from(151763817));
}

/// Commit df95cee73812689bbae77bfb547c1d73a49635d4 introduced a bug in Windows builds that resulted in Block 33923
/// failing validation tests. This test checks to see that this issue has been resolved.
#[test]
fn test_block_33923() {
    let _ = env_logger::try_init();
    let block_str = r#"
    {"header":{"version":1,"height":33923,"prev_hash":"2667f3f8b03c51536580a6d0aab83c95d86d9e50d323a7f1657493aee0f2c1f2","timestamp":1592196071,"output_mr":"192b033bfc45b9e58e04ce74932ac5fde32a2f3d56dcbe4b5d6d732ee2f93329","range_proof_mr":"6896196eebaf95739f252ea3768f95b28eb3643ba01e5468b024f0ccabe64ba8","kernel_mr":"ed2f4ab11ffce5f154e4ed6e3d15306643f4fe8ad15628687a0e4daaa227b0bc","total_kernel_offset":"0000000000000000000000000000000000000000000000000000000000000000","nonce":16209196803159005034,"pow":{"accumulated_monero_difficulty":1,"accumulated_blake_difficulty":179054684906801,"target_difficulty":1138781471,"pow_algo":"Blake","pow_data":[]}},"body":{"sorted":true,"inputs":[],"outputs":[{"features":{"flags":{"bits":1},"maturity":33983},"commitment":"409c7c8bd3e8f569d497e7adf6b58cf55feb0bdfb2a17b4d59579c2326b6720f","proof":"7ed3369d214cb906998c2aa7100df0f290e8004d5c00229639962c3a5f0e8b7c1e88b2b2b59d32283f14fe2cf478632029326a0ca2abe639a06d81a31af30c3466a4e2cdb0ed613a5f78948ce53fafe62dcfce139498440d59d7fb7831141c087a7948b035a18e7d08fa1bb50d61f12cc96fb7878f0aa208dfdd7ef8e848c95cce12d3c4a75a0b52ae0b5a7e500f9f6b4b19a87b49d2ce0187b8ef7e7c5be907fdeb0eba5e10e9a336e3225d027fba8457e52c55a4b8fc4f7c6c8c10d38cb30487457b9f56b89112dd722941ab176b2295831b3208b0c03f12c4caac687ae80e0eb6ad5e51180fedde2fe228caf68084358c9c05f84822677608ac83d3aaae17ba498cbd5c3aea09d24f46ad6ddd9278de6f7372d5cc37ae5428ead47cd53a71f2d52df3224880d91bb5c9e39f62b8d27f74519298cdf03cceacfeefa4f39212ee8b6ca4eedca825b166fe644f7fac8152183f01a9bc82f9947fcb22b4112742da10ed9ef4be89665b54a66548ded1922e864177a745870ebfe4c412ed2a7f5a9ca329cb7c58a945d765c826dbf2a6d2ba1757845b82f681c9c9ad7a122a784edc48e6eb5a2b233af1a60848c67cd3f5e46774d8bc948092201a4dee32a0db7b4ecb0a392d38e7686a17f9ef2f0527b74032c41c76ece94a9d3a49e322154b4a48b2104b8a58ef3e84e7fb6532a5ceccb87a678a69d23ece5881b3c7b95a9f4f9a2cde13752ef953ac8deb58b9cd0d2cad66e7adeb7c6114123022f1e293695c0ce9b56d76b90542adba7d05fe60f344c7e6d684a156cc8d90f453480f24e019128d028a38a5067d6bed3440a313af6b10b38c80799fcb785d9256ca0bc20d20c11f500f6bdf113b64fdba74944355d1ae34584c4b96626b99f47e25826e240ff85303e656c8cb50d1c4be116975652001263a4cfa803e551f782b6395b03f05"}],"kernels":[{"features":{"bits":1},"fee":0,"lock_height":0,"meta_info":null,"linked_kernel":null,"excess":"f0dc9a40985667c7b07586282bf6f96de7b9a1bbb17e28f7a7cf350a499f3727","excess_sig":{"public_nonce":"cc59fd512cc8659f11b33a5417707fc50157224203df9f2667364623c5a77f4c","signature":"2d4114041e18e66f0462c2f752b629c0b55ac6a388ec6ee7a44befe693a6430c"}}]}}
    "#;
    let block = Block::from_json(block_str).unwrap();
    let rules = ConsensusManagerBuilder::new(Network::Rincewind).build();
    let factories = CryptoFactories::default();
    println!("{}", block);
    let validator = StatelessBlockValidator::new(rules, factories);
    assert!(validator.validate(&block).is_ok());
    assert!(block.header.achieved_difficulty().unwrap() > Difficulty::from(1156044026));
}

/// Commit df95cee73812689bbae77bfb547c1d73a49635d4 introduced a bug in Windows builds that resulted in Block 34947
/// failing validation tests. This test checks to see that this issue has been resolved.
#[test]
fn test_block_34947() {
    let _ = env_logger::try_init();
    let block_str = r#"
    {"header":{"version":1,"height":34947,"prev_hash":"3bc175fa4f3c94f13ba4ab99fc803f0883e5cfc3f55f1b7e22c5ce64166d6f1a","timestamp":1592325285,"output_mr":"f27e72f16c4396b93deabc0f2219c3ffdc0dc860bd5b198f7b567bcf6c69846d","range_proof_mr":"908961b7bf09d1bedb1c977ba98e0884e85eaa28f89bc357e9ef1afd9f8a145e","kernel_mr":"20f906342d01c6449b1e134c489cfc47b8253cb13d23884eeffbfa6a0b826192","total_kernel_offset":"0000000000000000000000000000000000000000000000000000000000000000","nonce":12532829394156024964,"pow":{"accumulated_monero_difficulty":1,"accumulated_blake_difficulty":188768132507609,"target_difficulty":448647867,"pow_algo":"Blake","pow_data":[]}},"body":{"sorted":true,"inputs":[],"outputs":[{"features":{"flags":{"bits":1},"maturity":35007},"commitment":"a0150178f00241f49dadb65afda07fcc1158a1cad111699f104024169bd47401","proof":"46a1b63428273fcf1cc31d2ca22f027775d0fd7a93a243ac884a553c81b17348d287ceb7d202d4389300970c7cf4e236a4eaa1f27472dacdb69abccbbb9e09174039344cc8bb78ca508b1edf381fa54699a01f9a74fecd6dddc2848d4e28761cd4fc0054ecaf964750dd99487540a633f539cce8f6299d29b410e8f9bbd14b121c2955e544977623e4ec37aca4096a85875677ba14e157e8949e00a84c09d80ff60fa2f1f9993502df91f1170bff5fb756f1fb4bce32677ee60ff9afa2e3b107043b240204e3b8d98aefd3a801394a9fd9922a1bbdb22ff082e6a0b43a4eb80fc22ac54138386b57b1c1558bc514133a923d11d5bfa977113faaa608c2792b44ae34cd9b81de8b68fe669772793ade290417e89a71e8eb64399b5c018365b15d2877a7599e4db884346d8ef6925420f9a81aaf3c0f3da7b94fcbae09e696e32f80410d9da387149c7e648a6a66dfd48283256ec94d3c32beb80fdd73c241c45096d8a0690662786dd534d2c798d83df08e61055266a4ad8fa76e8934383a1959007ae398fd59225192cbff1c4b8ef5fd7ca29a9b5fd2ffbe933a5b6d492dd85ee4aa567e8225dd3815bb45bd85512f9a01b6aabb6b3b5a3248b71c0491cf1075f637181fb54388bedf121161d4d2d5638f1cdaf5f81f06ee9d3682c69c7e70041cef8d9789121e99bd7a0163ad64fc836440e6b8881961c2501ad6af6f138e6d7883f87413a7efd72173a3d47aab940c37656ee7b01bb2da9a0c076460087f64b851b3ac70c5f3f561ada4b5a4dd920ced3f17dbb83061f96906b1d0af991d5f66a5e072a5568b63656f45b0e2ee378ceeddb8ac8a8e7ffae87e3780b4eae1717f2f71b86d1e04ca27ebf13b492ea79945d3ed018d6831a494f24fee89738f0fb017fc6baee803c5e13ce22843a73331eebc88c7fcfcdec45352295e655dbb02"}],"kernels":[{"features":{"bits":1},"fee":0,"lock_height":0,"meta_info":null,"linked_kernel":null,"excess":"ca540a21fbc2e37ff9e81bb697338e04f17701bfeeefe46c95df221451f6670b","excess_sig":{"public_nonce":"228881a8525eeaad930b372454958cb9241a75f9ee4c74b9441563db66899d67","signature":"84687ad164cb0311ce868272dde70f76b8bdb3e7840796daf3efc59a8927500b"}}]}}
    "#;
    let block = Block::from_json(block_str).unwrap();
    let rules = ConsensusManagerBuilder::new(Network::Rincewind).build();
    let factories = CryptoFactories::default();
    println!("{}", block);
    let validator = StatelessBlockValidator::new(rules, factories);
    assert!(validator.validate(&block).is_ok());
    assert!(block.header.achieved_difficulty().unwrap() > Difficulty::from(448647867));
}
