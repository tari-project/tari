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
    validation::{block_validators::StatelessBlockValidator, StatelessValidation},
};
use tari_crypto::range_proof::RangeProofService;

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
    assert!(block.header.achieved_difficulty() > Difficulty::from(53193415));
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
    assert!(block.header.achieved_difficulty() > Difficulty::from(151763817));
}

/// Commit df95cee73812689bbae77bfb547c1d73a49635d4 introduced a bug in Windows builds that resulted in Block 33924
/// failing validation tests. This test checks to see that this issue has been resolved.
#[test]
fn test_block_33924() {
    let _ = env_logger::try_init();
    let block_str = r#"
    {"header":{"version":1,"height":33924,"prev_hash":"3a541cf21e19650697d8d785ba9b5f38e86868af94a7fca4adec3c3bcf93aef1","timestamp":1592196113,"output_mr":"745f901d46375951d9cd451c65947e3fae4da4772ab72f7b9586338263726800","range_proof_mr":"2ad6758036ff1a6a3b74fddd2a93171d7cf9de8c2fd1a95c841aa26d0041d957","kernel_mr":"8fc372f396cd03b81b6758818d935c3a3a533ecd2ce46dfd9166828f3712f256","total_kernel_offset":"0000000000000000000000000000000000000000000000000000000000000000","nonce":7018292014280356823,"pow":{"accumulated_monero_difficulty":1,"accumulated_blake_difficulty":179059216796238,"target_difficulty":1156044026,"pow_algo":"Blake","pow_data":[]}},"body":{"sorted":true,"inputs":[],"outputs":[{"features":{"flags":{"bits":1},"maturity":33984},"commitment":"eed0c27c76a1a479845f90a9de6f69003c64bda8bc55fdbabd02829a88da767a","proof":"acb3092a2358e219ef502d227ef5c5ea9bd99e0e128dc557756b7207870f192b0429bd112d0878ff57d378b0b9d161b91876a868333cdd58e479d947dd27a8101e1e53073a5794a7e5a84f5e15aad01db2fda2f8da965c21c9e4d8cbfcee0f5efe56cae568776e6370034736879844f2115c4fe92d70deb90bba553a3250301cb06a6992870e3113702d2672c286aca2631a2b1ee910b4ce29d73a0d8ef80905aa93fcb696edc0075ceaff2fb5c404062c5723fac95e0fce92862756a76e1204e3049163227d3efdf2658eedabf8ac1552d1c3233217308225a3942121a7d604e014bbcfbc5b4b8368f8bde6017916dca356d4420ed3f0f873ad41ebc8a403116eba96a42248b98913f975e159d7eb2626a84b739169af248d5a7bf0674ef332cefd7ba3a1e6ad9306ffb67aaf0f4832c92aef63b20aac10479008ce8c40761e284389e8a6d4425eabdaba2d08d59a98a5dc5f7b802aae20e57351a8a939c176b428c996526d079e121e91d1489dc907eed9d6bfde363c27949501c377ba806b7a75b99a69e04d944c08f07dd3875eb6d47bd758b1f664cf8bc3251346469b7fac4f17e03747864c8d3f3981c0eb7269e1c00cb9af7181d78c25156afbe91a247c6a7d08244a5c4284f552af110fd56b905da7719794e8cb9f95bbfc57139a60865b86181a00660052a02f095d886be2a76e612fda7b9c756df07561421be72fc21029d95d2bbfd1af294308b0d38c6871db3078f329902aa59fab56731fe64300940d39d99b35bd3574331b932124eb01dd2cbba5af88765d3eab3371caa51a029e29bc7887b0c0d65442724832e3fbe8d12a0bfe81dba305672a56ce652146f9193c634df031057a9050a3c158818469fdf0331c0a59d3638276435143b308c3027de3e59a5b97a02f02767798d4989d99e824a4138831bf0b7ee939f0970c"}],"kernels":[{"features":{"bits":1},"fee":0,"lock_height":0,"meta_info":null,"linked_kernel":null,"excess":"4897d2256b8d8004a48d8b375de69c52b499d3041648c773e4edf1afe5aa1615","excess_sig":{"public_nonce":"2ca3856341ad84edc86bd7dde58e8cd63ad905a0968f1fae41a4146003da263e","signature":"7c4b6544dd8afe01fe813f368b8f4e6a0b1a9328ff4841293acddf87d163dd0b"}}]}}
    "#;
    let block = Block::from_json(block_str).unwrap();
    let rules = ConsensusManagerBuilder::new(Network::Rincewind).build();
    let factories = CryptoFactories::default();
    println!("{}", block);
    let validator = StatelessBlockValidator::new(rules, factories);
    assert!(validator.validate(&block).is_ok());
    assert!(block.header.achieved_difficulty() > Difficulty::from(1156044026));
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
    assert!(block.header.achieved_difficulty() > Difficulty::from(448647867));
}
