// Copyright 2019, The Tari Project
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

use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// Represents network features offered by nodes
#[derive(Serialize_repr, Deserialize_repr, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum NodeFeature {
    /// This node is able to propagate messages
    MessagePropagation = 0,
}

/// A collection of `NodeFeature`s.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeFeatures(Vec<NodeFeature>);

impl Default for NodeFeatures {
    fn default() -> Self {
        Self::empty()
    }
}

impl NodeFeatures {
    pub fn empty() -> Self {
        Self::new(Vec::new())
    }

    pub fn new(features: Vec<NodeFeature>) -> Self {
        Self(dedup(features))
    }

    #[inline]
    pub fn inner(&self) -> &Vec<NodeFeature> {
        &self.0
    }

    #[inline]
    pub fn inner_mut(&mut self) -> &mut Vec<NodeFeature> {
        &mut self.0
    }

    pub fn count(&self) -> usize {
        self.inner().len()
    }

    pub fn add(&mut self, feature: NodeFeature) {
        if !self.contains(&feature) {
            self.inner_mut().push(feature);
        }
    }

    pub fn contains(&self, feature: &NodeFeature) -> bool {
        self.inner().contains(feature)
    }
}

fn dedup(features: Vec<NodeFeature>) -> Vec<NodeFeature> {
    features.into_iter().fold(Vec::new(), |mut acc, feature| {
        if !acc.contains(&feature) {
            acc.push(feature)
        }
        acc
    })
}

impl<T> From<T> for NodeFeatures
where T: AsRef<[NodeFeature]>
{
    fn from(items: T) -> Self {
        NodeFeatures::new(items.as_ref().to_vec())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tari_utilities::message_format::MessageFormat;

    #[test]
    fn serialize_u8() {
        assert_eq!(NodeFeature::MessagePropagation.to_binary().unwrap(), &[0]);
    }

    #[test]
    fn dedup() {
        let features: NodeFeatures = [NodeFeature::MessagePropagation, NodeFeature::MessagePropagation].into();
        assert_eq!(features.count(), 1);
    }

    #[test]
    fn add() {
        let mut features = NodeFeatures::default();
        assert_eq!(features.contains(&NodeFeature::MessagePropagation), false);
        features.add(NodeFeature::MessagePropagation);
        assert_eq!(features.contains(&NodeFeature::MessagePropagation), true);
    }
}
