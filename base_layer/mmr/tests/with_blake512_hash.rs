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
use tari_mmr::{mmr_hash_domain, MerkleMountainRange};
use tari_utilities::hex::Hex;
#[allow(clippy::vec_init_then_push)]
pub fn hash_values() -> Vec<String> {
    let mut hashvalues = Vec::new();
    // list of hex values of blake2b hashes
    hashvalues.push("e6fcab78b5658e811fdf38d9c6e19d53e0d66f2d869ee183add0bf860f5b2334420481361872fdb51dbbe8f487730c31a6aeaa3b7d6bf3e1cca161b9b9f08b25".to_string()); // [ 1] 1
    hashvalues.push("5d14a37548f17fdcbdf69be1126f877b2b3e894b03bab66056993e58efd4f66622daa5c4003cf747fb5b00cdd74400718f4e9e21788ee8a81cb501396810243a".to_string()); // [ 2] 2
    hashvalues.push("bd9843bf3adc0107e902ec3434e42391466da1184583bc018beaf10abc9ac0589b12b08b28a7625564228bf94f08c6063b5b346b198ecad3f255b10862b3f924".to_string()); // [ 3] (1-2)
    hashvalues.push("63d113af3ce563a3ba3b284193d41fd081f4d29e054f97fc703c76eeabbed4c81e2db8caff42829378d4e2b3c3a767ef92f02dd893c1226aee013e70564f1f72".to_string()); // [ 4] 3
    hashvalues.push("3614279022e0fba91a6e491c06b3c73d8ea3131d4e2244ab86bc0633a4d119ced25006106d970c5aad9720c782b648dc60261e79b6394c8ab35c65e961410b8d".to_string()); // [ 5] 4
    hashvalues.push("2e7ed9ea70fd7d27492cc515c8acbb1ff6fc334cbcb51dd947e9bce9fce8c0b9d52f9b603e49442ef3fdceaf582e382f18b7bdc167c2a08d901031c5367181c8".to_string()); // [ 6] (3-4)
    hashvalues.push("c198b7ee1549bedb3e694caaa0a22741c0c7f7482e7f5c59a2725b0257f6109de87fca525b059f635297f69deff9b412913c72a6567b579afd51c0465c6a80c6".to_string()); // [ 7] (1-2)(3-4)
    hashvalues.push("48461f727594ce1e97f3b8f032657722fa282069b74e8e4de85992862d4d64457dcbde4cc8d864acf4ddbab57e95fb795d2fb3d79cda6203b35bfdaf3655845f".to_string()); // [ 8] 5
    hashvalues.push("706e81c091c552d8fefb950179767b640b7f22221813b8c3afad4b43ef6198519a25e19d673f75a8b0a5de3370f22b9e5880d504c58785748897218d6a4345de".to_string()); // [ 9] 6
    hashvalues.push("9e0db25d14f43e8063a65d9eb41af218a7ef07297c56555152903a0424b87810c276e0c653151806723f260275e5379631c5c63d3aa57ae03d131a06956ecf2f".to_string()); // [10] (5-6)
                                                                                                                                                                     // index 10 ^
    hashvalues.push("df34879393993bc14523aac3291117e32c739f45d48cd21dd8aa681e86db86cdca48adfef067b7e154ea5e94a55144841e18c5e142e9bffbee9f9f504960010f".to_string()); // [11] 7
    hashvalues.push("6be3ac045bcb39c87cc84fe01f424431862177b5963ae91dd09d7f3dd791683f5e6072a39a4c882819c388ed436e6b66bb2da0c24b1fde909e9b9921a1b98a6b".to_string()); // [12] 8
    hashvalues.push("fee7377e1e4ce65d1b9f84580cce4405d4e9432cfa2bdce3c36a70ec0caed36d03e9d48a7f65993bda31e78c5899c198b6cd4011d21334a01af7de82bf49e87d".to_string()); // [13] (7-8)
    hashvalues.push("213c0bb50213a3e35daeb1811abfae9a80b5296b6735bcb131cdbf715836e5b79dcee602deae24afb4c3cc357aee776f0e4a9e120ad10def97aee9faf17f7e1b".to_string()); // [14] (5-6)(7-8)
    hashvalues.push("ad8ad5e3a22d2cc4bcfa24f9b602eba96a4351e1cfdb01b76402bdebd3ed8d06aea037f814997b6783267b33f238ba9a19c14f507f7797674ddfbc32b79872a6".to_string()); // [15] (1-2)(3-4)(5-6)(7-8)
    hashvalues.push("a5b472195eace5154dd6823b795340562c7c60ef81e59cb2c350a85aa5d2daf2c78f2148a68b8b38030cd02e5f0e73c07dd05b139e8211c032e78c900201d195".to_string()); // [16] 9
    hashvalues.push("3801f29f326cd55b3938f4df3daf3edeef365b1cd4d5836279912bcda3e7cada52ea7e8a9b0b212274ec2834fad58c5bcac189544a17bbba8601ef8ae0a1bf5b".to_string()); // [17] 10
    hashvalues.push("0cfb7ffb59ae3e2ce5fd72845d7b0ab6174c34793e7c3172dc344772767af69b59a957023ec818191c4232f207c4b65437d0c68cb0ebde2b52e2444e6b6aa375".to_string()); // [18] (9-10)
    hashvalues.push("0829da904ec76f4ac2ef7a56048fa0a109fdef804471980d2e4e02c9c2af34128a12561a23c383a03a3452db1a3a3bc664c61442e3bf8c15d228abec8add03e4".to_string()); // [19] 11
    hashvalues.push("762332f5bec7ba1337381c36effd416318ee227b92392f4a3a4c7a3385128a605818c0fca43cf293920fa957d659e477bd8cc4f974be478a460d73ed02b9432d".to_string()); // [20] 12
                                                                                                                                                                     // index 20 ^
    hashvalues.push("b757fb02925f94a0710cd38888cbda1cdd96613eac6e7d51d1889ded053e1771ddb5e21a9ee8434698a70186a6eb7e1078239a1dbc6cde1c091060fa227f21a7".to_string()); // [21] (11-12)
    hashvalues.push("4c436b4e7dfb1b5f13c846f184ef0e5c0962815aa1c6a0c58ba2693956a7d40eb67109a2fc429a57f69f4c3f6062f87597353c64fa06c473235cb1ccad59cd64".to_string()); // [22] (9-10)(11-12)
    hashvalues.push("0f849d251f7e1e7981eba55b51701bbe3dd61dfed3e892b75621c79ae1bd790ef62656ba647e029dd592bf8a0feba96dbbfd39ccb119750447ec3cc03f274ae6".to_string()); // [23] 13
    hashvalues.push("1849246d089c823aaa4e9116055982e9683ce3bdd47fccb72191ddbb3a852dc2180325147939edc0dde6d93568478405681fbefed45b1c0b375e7ea402f296c0".to_string()); // [24] 14
    hashvalues.push("934133cd4816541a6a2ca9e20ff649df02684908262055eefebf553192a434617ef8048115235e3cb756e8730205874221531e448e7013389d7089b751769fd6".to_string()); // [25] (13-14)
    hashvalues.push("deb64efa9a209e0df5f2223c8c90867717cf952f53c93ede9d0b6bc9f8f7bd64ca221a208f336747e0c7e6b58c75661321b942e6c496d50c2e030ae22621ef19".to_string()); // [26] 15
    hashvalues.push("064146c98414c0cf33e1083cf9f528afc1ec526e6aac570ee7e23b2920cad74aa8bcff370152cb602618a9b3d98d04468a33cfa81088832e2738f05e94301580".to_string()); // [27] 16
    hashvalues.push("0e74d18733e4c30fe84d1010e390bc0042f64068a98f6c230e965d42b6fa0ac86929a16a38b96b6ea9f361291cb71cf2fa2093a3389a4dc8973f1af57a12d341".to_string()); // [28] (15-16)
    hashvalues.push("5c79c630d3638765219c2afa26c075015ad339fb87f5d8affffbc1a6ac9ced0dea38be38c7a6efb4c7fa6e8fff54ab8eb3e195a2931037573cd91894bd3ae103".to_string()); // [29] (13-14)(15-16)
    hashvalues.push("65515f741a4b925f757c80f4896b1ce5045c84f0b1421469812f59ddc25dada67193b099cb9009f11e9c5ccd1346ed19a709c6777cf9c9cf83cd2e508eadca42".to_string()); // [30] (9-10)(11-12)(13-14)(15-16)
                                                                                                                                                                     // index 30 ^
    hashvalues.push("3378414485e9f0f485cad5cbfecb463cd45e42f7057300967a263580eada87497ddf4f27ea55fd837add981c7b1ac4c9a1e4d10649309560e20dad2330211da2".to_string()); // [31] (1-2)(3-4)(5-6)(7-8)(9-10)(11-12)(13-14)(15-16)
    hashvalues.push("7945c1fab0170b701ee13cbebc6aeb32b5c17918bba0b82c4ac357fb1e77efd24755cb1e96ffe531b3cff873ad953ca308756100c1580341ea5e9c948fe59d92".to_string()); // [32] 17
    hashvalues.push("712c370077faecd2cd2fc4052f6737391b2203779850121ecfcc5c0e3b3d6e8842dc01e3d860b0ef8943ba141d334cf31e33384dbeeec92fbf5d8236a36c7f98".to_string()); // [33] 18
    hashvalues.push("b13fbe74c00eb57d4515a095c2df1cbcad6cca4cbcdc3bf2d3a5d5630be96a21f3925bc26f20d77263d15ca715fe811debb09a4404c1f08f0549c358220a5149".to_string()); // [34] (17-18)
    hashvalues.push("246089a57c3ca2a2e1cfa0fc269d4ecf10fad8213b404920c1f79b9462222286fc960c96e5989fb5c3722d87b55b3bfab668471c33f28e34829b69db4802b183".to_string()); // [35] 19
    hashvalues.push("13c4a7cdd5891997b5047a11a829ad67162cd7347407e1244ed40be9757222cd102045b1c87bc86064b40a5d04ee699cd2d099f37cbf3de6ad25ce4a35d3a4a5".to_string()); // [36] 20
    hashvalues.push("76aa78ca9b70c5c4ca88a63648c16b21278fcbc1bdc50d2068c532e409686d905700caa756fda33a9515d9f84fd5820e31c093dff53b2fab066508077ad8132e".to_string()); // [37] (19-20)
    hashvalues.push("3dee616761243270760eb962b8c966bff72835da9577d96a02e7ea3f030e8bbfed429b83fa2361e1e94a149aa569f1509aec0ec9ba9b87e44dd48fa97c404c7f".to_string()); // [38] (17-18)(19-20)
    hashvalues.push("3c40d4a9cb1e1703187069ffd2bd5ba2eea14eba2da31b6217a34d8298d0f7c3742553076db41abdfb1f313cb6a1da312f5ecd6a68fcff9bd57d8f57a0992c5c".to_string()); // [39] 21
    hashvalues.push("2df0344622868cd3c49792dd1410ead4802253849d3b4981a0bd5b00b0518b9a67c576463802b90ddf03b3112797965c235872e5f6e6f3c1d959704d7258c187".to_string()); // [40] 22
                                                                                                                                                                     // index 40 ^
    hashvalues.push("fc3448f1245cb8d3900f67754349658ae670c9d71f1e9ba298bde04416e958ff57f9dd7a4e72225b51d887a1b653521a5f7c9ca2cbaeea5ae3e1cc50540ee3a9".to_string()); // [41] (21-22)
    hashvalues.push("d7af3430704b678d7f16ed403dd3a13d0901014b3deb97012df6b23cda608877af41efcd29d1add2143e8130f4b67bd42358423c58516dd2df7c190cd985642d".to_string()); // [42] 23

    hashvalues
}

fn create_mmr() -> MerkleMountainRange<Blake2b, Vec<Vec<u8>>> {
    let mut mmr = MerkleMountainRange::<Blake2b, _>::new(Vec::default());
    for i in 1..24 {
        let hash = mmr_hash_domain().digest::<Blake2b>(i.to_string().as_bytes()).into_vec();
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
