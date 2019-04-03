use blake2::Blake2b;
use digest::Digest;
use merklemountainrange::{error::MerkleMountainRangeError, merklemountainrange::MerkleMountainRange};
use tari_utilities::Hashable;

#[derive(PartialEq, Eq, Debug, Clone)]
struct MyObject {
    pub val: String,
}

impl Hashable for MyObject {
    fn hash(&self) -> Vec<u8> {
        let h = Blake2b::digest(self.val.as_bytes());
        h.to_vec()
    }
}

const WORDLIST: &str = "Then let not winter's ragged hand deface
In thee thy summer, ere thou be distill'd:
Make sweet some vial; treasure thou some place
With beauty's treasure, ere it be self-kill'd.
That use is not forbidden usury,
Which happies those that pay the willing loan;
That's for thyself to breed another thee,
Or ten times happier, be it ten for one;
Ten times thyself were happier than thou art,
If ten of thine ten times refigured thee:
Then what could death do, if thou shouldst depart,
Leaving thee living in posterity?
Be not self-will'd, for thou art much too fair
To be death's conquest and make worms thine heir.";

fn create_word_list(n: usize) -> Vec<MyObject> {
    WORDLIST
        .split_whitespace()
        .take(n)
        .map(|s| MyObject { val: s.into() })
        .collect()
}
#[test]
fn create_mmr() {
    let words = create_word_list(15);
    // let mmr = MerkleMountainRange::create_from_vec::<Blake2b>(words);
    let mmr: MerkleMountainRange<MyObject, Blake2b> = words.into();
    assert_eq!(mmr.len(), words.len());
    assert_eq!(mmr.get_peak_height(), 3);
    let summer = MyObject { val: "summer,".into() };
    let summer_hash = summer.hash();
    assert_eq!(mmr.get_object(10).unwrap(), summer);
    assert_eq!(mmr.get_hash(10).unwrap(), summer_hash);
    let tree_hash_index = MerkleMountainRange::index_to_tree_index(10);
    assert_eq!(tree_hash_index, 15);
    assert_eq!(mmr.get_tree_hash(tree_hash_index).unwrap(), summer_hash);
    let proof = mmr.construct_proof(10).unwrap();
    assert!(MerkleMountainRange::verify_proof(&summer.hash(), &proof));
    let root = mmr.get_root().unwrap();
    assert_eq!(root, "??????");
}

#[test]
fn append_to_mmr() {
    let words = create_word_list(15);
    let mmr: MerkleMountainRange<MyObject, Blake2b> = words.into();
    let words = create_word_list(20);
    assert_eq!(mmr.len(), 15);
    assert_eq!(mmr.get_peak_height(), 3);
    mmr.append(words[15].clone());
    assert_eq!(mmr.len(), 16);
    assert_eq!(mmr.get_peak_height(), 4);
    let root_1 = mmr.get_root();
    mmr.append(words[16].clone());
    assert_eq!(mmr.len(), 17);
    assert_eq!(mmr.get_peak_height(), 4);
    let proof = mmr.construct_proof(0).unwrap();
    // The second-to-last hash of the proof should equal the root of the previous mmr
    assert_eq!(root_1, proof.hashes[proof.len() - 1])
}

#[test]
fn deserialize_proof() {
    // MMR the whole sonnet
    let words = create_word_list(108);
    let mmr: MerkleMountainRange<MyObject, Blake2b> = words.into();
    // Proof of word 20: thou
    let thou = MyObject { val: "thou".into() };
    let though_proof = "???";
    let proof = MerkleProof::<Blake2b>::from_hex().unwrap();
    assert!(mmr.verify_proof(&thou, &proof));
    // Proof of word 102: conquest
    let conquest = MyObject { val: "conquest".into() };
    let conquest_proof = "???";
    let proof = MerkleProof::<Blake2b>::from_hex().unwrap();
    assert!(MerkleProof::verify(&thou.hash(), &proof, &root));
}
