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

use blake2::Blake2b;
use digest::consts::U64;
use tari_crypto::{hash_domain, hashing::DomainSeparatedHasher};
use tari_mmr::MerkleMountainRange;
use tari_utilities::hex::Hex;

hash_domain!(
    Blake512TestMmrHashDomain,
    "com.tari.test.mmr.tests.with_blake512_hash",
    1
);
pub type Blake512TestMmrHasherBlake2b = DomainSeparatedHasher<Blake2b<U64>, Blake512TestMmrHashDomain>;

#[allow(clippy::vec_init_then_push)]
pub fn hash_values() -> Vec<String> {
    let mut hashvalues = Vec::new();
    // list of hex values of blake2b hashes
    hashvalues.push("d069265d96db922d40b89a28a7b00dafa617b47ea41bf5f3b6efaf3c021ea784771d39f6e6c0250fd3fc59e4eb28817cb40f82d2e61e22bb6f9e32eca86f8e8e".to_string()); // [1] 1
    hashvalues.push("8e023cc32f354e2fe3329df6f845c52b2cc61a873838849afa53983ade4836f4e655da77e6ad31a5918d6406dc9f3df49491423d361f2c92ab9594febced5949".to_string()); // [2] 2
    hashvalues.push("fbd8a6bd674c77bb354875eb7fa98faa377d5c91e20837872f5d7ca4f36da56b9ef0b05ccc9e6e25b53a9419593a6af57c95297c237c14e239d6ad8f0011eba8".to_string()); // [3] (1-2)
    hashvalues.push("7d33eb1f14976e23d6de9bb9ee5d13274b0f1040dc9a15881be5be46a9cc9613391892a2ab3936f45a36846b5f14b16b68e4d4af3949699b9cfa288a7418641c".to_string()); // [4] 3
    hashvalues.push("5bdb3093ba5699baba7d8bcea914947a45e77c2fed74d0c4abb2adb716ac144610b7bfc326aaa237d8f9c7f7caccd98cbe5ef766a760d0f6f65293dd3ab70ac4".to_string()); // [5] 4
    hashvalues.push("ada17fac272e34b89bbe2a1930ad518c9d0a011fc6e5e8b92dc364a9aa4cfe3a82e3b62cf776a12a4354748dde2fd1e43dd05a165778388049ed5530757f284c".to_string()); // [6] (3-4)
    hashvalues.push("b992e381948bdd8f54d8781ea75499014ea0ac1de38b7b7e7b66db265a1e4540b5376141ddacf4422eab570f547714e50f5011ed1a9376e31fb71a2546deb236".to_string()); // [7] (1-2)(3-4)
    hashvalues.push("3cafa30e07f8097b63cd9a5ae078a0d347b1cc7229c411882266fa7ad8979a614bfcbc1def863a021b966240649c2cb83ac8aa357d30de350bec07f076a31382".to_string()); // [8] 5
    hashvalues.push("f1eea8698fa9266b99dd96deea26ad9bda0fef60e18ebfbf17c748b9ee2c2b61e5a96a4fd4a188d329af2ea7659ed2e25650ba4d9e8d55dcc16c7b68e16086e1".to_string()); // [9] 6
    hashvalues.push("10ec9208aa04694c977c2d14245f3b24d0a563bf2211a3f693d55ea2d22dd7eaa92190caaf47927f8b05f6dd6d37dc57aac7a643c670ca177aa41ca675d70921".to_string()); // [10] (5-6)
                                                                                                                                                                     // index 10 ^
    hashvalues.push("8c1588df73cfb75143a880cb154a6885f0eea6a9c00fde1c31bc5a0e3929ee65c4514e4fc48f04b4bbaf8d44580b50b5dcc4d5b058b63458a0f590bd0882a1cd".to_string()); // [11] 7
    hashvalues.push("d123a7eac30be8c6d45d895178b1c16e1a1608eaab35a684802e3838bba1aafd59af45081f307db61d4adb170286df2973a14dc0e5b0078da79fdbc6a7440674".to_string()); // [12] 8
    hashvalues.push("7cd5190b86e66b3e117a85ea2f3a53b2e4f9d02d82ce2fce63564115ab687fc75867a23b1e1352acb164e39bcc4768165bcf7171eb060fa92b255a3f16072d30".to_string()); // [13] (7-8)
    hashvalues.push("68d0760c8c0c17872c3634cafa36186c7762779a92c62b9325c7a386efb6d36ed7e88ca0067ef0eba7cd1f023533b297d4ed9426ca851eea70373be7fcd5fe59".to_string()); // [14] (5-6)(7-8)
    hashvalues.push("d7f2b7e212934a585df88f6ad020b332ea7d2d9c152122bb23d40457b0efa45a8eebef4e6a7b355c8fea5daa4c35c9ed981dcca83c2bfa327e02377717c838b1".to_string()); // [15] (1-2)(3-4)(5-6)(7-8)
    hashvalues.push("018774264c13412f4b8d50b77acd71cca22ad127891b1317c76aced179b4f48af23108a0afb44d6c8b9af290944bd2dbe10d3b6482200388b3d6f2e3a64b24e8".to_string()); // [16] 9
    hashvalues.push("7757c6eba47864b47a20c7962056c9322ad98f1c17b743742e9ef947f12de03a7ebc101b78e3cb8d93797d3a8167d3a97c0fbc39d703e5e890cdb6cbed525785".to_string()); // [17] 10
    hashvalues.push("c215740084c82bcab3ffc82bd5714566849d0a57f9d12698cd477f3646cbf9a5834108cc5b1e87bee6e014e8c1fe61cee8a4a0fe7ef1331b99194fafc3d0a953".to_string()); // [18] (9-10)
    hashvalues.push("80be59ffa422ea37469f1eaad94fab4b3a3d7e58b2bc3059d471e861103626160bd5f9b5ffb42720499477120b167c1a953eb6eee65055ac1b6797af24973ba3".to_string()); // [19] 11
    hashvalues.push("ec122f77e3dbbb6cb49bcc766e4b33a6b93f569d7ef9ae408c79faf45ec5b145c641285740e9c9a3ea5d722c75250f7c8583b618cb37408d4f49fdf0019c0cc6".to_string()); // [20] 12
                                                                                                                                                                     // index 20 ^
    hashvalues.push("61ceaf14cf9ce98a2d095455334d2c648a7ead63075470a1801ca31cbf5c8956684bb3b3570e68e28490b8c0dd53e6cf7e7a41ce68b258d9360e25456873c916".to_string()); // [21] (11-12)
    hashvalues.push("280117919ed3453543d8e9209c21cb7e5d9899d478eca0e0dd0fc76c4d24c3476775b0580691d55ce38d27e8db2462791f580a9b4e58a4ee4d6ae7758de493b9".to_string()); // [22] (9-10)(11-12)
    hashvalues.push("a88ec928e31fd78d8cbc54f93fd7229377fdf04df21597a1267a7ab880abe068545389d85ae9e91b8d40ea7a820110abdf13bbdf6463abe3ce20e17f70391051".to_string()); // [23] 13
    hashvalues.push("4cfdc2aab9b46dd04e62c4959e4f5f9398f73afd9df75850d3a6aa0569f9d6abb46d310d2a92deed79a954dbdcb53939ba4be4c35a9c9f0c697ccac4ae2722e4".to_string()); // [24] 14
    hashvalues.push("13de5fe583c12c4012f3fffcd4de69c40d6f4154d30c6ab1fc8ac725f6d8891beb66f61b326ccd490da34969dcd0128307f974f03cf9ba5c189ea95cde96efcb".to_string()); // [25] (13-14)
    hashvalues.push("066ac342b419adb87b48c8f2d2b05bafe3ffc56cac70efb9fbbc4af9c6f05a38f1c46f3133879b6cc2cd3640a702a716f9f0a9cbdfda99dc3728bcf081034259".to_string()); // [26] 15
    hashvalues.push("06a024681bf4c3c2e39256c74efd5df5a2ab65f1144ad11103ce019340bded4dc9241619b81532d28d674177ead5bee29a13970326dae96bc8991fcc8c7f7df2".to_string()); // [27] 16
    hashvalues.push("fcec9eb0fe9b57be455a23666e04bb206a823277fa230b56b2ab73485a5c75372f6654570a1f52adc8decf7350669c5e98e43f014f60048354ebc31966aa33ca".to_string()); // [28] (15-16)
    hashvalues.push("dd234adeb2133c55d37ef8e54dcb9797768fbe6c347bab3cdbd63ecc58397bc11c3f1ea442ad0d74a5b23eca0b2fa6168af2714a7b6821b5ceb251bab608b969".to_string()); // [29] (13-14)(15-16)
    hashvalues.push("ee2cf6fcf9d851c6c777f5a1664280f0a0bd17040b6ab3f6bdee172fed85a9e320002f8027988c6da8f62afe274c8354a09206b101ee2e41095cce883bdd0071".to_string()); // [30] (9-10)(11-12)(13-14)(15-16)
                                                                                                                                                                     // index 30 ^
    hashvalues.push("6797f81c15bb6834971111385a234ed1fa7915b8c785dc7bf4707b80c4b71621b33e9b41c75a3e1af1f5888d379e300000917e902e48a63053795c4dc66d3233".to_string()); // [31] (1-2)(3-4)(5-6)(7-8)(9-10)(11-12)(13-14)(15-16)
    hashvalues.push("73ab8375d54b2d6ba89a74622f698203b893301a0dcf8458455b7e0383afb1e8b24c5183f1b8f8c97ae9cdfa89d006d818f0af18107620fb6665475508cf5822".to_string()); // [32] 17
    hashvalues.push("fbd113743591d6646211507e4f3ef68e1881784e1bbb02aa4ec3bda083f0cdbddf1dc82476396a1500eba2e401eb273172ca02a940eea4ae08aae91f6d08b82a".to_string()); // [33] 18
    hashvalues.push("4a834d79d3038046f78c9a3c1487a3a3689aa29f0fade2f777e3aefe5e393950b74e67b642cd684881533be85c6e125fa43296b4a89c585d9367f95c78bf9d0d".to_string()); // [34] (17-18)
    hashvalues.push("3ffea68db4bff0d3c9159f52a63e7c0b0ae7d1088bb8d89697b10a01ebe7847eb2528eb817cb9185f6d99c9fffb178c421286540e9b58238326603cd6d9c53b2".to_string()); // [35] 19
    hashvalues.push("655df8ba8dc8245b5fcf91600b876aa0f8edfe3d4ce1b1d6cc26252b3f7ab6e648eb90778f03bf21ac030b8a1c6d3d59edc32c9cf87076bd28972f52f187480f".to_string()); // [36] 20
    hashvalues.push("992b7d6c5bf75224033678aed673096cfaf37581ee03c651f6802009620cf010f38aaba7f7191fd9fece16ff637366a04af370f04b9d4ac033bc14ba4957e603".to_string()); // [37] (19-20)
    hashvalues.push("50d92690a46800ecc58531ef471cbcc11fac0f48655c5eeb15d1019674c965a1d647794b0f0e536df2af37f75305fcdac04fd58a62bd4c59fa93e8e75cee4362".to_string()); // [38] (17-18)(19-20)
    hashvalues.push("1c400bbc9eb1a18830d5e100a9710697689f4acd1541210ed78f84f686c50be7d8d5edaa71c67fdda052700951fb1e989b5abeb5d9f7e8caab6cffe46f059b11".to_string()); // [39] 21
    hashvalues.push("7a03660b652c51650428201fe1e309882c862a3f0b7a9b530d8dc3bdd2baa5c71e0557c9a7c266b7b3e318cb8694f332d6bef89eb2fdf1240c3e0bf32ae1f093".to_string()); // [40] 22
                                                                                                                                                                     // index 40 ^
    hashvalues.push("2ca6615a901c1885f7d7864af2b6d53556b17a7a5908d5610e28125e4b27c6f89f38f4ec0d60af5cb91415c10c1342b42f86700638b51bdd09336082a9e7cb99".to_string()); // [41] (21-22)
    hashvalues.push("ad288c8a70039a8869371b95b0e67218bb672d82a880d804275c5735c74d385d41654c4301e22c28186930bc76b49814d886596b6c36bd44913edf277b853655".to_string()); // [42] 23

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
