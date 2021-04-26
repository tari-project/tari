use crate::chain_storage::ChainHeader;
use std::{cmp::Ordering, fmt::Debug};

pub trait ChainStrengthComparer: Debug {
    fn compare(&self, a: &ChainHeader, b: &ChainHeader) -> Ordering;
}

#[derive(Default, Debug)]
pub struct AccumulatedDifficultySquaredComparer {}

impl ChainStrengthComparer for AccumulatedDifficultySquaredComparer {
    fn compare(&self, a: &ChainHeader, b: &ChainHeader) -> Ordering {
        let a_val = a.accumulated_data().total_accumulated_difficulty;
        let b_val = b.accumulated_data().total_accumulated_difficulty;
        a_val.cmp(&b_val)
    }
}

#[derive(Debug)]
pub struct ThenComparer {
    before: Box<dyn ChainStrengthComparer + Send + Sync>,
    after: Box<dyn ChainStrengthComparer + Send + Sync>,
}

impl ThenComparer {
    pub fn new(
        before: Box<dyn ChainStrengthComparer + Send + Sync>,
        after: Box<dyn ChainStrengthComparer + Send + Sync>,
    ) -> Self
    {
        ThenComparer { before, after }
    }
}

impl ChainStrengthComparer for ThenComparer {
    fn compare(&self, a: &ChainHeader, b: &ChainHeader) -> Ordering {
        match self.before.compare(a, b) {
            Ordering::Equal => self.after.compare(a, b),
            Ordering::Less => Ordering::Less,
            Ordering::Greater => Ordering::Greater,
        }
    }
}

#[derive(Default, Debug)]
pub struct MoneroDifficultyComparer {}

impl ChainStrengthComparer for MoneroDifficultyComparer {
    fn compare(&self, a: &ChainHeader, b: &ChainHeader) -> Ordering {
        a.accumulated_data()
            .accumulated_monero_difficulty
            .cmp(&b.accumulated_data().accumulated_monero_difficulty)
    }
}

#[derive(Default, Debug)]
pub struct Sha3DifficultyComparer {}

impl ChainStrengthComparer for Sha3DifficultyComparer {
    fn compare(&self, a: &ChainHeader, b: &ChainHeader) -> Ordering {
        a.accumulated_data()
            .accumulated_blake_difficulty
            .cmp(&b.accumulated_data().accumulated_blake_difficulty)
    }
}

#[derive(Default, Debug)]
pub struct HeightComparer {}

impl ChainStrengthComparer for HeightComparer {
    fn compare(&self, a: &ChainHeader, b: &ChainHeader) -> Ordering {
        a.height().cmp(&b.height())
    }
}

pub struct ChainStrengthComparerBuilder {
    target: Option<Box<dyn ChainStrengthComparer + Send + Sync>>,
}

impl ChainStrengthComparerBuilder {
    pub fn new() -> ChainStrengthComparerBuilder {
        ChainStrengthComparerBuilder { target: None }
    }

    fn add_comparer_as_then(mut self, inner: Box<dyn ChainStrengthComparer + Send + Sync>) -> Self {
        self.target = match self.target {
            Some(t) => Some(Box::new(ThenComparer::new(t, inner))),
            None => Some(inner),
        };
        self
    }

    pub fn by_accumulated_difficulty(self) -> Self {
        self.add_comparer_as_then(Box::new(AccumulatedDifficultySquaredComparer::default()))
    }

    pub fn by_monero_difficulty(self) -> Self {
        self.add_comparer_as_then(Box::new(MoneroDifficultyComparer::default()))
    }

    pub fn by_sha3_difficulty(self) -> Self {
        self.add_comparer_as_then(Box::new(Sha3DifficultyComparer::default()))
    }

    pub fn by_height(self) -> Self {
        self.add_comparer_as_then(Box::new(HeightComparer::default()))
    }

    pub fn then(self) -> Self {
        // convenience method for wording
        self
    }

    pub fn build(self) -> Box<dyn ChainStrengthComparer + Send + Sync> {
        self.target.unwrap()
    }
}

pub fn strongest_chain() -> ChainStrengthComparerBuilder {
    ChainStrengthComparerBuilder::new()
}
