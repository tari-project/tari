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

use std::ops::Add;

#[derive(Clone, Debug)]
pub struct FixedSet<T> {
    items: Vec<Option<T>>,
}

impl<T: Clone + PartialEq + Default> FixedSet<T> {
    /// Creates a new fixed set of size n.
    pub fn new(n: usize) -> FixedSet<T> {
        FixedSet { items: vec![None; n] }
    }

    /// Returns the size of the fixed set, NOT the number of items that have been set
    pub fn size(&self) -> usize {
        self.items.len()
    }

    /// Set the `index`th item to `val`. Any existing item is overwritten. The set takes ownership of `val`.
    pub fn set_item(&mut self, index: usize, val: T) -> bool {
        if index >= self.items.len() {
            return false;
        }
        self.items[index] = Some(val);
        true
    }

    /// Return a reference to the `index`th item, or `None` if that item has not been set yet.
    pub fn get_item(&self, index: usize) -> Option<&T> {
        match self.items.get(index) {
            None => None,
            Some(option) => option.as_ref(),
        }
    }

    /// Delete an item from the set by setting the `index`th value to None
    pub fn clear_item(&mut self, index: usize) {
        if index < self.items.len() {
            self.items[index] = None;
        }
    }

    /// Returns true if every item in the set has been set. An empty set returns true as well.
    pub fn is_full(&self) -> bool {
        self.items.iter().all(|v| v.is_some())
    }

    /// Return the index of the given item in the set by performing a linear search through the set
    pub fn search(&self, val: &T) -> Option<usize> {
        let key = self
            .items
            .iter()
            .enumerate()
            .find(|v| v.1.is_some() && v.1.as_ref().unwrap() == val);
        match key {
            Some(item) => Some(item.0),
            None => None,
        }
    }

    /// Produces the sum of the values in the set, provided the set is full
    pub fn sum(&self) -> Option<T>
    where for<'a> &'a T: Add<&'a T, Output = T> {
        // This function uses HTRB to work: See https://doc.rust-lang.org/nomicon/hrtb.html
        // or here https://users.rust-lang.org/t/lifetimes-for-type-constraint-where-one-reference-is-local/11087
        if self.size() == 0 {
            return Some(T::default());
        }
        if !self.is_full() {
            return None;
        }
        let mut iter = self.items.iter().filter_map(|v| v.as_ref());
        // Take the first item
        let mut sum = iter.next().unwrap().clone();
        for v in iter {
            sum = &sum + v;
        }
        Some(sum)
    }

    /// Collects all non-empty elements of the set into a Vec instance
    pub fn into_vec(self) -> Vec<T> {
        self.items.into_iter().filter_map(|v| v).collect()
    }
}

//-------------------------------------------         Tests              ---------------------------------------------//

#[cfg(test)]
mod test {
    use super::FixedSet;

    #[derive(Eq, PartialEq, Clone, Debug, Default)]
    struct Foo {
        baz: String,
    }

    #[test]
    fn zero_sized_fixed_set() {
        let mut s = FixedSet::<usize>::new(0);
        assert!(s.is_full(), "Set should be full");
        assert_eq!(s.set_item(1, 1), false, "Should not be able to set item");
        assert_eq!(s.get_item(0), None, "Should not return a value");
        assert_eq!(s.sum(), Some(0));
    }

    fn data(s: &str) -> Foo {
        match s {
            "patrician" => Foo {
                baz: "The Patrician".into(),
            },
            "rincewind" => Foo {
                baz: "Rincewind".into(),
            },
            "vimes" => Foo {
                baz: "Commander Vimes".into(),
            },
            "librarian" => Foo {
                baz: "The Librarian".into(),
            },
            "carrot" => Foo {
                baz: "Captain Carrot".into(),
            },
            _ => Foo { baz: "None".into() },
        }
    }

    #[test]
    fn small_set() {
        let mut s = FixedSet::<Foo>::new(3);
        // Set is empty
        assert_eq!(s.is_full(), false);
        // Add an item
        assert!(s.set_item(1, data("patrician")));
        assert_eq!(s.is_full(), false);
        // Add an item
        assert!(s.set_item(0, data("vimes")));
        assert_eq!(s.is_full(), false);
        // Replace an item
        assert!(s.set_item(1, data("rincewind")));
        assert_eq!(s.is_full(), false);
        // Add item, filling set
        assert!(s.set_item(2, data("carrot")));
        assert_eq!(s.is_full(), true);
        // Try add an invalid item
        assert_eq!(s.set_item(3, data("librarian")), false);
        assert_eq!(s.is_full(), true);
        // Clear an item
        s.clear_item(1);
        assert_eq!(s.is_full(), false);
        // Check contents
        assert_eq!(s.get_item(0).unwrap().baz, "Commander Vimes");
        assert!(s.get_item(1).is_none());
        assert_eq!(s.get_item(2).unwrap().baz, "Captain Carrot");
        // Size is 3
        assert_eq!(s.size(), 3);
        // Slow search
        assert_eq!(s.search(&data("carrot")), Some(2));
        assert_eq!(s.search(&data("vimes")), Some(0));
        assert_eq!(s.search(&data("librarian")), None);
    }

    #[test]
    fn sum_values() {
        let mut s = FixedSet::<usize>::new(4);
        s.set_item(0, 5);
        assert_eq!(s.sum(), None);
        s.set_item(1, 4);
        assert_eq!(s.sum(), None);
        s.set_item(2, 3);
        assert_eq!(s.sum(), None);
        s.set_item(3, 2);
        assert_eq!(s.sum(), Some(14));
        s.set_item(1, 0);
        assert_eq!(s.sum(), Some(10));
    }
}
