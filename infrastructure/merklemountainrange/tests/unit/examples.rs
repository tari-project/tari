use blake2::Blake2b;
use digest::Digest;
use merklemountainrange::mmr::{self, *};
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
    let mmr: MerkleMountainRange<MyObject, Blake2b> = words.clone().into();
    assert_eq!(mmr.len(), words.len());
    assert_eq!(mmr.get_peak_height(), 3);
    let summer = MyObject { val: "summer,".into() };
    let summer_hash = summer.hash();
    assert_eq!(*mmr.get_object_by_object_index(10).unwrap(), summer);
    assert_eq!(mmr.get_object_hash(10).unwrap(), summer_hash);
    let tree_hash_index = mmr::get_object_index(10);
    assert_eq!(tree_hash_index, 18);
    assert_eq!(mmr.get_node_hash(tree_hash_index).unwrap(), summer_hash);
    let mut proof = mmr.get_object_index_proof(10);
    assert_eq!(proof.verify_proof::<Blake2b>(&summer.hash()), true);
    let _root = mmr.get_merkle_root();
}

#[test]
fn append_to_mmr() {
    let words = create_word_list(15);
    let mut mmr: MerkleMountainRange<MyObject, Blake2b> = words.into();
    let words = create_word_list(20);
    assert_eq!(mmr.len(), 15);
    assert_eq!(mmr.get_peak_height(), 3);
    mmr.push(words[15].clone());
    assert_eq!(mmr.len(), 16);
    assert_eq!(mmr.get_peak_height(), 4);
    let root_1 = mmr.get_merkle_root();
    mmr.push(words[16].clone());
    assert_eq!(mmr.len(), 17);
    assert_eq!(mmr.get_peak_height(), 4);
    let mut proof = mmr.get_object_index_proof(0);
    assert_eq!(proof.verify::<Blake2b>(), true);
    // The third-to-last hash of the proof should equal the root of the previous mmr
    assert_eq!(root_1, proof[proof.len() - 3].clone().unwrap())
}

#[test]
fn deserialize_proof() {
    // MMR the whole sonnet
    let words = create_word_list(108);
    let mmr: MerkleMountainRange<MyObject, Blake2b> = words.into();
    // Proof of word 20: thou
    let thou = MyObject { val: "thou".into() };
    let mut proof = mmr.get_hash_proof(&thou.hash());
    assert!(proof.verify_proof::<Blake2b>(&thou.hash()));
    // Proof of word 102: conquest
    let conquest = MyObject { val: "conquest".into() };
    let mut proof2 = mmr.get_hash_proof(&conquest.hash());
    assert!(proof2.verify_proof::<Blake2b>(&conquest.hash()));
}
