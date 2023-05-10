// Copyright 2019. The Tari Project
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

use std::string::ToString;

use blake2::Blake2b;
use tari_crypto::{hash_domain, hashing::DomainSeparatedHasher};
use tari_mmr::MerkleMountainRange;
use tari_utilities::hex::Hex;

hash_domain!(Blake512TestMmrHashDomain, "mmr.tests.with_blake512_hash", 1);
pub type Blake512TestMmrHasherBlake2b = DomainSeparatedHasher<Blake2b, Blake512TestMmrHashDomain>;

#[allow(clippy::vec_init_then_push)]
pub fn hash_values() -> Vec<String> {
    let mut hashvalues = Vec::new();
    // list of hex values of blake2b hashes
    hashvalues.push("f41960a92281bd1b35b764a4bae2aacbbe5323f9d19fdcef8a0f6b3bdaf62aa18b2d8cb1dfa1f036d6516d9767e27f2db630ac623c9019876e8361a77693a218".to_string()); // [1] 1
    hashvalues.push("5df76cd358050159e016620f955d66185ba141b295a617a5075a1860a06e269b3822557108f281734da189d3ca12d4bed9e79be44ef99d008b40fe2f4c0d5998".to_string()); // [2] 2
    hashvalues.push("fac58aa36091b0a736977e5b4131f103dca621dd86ae029ec20d32feb107d2258598c4518f1de9b32e3e919e0643e490d0ed0461f1364bad5960dc09819dd09c".to_string()); // [3] (1-2)
    hashvalues.push("2ace5e8b05a801f016d29aecf8bef9c596d9be5e645d3658b5d3afa74c7f9e687734e214fbfac47102f3e708a47ee2f3d1c69ce7ef122e7e1d3a8a718036fb09".to_string()); // [4] 3
    hashvalues.push("57431a18216bd392fd5a2227c88f0a9ab3ab3ae1ebb4859d9b1c8d5cb5d00f4ef5963b58a231b8ea54b52c04ee3178d7aa67acb387b109214d8c6d3e5a053700".to_string()); // [5] 4
    hashvalues.push("7fd592e7c65d59672ca125a669f84eb93dc574b5cab542abc9d3fe9ef870d1cf68f737fdc9d78f2b34b1a828761c900828a39092211cbd72f758d0d7d2fa01d3".to_string()); // [6] (3-4)
    hashvalues.push("e294c94a8e89adb73964309ed01bc84ff2bdd824a79c089b4f0600dd1fd02703a1753174c4a1943f5ec29fe8f6e0fd6681ad8169c5313f0ad191e353f4c59750".to_string()); // [7] (1-2)(3-4)
    hashvalues.push("3fdeda4e128dd735acd9f6d97811f0a333968bd1f984230c5d2d0eb06961d3ff95a23a7c83913b06a1a3e40f21740820847cf9499eafcb298b10e5b09a1f2bbd".to_string()); // [8] 5
    hashvalues.push("8e98aa650e1c0dffa58da99e64ed0d075f98dc25bfaf41fcedd1296c6b745c5895c53cbffc193ec4f64ea07a3797396d5e46ba1bf835f67aa11ef8063d9c716a".to_string()); // [9] 6
    hashvalues.push("ec200c9b1920f0196466a7ee5fffac20938781043953e749080ed7f644768f16b604f904c18d0d56b507fdfeede245f75bb6f80c4c94111950355b90a28dbd1a".to_string()); // [10] (5-6)
                                                                                                                                                                     // index 10 ^
    hashvalues.push("849f295777f919b45f4ca987058b0ffa0491dd681c499fc33a76b51d1841ffc75f809f762b847c3ece9e87f862a3e18124c1095b8f9edd453b4f3e12b4353960".to_string()); // [11] 7
    hashvalues.push("a47af3fb62de49b42cce01dd5ccd1c60117d7ba14a5fd8bb14d781d3c5417a13f2d78a6e5077f3473ad232c73a3be87e6bfda1d446a400d716fa2b45525ea698".to_string()); // [12] 8
    hashvalues.push("6399503dd769db4876f887ad2d73c4502ebbfa3ef18e69cdf3ac8a60478395b0da0851a134c4848af5d23d3063694e9b17c082516fcf253911f9ac42d87aadc8".to_string()); // [13] (7-8)
    hashvalues.push("d2ce84bb239097d9b320be4d2e0052e896a54d24df70da05dbe0a179b36d0527d20cb071f0f06d57ed86978492d846b8c71465a9c94a2bc2b9ad4e5bb3a9a1d5".to_string()); // [14] (5-6)(7-8)
    hashvalues.push("d0db3dcb90db9751c97307e1c1e6dd8e776d4b5758e5e5b1dabbd365bc5c5dd302f5c01d11c2a8f6964f84a48d509e0c278cca5e279c49dd3cad8bc9dfa583c0".to_string()); // [15] (1-2)(3-4)(5-6)(7-8)
    hashvalues.push("f565b57a5486f941b06fed78ac40216847e280c513e3e7493acb4b76aa71d5282b4fd8b06095f8d86a0e93038bfdd5fa626f6f8e14eb779f89a4c9a0b32a267d".to_string()); // [16] 9
    hashvalues.push("e54dd791dec040e50633242e3f8ad1fdfb45c3809f2c04f4d8c05f1ed2a920df84040fb24f3aa211df515205c67966e506ab30f582ffbc1c90245d10ece7e6c4".to_string()); // [17] 10
    hashvalues.push("c16dbf2f3863ed61b2cdba3247608b1d2dafa1d00b03d6e2777a98fc602369c29bdbfd55dbf0f5fccab24205df1795e2d923cfb0699cb9d1958e85c9a798c16c".to_string()); // [18] (9-10)
    hashvalues.push("13abd19fb5c16d162a11b56eee943d2aa327e8cb76d26e08cd31c1390499630c28e4f2ecb3d7e856140c9576f47121d4a421e709504a67dbdb8c4e6332f5f6bd".to_string()); // [19] 11
    hashvalues.push("863de8d58caee972ccdffd1d4c494adb5079c0095ef51dceecac0319b073b9021bc315d10fe0ba1a260c4529cf43313cce953583ea65a92e5fcba568d93381fb".to_string()); // [20] 12
                                                                                                                                                                     // index 20 ^
    hashvalues.push("4fc0c56d6a7cacf460ce8d13719c02f5aea15b3314b912ddebd10e06273d7c6c1dcf301f2c1257c4b690e1e61467ca33eb9708b834718dcf0318fa0c873aba66".to_string()); // [21] (11-12)
    hashvalues.push("6351120253082a8caf598a25c8617c5c972e404c0df469e81925f9ab47e41028d1017d8b3d09d1bae49c0ae3258e76ca383d850682a67776574936609f4dde8e".to_string()); // [22] (9-10)(11-12)
    hashvalues.push("15b983072eac1f7894c39c73d8a81d1bef67c6f30556d86c8cebb9f597d277c23caa54cdb58e3895247a56ebd94b0eef71b5e9e212def07a9c20d4e5fa9ae489".to_string()); // [23] 13
    hashvalues.push("3c81e1d6e7a57d43afdb108569a6eeb7047456bdbfee60c673adb974fbe35a0a249b71e9395eebdaab44c8ecabca7f5b1bf4eaf47bb6fd070daf4c747d1ce983".to_string()); // [24] 14
    hashvalues.push("890b2be31113f376f3be93581463cc5217feb3a00a4d092f1d65d7b635d684efab937207ac450f7cfceb013c89dad9a58322235b9d0e8d18245306f6dae11371".to_string()); // [25] (13-14)
    hashvalues.push("552cde3a14a2025d81586cbe469f2ec53b59916d414e0c26369e6199c56121ad86c5a57fe6beb5f00a8d208556e38c34d64c648c75bc27e47326c2004cee5206".to_string()); // [26] 15
    hashvalues.push("d3f20d372fd0af3cf6002d7437f267c5919ae043aeb2c8a93433216586d05151e28722ccbb80137e950d5db26f43620ea1ec6b65a6cde561fb60fab32af753db".to_string()); // [27] 16
    hashvalues.push("93bb4769b2added4433a56144485a6c1e6b4fd4a2c6481054e087ca54b58b3a8133283f555cdc075c60abf3751d8beb8834655acd5c993277cfc613fa97cf2ca".to_string()); // [28] (15-16)
    hashvalues.push("755782b3c6d6539f93f4b7eafecd94b41fb13a55fcf4332ae07b237fa058da6e0ad09206795205363a18ffc03eb5948ac1ddf16600f4298fa969edd63bb32754".to_string()); // [29] (13-14)(15-16)
    hashvalues.push("6eb2973c8ed37717f50ea903a7a97f5343ae5cb9318fe120526afcddb41e1d6fbfd7932f25218a740e2f9f8cd56ecb5365d0cf8e15d4fcaaba6d7e00f8fc900e".to_string()); // [30] (9-10)(11-12)(13-14)(15-16)
                                                                                                                                                                     // index 30 ^
    hashvalues.push("5eb2c0de844a88872d072ad6e5d72fcbb49541fda7973af836e177be517eaac825d6710a44247b8f88050e53883ed3a73b9011925315177880051fbe97788c83".to_string()); // [31] (1-2)(3-4)(5-6)(7-8)(9-10)(11-12)(13-14)(15-16)
    hashvalues.push("ce51144c142e5ef6a03ab0e1aff9e1ea2336d02da4a53b303c436039e5e41cb7560c5cc913cd156c6f9569ef701a9e6e195e0cfd0f76d6af6d4335568e04dcb0".to_string()); // [32] 17
    hashvalues.push("88bdc0b1fe82a921e8027ca0ed0fdf38a93b98bd7b6487599aa6562bdefd50f0acb6455444bbf1063b63bd3020ba9df8c2e2f4523bfd87f3ce341ae680c64240".to_string()); // [33] 18
    hashvalues.push("5c936f46932b142c97c9421b4103e979cee6a12360be87cda7b975d591fdf8c276301d058ea3e855b8efdee1de2185f086f89417d1c2d28cf89b69644f961748".to_string()); // [34] (17-18)
    hashvalues.push("64b430d42973ee0743fb272d0e12fdd3f9c8aa56848c4617b90b60cf40faab4f5b11ff795979e492e14b0dafb305cc8effa5cc62e52dc500d86efecb7be549cf".to_string()); // [35] 19
    hashvalues.push("e56bc2dc73e6249832a1e937f3dad9372b89313a4f3d0bc42d138835644c7ad54a7284a9b6ddea0580f2ccb0ddddc96fd2caa3c22f17b40c81503312680bad2c".to_string()); // [36] 20
    hashvalues.push("030ba2a5bc1387865488ecdfb95f74f261d7637603cf10f6e329c4916729795a2873c8104f8361d438e9fdf799006e139a357efaa02b85504a8ab8b3cd3fce06".to_string()); // [37] (19-20)
    hashvalues.push("99df3b2af84731794963c268f365f0ad24edbb3e7a28d12c7545e2fd7f3b20caac933fb0146884a898ad47876409398cfe0d76a8822c979d8ef1d2b2d8282a54".to_string()); // [38] (17-18)(19-20)
    hashvalues.push("a106bc373d2c64dc945be67cf9ff060f6c0203f9a26f0f81fffe3cca01a145db51c0ac55db81e79f8b13d5c1b791f83bb99a2a85dc18e7ec506c0b06c5f99cd3".to_string()); // [39] 21
    hashvalues.push("6569a6e61d5641a0cba01c421990b7caea38d120f3c8e4088c82c06eca41b25448f2a802371059f0cf8885f78c7c5516e5427ee88e841d999083cbf1a70e14a1".to_string()); // [40] 22
                                                                                                                                                                     // index 40 ^
    hashvalues.push("50b4d2e4705e73ef6bba2f15bb10affa669ccd4ceba1aa4283b1358a5a352756926e38823606be5c8fdb074a8e77e7f0699c60a81860c43441b447d9a4cdbb6d".to_string()); // [41] (21-22)
    hashvalues.push("8230fa69d4b843ecc925ec619e8fdc69989b12c48974cbe9cf747166c28c3ae03be378071b9550351d84d82cd2197aeb25c6c9ee6cf058a08533027481d063fc".to_string()); // [42] 23

    hashvalues
}

fn create_mmr() -> MerkleMountainRange<Blake512TestMmrHasherBlake2b, Vec<Vec<u8>>> {
    let mut mmr = MerkleMountainRange::<Blake512TestMmrHasherBlake2b, _>::new(Vec::default());
    for i in 1..24 {
        let hash = Blake512TestMmrHasherBlake2b::new()
            .digest(i.to_string().as_bytes())
            .as_ref()
            .to_vec();
        assert!(mmr.push(hash).is_ok());
    }
    mmr
}

#[test]
fn check_mmr_hashes() {
    let mmr = create_mmr();
    let hashes = hash_values();
    assert_eq!(mmr.len(), Ok(42));
    let mut failed = false;
    for (i, item) in hashes.iter().enumerate().take(42) {
        let hash = mmr.get_node_hash(i).unwrap().unwrap();
        if &hash.to_hex() != item {
            failed = true;
            println!("{}: expected {}\n{}: got      {}\n", i + 1, hash.to_hex(), i + 1, item);
        }
    }
    assert!(!failed);
}
