// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::collections::VecDeque;

const FREE: usize = usize::MAX;
const UNREACHED: usize = usize::MAX;

/// Returns the size of a maximum matching between the left vertices (one per entry of `adjacency`, listing the right
/// vertices it may be matched to) and `num_right` right vertices, using Hopcroft-Karp. Every entry of `adjacency`
/// must be less than `num_right`.
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
pub(crate) fn hopcroft_karp(adjacency: &[Vec<u32>], num_right: usize) -> usize {
    let num_left = adjacency.len();
    let mut match_of_left = vec![FREE; num_left];
    let mut match_of_right = vec![FREE; num_right];
    let mut layer = vec![UNREACHED; num_left];
    let mut matched = 0;

    loop {
        // Breadth first search from every free left vertex, layering the left vertices by distance
        let mut queue = VecDeque::new();
        for left in 0..num_left {
            if match_of_left[left] == FREE {
                layer[left] = 0;
                queue.push_back(left);
            } else {
                layer[left] = UNREACHED;
            }
        }
        let mut found_free_right = false;
        while let Some(left) = queue.pop_front() {
            for &right in &adjacency[left] {
                let next = match_of_right[right as usize];
                if next == FREE {
                    found_free_right = true;
                    continue;
                }
                if layer[next] == UNREACHED {
                    layer[next] = layer[left] + 1;
                    queue.push_back(next);
                }
            }
        }
        if !found_free_right {
            return matched;
        }

        // Depth first search along the layers for augmenting paths, with an explicit stack of left vertices. Each
        // left vertex on the stack has tried the edges before `next_edge` and moved on along the last of them.
        let mut next_edge = vec![0usize; num_left];
        for root in 0..num_left {
            if match_of_left[root] != FREE {
                continue;
            }
            let mut stack = vec![root];
            while let Some(&left) = stack.last() {
                let Some(&right) = adjacency[left].get(next_edge[left]) else {
                    // Dead end: no augmenting path goes through this vertex in this phase
                    layer[left] = UNREACHED;
                    stack.pop();
                    continue;
                };
                next_edge[left] += 1;
                let next = match_of_right[right as usize];
                if next == FREE {
                    // Flip every edge on the path
                    for &on_path in &stack {
                        let right = adjacency[on_path][next_edge[on_path] - 1] as usize;
                        match_of_left[on_path] = right;
                        match_of_right[right] = on_path;
                    }
                    matched += 1;
                    break;
                }
                if layer[next] != UNREACHED && layer[next] == layer[left] + 1 {
                    stack.push(next);
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn it_finds_a_maximum_matching() {
        assert_eq!(hopcroft_karp(&[], 3), 0);
        assert_eq!(hopcroft_karp(&[vec![0], vec![0]], 1), 1);
        // Greedy lowest index would give 0 to the first and leave the second unmatched
        assert_eq!(hopcroft_karp(&[vec![0, 1], vec![0]], 2), 2);
        assert_eq!(hopcroft_karp(&[vec![0, 1, 2], vec![0, 1], vec![0]], 3), 3);
        assert_eq!(hopcroft_karp(&[vec![0, 1], vec![0, 1], vec![0, 1]], 2), 2);
        // Left i admits i and i + 1, the last admits only 0: matching the last needs an augmenting path through all
        let chain: Vec<Vec<u32>> = (0..100u32)
            .map(|i| if i == 99 { vec![0] } else { vec![i, i.saturating_add(1)] })
            .collect();
        assert_eq!(hopcroft_karp(&chain, 100), 100);
    }
}
