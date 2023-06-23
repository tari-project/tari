pub fn shrink_hashmap<K: Eq + Hash, V>(map: &mut HashMap<K, V>) -> (usize, usize) {
    let cap = map.capacity();
    let extra_cap = cap - map.len();
    if extra_cap > 100 {
        map.shrink_to(map.len() + 100);
    }

    (cap, map.capacity())
}